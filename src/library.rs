//! The library: songs from local files and YouTube Music, grouped into collections.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use iced::Color;
use iced::widget::image;
use lofty::prelude::*;

use crate::theme;

const AUDIO_EXTS: [&str; 9] = ["mp3", "flac", "ogg", "oga", "opus", "m4a", "mp4", "wav", "aac"];
const COVER_NAMES: [&str; 6] = ["cover.jpg", "cover.png", "folder.jpg", "folder.png", "front.jpg", "front.png"];

/// Where a song's audio comes from.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Origin {
    File(PathBuf),
    YouTube(String),
}

/// Artwork plus the color sampled from it. Generated art is drawn from `seed` when there's no image.
#[derive(Debug, Clone)]
pub struct Cover {
    pub image: Option<image::Handle>,
    pub color: Color,
    pub seed: u64,
    /// Remote artwork still to fetch (YouTube thumbnails).
    pub url: Option<String>,
    /// Where remote artwork came from; kept after loading so a sharper copy can be fetched.
    pub remote: Option<String>,
    /// Full-resolution copy for the Now playing cover.
    pub hires: Option<image::Handle>,
}

impl Cover {
    pub fn generated(seed: u64) -> Self {
        Self {
            image: None,
            color: theme::ALBUM_COLORS[(seed % theme::ALBUM_COLORS.len() as u64) as usize],
            seed,
            url: None,
            remote: None,
            hires: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Track {
    pub origin: Origin,
    pub title: String,
    pub artist: String,
    pub album_title: String,
    pub number: Option<u32>,
    pub disc: Option<u32>,
    pub duration: Duration,
    pub cover: Cover,
}

impl Track {
    pub fn path(&self) -> Option<&Path> {
        match &self.origin {
            Origin::File(p) => Some(p),
            Origin::YouTube(_) => None,
        }
    }

    /// Stable key for likes. Files use their path (compatible with older liked.txt files).
    pub fn key(&self) -> String {
        match &self.origin {
            Origin::File(p) => p.to_string_lossy().to_string(),
            Origin::YouTube(id) => format!("yt:{id}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Album,
    Playlist,
    Mix,
    /// YouTube Music's Liked Music playlist.
    Liked,
}

/// Anything with a list of songs: a local album, a YouTube playlist, mix or album.
#[derive(Debug, Clone)]
pub struct Collection {
    pub title: String,
    /// Artist for albums, owner or description for playlists.
    pub subtitle: String,
    pub kind: Kind,
    pub year: Option<u16>,
    pub tracks: Vec<usize>,
    pub cover: Cover,
    /// YouTube browse id (VL…, MPRE…); None for local albums.
    pub browse_id: Option<String>,
    /// Songs have been fetched (always true for local albums).
    pub loaded: bool,
    /// Shown on the shelf: local albums and your YouTube library. Home-page tiles aren't.
    pub saved: bool,
}

impl Collection {
    pub fn is_youtube(&self) -> bool {
        self.browse_id.is_some()
    }
}

/// A row on YouTube Music's home page.
#[derive(Debug, Clone)]
pub struct Shelf {
    pub title: String,
    pub items: Vec<ShelfItem>,
}

#[derive(Debug, Clone, Copy)]
pub enum ShelfItem {
    Collection(usize),
    Song(usize),
}

#[derive(Debug, Default, Clone)]
pub struct Library {
    pub root: PathBuf,
    pub tracks: Vec<Track>,
    pub collections: Vec<Collection>,
    pub shelves: Vec<Shelf>,
    by_video: HashMap<String, usize>,
    by_browse: HashMap<String, usize>,
}

impl Library {
    pub fn duration(&self, c: usize) -> Duration {
        self.collections[c].tracks.iter().map(|&t| self.tracks[t].duration).sum()
    }

    pub fn local_albums(&self) -> impl Iterator<Item = (usize, &Collection)> {
        self.collections.iter().enumerate().filter(|(_, c)| !c.is_youtube())
    }

    pub fn youtube_collections(&self) -> impl Iterator<Item = (usize, &Collection)> {
        self.collections.iter().enumerate().filter(|(_, c)| c.is_youtube())
    }

    pub fn has_local(&self) -> bool {
        self.collections.iter().any(|c| !c.is_youtube())
    }

    /// Adds a YouTube song, or returns the existing one with the same video id.
    /// Tracks are only ever appended, so indices held by the queue stay valid.
    pub fn add_video(&mut self, track: Track) -> usize {
        let Origin::YouTube(id) = &track.origin else { unreachable!("add_video takes YouTube tracks") };
        if let Some(&i) = self.by_video.get(id) {
            let existing = &mut self.tracks[i];
            if existing.duration.is_zero() {
                existing.duration = track.duration;
            }
            if existing.album_title.is_empty() {
                existing.album_title = track.album_title;
            }
            return i;
        }
        self.by_video.insert(id.clone(), self.tracks.len());
        self.tracks.push(track);
        self.tracks.len() - 1
    }

    /// Finds or creates the collection for a YouTube browse id.
    pub fn add_collection(&mut self, c: Collection) -> usize {
        let id = c.browse_id.clone().expect("YouTube collections have a browse id");
        if let Some(&i) = self.by_browse.get(&id) {
            let existing = &mut self.collections[i];
            if existing.subtitle.is_empty() {
                existing.subtitle = c.subtitle;
            }
            existing.saved |= c.saved;
            return i;
        }
        self.by_browse.insert(id, self.collections.len());
        self.collections.push(c);
        self.collections.len() - 1
    }

    pub fn find_browse(&self, id: &str) -> Option<usize> {
        self.by_browse.get(id).copied()
    }

    /// Applies a downloaded thumbnail everywhere it's used.
    pub fn set_art(&mut self, url: &str, image: image::Handle, color: Color) {
        let apply = |cover: &mut Cover| {
            if cover.url.as_deref() == Some(url) {
                cover.image = Some(image.clone());
                cover.color = color;
                cover.url = None;
            }
        };
        self.tracks.iter_mut().for_each(|t| apply(&mut t.cover));
        self.collections.iter_mut().for_each(|c| apply(&mut c.cover));
    }

    pub fn set_hires(&mut self, url: &str, image: image::Handle) {
        for cover in self.tracks.iter_mut().map(|t| &mut t.cover).chain(self.collections.iter_mut().map(|c| &mut c.cover)) {
            if cover.remote.as_deref() == Some(url) {
                cover.hires = Some(image.clone());
            }
        }
    }

    /// Thumbnails that still need downloading, most important (collections) first.
    pub fn pending_art(&self) -> Vec<String> {
        let mut urls: Vec<String> = Vec::new();
        for url in self.collections.iter().map(|c| &c.cover).chain(self.tracks.iter().map(|t| &t.cover)).filter_map(|c| c.url.clone()) {
            if !urls.contains(&url) {
                urls.push(url);
            }
        }
        urls
    }
}

pub fn default_root() -> PathBuf {
    dirs::audio_dir()
        .or_else(|| dirs::home_dir().map(|h| h.join("Music")))
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn scan(root: PathBuf) -> Library {
    struct Raw {
        path: PathBuf,
        title: String,
        artist: String,
        album_artist: String,
        album: String,
        number: Option<u32>,
        disc: Option<u32>,
        year: Option<u16>,
        duration: Duration,
        art: Option<Vec<u8>>,
    }

    let mut raws = Vec::new();
    for entry in walkdir::WalkDir::new(&root).follow_links(true).into_iter().flatten() {
        let path = entry.path();
        let is_audio = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| AUDIO_EXTS.contains(&e.to_ascii_lowercase().as_str()));
        if !entry.file_type().is_file() || !is_audio {
            continue;
        }
        let Ok(file) = lofty::read_from_path(path) else { continue };
        let tag = file.primary_tag().or_else(|| file.first_tag());
        let get = |f: fn(&lofty::tag::Tag) -> Option<std::borrow::Cow<'_, str>>| {
            tag.and_then(|t| f(t).map(|s| s.trim().to_string())).filter(|s| !s.is_empty())
        };
        let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let parent = |n: usize| {
            path.ancestors()
                .nth(n)
                .and_then(|p| p.file_name())
                .map(|s| s.to_string_lossy().to_string())
        };
        let artist = get(|t| t.artist()).or_else(|| parent(2)).unwrap_or_else(|| "Unknown artist".into());
        let album_artist = tag
            .and_then(|t| t.get_string(ItemKey::AlbumArtist).map(str::to_string))
            .unwrap_or_else(|| artist.clone());
        raws.push(Raw {
            title: get(|t| t.title()).unwrap_or(stem),
            album: get(|t| t.album()).or_else(|| parent(1)).unwrap_or_else(|| "Unknown album".into()),
            artist,
            album_artist,
            number: tag.and_then(|t| t.track()),
            disc: tag.and_then(|t| t.disk()),
            year: tag.and_then(|t| t.date()).map(|d| d.year),
            duration: file.properties().duration(),
            art: tag.and_then(|t| t.pictures().first().map(|p| p.data().to_vec())),
            path: path.to_path_buf(),
        });
    }

    let mut albums: Vec<Collection> = Vec::new();
    let mut index: HashMap<(String, String), usize> = HashMap::new();
    let mut art: Vec<Option<Vec<u8>>> = Vec::new();
    let mut tracks: Vec<Track> = Vec::with_capacity(raws.len());
    let mut track_album: Vec<usize> = Vec::with_capacity(raws.len());

    for raw in raws {
        let key = (raw.album_artist.to_lowercase(), raw.album.to_lowercase());
        let album = *index.entry(key).or_insert_with(|| {
            albums.push(Collection {
                title: raw.album.clone(),
                subtitle: raw.album_artist.clone(),
                kind: Kind::Album,
                year: raw.year,
                tracks: Vec::new(),
                cover: Cover::generated(hash(&raw.album) ^ hash(&raw.album_artist)),
                browse_id: None,
                loaded: true,
                saved: true,
            });
            art.push(None);
            albums.len() - 1
        });
        if art[album].is_none() {
            art[album] = raw.art.or_else(|| folder_art(&raw.path));
        }
        albums[album].year = albums[album].year.or(raw.year);
        albums[album].tracks.push(tracks.len());
        track_album.push(album);
        tracks.push(Track {
            origin: Origin::File(raw.path),
            title: raw.title,
            artist: raw.artist,
            album_title: raw.album,
            number: raw.number,
            disc: raw.disc,
            duration: raw.duration,
            cover: Cover::generated(0),
        });
    }

    for (album, bytes) in albums.iter_mut().zip(art) {
        if let Some(color) = bytes.as_deref().and_then(sample_color) {
            album.cover.color = color;
        }
        album.cover.image = bytes.map(image::Handle::from_bytes);
        album.tracks.sort_by_key(|&t| {
            let t = &tracks[t];
            (t.disc.unwrap_or(1), t.number.unwrap_or(u32::MAX), t.key())
        });
    }
    // Local songs share their album's cover.
    for (t, &a) in tracks.iter_mut().zip(&track_album) {
        t.cover = albums[a].cover.clone();
    }

    albums.sort_by_key(|a| (a.subtitle.to_lowercase(), a.year, a.title.to_lowercase()));

    Library { root, tracks, collections: albums, ..Default::default() }
}

fn folder_art(track: &Path) -> Option<Vec<u8>> {
    let dir = track.parent()?;
    COVER_NAMES.iter().find_map(|n| std::fs::read(dir.join(n)).ok())
}

pub fn hash(s: &str) -> u64 {
    s.bytes().fold(0xcbf29ce484222325, |h, b| (h ^ b as u64).wrapping_mul(0x100000001b3))
}

/// The cover's dominant color, pulled into a range that reads well on ink.
pub fn sample_color(bytes: &[u8]) -> Option<Color> {
    let img = ::image::load_from_memory(bytes).ok()?.thumbnail(24, 24).to_rgb8();
    let (mut r, mut g, mut b, mut w) = (0.0, 0.0, 0.0, 0.0);
    for p in img.pixels() {
        let [pr, pg, pb] = p.0.map(|c| c as f32 / 255.0);
        let max = pr.max(pg).max(pb);
        let min = pr.min(pg).min(pb);
        let weight = 0.05 + (max - min); // favour saturated pixels
        r += pr * weight;
        g += pg * weight;
        b += pb * weight;
        w += weight;
    }
    let (h, s, l) = to_hsl(r / w, g / w, b / w);
    let (r, g, b) = from_hsl(h, s.clamp(0.3, 0.6), l.clamp(0.38, 0.55));
    Some(Color::from_rgb(r, g, b))
}

fn to_hsl(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    if max == min {
        return (0.0, 0.0, l);
    }
    let d = max - min;
    let s = if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) };
    let h = if max == r {
        (g - b) / d + if g < b { 6.0 } else { 0.0 }
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    (h / 6.0, s, l)
}

fn from_hsl(h: f32, s: f32, l: f32) -> (f32, f32, f32) {
    if s == 0.0 {
        return (l, l, l);
    }
    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;
    let f = |mut t: f32| {
        t = t.rem_euclid(1.0);
        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    };
    (f(h + 1.0 / 3.0), f(h), f(h - 1.0 / 3.0))
}

pub fn format_time(d: Duration) -> String {
    let s = d.as_secs();
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

pub fn format_minutes(d: Duration) -> String {
    let m = (d.as_secs() + 30) / 60;
    if m >= 60 { format!("{} h {} min", m / 60, m % 60) } else { format!("{m} min") }
}
