//! Command-line entry point.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use git_planet::encode;
use git_planet::error::{Error, Result};
use git_planet::github;
use git_planet::hud::render_share_card;
use git_planet::palette::Palette;
use git_planet::render::{Renderer, STILL_FRAME};
use git_planet::snapshot::{AccountSnapshot, ContributionWindow};

const USAGE: &str = "\
Usage:
  git-planet collect <login> [--window annual|rolling] [--output snapshot.json]
  git-planet render <snapshot.json> [--output-dir planet] [--canon]
  git-planet demo [--commits 2800] [--output-dir planet] [--canon]

`collect` needs a GitHub token in GITHUB_TOKEN (or GH_TOKEN).";

const DEFAULT_OUTPUT_DIR: &str = "planet";
const DEFAULT_DEMO_COMMITS: u64 = 2800;

enum Command {
    Collect {
        login: String,
        window: ContributionWindow,
        output: Option<PathBuf>,
    },
    Render {
        snapshot: PathBuf,
        options: RenderOptions,
    },
    Demo {
        commits: u64,
        options: RenderOptions,
    },
}

struct RenderOptions {
    output_dir: PathBuf,
    canon: bool,
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if matches!(
        args.first().map(String::as_str),
        Some("-h" | "--help" | "help")
    ) {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    match parse(&args).and_then(run) {
        Ok(()) => ExitCode::SUCCESS,
        Err(Error::Usage(msg)) => {
            eprintln!("git-planet: {msg}\n\n{USAGE}");
            ExitCode::from(2)
        }
        Err(err) => {
            eprintln!("git-planet: {err}");
            ExitCode::FAILURE
        }
    }
}

fn parse(args: &[String]) -> Result<Command> {
    let (subcommand, rest) = args
        .split_first()
        .ok_or_else(|| Error::Usage("missing command".to_owned()))?;
    let mut flags = Flags::new(rest);

    let command = match subcommand.as_str() {
        // Flags (and their values) are consumed before positionals, in field order.
        "collect" => Command::Collect {
            window: flags
                .value("--window")?
                .map(|w| w.parse())
                .transpose()?
                .unwrap_or_default(),
            output: flags.value("--output")?.map(PathBuf::from),
            login: flags.positional("<login>")?,
        },
        "render" => Command::Render {
            options: render_options(&mut flags)?,
            snapshot: PathBuf::from(flags.positional("<snapshot.json>")?),
        },
        "demo" => Command::Demo {
            commits: flags
                .value("--commits")?
                .map(|c| {
                    c.parse()
                        .map_err(|_| Error::Usage(format!("invalid --commits value `{c}`")))
                })
                .transpose()?
                .unwrap_or(DEFAULT_DEMO_COMMITS),
            options: render_options(&mut flags)?,
        },
        other => return Err(Error::Usage(format!("unknown command `{other}`"))),
    };
    flags.finish()?;
    Ok(command)
}

fn render_options(flags: &mut Flags) -> Result<RenderOptions> {
    Ok(RenderOptions {
        output_dir: flags
            .value("--output-dir")?
            .map_or_else(|| PathBuf::from(DEFAULT_OUTPUT_DIR), PathBuf::from),
        canon: flags.switch("--canon"),
    })
}

/// Minimal flag parser: values are consumed as they are requested and leftovers are errors.
struct Flags {
    args: Vec<Option<String>>,
}

impl Flags {
    fn new(args: &[String]) -> Self {
        Self {
            args: args.iter().cloned().map(Some).collect(),
        }
    }

    fn positional(&mut self, name: &str) -> Result<String> {
        self.args
            .iter_mut()
            .find(|a| a.as_deref().is_some_and(|s| !s.starts_with("--")))
            .and_then(Option::take)
            .ok_or_else(|| Error::Usage(format!("missing {name}")))
    }

    fn value(&mut self, flag: &str) -> Result<Option<String>> {
        let Some(pos) = self.args.iter().position(|a| a.as_deref() == Some(flag)) else {
            return Ok(None);
        };
        self.args[pos] = None;
        self.args
            .get_mut(pos + 1)
            .and_then(Option::take)
            .map(Some)
            .ok_or_else(|| Error::Usage(format!("missing value after {flag}")))
    }

    fn switch(&mut self, flag: &str) -> bool {
        self.args
            .iter_mut()
            .find(|a| a.as_deref() == Some(flag))
            .map(Option::take)
            .is_some()
    }

    fn finish(self) -> Result<()> {
        match self.args.into_iter().flatten().next() {
            Some(extra) => Err(Error::Usage(format!("unexpected argument `{extra}`"))),
            None => Ok(()),
        }
    }
}

fn run(command: Command) -> Result<()> {
    match command {
        Command::Collect {
            login,
            window,
            output,
        } => {
            let snapshot = github::collect(&login, window)?;
            let json =
                serde_json::to_string_pretty(&snapshot).map_err(|e| Error::Json(e.to_string()))?;
            match output {
                Some(path) => {
                    write_file(&path, format!("{json}\n").as_bytes())?;
                    println!("saved snapshot of @{login} to {}", path.display());
                }
                None => println!("{json}"),
            }
            Ok(())
        }
        Command::Render { snapshot, options } => {
            let raw = fs::read_to_string(&snapshot).map_err(|source| Error::Io {
                path: snapshot.clone(),
                source,
            })?;
            let snapshot: AccountSnapshot = serde_json::from_str(&raw)
                .map_err(|e| Error::Json(format!("{}: {e}", snapshot.display())))?;
            render_to_directory(&snapshot, &options)
        }
        Command::Demo { commits, options } => {
            render_to_directory(&AccountSnapshot::demo(commits), &options)
        }
    }
}

fn render_to_directory(snapshot: &AccountSnapshot, options: &RenderOptions) -> Result<()> {
    let palette = Palette::from_snapshot(snapshot, options.canon);
    let renderer = Renderer::new(snapshot, &palette);
    let frames = renderer.render_loop();
    let still = &frames[STILL_FRAME as usize];
    let bio = renderer.biology();

    let gif = encode::animated_gif(&frames)?;
    let png = encode::frame_png(still)?;
    let card = encode::share_card_png(&render_share_card(bio, &palette, still))?;

    let dir = &options.output_dir;
    fs::create_dir_all(dir).map_err(|source| Error::Io {
        path: dir.clone(),
        source,
    })?;
    for (name, bytes) in [
        ("planet.gif", &gif),
        ("planet.png", &png),
        ("share-card.png", &card),
    ] {
        write_file(&dir.join(name), bytes)?;
    }

    println!(
        "@{}: {} ({} contributions) -> {}",
        snapshot.login,
        bio.stage_name(),
        bio.commits,
        dir.display()
    );
    Ok(())
}

fn write_file(path: &Path, bytes: &[u8]) -> Result<()> {
    fs::write(path, bytes).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })
}
