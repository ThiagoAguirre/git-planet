use chrono::{TimeZone, Utc};
use git_planet::encode;
use git_planet::hud::render_share_card;
use git_planet::palette::{parse_hex_color, Palette};
use git_planet::planet::PlanetBiology;
use git_planet::render::{Renderer, LOOP_FRAME_COUNT, STILL_FRAME};
use git_planet::snapshot::{AccountSnapshot, ContributionWindow};

#[test]
fn stages_follow_contribution_thresholds() {
    let cases = [
        (0, "STAGE 1: PROTOPLANET", 0),
        (149, "STAGE 1: PROTOPLANET", 0),
        (150, "STAGE 2: RED PLANET", 0),
        (500, "STAGE 3: DARK MARIA", 0),
        (1200, "STAGE 4: ATMOSPHERE", 1),
        (2500, "STAGE 5: FULL MARS", 2),
    ];
    for (commits, label, moons) in cases {
        let bio = PlanetBiology::from_commits(commits);
        assert_eq!(bio.hud_stage_label(), label, "{commits} contributions");
        assert_eq!(bio.moons().len(), moons, "{commits} contributions");
    }
    assert!(PlanetBiology::from_commits(2500).radius > PlanetBiology::from_commits(0).radius);
}

#[test]
fn contribution_windows_cover_the_expected_range() {
    let now = Utc.with_ymd_and_hms(2026, 3, 15, 12, 0, 0).unwrap();

    let (from, to) = ContributionWindow::Annual.range(now);
    assert_eq!(from, Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap());
    assert_eq!(to, now);

    let (from, _) = ContributionWindow::Rolling.range(now);
    assert_eq!(from, Utc.with_ymd_and_hms(2025, 3, 16, 0, 0, 0).unwrap());

    assert_eq!(
        "rolling".parse::<ContributionWindow>().unwrap(),
        ContributionWindow::Rolling
    );
    assert!("weekly".parse::<ContributionWindow>().is_err());
}

#[test]
fn hex_colors_are_parsed_strictly() {
    assert_eq!(parse_hex_color("#dea584"), Some([222, 165, 132]));
    assert_eq!(parse_hex_color("3178c6"), Some([49, 120, 198]));
    assert_eq!(parse_hex_color("#123"), None);
    assert_eq!(parse_hex_color("#zzzzzz"), None);
}

#[test]
fn rendering_is_deterministic_and_loops_seamlessly() {
    let snapshot = AccountSnapshot::demo(2800);
    let palette = Palette::from_snapshot(&snapshot, false);
    let renderer = Renderer::new(&snapshot, &palette);

    let first = renderer.render_frame(0);
    assert_eq!(first, renderer.render_frame(0));
    assert_eq!(first, renderer.render_frame(LOOP_FRAME_COUNT));
    assert_ne!(first, renderer.render_frame(LOOP_FRAME_COUNT / 2));
}

#[test]
fn outputs_encode_to_valid_files() {
    let snapshot = AccountSnapshot::demo(800);
    let palette = Palette::from_snapshot(&snapshot, true);
    let renderer = Renderer::new(&snapshot, &palette);
    let frames = renderer.render_loop();
    assert_eq!(frames.len(), LOOP_FRAME_COUNT as usize);

    let still = &frames[STILL_FRAME as usize];
    let gif = encode::animated_gif(&frames).expect("gif encodes");
    let png = encode::frame_png(still).expect("png encodes");
    let card = render_share_card(renderer.biology(), &palette, still);
    let card_png = encode::share_card_png(&card).expect("card encodes");

    assert!(gif.starts_with(b"GIF89a"));
    assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
    assert!(card_png.starts_with(b"\x89PNG\r\n\x1a\n"));
    assert!(encode::animated_gif(&[]).is_err());
}
