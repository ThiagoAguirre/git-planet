//! The account data the planet is drawn from.

use std::collections::BTreeMap;
use std::str::FromStr;

use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::Error;

/// Which contributions count toward the planet's growth.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ContributionWindow {
    /// January 1st of the current year until now: the planet starts over every new year.
    #[default]
    Annual,
    /// The trailing 365 days.
    Rolling,
}

impl ContributionWindow {
    /// The `[from, to]` range of this window as of `now`.
    pub fn range(self, now: DateTime<Utc>) -> (DateTime<Utc>, DateTime<Utc>) {
        let start_day = match self {
            Self::Annual => NaiveDate::from_ymd_opt(now.year(), 1, 1).unwrap_or(now.date_naive()),
            Self::Rolling => now.date_naive() - Duration::days(364),
        };
        (start_day.and_time(NaiveTime::MIN).and_utc(), now)
    }
}

impl FromStr for ContributionWindow {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "annual" => Ok(Self::Annual),
            "rolling" => Ok(Self::Rolling),
            _ => Err(Error::Usage(format!(
                "unknown window `{s}` (expected `annual` or `rolling`)"
            ))),
        }
    }
}

/// A language that is the primary language of some of the account's repositories.
#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct LanguageShare {
    pub name: String,
    /// GitHub's color for the language, as `#rrggbb`.
    pub hex_color: String,
    pub repo_count: u32,
}

/// Everything the renderer needs to know about an account.
#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct AccountSnapshot {
    pub login: String,
    pub account_id: u64,
    pub observed_on: NaiveDate,
    pub window: ContributionWindow,
    /// Contributions inside `window`; this is what makes the planet grow.
    pub total_contributions: u64,
    /// Languages ranked by how many repositories use them.
    pub languages: Vec<LanguageShare>,
}

impl AccountSnapshot {
    /// A fixed snapshot for previewing any contribution count without network access.
    pub fn demo(total_contributions: u64) -> Self {
        let language = |name: &str, hex_color: &str, repo_count| LanguageShare {
            name: name.to_owned(),
            hex_color: hex_color.to_owned(),
            repo_count,
        };
        Self {
            login: "demo-user".to_owned(),
            account_id: 42,
            observed_on: Utc::now().date_naive(),
            window: ContributionWindow::Annual,
            total_contributions,
            languages: vec![
                language("TypeScript", "#3178c6", 10),
                language("Rust", "#dea584", 8),
                language("Python", "#3572A5", 5),
            ],
        }
    }

    /// A deterministic seed unique to the account, used to vary the planet's terrain.
    pub fn identity_seed(&self) -> u64 {
        let mut h = self.account_id ^ 0x9e37_79b9_7f4a_7c15;
        for b in self.login.bytes() {
            h = (h ^ u64::from(b)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        }
        h
    }
}

/// Turn `{language: (color, repo_count)}` into a list sorted by repo count (then name).
pub fn rank_languages(counts: BTreeMap<String, (String, u32)>) -> Vec<LanguageShare> {
    let mut list: Vec<LanguageShare> = counts
        .into_iter()
        .map(|(name, (hex_color, repo_count))| LanguageShare {
            name,
            hex_color,
            repo_count,
        })
        .collect();
    list.sort_by(|a, b| {
        b.repo_count
            .cmp(&a.repo_count)
            .then_with(|| a.name.cmp(&b.name))
    });
    list
}
