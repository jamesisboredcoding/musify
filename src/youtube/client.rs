//! YouTube Music's private web API (the one music.youtube.com itself uses), read-only.
//! Shapes follow ytmusicapi's parsers; the walkers below are deliberately loose so small
//! layout changes on YouTube's side don't break everything.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};
use sha1::{Digest, Sha1};

use super::cookies::Jar;

const ORIGIN: &str = "https://music.youtube.com";
const API: &str = "https://music.youtube.com/youtubei/v1/";
const USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64; rv:140.0) Gecko/20100101 Firefox/140.0";

#[derive(Debug, Clone)]
pub struct Client {
    http: reqwest::Client,
    jar: Jar,
}

/// A song as listed by YouTube.
#[derive(Debug, Clone)]
pub struct Song {
    pub video_id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration: Duration,
    pub thumbnail: Option<String>,
}

/// A playlist, mix or album tile.
#[derive(Debug, Clone)]
pub struct Card {
    pub browse_id: String,
    pub title: String,
    pub subtitle: String,
    pub thumbnail: Option<String>,
    pub kind: CardKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardKind {
    Playlist,
    Mix,
    Album,
}

#[derive(Debug, Clone)]
pub enum Item {
    Card(Card),
    Song(Song),
}

#[derive(Debug, Clone)]
pub struct Shelf {
    pub title: String,
    pub items: Vec<Item>,
}

/// What a YouTube Music search found.
#[derive(Debug, Clone, Default)]
pub struct Results {
    pub songs: Vec<Song>,
    pub collections: Vec<Card>,
}

#[derive(Debug, Clone, Default)]
pub struct Overview {
    pub playlists: Vec<Card>,
    pub home: Vec<Shelf>,
}

impl Client {
    pub fn new(jar: Jar) -> Self {
        let http = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .gzip(true)
            .timeout(Duration::from_secs(20))
            .build()
            .expect("HTTP client");
        Self { http, jar }
    }

    pub fn http(&self) -> &reqwest::Client {
        &self.http
    }

    fn authorization(&self) -> String {
        let ts = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let sap = self.jar.sapisid().unwrap_or_default();
        let digest = Sha1::digest(format!("{ts} {sap} {ORIGIN}").as_bytes());
        let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        format!("SAPISIDHASH {ts}_{hex}")
    }

    async fn browse(&self, body: Value) -> Result<Value, String> {
        self.post("browse", body).await
    }

    /// Likes or un-likes a song on the signed-in account (shows up in Liked Music everywhere).
    pub async fn rate(&self, video_id: &str, like: bool) -> Result<(), String> {
        let endpoint = if like { "like/like" } else { "like/removelike" };
        self.post(endpoint, json!({ "target": { "videoId": video_id } })).await.map(|_| ())
    }

    /// YouTube Music's lyrics for a song (plain text), via the "Lyrics" tab of its watch page.
    pub async fn lyrics(&self, video_id: &str) -> Result<Option<String>, String> {
        let next = self
            .post("next", json!({ "videoId": video_id, "isAudioOnly": true, "enablePersistentPlaylistPanel": true }))
            .await?;
        let lyrics_id = find_all(&next, "tabRenderer").iter().find_map(|tab| {
            let endpoint = &tab["endpoint"]["browseEndpoint"];
            let page = endpoint["browseEndpointContextSupportedConfigs"]["browseEndpointContextMusicConfig"]["pageType"].as_str();
            (page == Some("MUSIC_PAGE_TYPE_TRACK_LYRICS")).then(|| endpoint["browseId"].as_str().map(str::to_string)).flatten()
        });
        let Some(id) = lyrics_id else { return Ok(None) };
        let page = self.browse(json!({ "browseId": id })).await?;
        Ok(find_all(&page, "musicDescriptionShelfRenderer").first().map(|d| text(&d["description"])).filter(|t| !t.is_empty()))
    }

    async fn post(&self, endpoint: &str, mut body: Value) -> Result<Value, String> {
        body["context"] = json!({
            "client": { "clientName": "WEB_REMIX", "clientVersion": client_version(), "hl": "en" },
            "user": {}
        });
        let response = self
            .http
            .post(format!("{API}{endpoint}?prettyPrint=false"))
            .header("cookie", self.jar.header())
            .header("authorization", self.authorization())
            .header("x-goog-authuser", "0")
            .header("x-origin", ORIGIN)
            .header("origin", ORIGIN)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Couldn't reach YouTube Music ({e})"))?;
        let status = response.status();
        if status.as_u16() == 401 || status.as_u16() == 403 {
            return Err("YouTube Music signed you out. Connect again to refresh your login.".into());
        }
        if !status.is_success() {
            return Err(format!("YouTube Music answered with an error ({status})"));
        }
        response.json().await.map_err(|e| format!("YouTube Music sent something unexpected ({e})"))
    }

