//! Lyrics. In order of preference:
//! 1. an `.lrc` file next to a local song,
//! 2. synced lyrics from LRCLIB (lrclib.net, an open lyrics database),
//! 3. YouTube Music's own lyrics (plain text; its timed lyrics need the Android app client),
//! 4. plain lyrics from LRCLIB.
//! Results, including "none found", are cached in ~/.cache/musify/lyrics.

use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct Lyrics {
    pub lines: Vec<(Duration, String)>,
    /// Lines have timestamps; plain lyrics are shown as a static page.
    pub synced: bool,
    /// Where they came from, credited under the lyrics.
    pub source: Option<String>,
}

impl Lyrics {
    pub fn load(audio: &Path) -> Option<Self> {
        let text = std::fs::read_to_string(audio.with_extension("lrc")).ok()?;
        parse_lrc(&text, None)
    }

    /// Index of the line being sung at `pos`.
    pub fn current(&self, pos: Duration) -> Option<usize> {
        if !self.synced {
            return None;
        }
        self.lines.iter().rposition(|(t, _)| *t <= pos)
    }
}

pub fn parse_lrc(text: &str, source: Option<String>) -> Option<Lyrics> {
    let mut lines = Vec::new();
    for raw in text.lines() {
        let mut rest = raw;
        let mut stamps = Vec::new();
        while let Some(stripped) = rest.strip_prefix('[') {
            let Some(end) = stripped.find(']') else { break };
            if let Some(t) = parse_stamp(&stripped[..end]) {
                stamps.push(t);
            }
            rest = &stripped[end + 1..];
        }
        for t in stamps {
            lines.push((t, rest.trim().to_string()));
        }
    }
    lines.sort_by_key(|(t, _)| *t);
    (!lines.is_empty()).then_some(Lyrics { lines, synced: true, source })
}

fn plain(text: &str, source: Option<String>) -> Option<Lyrics> {
    let lines: Vec<_> = text.lines().map(|l| (Duration::ZERO, l.trim().to_string())).collect();
    lines.iter().any(|(_, l)| !l.is_empty()).then_some(Lyrics { lines, synced: false, source })
}

fn parse_stamp(s: &str) -> Option<Duration> {
    let (m, rest) = s.split_once(':')?;
    let m: u64 = m.trim().parse().ok()?;
    let secs: f64 = rest.trim().parse().ok()?;
    Some(Duration::from_secs(m * 60) + Duration::from_secs_f64(secs))
}

/// What to look up.
#[derive(Debug, Clone)]
pub struct Query {
    /// Cache key: YouTube video id, or a hash of the file path.
    pub key: String,
    pub video_id: Option<String>,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration: Duration,
}

fn cache_dir() -> PathBuf {
    dirs::cache_dir().unwrap_or_else(|| PathBuf::from("/tmp")).join("musify").join("lyrics")
}

const LRCLIB: &str = "LRCLIB";
const YOUTUBE: &str = "YouTube Music";

fn from_cache(key: &str) -> Option<Option<Lyrics>> {
    let dir = cache_dir();
    if dir.join(format!("{key}.none")).exists() {
        return Some(None);
    }
    for (ext, synced) in [("lrc", true), ("txt", false)] {
        if let Ok(text) = std::fs::read_to_string(dir.join(format!("{key}.{ext}"))) {
            let (source, body) = text.split_once('\n').unwrap_or(("", &text));
            let source = Some(source.trim_start_matches("# ").to_string()).filter(|s| !s.is_empty());
            return Some(if synced { parse_lrc(body, source) } else { plain(body, source) });
        }
    }
    None
}

fn to_cache(key: &str, found: &Option<Lyrics>) {
    let dir = cache_dir();
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let (name, body) = match found {
        None => (format!("{key}.none"), String::new()),
        Some(l) => {
            let head = format!("# {}\n", l.source.clone().unwrap_or_default());
            let text: Vec<String> = if l.synced {
                l.lines
                    .iter()
                    .map(|(t, s)| format!("[{:02}:{:05.2}]{s}", t.as_secs() / 60, t.as_secs_f64() % 60.0))
                    .collect()
            } else {
                l.lines.iter().map(|(_, s)| s.clone()).collect()
            };
            (format!("{key}.{}", if l.synced { "lrc" } else { "txt" }), head + &text.join("\n"))
        }
    };
    let _ = std::fs::write(dir.join(name), body);
}

