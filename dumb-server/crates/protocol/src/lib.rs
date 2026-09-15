//! Shared protocol: wire message types and display-name validation.
//!
//! Used by the server (parse client frames / build snapshots) and the bot
//! loader (build client frames / parse snapshots). The Unity client mirrors
//! these same structs independently in C#.

pub mod messages;
pub mod validation;

pub use messages::{ClientMsg, PlayerState, ServerMsg, SnapshotPlayer};
pub use validation::{sanitize_name, NameError, MAX_NAME_LEN};