    /// Follows "load more" pages, both the older ctoken style and the 2025 continuation command.
    async fn browse_all(&self, browse_id: &str, max_pages: usize) -> Result<Vec<Value>, String> {
        let mut pages = vec![self.browse(json!({ "browseId": browse_id })).await?];
        while pages.len() < max_pages {
            let Some(token) = continuation(pages.last().unwrap()) else { break };
            let page = self.browse(json!({ "continuation": token })).await?;
            pages.push(page);
        }
        Ok(pages)
    }

    /// Library playlists and the home page shelves.
    pub async fn overview(&self) -> Result<Overview, String> {
        let library = self.browse_all("FEmusic_liked_playlists", 6).await?;
        if !signed_in(&library[0]) {
            return Err("YouTube Music doesn't see you as signed in. Connect again to refresh your login.".into());
        }
        let mut playlists = Vec::new();
        for page in &library {
            for card in find_all(page, "musicTwoRowItemRenderer").into_iter().filter_map(parse_card) {
                if !playlists.iter().any(|c: &Card| c.browse_id == card.browse_id) {
                    playlists.push(card);
                }
            }
        }
        let home_pages = self.browse_all("FEmusic_home", 4).await.unwrap_or_default();
        let mut home = Vec::new();
        for page in &home_pages {
            for shelf in find_all(page, "musicCarouselShelfRenderer") {
                let title = text(&shelf["header"]["musicCarouselShelfBasicHeaderRenderer"]["title"]);
                let mut items = Vec::new();
                for content in shelf["contents"].as_array().into_iter().flatten() {
                    if let Some(c) = content.get("musicTwoRowItemRenderer") {
                        if let Some(card) = parse_card(c) {
                            items.push(Item::Card(card));
                        } else if let Some(song) = parse_song_card(c) {
                            items.push(Item::Song(song));
                        }
                    } else if let Some(r) = content.get("musicResponsiveListItemRenderer") {
                        items.extend(parse_song(r).map(Item::Song));
                    }
                }
                if !title.is_empty() && !items.is_empty() {
                    home.push(Shelf { title, items });
                }
            }
        }
        Ok(Overview { playlists, home })
    }

    /// Searches all of YouTube Music: a songs-only search for the song list, and the general one
    /// for albums and playlists.
    pub async fn search(&self, query: &str) -> Result<Results, String> {
        // The "Songs" filter chip, as the website sends it.
        const SONGS: &str = "EgWKAQIIAWoMEA4QChADEAQQCRAF";
        let (songs, all) = tokio::join!(
            self.post("search", json!({ "query": query, "params": SONGS })),
            self.post("search", json!({ "query": query })),
        );
        let mut results = Results::default();
        for r in find_all(&songs?, "musicResponsiveListItemRenderer").into_iter().filter_map(parse_song) {
            if !results.songs.iter().any(|s| s.video_id == r.video_id) {
                results.songs.push(r);
            }
        }
        if let Ok(all) = all {
            for r in find_all(&all, "musicResponsiveListItemRenderer") {
                if let Some(card) = parse_card_row(r)
                    && !results.collections.iter().any(|c| c.browse_id == card.browse_id)
                {
                    results.collections.push(card);
                }
            }
        }
        Ok(results)
    }

    /// Songs in a playlist, mix or album.
    pub async fn songs(&self, browse_id: &str) -> Result<Vec<Song>, String> {
        let pages = self.browse_all(browse_id, 40).await?;
        let mut songs: Vec<Song> = Vec::new();
        for page in &pages {
            for song in find_all(page, "musicResponsiveListItemRenderer").into_iter().filter_map(parse_song) {
                if !songs.iter().any(|s| s.video_id == song.video_id) {
                    songs.push(song);
                }
            }
        }
        // Album pages leave artist and art off each row; take them from the header.
        if let Some(header) = find_all(&pages[0], "musicResponsiveHeaderRenderer").first() {
            let artist = text(&header["straplineTextOne"]);
            let thumb = best_thumbnail(&header["thumbnail"]);
            for s in &mut songs {
                if s.artist.is_empty() {
                    s.artist = artist.clone();
                }
                if s.thumbnail.is_none() {
                    s.thumbnail = thumb.clone();
                }
            }
        }
        Ok(songs)
    }
}

/// e.g. "1.20261118.01.00", the same shape the website sends.
fn client_version() -> String {
    let days = (SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) / 86_400) as i64;
    // Civil-from-days (Howard Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("1.{y:04}{m:02}{d:02}.01.00")
}

fn signed_in(page: &Value) -> bool {
    // Signed-out responses carry a sign-in prompt instead of the library grid.
    !find_all(page, "musicTwoRowItemRenderer").is_empty() || find_all(page, "gridRenderer").len() == 1
}

