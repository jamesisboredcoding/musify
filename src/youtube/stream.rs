//! Getting audio: yt-dlp downloads each song once into ~/.cache/musify/audio as m4a
//! (AAC, which rodio can decode), then Musify plays the file.

use std::path::PathBuf;
use std::time::Duration;

use tokio::process::Command;

use super::cookies;

pub fn cache_dir() -> PathBuf {
    dirs::cache_dir().unwrap_or_else(|| PathBuf::from("/tmp")).join("musify").join("audio")
}

pub fn cached(video_id: &str) -> Option<PathBuf> {
    let path = cache_dir().join(format!("{video_id}.m4a"));
    path.exists().then_some(path)
}

/// The download error that means yt-dlp needs updating (the app offers a button for it).
pub const OUTDATED: &str = "yt-dlp is out of date for YouTube's latest change.";

/// pip's copy in ~/.local/bin first: apps started from the menu often don't have it on PATH.
fn program() -> std::path::PathBuf {
    let local = dirs::home_dir().unwrap_or_default().join(".local/bin/yt-dlp");
    if local.is_file() { local } else { "yt-dlp".into() }
}

/// Updates yt-dlp and its challenge solver for this user with pip.
pub async fn update_ytdlp() -> Result<(), String> {
    let run = Command::new("python3")
        .args(["-m", "pip", "install", "--user", "--upgrade", "yt-dlp", "yt-dlp-ejs"])
        .kill_on_drop(true)
        .output();
    let output = match tokio::time::timeout(Duration::from_secs(300), run).await {
        Err(_) => return Err("Updating yt-dlp took too long.".into()),
        Ok(Err(e)) => return Err(format!("Couldn't run pip ({e})")),
        Ok(Ok(o)) => o,
    };
    if output.status.success() {
        return Ok(());
    }
    let err = String::from_utf8_lossy(&output.stderr);
    Err(format!("pip couldn't update yt-dlp: {}", err.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("unknown error")))
}

pub async fn fetch(video_id: String) -> Result<PathBuf, String> {
    if let Some(path) = cached(&video_id) {
        return Ok(path);
    }
    let dir = cache_dir();
    tokio::fs::create_dir_all(&dir).await.map_err(|e| e.to_string())?;
    let mut cmd = Command::new(program());
    cmd.args([
        "--format",
        "bestaudio[ext=m4a]/bestaudio",
        // Opus-only songs are converted to AAC so rodio can decode them.
        "--extract-audio",
        "--audio-format",
        "m4a",
        "--no-playlist",
        "--no-progress",
        "--quiet",
        "--no-warnings",
        "--output",
    ])
    .arg(dir.join("%(id)s.%(ext)s"))
    .kill_on_drop(true);
    // YouTube's player challenges need a JavaScript runtime; yt-dlp only enables deno by default.
    if on_path("node") {
        cmd.args(["--js-runtimes", "node"]);
    }
    // yt-dlp writes cookies back to the file it's given (and trims it), so it gets a throwaway copy.
    let jar_copy = private_copy_of_jar(&video_id);
    if let Some(copy) = &jar_copy {
        cmd.arg("--cookies").arg(copy);
    }
    cmd.arg(format!("https://music.youtube.com/watch?v={video_id}"));

    let result = tokio::time::timeout(Duration::from_secs(180), cmd.output()).await;
    if let Some(copy) = jar_copy {
        let _ = tokio::fs::remove_file(copy).await;
    }
    let output = match result {
        Err(_) => return Err("Downloading took too long.".into()),
        Ok(Err(e)) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err("yt-dlp isn't installed. Install it (sudo dnf install yt-dlp) to play YouTube songs.".into());
        }
        Ok(Err(e)) => return Err(e.to_string()),
        Ok(Ok(o)) => o,
    };
    if let Some(path) = cached(&video_id) {
        return Ok(path);
    }
    let err = String::from_utf8_lossy(&output.stderr);
    // YouTube changes its player every few weeks; an old yt-dlp then finds no audio at all.
    if err.contains("challenge solving failed") || err.contains("Requested format is not available") {
        return Err(OUTDATED.into());
    }
    let line = err.lines().rev().find(|l| l.contains("ERROR")).unwrap_or("yt-dlp couldn't get this song.");
    Err(line.trim_start_matches("ERROR: ").chars().take(160).collect())
}

fn on_path(program: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|paths| std::env::split_paths(&paths).any(|p| p.join(program).is_file()))
}

fn private_copy_of_jar(video_id: &str) -> Option<PathBuf> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    let jar = std::fs::read(cookies::jar_path()).ok()?;
    let dir = cookies::data_dir().join("tmp");
    std::fs::create_dir_all(&dir).ok()?;
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).ok()?;
    let path = dir.join(format!("cookies-{video_id}.txt"));
    let mut f = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&path).ok()?;
    std::io::Write::write_all(&mut f, &jar).ok()?;
    Some(path)
}

/// Downloads a thumbnail and samples its color for the cover-tinted UI.
pub async fn artwork(http: reqwest::Client, url: String) -> Option<(Vec<u8>, iced::Color)> {
    let mut bytes = get_image(&http, &url).await;
    // Not every video has an hq720 frame; fall back to the standard one.
    if bytes.is_none()
        && let Some(id) = super::client::video_thumb_id(&url)
    {
        bytes = get_image(&http, &format!("https://i.ytimg.com/vi/{id}/hqdefault.jpg")).await;
    }
    let bytes = bytes?;
    let color = tokio::task::spawn_blocking({
        let bytes = bytes.clone();
        move || crate::library::sample_color(&bytes)
    })
    .await
    .ok()??;
    Some((bytes, color))
}

async fn get_image(http: &reqwest::Client, url: &str) -> Option<Vec<u8>> {
    let r = http.get(url).send().await.ok()?;
    if !r.status().is_success() {
        return None;
    }
    let bytes = r.bytes().await.ok()?.to_vec();
    // ytimg answers some misses with a tiny grey placeholder instead of a 404.
    (bytes.len() > 2_000).then_some(bytes)
}

/// The sharpest available version of a cover, for Now playing.
pub async fn hires(http: reqwest::Client, url: String) -> Option<Vec<u8>> {
    for candidate in super::client::hires_candidates(&url) {
        if let Some(bytes) = get_image(&http, &candidate).await {
            return Some(bytes);
        }
    }
    None
}
