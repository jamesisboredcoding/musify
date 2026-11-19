//! "Listening to …" on your Discord profile, through the desktop app's local IPC socket.
//! Off unless turned on; nothing is sent anywhere but the Discord app on this computer.

use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;

use discord_rich_presence::activity::{Activity, ActivityType, Assets, Button, StatusDisplayType, Timestamps};
use discord_rich_presence::{DiscordIpc, DiscordIpcClient};

/// The Discord application "Musify" (discord.com/developers). Its name is what profiles show
/// when the song title doesn't fit; the id isn't a secret.
const APP_ID: &str = "1572922815102058528";

#[derive(Debug, Clone, PartialEq)]
pub struct Presence {
    pub title: String,
    pub artist: String,
    pub album: String,
    /// Cover art URL; Discord fetches it itself.
    pub cover: Option<String>,
    /// music.youtube.com link for YouTube songs.
    pub link: Option<String>,
    /// Unix milliseconds the song (would have) started and will end; None while paused.
    pub span: Option<(i64, i64)>,
}

/// Talks to Discord from its own thread, so a slow or missing Discord never blocks the UI.
pub struct Discord {
    tx: mpsc::Sender<Option<Presence>>,
}

impl Discord {
    pub fn configured() -> bool {
        !APP_ID.is_empty()
    }

    pub fn start() -> Self {
        let (tx, rx) = mpsc::channel::<Option<Presence>>();
        std::thread::Builder::new()
            .name("discord".into())
            .spawn(move || run(rx))
            .expect("spawn discord thread");
        Self { tx }
    }

    /// Show this song, or nothing. Dropping `Discord` ends the thread, which clears the status.
    pub fn set(&self, presence: Option<Presence>) {
        let _ = self.tx.send(presence);
    }
}

fn run(rx: mpsc::Receiver<Option<Presence>>) {
    let mut client: Option<DiscordIpcClient> = None;
    let mut want: Option<Presence> = None;
    let mut shown = false;
    loop {
        // Wake for new songs, or every 15 s to retry if Discord wasn't running.
        match rx.recv_timeout(Duration::from_secs(15)) {
            Ok(p) => want = p,
            Err(RecvTimeoutError::Timeout) if shown || want.is_none() => continue,
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        // Only the newest request matters.
        while let Ok(p) = rx.try_recv() {
            want = p;
        }
        if client.is_none() {
            let mut c = DiscordIpcClient::new(APP_ID);
            if c.connect().is_err() {
                shown = false;
                continue;
            }
            client = Some(c);
        }
        let c = client.as_mut().unwrap();
        let ok = match &want {
            Some(p) => c.set_activity(activity(p)).is_ok(),
            None => c.clear_activity().is_ok(),
        };
        shown = ok;
        if !ok {
            // Discord quit or restarted; reconnect next time.
            let _ = c.close();
            client = None;
        }
    }
    if let Some(mut c) = client {
        let _ = c.clear_activity();
        let _ = c.close();
    }
}

/// Discord wants 2–128 characters per text field.
fn fit(s: &str) -> String {
    let s: String = s.chars().take(128).collect();
    if s.chars().count() < 2 { format!("{s}  ") } else { s }
}

fn activity(p: &Presence) -> Activity<'_> {
    let mut a = Activity::new()
        .activity_type(ActivityType::Listening)
        .status_display_type(StatusDisplayType::Details)
        .details(fit(&p.title))
        .state(fit(&if p.span.is_some() { p.artist.clone() } else { format!("Paused · {}", p.artist) }));
    let mut assets = Assets::new().large_text(fit(if p.album.is_empty() { &p.title } else { &p.album }));
    if let Some(cover) = &p.cover {
        assets = assets.large_image(cover.as_str());
    }
    a = a.assets(assets);
    if let Some((start, end)) = p.span {
        // Without a known length, Discord shows time elapsed instead of a bar.
        let t = Timestamps::new().start(start);
        a = a.timestamps(if end > start { t.end(end) } else { t });
    }
    if let Some(link) = &p.link {
        a = a.details_url(link.as_str()).buttons(vec![Button::new("Listen on YouTube Music", link.as_str())]);
    }
    a
}

#[cfg(test)]
mod tests {
    /// `cargo test discord -- --ignored` checks the handshake with a running Discord (shows nothing).
    #[test]
    #[ignore]
    fn connects_to_discord() {
        use discord_rich_presence::{DiscordIpc, DiscordIpcClient};
        let mut c = DiscordIpcClient::new(super::APP_ID);
        c.connect().expect("Discord running and app id accepted");
        c.clear_activity().expect("clear");
        c.close().ok();
    }

    /// Shows a made-up song 1:15 into 3:00 for `MUSIFY_DISCORD_HOLD` seconds, to check the progress bar.
    #[test]
    #[ignore]
    fn shows_progress() {
        use discord_rich_presence::{DiscordIpc, DiscordIpcClient};
        let hold: u64 = std::env::var("MUSIFY_DISCORD_HOLD").ok().and_then(|s| s.parse().ok()).unwrap_or(60);
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;
        let p = super::Presence {
            title: "Progress test".into(),
            artist: "Musify".into(),
            album: String::new(),
            cover: None,
            link: None,
            span: Some((now - 75_000, now + 105_000)),
        };
        let mut c = DiscordIpcClient::new(super::APP_ID);
        c.connect().unwrap();
        c.set_activity(super::activity(&p)).unwrap();
        std::thread::sleep(std::time::Duration::from_secs(hold));
        c.clear_activity().ok();
        c.close().ok();
    }
}