fn find_all<'a>(v: &'a Value, key: &str) -> Vec<&'a Value> {
    let mut out = Vec::new();
    fn walk<'a>(v: &'a Value, key: &str, out: &mut Vec<&'a Value>) {
        match v {
            Value::Object(map) => {
                for (k, child) in map {
                    if k == key {
                        out.push(child);
                    } else if k != "menu" && k != "frameworkUpdates" {
                        walk(child, key, out);
                    }
                }
            }
            Value::Array(items) => items.iter().for_each(|c| walk(c, key, out)),
            _ => {}
        }
    }
    walk(v, key, &mut out);
    out
}

fn continuation(page: &Value) -> Option<String> {
    if let Some(c) = find_all(page, "nextContinuationData").first() {
        return c["continuation"].as_str().map(str::to_string);
    }
    find_all(page, "continuationCommand").iter().find_map(|c| c["token"].as_str().map(str::to_string))
}

fn text(v: &Value) -> String {
    v["runs"].as_array().map(|runs| runs.iter().filter_map(|r| r["text"].as_str()).collect::<String>()).unwrap_or_default()
}

/// Largest thumbnail, upsized for Google-hosted square art.
fn best_thumbnail(v: &Value) -> Option<String> {
    let thumbs = find_all(v, "thumbnails");
    let url = thumbs.first()?.as_array()?.last()?["url"].as_str()?.to_string();
    Some(match url.split_once('=') {
        Some((base, _)) if url.contains("googleusercontent.com") || url.contains("ggpht.com") => format!("{base}=w544-h544-l90-rj"),
        // Video thumbnails: hqdefault has black bars baked in; hq720 is the clean 16:9 frame.
        _ => match video_thumb_id(&url) {
            Some(id) => format!("https://i.ytimg.com/vi/{id}/hq720.jpg"),
            None => url,
        },
    })
}

/// "https://i.ytimg.com/vi/<id>/hqdefault.jpg?…" → "<id>"
pub fn video_thumb_id(url: &str) -> Option<&str> {
    let rest = url.split("i.ytimg.com/vi/").nth(1)?;
    rest.split('/').next().filter(|id| !id.is_empty())
}

/// Bigger versions of a thumbnail to try for the Now playing cover, best first.
pub fn hires_candidates(url: &str) -> Vec<String> {
    if let Some(id) = video_thumb_id(url) {
        return ["maxresdefault", "hq720", "sddefault"].iter().map(|v| format!("https://i.ytimg.com/vi/{id}/{v}.jpg")).collect();
    }
    match url.split_once('=') {
        Some((base, _)) if url.contains("googleusercontent.com") || url.contains("ggpht.com") => vec![format!("{base}=w1200-h1200-l90-rj")],
        _ => Vec::new(),
    }
}

fn parse_card(r: &Value) -> Option<Card> {
    let nav = &r["navigationEndpoint"]["browseEndpoint"];
    let browse_id = nav["browseId"].as_str()?.to_string();
    let page_type = nav["browseEndpointContextSupportedConfigs"]["browseEndpointContextMusicConfig"]["pageType"].as_str().unwrap_or("");
    let kind = match page_type {
        "MUSIC_PAGE_TYPE_ALBUM" => CardKind::Album,
        "MUSIC_PAGE_TYPE_PLAYLIST" if browse_id.starts_with("VLRD") => CardKind::Mix,
        "MUSIC_PAGE_TYPE_PLAYLIST" => CardKind::Playlist,
        _ => return None, // artists, podcasts, etc.
    };
    Some(Card {
        browse_id,
        title: text(&r["title"]),
        subtitle: text(&r["subtitle"]),
        thumbnail: best_thumbnail(&r["thumbnailRenderer"]),
        kind,
    })
}

/// Album and playlist rows in search results.
fn parse_card_row(r: &Value) -> Option<Card> {
    let nav = &r["navigationEndpoint"]["browseEndpoint"];
    let browse_id = nav["browseId"].as_str()?.to_string();
    let page_type = nav["browseEndpointContextSupportedConfigs"]["browseEndpointContextMusicConfig"]["pageType"].as_str().unwrap_or("");
    let kind = match page_type {
        "MUSIC_PAGE_TYPE_ALBUM" => CardKind::Album,
        "MUSIC_PAGE_TYPE_PLAYLIST" if browse_id.starts_with("VLRD") => CardKind::Mix,
        "MUSIC_PAGE_TYPE_PLAYLIST" => CardKind::Playlist,
        _ => return None,
    };
    let title = text(column(r, 0));
    // "Album • Daft Punk • 2013": the kind is shown by the tile already.
    let subtitle = text(column(r, 1));
    let subtitle = subtitle.split_once(" • ").filter(|(k, _)| matches!(*k, "Album" | "Playlist" | "Single" | "EP")).map_or(subtitle.clone(), |(_, rest)| rest.to_string());
    (!title.is_empty()).then(|| Card { browse_id, title, subtitle, thumbnail: best_thumbnail(&r["thumbnail"]), kind })
}

