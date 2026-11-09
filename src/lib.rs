//! `git-planet`: a pixel-art Mars-like planet for a GitHub profile README that grows from a
//! gray protoplanet into a full red planet as the account's contributions add up.
//!
//! Pipeline: [`github::collect`] → [`snapshot::AccountSnapshot`] → [`render::Renderer`]
//! (using [`planet`], [`palette`] and [`hud`]) → [`encode`].

pub mod encode;
pub mod error;
pub mod github;
pub mod hud;
pub mod palette;
pub mod planet;
pub mod render;
pub mod snapshot;
