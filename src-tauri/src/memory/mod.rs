//! Local persistent memory: conversations, messages, memories and projects.
//! Everything is stored in the local SQLite database; nothing is synced.

pub mod conversations;
pub mod memories;
pub mod projects;
pub mod sensitive;

pub use conversations::*;
pub use memories::*;
pub use projects::*;