/// Song tiles on the home page ("Listen again" etc.).
fn parse_song_card(r: &Value) -> Option<Song> {
    let video_id = r["navigationEndpoint"]["watchEndpoint"]["videoId"].as_str()?.to_string();
    let subtitle = text(&r["subtitle"]);
    let artist = subtitle.split(" • ").find(|p| !matches!(*p, "Song" | "Video")).unwrap_or("").to_string();
    Some(Song {
        video_id,
        title: text(&r["title"]),
        artist,
        album: String::new(),
        duration: Duration::ZERO,
        thumbnail: best_thumbnail(&r["thumbnailRenderer"]),
    })
}

fn column(r: &Value, i: usize) -> &Value {
    &r["flexColumns"][i]["musicResponsiveListItemFlexColumnRenderer"]["text"]
}

fn parse_song(r: &Value) -> Option<Song> {
    let video_id = r["playlistItemData"]["videoId"]
        .as_str()
        .or_else(|| column(r, 0)["runs"][0]["navigationEndpoint"]["watchEndpoint"]["videoId"].as_str())
        .or_else(|| {
            r["overlay"]["musicItemThumbnailOverlayRenderer"]["content"]["musicPlayButtonRenderer"]["playNavigationEndpoint"]["watchEndpoint"]
                ["videoId"]
                .as_str()
        })?
        .to_string();
    let title = text(column(r, 0));
    if title.is_empty() {
        return None;
    }
    // Column 1 is "Artist" or "Song • Artist • Album • 3:21" depending on the page.
    let second = text(column(r, 1));
    let mut parts = second.split(" • ").filter(|p| !matches!(*p, "Song" | "Video" | "Episode"));
    let artist = parts.next().unwrap_or("").to_string();
    let album = {
        let third = text(column(r, 2));
        // Search results put the play count there instead.
        let counts = third.ends_with(" plays") || third.ends_with(" views");
        if third.is_empty() || counts { parts.next().unwrap_or("").to_string() } else { third }
    };
    let duration = r["fixedColumns"][0]["musicResponsiveListItemFixedColumnRenderer"]["text"]["runs"][0]["text"]
        .as_str()
        .or_else(|| second.split(" • ").last())
        .and_then(parse_duration)
        .unwrap_or_default();
    Some(Song { video_id, title, artist, album: if parse_duration(&album).is_some() { String::new() } else { album }, duration, thumbnail: best_thumbnail(&r["thumbnail"]) })
}

fn parse_duration(s: &str) -> Option<Duration> {
    let mut secs = 0u64;
    for part in s.trim().split(':') {
        secs = secs * 60 + part.parse::<u64>().ok()?;
    }
    Some(Duration::from_secs(secs))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations() {
        assert_eq!(parse_duration("3:21"), Some(Duration::from_secs(201)));
        assert_eq!(parse_duration("1:02:03"), Some(Duration::from_secs(3723)));
        assert_eq!(parse_duration("Song"), None);
    }

    #[test]
    fn version_shape() {
        let v = client_version();
        assert!(v.starts_with("1.20") && v.ends_with(".01.00") && v.len() == 16, "{v}");
    }

    #[test]
    fn parses_made_up_playlist_row() {
        let row = json!({
            "flexColumns": [
                {"musicResponsiveListItemFlexColumnRenderer": {"text": {"runs": [{"text": "Harbour Lights",
                    "navigationEndpoint": {"watchEndpoint": {"videoId": "abc123"}}}]}}},
                {"musicResponsiveListItemFlexColumnRenderer": {"text": {"runs": [{"text": "Maren Holt"}]}}},
                {"musicResponsiveListItemFlexColumnRenderer": {"text": {"runs": [{"text": "Low Tide Choir"}]}}}
            ],
            "fixedColumns": [{"musicResponsiveListItemFixedColumnRenderer": {"text": {"runs": [{"text": "3:42"}]}}}],
            "thumbnail": {"musicThumbnailRenderer": {"thumbnail": {"thumbnails": [
                {"url": "https://lh3.googleusercontent.com/x=w60-h60", "width": 60, "height": 60}]}}}
        });
        let s = parse_song(&row).unwrap();
        assert_eq!((s.video_id.as_str(), s.title.as_str(), s.artist.as_str(), s.album.as_str()), ("abc123", "Harbour Lights", "Maren Holt", "Low Tide Choir"));
        assert_eq!(s.duration, Duration::from_secs(222));
        assert_eq!(s.thumbnail.as_deref(), Some("https://lh3.googleusercontent.com/x=w544-h544-l90-rj"));
    }
}
