//! YouTube Music: sign-in via Firefox, library and home from the private web API,
//! audio through yt-dlp.

pub mod client;
pub mod cookies;
pub mod stream;

pub use client::{Card, CardKind, Client, Item, Overview, Song};
pub use cookies::Jar;