/// Strips video-title noise so lyric databases can match: "(Official Video)", "[Lyrics]", "feat." parts.
fn clean_title(title: &str) -> String {
    let mut out = String::new();
    let mut depth = 0;
    for c in title.chars() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = (depth - 1).max(0),
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    let out = out.split(" - ").last().unwrap_or(&out).to_string();
    out.split(" ft.").next().unwrap_or(&out).split(" feat.").next().unwrap_or(&out).trim().to_string()
}

#[derive(serde::Deserialize)]
struct LrcLib {
    #[serde(rename = "syncedLyrics")]
    synced: Option<String>,
    #[serde(rename = "plainLyrics")]
    plain: Option<String>,
    duration: Option<f64>,
}

async fn lrclib(http: &reqwest::Client, q: &Query) -> Option<LrcLib> {
    let title = clean_title(&q.title);
    let artist = q.artist.split([',', '&']).next().unwrap_or(&q.artist).trim().to_string();
    let secs = q.duration.as_secs();
    let mut get = http
        .get("https://lrclib.net/api/get")
        .query(&[("track_name", title.as_str()), ("artist_name", artist.as_str())]);
    if !q.album.is_empty() {
        get = get.query(&[("album_name", q.album.as_str())]);
    }
    if secs > 0 {
        get = get.query(&[("duration", secs.to_string())]);
    }
    if let Ok(r) = get.send().await
        && r.status().is_success()
        && let Ok(hit) = r.json::<LrcLib>().await
    {
        return Some(hit);
    }
    // Looser search: the closest-length result with lyrics.
    let hits: Vec<LrcLib> = http
        .get("https://lrclib.net/api/search")
        .query(&[("track_name", title.as_str()), ("artist_name", artist.as_str())])
        .send()
        .await
        .ok()?
        .json()
        .await
        .ok()?;
    hits.into_iter()
        .filter(|h| h.synced.is_some() || h.plain.is_some())
        .filter(|h| secs == 0 || h.duration.is_none_or(|d| (d - secs as f64).abs() <= 4.0))
        .min_by_key(|h| h.synced.is_none())
}

pub async fn fetch(http: reqwest::Client, youtube: Option<crate::youtube::Client>, q: Query) -> Option<Lyrics> {
    if let Some(cached) = from_cache(&q.key) {
        return cached;
    }
    let lib = lrclib(&http, &q).await;
    let mut found = lib.as_ref().and_then(|h| h.synced.as_deref()).and_then(|t| parse_lrc(t, Some(LRCLIB.into())));
    if found.is_none()
        && let (Some(yt), Some(id)) = (&youtube, &q.video_id)
        && let Ok(Some(text)) = yt.lyrics(id).await
    {
        found = plain(&text, Some(YOUTUBE.into()));
    }
    if found.is_none() {
        found = lib.as_ref().and_then(|h| h.plain.as_deref()).and_then(|t| plain(t, Some(LRCLIB.into())));
    }
    to_cache(&q.key, &found);
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleans_video_titles() {
        assert_eq!(clean_title("Rick Astley - Never Gonna Give You Up (Official Video)"), "Never Gonna Give You Up");
        assert_eq!(clean_title("Starboy (feat. Daft Punk)"), "Starboy");
        assert_eq!(clean_title("two [Lyrics]"), "two");
    }

    #[test]
    fn lrc_round_trip() {
        let l = parse_lrc("[00:12.50]hello\n[01:02.00]world", None).unwrap();
        assert_eq!(l.lines[1].0, Duration::from_secs(62));
        assert_eq!(l.current(Duration::from_secs(13)), Some(0));
    }
}
