//! Fetches an account's contribution total and repository languages from the GitHub GraphQL API.
//!
//! Every request has connect/read/overall timeouts, and transient failures (network errors,
//! `429`, `5xx`) are retried with exponential backoff and jitter, so a bad connection fails
//! with a clear error instead of hanging.

use std::collections::BTreeMap;
use std::env;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::json;

use crate::error::{Error, Result};
use crate::snapshot::{rank_languages, AccountSnapshot, ContributionWindow};

const GRAPHQL_URL: &str = "https://api.github.com/graphql";
const USER_AGENT: &str = concat!("git-planet/", env!("CARGO_PKG_VERSION"));
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const READ_TIMEOUT: Duration = Duration::from_secs(20);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_ATTEMPTS: u32 = 4;
const BASE_BACKOFF: Duration = Duration::from_millis(500);
const MAX_LOGIN_LEN: usize = 39;

const ACCOUNT_QUERY: &str = r#"
query($login: String!, $from: DateTime!, $to: DateTime!) {
  user(login: $login) {
    databaseId
    repositories(first: 100, ownerAffiliations: OWNER, isFork: false, orderBy: {field: UPDATED_AT, direction: DESC}) {
      nodes { primaryLanguage { name color } }
    }
    contributionsCollection(from: $from, to: $to) {
      contributionCalendar { totalContributions }
    }
  }
}
"#;

#[derive(Deserialize)]
struct Response {
    data: Option<Data>,
    #[serde(default)]
    errors: Vec<GraphQlError>,
}

#[derive(Deserialize)]
struct GraphQlError {
    message: String,
}

#[derive(Deserialize)]
struct Data {
    user: Option<User>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct User {
    database_id: Option<u64>,
    repositories: Repositories,
    contributions_collection: ContributionsCollection,
}

#[derive(Deserialize)]
struct Repositories {
    nodes: Vec<Repository>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Repository {
    primary_language: Option<Language>,
}

#[derive(Deserialize)]
struct Language {
    name: String,
    color: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ContributionsCollection {
    contribution_calendar: ContributionCalendar,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ContributionCalendar {
    total_contributions: u64,
}

/// GitHub logins are 1-39 ASCII letters, digits or single hyphens, not starting or ending
/// with a hyphen.
pub fn validate_login(login: &str) -> Result<()> {
    let valid = !login.is_empty()
        && login.len() <= MAX_LOGIN_LEN
        && login.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        && !login.starts_with('-')
        && !login.ends_with('-')
        && !login.contains("--");
    if valid {
        Ok(())
    } else {
        Err(Error::Config(format!(
            "`{login}` is not a valid GitHub login"
        )))
    }
}

/// Read the API token from `GITHUB_TOKEN` (or `GH_TOKEN`).
fn token_from_env() -> Result<String> {
    ["GITHUB_TOKEN", "GH_TOKEN"]
        .iter()
        .filter_map(|name| env::var(name).ok())
        .map(|value| value.trim().to_owned())
        .find(|value| !value.is_empty())
        .ok_or_else(|| {
            Error::Config(
                "set GITHUB_TOKEN (for example `export GITHUB_TOKEN=$(gh auth token)`); \
                 GitHub Actions provides it automatically"
                    .to_owned(),
            )
        })
}

/// Collect a snapshot of `login` over `window`.
pub fn collect(login: &str, window: ContributionWindow) -> Result<AccountSnapshot> {
    validate_login(login)?;
    let token = token_from_env()?;
    let now = Utc::now();
    let (from, to) = window.range(now);
    let user = fetch_user(login, &token, from, to)?;

    let mut counts: BTreeMap<String, (String, u32)> = BTreeMap::new();
    for language in user
        .repositories
        .nodes
        .into_iter()
        .filter_map(|repo| repo.primary_language)
    {
        let color = language.color.unwrap_or_else(|| "#8b949e".to_owned());
        counts.entry(language.name).or_insert((color, 0)).1 += 1;
    }

    Ok(AccountSnapshot {
        login: login.to_owned(),
        account_id: user.database_id.unwrap_or_default(),
        observed_on: now.date_naive(),
        window,
        total_contributions: user
            .contributions_collection
            .contribution_calendar
            .total_contributions,
        languages: rank_languages(counts),
    })
}

fn fetch_user(login: &str, token: &str, from: DateTime<Utc>, to: DateTime<Utc>) -> Result<User> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(CONNECT_TIMEOUT)
        .timeout_read(READ_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .user_agent(USER_AGENT)
        .build();
    let payload = json!({
        "query": ACCOUNT_QUERY,
        "variables": { "login": login, "from": from.to_rfc3339(), "to": to.to_rfc3339() },
    });
    let authorization = format!("Bearer {token}");

    let response: Response = with_retries(|| {
        agent
            .post(GRAPHQL_URL)
            .set("Authorization", &authorization)
            .send_json(&payload)
            .map_err(Box::new)
    })?
    .into_json()
    .map_err(|e| Error::Json(e.to_string()))?;

    if let Some(first) = response.errors.first() {
        return Err(Error::GitHub(first.message.clone()));
    }
    response
        .data
        .and_then(|data| data.user)
        .ok_or_else(|| Error::GitHub(format!("user `{login}` not found")))
}

/// Run an idempotent request, retrying transient failures with exponential backoff + jitter.
fn with_retries(
    mut request: impl FnMut() -> std::result::Result<ureq::Response, Box<ureq::Error>>,
) -> Result<ureq::Response> {
    let mut attempt = 1;
    loop {
        let error = match request() {
            Ok(response) => return Ok(response),
            Err(error) => *error,
        };
        let retryable = match &error {
            ureq::Error::Status(code, _) => *code == 429 || *code >= 500,
            ureq::Error::Transport(_) => true,
        };
        if !retryable || attempt >= MAX_ATTEMPTS {
            return Err(match error {
                ureq::Error::Status(401, _) => {
                    Error::GitHub("the token was rejected (HTTP 401)".to_owned())
                }
                ureq::Error::Status(code, _) => Error::GitHub(format!("HTTP {code}")),
                ureq::Error::Transport(t) => {
                    Error::Network(format!("{t} (after {attempt} attempts)"))
                }
            });
        }
        thread::sleep(backoff(attempt));
        attempt += 1;
    }
}

/// `BASE_BACKOFF * 2^(attempt-1)`, plus up to 50% random jitter.
fn backoff(attempt: u32) -> Duration {
    let base = BASE_BACKOFF * 2_u32.pow(attempt.saturating_sub(1));
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or_default();
    base + base.mul_f64(f64::from(nanos % 1000) / 2000.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_valid_logins() {
        for login in ["octocat", "a", "my-user-1", &"x".repeat(MAX_LOGIN_LEN)] {
            assert!(validate_login(login).is_ok(), "{login}");
        }
    }

    #[test]
    fn rejects_invalid_logins() {
        let too_long = "x".repeat(MAX_LOGIN_LEN + 1);
        for login in [
            "",
            "-user",
            "user-",
            "a--b",
            "has space",
            "ação",
            "../etc",
            &too_long,
        ] {
            assert!(validate_login(login).is_err(), "{login}");
        }
    }

    #[test]
    fn backoff_grows_and_stays_bounded() {
        let first = backoff(1);
        let third = backoff(3);
        assert!(first >= BASE_BACKOFF && first <= BASE_BACKOFF.mul_f64(1.5));
        assert!(third >= BASE_BACKOFF * 4 && third <= (BASE_BACKOFF * 4).mul_f64(1.5));
    }
}
