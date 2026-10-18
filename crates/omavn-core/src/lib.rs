//! omavn-core: everything behind the `omavn` CLI, usable as a library.
//!
//! The pipeline is: [`ingest`] unwraps whatever the user downloaded (nested zips,
//! multi-volume rars, SFX archives) into a staging dir, [`detect`] figures out
//! whether that is a game directory or an installer and which engine it uses,
//! [`install`] turns it into a [`library::Game`], and [`launch`] builds the
//! command that runs it (natively when possible, through Proton otherwise).

pub mod archive;
pub mod art;
pub mod catalog;
pub mod desktop;
pub mod detect;
pub mod download;
pub mod feedback;
pub mod ingest;
pub mod install;
pub mod launch;
pub mod library;
pub mod meta;
pub mod omarchy;
pub mod paths;
pub mod report;
pub mod runtime;
pub mod saves;
pub mod update;
pub mod vndb;

pub use paths::Paths;
pub use report::{Event, Reporter};
