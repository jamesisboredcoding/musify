mod anim;
mod audio;
mod canvas;
mod discord;
mod icons;
mod library;
mod lyrics;
mod theme;
mod update;
mod view;
mod youtube;

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use iced::window::{self, Direction};
use iced::{Color, Size, Subscription, Task, keyboard};

use anim::Tween;
use audio::Audio;
use library::{Collection, Cover, Kind, Library, Origin, ShelfItem, Track};
use lyrics::Lyrics;

pub fn main() -> iced::Result {
    #[cfg(debug_assertions)]
    if std::env::args().nth(1).as_deref() == Some("--yt-check") {
        yt_check();
        return Ok(());
    }
    #[cfg(debug_assertions)]
    if std::env::args().nth(1).as_deref() == Some("--yt-search") {
        yt_search(&std::env::args().skip(2).collect::<Vec<_>>().join(" "));
        return Ok(());
    }
    let mut app = iced::application(Musify::boot, Musify::update, Musify::view)
        .title("Musify")
        .theme(|_: &Musify| theme::theme())
        .style(|_, _| iced::theme::Style { background_color: Color::TRANSPARENT, text_color: theme::PAPER })
        .subscription(Musify::subscription)
        .default_font(theme::BODY)
        .antialiasing(true)
        .window(window::Settings {
            size: initial_size(),
            min_size: Some(Size::new(560.0, 640.0)),
            decorations: false,
            transparent: true,
            // Wayland desktops take the icon from musify.desktop (scripts/install-desktop.sh); X11 uses this.
            icon: window::icon::from_file_data(include_bytes!("../assets/icons/musify-256.png"), None).ok(),
            platform_specific: window::settings::PlatformSpecific {
                application_id: "musify".into(),
                ..Default::default()
            },
            ..Default::default()
        });
    for font in theme::FONT_FILES {
        app = app.font(font);
    }
    app.run()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Home,
    Songs,
    Liked,
    Collection(usize),
}

/// Where a list of songs came from, so rows can start playback without cloning lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Collection(usize),
    Songs,
    Liked,
    Search,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Repeat {
    Off,
    All,
    One,
}

#[derive(Debug, Clone)]
pub enum Message {
    WindowId(Option<window::Id>),
    Resized(Size),
    Drag,
    DragResize(Direction),
    Minimize,
    ToggleMaximize,
    /// Double-clicking the top bar or F11: true fullscreen, over the panels.
    ToggleFullscreen,
    /// Show what's playing on your Discord profile.
    ToggleDiscord,
    UpdateFound(Option<update::Release>),
    UpdateInstall,
    UpdateDone(Result<PathBuf, String>),
    /// Start the new version and quit this one.
    UpdateRestart,
    OpenUrl(String),
    UpdateYtDlp,
    YtDlpUpdated(Result<(), String>),
    Maximized(bool),
    Fullscreen(bool),
    Close,
    Loaded(Arc<Library>),
    Open(Page),
    Back,
    Forward,
    Search(String),
    PlayFrom(Source, usize),
    PlayShuffled(Source),
    /// A single song from a home shelf.
    PlaySong(usize),
    TogglePlay,
    Next,
    Prev,
    Ribbon(usize, f32),
    ToggleShuffle,
    CycleRepeat,
    Volume(f32),
    ToggleMute,
    ToggleLike(usize),
    ToggleNowPlaying,
    Frame(Instant),
    Space,
    Escape,
    YtConnect,
    YtDisconnect,
    YtImported(Result<youtube::Jar, String>),
    YtSignInClosed,
    YtOverview(Result<youtube::Overview, String>),
    YtSongs(String, Result<Vec<youtube::Song>, String>),
    Art(String, Option<(Vec<u8>, Color)>),
    Downloaded(String, Result<PathBuf, String>),
    /// A like/unlike finished on YouTube: track, new state, result.
    Rated(usize, bool, Result<(), String>),
    /// Lyrics lookup finished for a track.
    LyricsFound(usize, Option<Lyrics>),
    /// Full-resolution cover for the song now playing.
    HiRes(String, Option<Vec<u8>>),
    /// The search box settled on these words; ask YouTube Music.
    SearchRun(String),
    SearchDone(String, Result<youtube::Results, String>),
}

/// YouTube Music's answer to the current search, as library indices.
#[derive(Debug, Clone, Default)]
pub struct Found {
    pub query: String,
    pub songs: Vec<usize>,
    pub collections: Vec<usize>,
}

/// A newer Musify on GitHub, and how far installing it got.
#[derive(Debug, Clone)]
pub enum Updating {
    Idle,
    Available(update::Release),
    Installing(update::Release),
    /// Installed; restarting runs this program.
    Ready(PathBuf),
    Failed(update::Release, String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum YtStatus {
    Off,
    /// Looking for a Firefox login.
    Connecting,
    /// Waiting for the user to sign in in the Firefox window Musify opened.
    SigningIn,
    /// Signed in; fetching the library.
    Loading,
    Ready,
    Failed(String),
}

pub struct Musify {
    pub window: Option<window::Id>,
    pub size: Size,
    pub maximized: bool,
    pub fullscreen: bool,
    pub install: update::Install,
    pub updating: Updating,
    /// The last download failed because yt-dlp is too old; the player offers to update it.
    pub ytdlp_stale: bool,
    pub ytdlp_updating: bool,
    /// Running while the Discord status is on.
    pub discord: Option<discord::Discord>,
    discord_sent: Option<discord::Presence>,
    pub root: PathBuf,
    pub library: Option<Arc<Library>>,
    pub page: Page,
    /// History of (page, search words): a search counts as a place you can go back to.
    back: Vec<(Page, String)>,
    forward: Vec<(Page, String)>,
    pub now_playing: bool,
    pub query: String,
    /// Track indices in play order.
    pub queue: Vec<usize>,
    original: Vec<usize>,
    pub cursor: Option<usize>,
    pub playing: bool,
    pub position: Duration,
    pub shuffle: bool,
    pub repeat: Repeat,
    pub volume: f32,
    pub muted: bool,
    pub liked: HashSet<String>,
    pub lyrics: Option<Lyrics>,
    pub notice: Option<String>,
    pub audio: Audio,
    pub tick: u64,
    rng: u64,
    // Motion
    pub now: Instant,
    pub booted: Instant,
    pub splash_exit: Option<Tween>,
    pub page_tween: Tween,
    pub tint_from: Color,
    pub tint_to: Color,
    pub tint_tween: Tween,
    pub np_tween: Tween,
    pub ribbon_leave: Option<(usize, Tween)>,
    pub lyric_from: Option<usize>,
    pub lyric_line: Option<usize>,
    pub lyric_tween: Tween,
    pub ribbon_reveal: Tween,
    // YouTube Music
    pub yt: YtStatus,
    yt_client: Option<youtube::Client>,
    pub found: Found,
    /// A YouTube Music search is on its way.
    pub searching: bool,
    pub search_error: Option<String>,
    signin_tried: bool,
    fetching: HashSet<String>,
    art_loading: HashSet<String>,
    downloads: HashSet<String>,
    /// Track waiting for its audio to download before it can play.
    pub waiting: Option<usize>,
    /// Play this as soon as its songs arrive (pressed play on a playlist that wasn't loaded yet).
    pending_play: Option<(Source, usize, bool)>,
    pub queue_source: Option<Source>,
    /// Video ids in your YouTube Liked Music.
    yt_likes: HashSet<String>,
    /// Loudness of what's playing, smoothed, 0 to 1; and a short history for the trail.
    pub level: f32,
    pub levels: std::collections::VecDeque<f32>,
    last_level_push: Instant,
    /// For lyrics lookups (no YouTube cookies on it).
    http: reqwest::Client,
    pub lyrics_loading: bool,
}

impl Musify {
    fn boot() -> (Self, Task<Message>) {
        let root = std::env::args_os().nth(1).map(PathBuf::from).unwrap_or_else(library::default_root);
        let scan_root = root.clone();
        let (volume, muted) = load_volume();
        let mut audio = Audio::new();
        audio.set_volume(if muted { 0.0 } else { volume });
        let app = Self {
            window: None,
            size: initial_size(),
            maximized: false,
            fullscreen: false,
            install: update::install(),
            updating: Updating::Idle,
            ytdlp_stale: false,
            ytdlp_updating: false,
            discord: (load_discord() && discord::Discord::configured()).then(discord::Discord::start),
            discord_sent: None,
            root,
            library: None,
            page: Page::Home,
            back: Vec::new(),
            forward: Vec::new(),
            now_playing: false,
            query: String::new(),
            queue: Vec::new(),
            original: Vec::new(),
            cursor: None,
            playing: false,
            position: Duration::ZERO,
            shuffle: false,
            repeat: Repeat::Off,
            volume,
            muted,
            liked: load_liked(),
            lyrics: None,
            notice: None,
            audio,
            tick: 0,
            now: Instant::now(),
            booted: Instant::now(),
            splash_exit: None,
            page_tween: Tween::done(),
            tint_from: theme::rgb(0x3E8E8A),
            tint_to: theme::rgb(0x3E8E8A),
            tint_tween: Tween::done(),
            np_tween: Tween::done(),
            ribbon_leave: None,
            lyric_from: None,
            lyric_line: None,
            lyric_tween: Tween::done(),
            ribbon_reveal: Tween::done(),
            yt: YtStatus::Off,
            yt_client: None,
            found: Found::default(),
            searching: false,
            search_error: None,
            signin_tried: false,
            fetching: HashSet::new(),
            art_loading: HashSet::new(),
            downloads: HashSet::new(),
            waiting: None,
            pending_play: None,
            queue_source: None,
            yt_likes: HashSet::new(),
            level: 0.0,
            levels: std::collections::VecDeque::from(vec![0.0; LEVEL_HISTORY]),
            last_level_push: Instant::now(),
            http: reqwest::Client::builder()
                .user_agent(concat!("Musify/", env!("CARGO_PKG_VERSION"), " (Linux music player)"))
                .timeout(Duration::from_secs(15))
                .build()
                .unwrap_or_default(),
            lyrics_loading: false,
            rng: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(7) | 1,
        };
        let scan = Task::perform(
            async move { tokio::task::spawn_blocking(move || library::scan(scan_root)).await.unwrap_or_default() },
            |lib| Message::Loaded(Arc::new(lib)),
        );
        // Release builds look for a newer version on GitHub once per launch.
        let check = if cfg!(debug_assertions) && std::env::var_os("MUSIFY_UPDATE_CHECK").is_none() {
            Task::none()
        } else {
            Task::perform(update::check(app.http.clone(), app.install), Message::UpdateFound)
        };
        (app, Task::batch([window::oldest().map(Message::WindowId), scan, check]))
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        let task = self.handle(message);
        self.sync_discord();
        task
    }

    /// Sends Discord the current song when it, play/pause or the position (a seek) changed.
    fn sync_discord(&mut self) {
        let Some(discord) = &self.discord else { return };
        let now = self.discord_presence();
        let same = match (&now, &self.discord_sent) {
            (Some(a), Some(b)) => {
                let close = match (a.span, b.span) {
                    (Some((x, _)), Some((y, _))) => (x - y).abs() < 2000,
                    (None, None) => true,
                    _ => false,
                };
                close && discord::Presence { span: None, ..a.clone() } == discord::Presence { span: None, ..b.clone() }
            }
            (None, None) => true,
            _ => false,
        };
        if !same {
            discord.set(now.clone());
            self.discord_sent = now;
        }
    }

    fn discord_presence(&self) -> Option<discord::Presence> {
        let lib = self.library.as_ref()?;
        let t = self.current_track()?;
        let track = &lib.tracks[t];
        let playing = self.playing && self.waiting != Some(t);
        let span = playing.then(|| {
            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as i64;
            let start = now - self.position.as_millis() as i64;
            (start, start + track.duration.as_millis() as i64)
        });
        let link = match &track.origin {
            Origin::YouTube(id) => Some(format!("https://music.youtube.com/watch?v={id}")),
            Origin::File(_) => None,
        };
        Some(discord::Presence {
            title: track.title.clone(),
            artist: track.artist.clone(),
            album: track.album_title.clone(),
            cover: track.cover.remote.clone().or_else(|| track.cover.url.clone()).filter(|u| u.starts_with("https://")),
            link,
            span,
        })
    }

    fn handle(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::WindowId(id) => self.window = id,
            Message::Resized(size) => {
                self.size = size;
                // Keep in step with maximizes the compositor does itself (shortcuts, edge snapping).
                return self.with_window(|id| {
                    Task::batch([
                        window::is_maximized(id).map(Message::Maximized),
                        window::mode(id).map(|m| Message::Fullscreen(m == window::Mode::Fullscreen)),
                    ])
                });
            }
            Message::Maximized(m) => self.maximized = m,
            Message::Fullscreen(f) => self.fullscreen = f,
            // A drag in fullscreen would swallow the second click of a double-click.
            Message::Drag if self.fullscreen => {}
            Message::Drag => return self.with_window(window::drag),
            Message::DragResize(d) => return self.with_window(move |id| window::drag_resize(id, d)),
            Message::Minimize => return self.with_window(|id| window::minimize(id, true)),
            // The maximize button brings a fullscreen window back first.
            Message::ToggleMaximize if self.fullscreen => return self.update(Message::ToggleFullscreen),
            Message::ToggleMaximize => {
                self.maximized = !self.maximized;
                return self.with_window(window::toggle_maximize);
            }
            Message::ToggleFullscreen => {
                self.fullscreen = !self.fullscreen;
                let mode = if self.fullscreen { window::Mode::Fullscreen } else { window::Mode::Windowed };
                return self.with_window(move |id| window::set_mode(id, mode));
            }
            Message::ToggleDiscord => {
                if self.discord.take().is_none() {
                    self.discord = Some(discord::Discord::start());
                }
                self.discord_sent = None;
                save_discord(self.discord.is_some());
            }
            Message::UpdateFound(Some(release)) => self.updating = Updating::Available(release),
            Message::UpdateFound(None) => {}
            Message::UpdateInstall => {
                let release = match &self.updating {
                    Updating::Available(r) | Updating::Failed(r, _) => r.clone(),
                    _ => return Task::none(),
                };
                if release.asset.is_none() {
                    // Built from source (or no matching download): show the release page.
                    return self.update(Message::OpenUrl(release.page));
                }
                self.updating = Updating::Installing(release.clone());
                return Task::perform(update::apply(self.install, release), Message::UpdateDone);
            }
            Message::UpdateDone(result) => {
                let Updating::Installing(release) = &self.updating else { return Task::none() };
                self.updating = match result {
                    Ok(program) => Updating::Ready(program),
                    Err(e) => Updating::Failed(release.clone(), e),
                };
            }
            Message::UpdateRestart => {
                if let Updating::Ready(program) = &self.updating
                    && std::process::Command::new(program).spawn().is_ok()
                {
                    return iced::exit();
                }
            }
            Message::UpdateYtDlp => {
                self.ytdlp_updating = true;
                return Task::perform(youtube::stream::update_ytdlp(), Message::YtDlpUpdated);
            }
            Message::YtDlpUpdated(result) => {
                self.ytdlp_updating = false;
                match result {
                    Ok(()) => {
                        self.ytdlp_stale = false;
                        self.notice = None;
                        // Try the song that failed again, unless something else started meanwhile.
                        if !self.playing && self.position.is_zero() {
                            return self.start();
                        }
                    }
                    Err(e) => self.notice = Some(e),
                }
            }
            Message::OpenUrl(url) => {
                let _ = std::process::Command::new("xdg-open").arg(url).spawn();
            }
            Message::Close => return iced::exit(),
            Message::Loaded(lib) => {
                self.library = Some(lib);
                // A saved YouTube login reconnects quietly once local music is in.
                let yt = match youtube::Jar::load() {
                    Some(jar) => self.update(Message::YtImported(Ok(jar))),
                    None => Task::none(),
                };
                #[cfg(debug_assertions)]
                return Task::batch([yt, self.apply_preview()]);
                #[cfg(not(debug_assertions))]
                return yt;
            }
            Message::Open(page) => {
                let changed = page != self.page || !self.query.is_empty();
                self.close_now_playing();
                if changed {
                    self.back.push((self.page, std::mem::take(&mut self.query)));
                    self.forward.clear();
                    self.page = page;
                }
                if changed {
                    self.transition_page();
                }
                if let Page::Collection(c) = page {
                    return self.ensure_songs(c);
                }
            }
            Message::Back => {
                if let Some((p, q)) = self.back.pop() {
                    self.forward.push((self.page, std::mem::replace(&mut self.query, q)));
                    self.page = p;
                    self.close_now_playing();
                    self.transition_page();
                }
                return self.search_again();
            }
            Message::Forward => {
                if let Some((p, q)) = self.forward.pop() {
                    self.back.push((self.page, std::mem::replace(&mut self.query, q)));
                    self.page = p;
                    self.close_now_playing();
                    self.transition_page();
                }
                return self.search_again();
            }
            Message::Search(q) => {
                let toggled = q.trim().is_empty() != self.query.trim().is_empty();
                self.query = q;
                self.close_now_playing();
                if toggled {
                    self.transition_page();
                }
                let words = self.query.trim().to_string();
                self.search_error = None;
                if self.yt_client.is_some() && !words.is_empty() && words != self.found.query {
                    // Wait for typing to pause before asking YouTube.
                    self.searching = true;
                    return Task::perform(tokio::time::sleep(Duration::from_millis(350)), move |_| Message::SearchRun(words.clone()));
                }
                self.searching = false;
            }
            Message::PlayFrom(source, i) => {
                if let Some(t) = self.load_first(source, i, false) {
                    return t;
                }
                let list = self.source(source);
                if i < list.len() {
                    self.original = list.clone();
                    self.queue = list;
                    self.queue_source = Some(source);
                    self.cursor = Some(i);
                    if self.shuffle {
                        self.shuffle_upcoming();
                    }
                    return self.start();
                }
            }
            Message::PlayShuffled(source) => {
                if let Some(t) = self.load_first(source, 0, true) {
                    return t;
                }
                let list = self.source(source);
                if !list.is_empty() {
                    self.original = list.clone();
                    self.queue = list;
                    self.queue_source = Some(source);
                    let first = (self.random() % self.queue.len() as u64) as usize;
                    self.queue.swap(0, first);
                    self.cursor = Some(0);
                    self.shuffle = true;
                    self.shuffle_upcoming();
                    return self.start();
                }
            }
            Message::PlaySong(t) => {
                self.queue = vec![t];
                self.original = self.queue.clone();
                self.queue_source = None;
                self.cursor = Some(0);
                return self.start();
            }
            Message::TogglePlay => match (self.cursor, self.playing) {
                (None, _) => {
                    let source = match self.page {
                        Page::Collection(a) => Source::Collection(a),
                        Page::Liked => Source::Liked,
                        _ => Source::Songs,
                    };
                    return self.update(Message::PlayFrom(source, 0));
                }
                (Some(_), true) => {
                    self.audio.pause();
                    self.playing = false;
                }
                (Some(_), false) => {
                    if self.waiting.is_some() {
                        return Task::none();
                    }
                    if self.audio.finished() {
                        return self.start();
                    } else {
                        self.audio.play();
                        self.playing = true;
                    }
                }
            },
            Message::Next => return self.advance(false),
            Message::Prev => {
                if self.position > Duration::from_secs(3) || self.cursor == Some(0) {
                    self.audio.seek(Duration::ZERO);
                    self.position = Duration::ZERO;
                } else if let Some(c) = self.cursor {
                    self.cursor = Some(c - 1);
                    return self.start();
                }
            }
            Message::Ribbon(offset, fraction) => {
                if let Some(c) = self.cursor {
                    if offset == 0 {
                        let to = self.current_duration().mul_f32(fraction);
                        self.audio.seek(to);
                        self.position = to;
                    } else {
                        self.cursor = Some(c + offset);
                        return self.start();
                    }
                }
            }
            Message::ToggleShuffle => {
                self.shuffle = !self.shuffle;
                if self.shuffle {
                    self.shuffle_upcoming();
                } else if let Some(c) = self.cursor {
                    let current = self.queue[c];
                    self.queue = self.original.clone();
                    self.cursor = self.queue.iter().position(|&t| t == current);
                }
            }
            Message::CycleRepeat => {
                self.repeat = match self.repeat {
                    Repeat::Off => Repeat::All,
                    Repeat::All => Repeat::One,
                    Repeat::One => Repeat::Off,
                }
            }
            Message::Volume(v) => {
                self.volume = v;
                self.muted = false;
                self.audio.set_volume(v);
                save_volume(self.volume, self.muted);
            }
            Message::ToggleMute => {
                self.muted = !self.muted;
                self.audio.set_volume(if self.muted { 0.0 } else { self.volume });
                save_volume(self.volume, self.muted);
            }
            Message::ToggleLike(t) => {
                if let Some(Origin::YouTube(id)) = self.library.as_ref().map(|l| l.tracks[t].origin.clone()) {
                    let Some(client) = self.yt_client.clone() else {
                        self.notice = Some("Connect YouTube Music to like songs.".into());
                        return Task::none();
                    };
                    // Show it straight away; roll back if YouTube says no.
                    let like = !self.yt_likes.contains(&id);
                    self.set_yt_like(t, like);
                    return Task::perform(async move { client.rate(&id, like).await }, move |r| Message::Rated(t, like, r));
                }
                if let Some(lib) = &self.library {
                    let key = lib.tracks[t].key();
                    if !self.liked.remove(&key) {
                        self.liked.insert(key);
                    }
                    save_liked(&self.liked);
                }
            }
            Message::ToggleNowPlaying => {
                self.now_playing = !self.now_playing;
                self.np_tween = Tween::new(Instant::now(), 420);
            }
            Message::Frame(at) => return self.frame(at),
            Message::Space => return self.update(Message::TogglePlay),
            Message::Escape => {
                if self.now_playing {
                    self.close_now_playing();
                } else if self.fullscreen {
                    return self.update(Message::ToggleFullscreen);
                }
            }
            Message::YtConnect => {
                self.yt = YtStatus::Connecting;
                self.signin_tried = false;
                return import_login();
            }
            Message::YtDisconnect => {
                youtube::Jar::forget();
                self.yt = YtStatus::Off;
                self.yt_client = None;
            }
            Message::YtImported(Ok(jar)) => {
                let client = youtube::Client::new(jar);
                self.yt_client = Some(client.clone());
                self.yt = YtStatus::Loading;
                return Task::perform(async move { client.overview().await }, Message::YtOverview);
            }
            Message::YtImported(Err(e)) => {
                if self.signin_tried {
                    self.yt = YtStatus::Failed(format!("{e} Sign in to music.youtube.com in the window Musify opens, then close it."));
                } else {
                    // No Firefox login found: open a Firefox window to sign in, and look again when it closes.
                    self.signin_tried = true;
                    match youtube::cookies::open_signin() {
                        Ok(mut child) => {
                            self.yt = YtStatus::SigningIn;
                            return Task::perform(
                                async move { tokio::task::spawn_blocking(move || child.wait()).await.ok() },
                                |_| Message::YtSignInClosed,
                            );
                        }
                        Err(err) => self.yt = YtStatus::Failed(err),
                    }
                }
            }
            Message::YtSignInClosed => {
                self.yt = YtStatus::Connecting;
                return import_login();
            }
            Message::YtOverview(Err(e)) => self.yt = YtStatus::Failed(e),
            Message::YtOverview(Ok(overview)) => {
                self.yt = YtStatus::Ready;
                self.merge_overview(overview);
                let liked = self.library.as_ref().and_then(|l| l.find_browse("VLLM"));
                let songs = liked.map(|c| self.ensure_songs(c)).unwrap_or_else(Task::none);
                return Task::batch([songs, self.fetch_art()]);
            }
            Message::SearchRun(words) => {
                if words != self.query.trim() || words == self.found.query {
                    return Task::none();
                }
                let Some(client) = self.yt_client.clone() else { return Task::none() };
                return Task::perform(async move { (words.clone(), client.search(&words).await) }, |(w, r)| Message::SearchDone(w, r));
            }
            Message::SearchDone(words, result) => {
                if words != self.query.trim() {
                    return Task::none();
                }
                self.searching = false;
                match result {
                    Ok(results) => {
                        self.merge_search(words, results);
                        return self.fetch_art();
                    }
                    Err(e) => self.search_error = Some(e),
                }
            }
            Message::YtSongs(id, result) => {
                self.fetching.remove(&id);
                match result {
                    Ok(songs) => self.merge_songs(&id, songs),
                    Err(e) => self.notice = Some(e),
                }
                let mut tasks = vec![self.fetch_art()];
                if let Some((source, i, shuffled)) = self.pending_play.take()
                    && source_ready(self, source)
                {
                    tasks.push(self.update(if shuffled { Message::PlayShuffled(source) } else { Message::PlayFrom(source, i) }));
                }
                return Task::batch(tasks);
            }
            Message::Art(url, art) => {
                if let (Some((bytes, color)), Some(lib)) = (art, self.library.as_mut()) {
                    Arc::make_mut(lib).set_art(&url, iced::widget::image::Handle::from_bytes(bytes), color);
                    if matches!(self.page, Page::Collection(_)) {
                        self.tint_to = self.page_tint();
                    }
                }
            }
            Message::HiRes(url, bytes) => {
                if let (Some(bytes), Some(lib)) = (bytes, self.library.as_mut()) {
                    Arc::make_mut(lib).set_hires(&url, iced::widget::image::Handle::from_bytes(bytes));
                }
            }
            Message::LyricsFound(t, lyrics) => {
                if self.current_track() == Some(t) {
                    self.lyrics = lyrics;
                    self.lyrics_loading = false;
                    self.lyric_line = None;
                    self.lyric_from = None;
                }
            }
            Message::Rated(t, like, result) => {
                if let Err(e) = result {
                    self.set_yt_like(t, !like);
                    self.notice = Some(format!("Couldn't update the like on YouTube: {e}"));
                }
            }
            Message::Downloaded(id, result) => {
                self.downloads.remove(&id);
                let Some(t) = self.waiting else { return Task::none() };
                let Some(lib) = self.library.clone() else { return Task::none() };
                if self.current_track() != Some(t) || lib.tracks[t].origin != Origin::YouTube(id) {
                    return Task::none();
                }
                self.waiting = None;
                match result {
                    Ok(path) => {
                        self.play_file(&path, t);
                        return self.prefetch();
                    }
                    Err(e) => {
                        self.ytdlp_stale = e == youtube::stream::OUTDATED;
                        self.notice = Some(format!("Couldn't play {}: {e}", lib.tracks[t].title));
                    }
                }
            }
        }
        Task::none()
    }

    fn subscription(&self) -> Subscription<Message> {
        let mut subs = vec![
            window::resize_events().map(|(_, size)| Message::Resized(size)),
            keyboard::listen().filter_map(|event| match event {
                keyboard::Event::KeyPressed { key: keyboard::Key::Named(keyboard::key::Named::Space), .. } => Some(Message::Space),
                keyboard::Event::KeyPressed { key: keyboard::Key::Named(keyboard::key::Named::Escape), .. } => Some(Message::Escape),
                keyboard::Event::KeyPressed { key: keyboard::Key::Named(keyboard::key::Named::F11), .. } => Some(Message::ToggleFullscreen),
                _ => None,
            }),
        ];
        if self.animating() {
            subs.push(window::frames().map(Message::Frame));
        } else if self.playing {
            subs.push(iced::time::every(Duration::from_millis(33)).map(Message::Frame));
        }
        Subscription::batch(subs)
    }

    fn frame(&mut self, at: Instant) -> Task<Message> {
        self.now = at;
        let mut task = Task::none();
        if self.playing {
            self.tick = self.tick.wrapping_add(1);
            self.position = self.audio.position();
            if self.audio.finished() {
                task = self.advance(true);
            }
        }
        // Loudness → 0..1 on a dB scale, quick to rise and slow to fall so it reads as a pulse.
        let rms = self.audio.level();
        let target = if rms > 0.0 { ((20.0 * rms.log10() + 36.0) / 28.0).clamp(0.0, 1.0) } else { 0.0 };
        let rate = if target > self.level { 0.45 } else { 0.08 };
        self.level += (target - self.level) * rate;
        if at.saturating_duration_since(self.last_level_push) >= Duration::from_millis(45) {
            self.last_level_push = at;
            self.levels.pop_front();
            self.levels.push_back(self.level);
        }
        if let Some(lyrics) = &self.lyrics {
            let line = lyrics.current(self.position);
            if line != self.lyric_line {
                self.lyric_from = self.lyric_line;
                self.lyric_line = line;
                self.lyric_tween = Tween::new(at, 380);
            }
        }
        if self.splash_exit.is_none()
            && self.library.is_some()
            && at.saturating_duration_since(self.booted).as_secs_f32() >= canvas::Splash::INTRO
        {
            self.splash_exit = Some(Tween::new(at, canvas::Splash::EXIT_MS));
            self.transition_page();
        }
        if self.ribbon_leave.is_some_and(|(_, t)| !t.running(at)) {
            self.ribbon_leave = None;
        }
        task
    }

    pub fn splash_done(&self) -> bool {
        self.splash_exit.is_some_and(|t| !t.running(self.now))
    }

    fn animating(&self) -> bool {
        let now = self.now;
        !self.splash_done()
            || self.page_tween.running(now)
            || self.tint_tween.running(now)
            || self.np_tween.running(now)
            || self.lyric_tween.running(now)
            || self.ribbon_reveal.running(now)
            || self.ribbon_leave.is_some()
            // Now playing moves with the music, so it needs every frame while it's open.
            || (self.now_playing && self.playing)
            || self.level > 0.01
            // The "getting song" spinner turns.
            || self.waiting.is_some()
            || self.searching
            || matches!(self.updating, Updating::Installing(_))
    }

    /// Content slides in and the cover-colored wash crossfades to the new page.
    fn transition_page(&mut self) {
        let now = Instant::now();
        self.tint_from = self.tint();
        self.tint_to = self.page_tint();
        self.tint_tween = Tween::new(now, 480);
        self.page_tween = Tween::new(now, 300);
    }

    pub fn tint(&self) -> Color {
        anim::mix(self.tint_from, self.tint_to, self.tint_tween.eased(self.now))
    }

    fn page_tint(&self) -> Color {
        if !self.query.trim().is_empty() {
            return theme::rgb(0x4A78B8);
        }
        match (self.page, &self.library) {
            (Page::Collection(a), Some(lib)) if a < lib.collections.len() => lib.collections[a].cover.color,
            (Page::Songs, _) => theme::rgb(0x6B5BC2),
            (Page::Liked, _) => theme::AMBER,
            _ => theme::rgb(0x3E8E8A),
        }
    }

    /// Where the queue came from, for "Playing from …".
    pub fn playing_from(&self, lib: &Library) -> String {
        match self.queue_source {
            Some(Source::Collection(c)) => lib.collections.get(c).map(|c| c.title.clone()).unwrap_or_default(),
            Some(Source::Liked) => "Liked songs".into(),
            Some(Source::Songs) => "All songs".into(),
            Some(Source::Search) => "your search".into(),
            None => self.current_track().map(|t| lib.tracks[t].album_title.clone()).unwrap_or_default(),
        }
    }

    fn close_now_playing(&mut self) {
        if self.now_playing {
            self.now_playing = false;
            self.np_tween = Tween::new(Instant::now(), 420);
        }
    }

    /// How far the Now playing sheet is open, 0 to 1.
    pub fn now_playing_progress(&self) -> f32 {
        let e = anim::ease_in_out(self.np_tween.linear(self.now));
        if self.now_playing { e } else { 1.0 - e }
    }

    fn with_window(&self, f: impl FnOnce(window::Id) -> Task<Message>) -> Task<Message> {
        self.window.map(f).unwrap_or_else(Task::none)
    }

    pub fn current_track(&self) -> Option<usize> {
        self.cursor.and_then(|c| self.queue.get(c).copied())
    }

    pub fn current_duration(&self) -> Duration {
        match (&self.library, self.current_track()) {
            (Some(lib), Some(t)) => lib.tracks[t].duration,
            _ => Duration::ZERO,
        }
    }

    /// The songs a list view shows, in order.
    pub fn source(&self, source: Source) -> Vec<usize> {
        let Some(lib) = &self.library else { return Vec::new() };
        let mut seen = HashSet::new();
        let all = lib.collections.iter().flat_map(|a| a.tracks.iter().copied()).filter(move |t| seen.insert(*t));
        match source {
            Source::Collection(a) => lib.collections.get(a).map(|a| a.tracks.clone()).unwrap_or_default(),
            Source::Songs => all.collect(),
            Source::Liked => all.filter(|&t| lib.tracks[t].path().is_some() && self.liked.contains(&lib.tracks[t].key())).collect(),
            Source::Search => {
                let q = self.query.trim().to_lowercase();
                if q.is_empty() {
                    return Vec::new();
                }
                // Your own songs first, then everything YouTube Music found.
                let mut list: Vec<usize> = all
                    .filter(|&t| {
                        let tr = &lib.tracks[t];
                        [&tr.title, &tr.artist, &tr.album_title].iter().any(|s| s.to_lowercase().contains(&q))
                    })
                    .collect();
                if self.found.query == self.query.trim() {
                    // Skip songs you already have, even as a different upload of the same song.
                    let same = |t: usize| (lib.tracks[t].title.to_lowercase(), lib.tracks[t].artist.to_lowercase());
                    let mut mine: HashSet<(String, String)> = list.iter().map(|&t| same(t)).collect();
                    list.extend(self.found.songs.iter().copied().filter(|&t| mine.insert(same(t))));
                }
                list
            }
        }
    }

    fn start(&mut self) -> Task<Message> {
        let (Some(lib), Some(t)) = (self.library.clone(), self.current_track()) else { return Task::none() };
        if !self.playing && self.position.is_zero() && self.ribbon_leave.is_none() {
            // Coming from silence: the queue grows into the ribbon.
            self.ribbon_reveal = Tween::new(Instant::now(), 900);
        }
        let track = &lib.tracks[t];
        self.position = Duration::ZERO;
        self.lyric_line = None;
        self.lyric_from = None;
        self.notice = None;
        self.ytdlp_stale = false;
        let lyrics = self.find_lyrics(t);
        let art = match (&track.cover.remote, &track.cover.hires) {
            (Some(url), None) => {
                let url = url.clone();
                Task::perform(youtube::stream::hires(self.http.clone(), url.clone()), move |b| Message::HiRes(url.clone(), b))
            }
            _ => Task::none(),
        };
        let playback = match &track.origin {
            Origin::File(path) => {
                self.waiting = None;
                self.play_file(&path.clone(), t);
                self.prefetch()
            }
            Origin::YouTube(id) => {
                if let Some(path) = youtube::stream::cached(id) {
                    self.waiting = None;
                    self.play_file(&path, t);
                    self.prefetch()
                } else {
                    // Not downloaded yet: stop the old song and wait for yt-dlp.
                    self.audio.stop();
                    self.playing = false;
                    self.waiting = Some(t);
                    Task::batch([self.download(id.clone()), self.prefetch()])
                }
            }
        };
        Task::batch([lyrics, art, playback])
    }

    /// A local .lrc right away, otherwise look online (LRCLIB, then YouTube Music).
    fn find_lyrics(&mut self, t: usize) -> Task<Message> {
        let Some(lib) = &self.library else { return Task::none() };
        let track = &lib.tracks[t];
        if let Some(local) = track.path().and_then(Lyrics::load) {
            self.lyrics = Some(local);
            self.lyrics_loading = false;
            return Task::none();
        }
        self.lyrics = None;
        self.lyrics_loading = true;
        let query = lyrics::Query {
            key: match &track.origin {
                Origin::YouTube(id) => id.clone(),
                Origin::File(p) => format!("file-{:016x}", library::hash(&p.to_string_lossy())),
            },
            video_id: match &track.origin {
                Origin::YouTube(id) => Some(id.clone()),
                Origin::File(_) => None,
            },
            title: track.title.clone(),
            artist: track.artist.clone(),
            album: track.album_title.clone(),
            duration: track.duration,
        };
        let (http, yt) = (self.http.clone(), self.yt_client.clone());
        Task::perform(lyrics::fetch(http, yt, query), move |l| Message::LyricsFound(t, l))
    }

    fn play_file(&mut self, path: &std::path::Path, t: usize) {
        let Some(lib) = &self.library else { return };
        let track = &lib.tracks[t];
        match self.audio.load(path, track.duration) {
            Ok(length) => {
                self.playing = true;
                // Songs from home-page tiles come without a length; the file knows it.
                if let Some(length) = length.filter(|_| track.duration.is_zero())
                    && let Some(lib) = self.library.as_mut()
                {
                    Arc::make_mut(lib).tracks[t].duration = length;
                }
            }
            Err(e) => {
                self.playing = false;
                self.notice = Some(format!("Couldn't play {}: {e}", track.title));
            }
        }
    }

    fn download(&mut self, id: String) -> Task<Message> {
        if !self.downloads.insert(id.clone()) {
            return Task::none();
        }
        Task::perform(youtube::stream::fetch(id.clone()), move |r| Message::Downloaded(id.clone(), r))
    }

    /// Fetch the next YouTube song while this one plays, so it starts instantly.
    fn prefetch(&mut self) -> Task<Message> {
        let (Some(lib), Some(c)) = (self.library.clone(), self.cursor) else { return Task::none() };
        match self.queue.get(c + 1).map(|&t| &lib.tracks[t].origin) {
            Some(Origin::YouTube(id)) if youtube::stream::cached(id).is_none() => self.download(id.clone()),
            _ => Task::none(),
        }
    }

    fn advance(&mut self, ended: bool) -> Task<Message> {
        let Some(c) = self.cursor else { return Task::none() };
        if ended && self.repeat == Repeat::One {
            self.start()
        } else if c + 1 < self.queue.len() {
            // The finished song shrinks out of the ribbon while the queue slides left.
            self.ribbon_leave = Some((self.queue[c], Tween::new(Instant::now(), 520)));
            self.cursor = Some(c + 1);
            self.start()
        } else if self.repeat != Repeat::Off && !self.queue.is_empty() {
            self.cursor = Some(0);
            self.start()
        } else {
            self.audio.stop();
            self.playing = false;
            self.position = Duration::ZERO;
            Task::none()
        }
    }

    // ---------- YouTube Music ----------

    /// Whether the heart is filled: your YouTube likes for YouTube songs, Musify's own for files.
    pub fn is_liked(&self, t: usize) -> bool {
        let Some(lib) = &self.library else { return false };
        match &lib.tracks[t].origin {
            Origin::YouTube(id) => self.yt_likes.contains(id),
            Origin::File(_) => self.liked.contains(&lib.tracks[t].key()),
        }
    }

    /// Updates the local view of a YouTube like: the heart and the Liked Music list.
    fn set_yt_like(&mut self, t: usize, like: bool) {
        let Some(lib) = self.library.as_mut() else { return };
        let lib = Arc::make_mut(lib);
        let Origin::YouTube(id) = lib.tracks[t].origin.clone() else { return };
        if like { self.yt_likes.insert(id) } else { self.yt_likes.remove(&id) };
        if let Some(c) = lib.find_browse("VLLM") {
            let col = &mut lib.collections[c];
            if col.loaded {
                col.tracks.retain(|&x| x != t);
                if like {
                    col.tracks.insert(0, t);
                }
            }
        }
    }

    /// Fetch a YouTube collection's songs if they haven't been yet.
    fn ensure_songs(&mut self, c: usize) -> Task<Message> {
        let (Some(lib), Some(client)) = (&self.library, &self.yt_client) else { return Task::none() };
        let Some(col) = lib.collections.get(c) else { return Task::none() };
        let Some(id) = col.browse_id.clone() else { return Task::none() };
        if col.loaded || !self.fetching.insert(id.clone()) {
            return Task::none();
        }
        let client = client.clone();
        Task::perform(async move { (id.clone(), client.songs(&id).await) }, |(id, r)| Message::YtSongs(id, r))
    }

    /// For play buttons on playlists whose songs haven't arrived yet: fetch, then play.
    fn load_first(&mut self, source: Source, i: usize, shuffled: bool) -> Option<Task<Message>> {
        let Source::Collection(c) = source else { return None };
        let loaded = self.library.as_ref().and_then(|l| l.collections.get(c)).is_none_or(|col| col.loaded);
        if loaded {
            return None;
        }
        self.pending_play = Some((source, i, shuffled));
        Some(self.ensure_songs(c))
    }

    fn merge_overview(&mut self, overview: youtube::Overview) {
        let Some(lib) = self.library.as_mut() else { return };
        let lib = Arc::make_mut(lib);
        lib.add_collection(Collection {
            title: "Liked Music".into(),
            subtitle: "YouTube Music".into(),
            kind: Kind::Liked,
            year: None,
            tracks: Vec::new(),
            cover: Cover { color: theme::AMBER, ..Cover::generated(7) },
            browse_id: Some("VLLM".into()),
            loaded: false,
            saved: true,
        });
        for card in overview.playlists {
            add_card(lib, card, true);
        }
        lib.shelves = overview
            .home
            .into_iter()
            .map(|shelf| library::Shelf {
                title: shelf.title,
                items: shelf
                    .items
                    .into_iter()
                    .map(|item| match item {
                        youtube::Item::Card(card) => ShelfItem::Collection(add_card(lib, card, false)),
                        youtube::Item::Song(song) => ShelfItem::Song(lib.add_video(song_track(song))),
                    })
                    .collect(),
            })
            .collect();
    }

    /// Back at a search whose results were replaced since: ask YouTube again.
    fn search_again(&mut self) -> Task<Message> {
        let words = self.query.trim().to_string();
        if self.yt_client.is_none() || words.is_empty() || words == self.found.query {
            return Task::none();
        }
        self.searching = true;
        self.update(Message::SearchRun(words))
    }

    fn merge_search(&mut self, query: String, results: youtube::Results) {
        let Some(lib) = self.library.as_mut() else { return };
        let lib = Arc::make_mut(lib);
        let songs = results.songs.into_iter().map(|s| lib.add_video(song_track(s))).collect();
        let collections = results.collections.into_iter().map(|c| add_card(lib, c, false)).collect();
        self.found = Found { query, songs, collections };
    }

    fn merge_songs(&mut self, browse_id: &str, songs: Vec<youtube::Song>) {
        if browse_id == "VLLM" {
            self.yt_likes = songs.iter().map(|s| s.video_id.clone()).collect();
        }
        let Some(lib) = self.library.as_mut() else { return };
        let lib = Arc::make_mut(lib);
        let Some(c) = lib.find_browse(browse_id) else { return };
        let tracks: Vec<usize> = songs.into_iter().map(|s| lib.add_video(song_track(s))).collect();
        let first_art = tracks.first().map(|&t| lib.tracks[t].cover.clone());
        let col = &mut lib.collections[c];
        col.tracks = tracks;
        col.loaded = true;
        // Liked Music has no artwork of its own; borrow the newest song's.
        if col.cover.image.is_none() && col.cover.url.is_none() && col.kind == Kind::Liked {
            if let Some(art) = first_art {
                col.cover.url = art.url.clone();
                col.cover.image = art.image;
            }
        }
    }

    /// Download thumbnails that aren't loaded yet, a batch at a time.
    fn fetch_art(&mut self) -> Task<Message> {
        let (Some(lib), Some(client)) = (&self.library, &self.yt_client) else { return Task::none() };
        let http = client.http().clone();
        let urls: Vec<String> = lib.pending_art().into_iter().filter(|u| !self.art_loading.contains(u)).take(80).collect();
        let tasks: Vec<Task<Message>> = urls
            .into_iter()
            .map(|url| {
                self.art_loading.insert(url.clone());
                let http = http.clone();
                Task::perform(youtube::stream::artwork(http, url.clone()), move |art| Message::Art(url.clone(), art))
            })
            .collect();
        Task::batch(tasks)
    }

    fn random(&mut self) -> u64 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        self.rng
    }

    /// Shuffle everything after the current song, leaving what's playing in place.
    fn shuffle_upcoming(&mut self) {
        let start = self.cursor.map(|c| c + 1).unwrap_or(0);
        for i in (start + 1..self.queue.len()).rev() {
            let j = start + (self.random() % (i - start + 1) as u64) as usize;
            self.queue.swap(i, j);
        }
    }
}

/// How many loudness samples the Now playing trail shows.
pub const LEVEL_HISTORY: usize = 56;

fn import_login() -> Task<Message> {
    Task::perform(
        async { tokio::task::spawn_blocking(youtube::cookies::import_firefox).await.unwrap_or_else(|e| Err(e.to_string())) },
        Message::YtImported,
    )
}

fn add_card(lib: &mut Library, card: youtube::Card, saved: bool) -> usize {
    let seed = library::hash(&card.browse_id);
    lib.add_collection(Collection {
        title: card.title,
        subtitle: card.subtitle,
        kind: match card.kind {
            youtube::CardKind::Album => Kind::Album,
            youtube::CardKind::Mix => Kind::Mix,
            youtube::CardKind::Playlist if card.browse_id == "VLLM" => Kind::Liked,
            youtube::CardKind::Playlist => Kind::Playlist,
        },
        year: None,
        tracks: Vec::new(),
        cover: Cover { remote: card.thumbnail.clone(), url: card.thumbnail, ..Cover::generated(seed) },
        browse_id: Some(card.browse_id),
        loaded: false,
        saved,
    })
}

fn song_track(song: youtube::Song) -> Track {
    Track {
        cover: Cover { remote: song.thumbnail.clone(), url: song.thumbnail, ..Cover::generated(library::hash(&song.video_id)) },
        origin: Origin::YouTube(song.video_id),
        title: song.title,
        artist: song.artist,
        album_title: song.album,
        number: None,
        disc: None,
        duration: song.duration,
    }
}

fn source_ready(app: &Musify, source: Source) -> bool {
    match source {
        Source::Collection(c) => app.library.as_ref().and_then(|l| l.collections.get(c)).is_some_and(|c| c.loaded),
        _ => true,
    }
}

fn volume_path() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("musify").join("volume.txt"))
}

/// Volume and mute from last time, as "0.62" or "0.62 muted".
fn discord_path() -> PathBuf {
    youtube::cookies::data_dir().join("discord.txt")
}

fn load_discord() -> bool {
    std::fs::read_to_string(discord_path()).is_ok_and(|s| s.trim() == "on")
}

fn save_discord(on: bool) {
    let _ = std::fs::create_dir_all(youtube::cookies::data_dir());
    let _ = std::fs::write(discord_path(), if on { "on" } else { "off" });
}

fn load_volume() -> (f32, bool) {
    let saved = volume_path().and_then(|p| std::fs::read_to_string(p).ok()).unwrap_or_default();
    let mut parts = saved.split_whitespace();
    let volume = parts.next().and_then(|v| v.parse::<f32>().ok()).filter(|v| (0.0..=1.0).contains(v)).unwrap_or(0.8);
    (volume, parts.next() == Some("muted"))
}

fn save_volume(volume: f32, muted: bool) {
    let Some(path) = volume_path() else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, format!("{volume:.2}{}", if muted { " muted" } else { "" }));
}

fn liked_path() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("musify").join("liked.txt"))
}

fn load_liked() -> HashSet<String> {
    liked_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|s| s.lines().filter(|l| !l.is_empty()).map(str::to_string).collect())
        .unwrap_or_default()
}

fn save_liked(liked: &HashSet<String>) {
    let Some(path) = liked_path() else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let mut lines: Vec<_> = liked.iter().cloned().collect();
    lines.sort();
    let _ = std::fs::write(path, lines.join("\n"));
}

fn initial_size() -> Size {
    // Debug builds accept MUSIFY_SIZE=WxH so layouts can be checked at any size.
    #[cfg(debug_assertions)]
    if let Some((w, h)) = std::env::var("MUSIFY_SIZE").ok().and_then(|v| {
        let (w, h) = v.split_once('x')?;
        Some((w.parse().ok()?, h.parse().ok()?))
    }) {
        return Size::new(w, h);
    }
    Size::new(1280.0, 820.0)
}

#[cfg(debug_assertions)]
impl Musify {
    /// Debug-only: MUSIFY_PREVIEW=album|playing|now|sheet|next jumps to a state for visual checks.
    /// MUSIFY_ALBUM picks the album by title; MUSIFY_PREVIEW=close|maximize fires those after a second.
    fn apply_preview(&mut self) -> Task<Message> {
        // MUSIFY_SEARCH="words" types a search once YouTube Music has had time to connect.
        if let Ok(words) = std::env::var("MUSIFY_SEARCH") {
            return Task::perform(tokio::time::sleep(Duration::from_millis(4000)), move |_| Message::Search(words.clone()));
        }
        let Ok(preview) = std::env::var("MUSIFY_PREVIEW") else { return Task::none() };
        let later = |m: Message| {
            Task::perform(async { tokio::time::sleep(Duration::from_millis(2600)).await }, move |_| m.clone())
        };
        match preview.as_str() {
            "close" => return later(Message::Close),
            "maximize" => return later(Message::ToggleMaximize),
            "fullscreen" => return later(Message::ToggleFullscreen),
            _ => {}
        }
        let Some(lib) = self.library.clone() else { return Task::none() };
        let wanted = std::env::var("MUSIFY_ALBUM").ok();
        let Some(album) = lib
            .collections
            .iter()
            .position(|a| wanted.as_deref().map_or(a.tracks.len() > 2, |w| a.title == w))
        else {
            return Task::none();
        };
        self.page = Page::Collection(album);
        if matches!(preview.as_str(), "playing" | "now" | "sheet" | "next") {
            let _ = self.update(Message::PlayFrom(Source::Collection(album), 1));
            self.audio.seek(Duration::from_secs(26));
            self.position = Duration::from_secs(26);
            self.now_playing = preview == "now";
            self.np_tween = Tween::done();
            match preview.as_str() {
                "sheet" => return later(Message::ToggleNowPlaying),
                "next" => return later(Message::Next),
                _ => {}
            }
        }
        Task::none()
    }
}

/// Debug-only: `musify --yt-search <words>` prints what a YouTube Music search returns.
#[cfg(debug_assertions)]
fn yt_search(query: &str) {
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().expect("runtime");
    rt.block_on(async {
        let Some(jar) = youtube::Jar::load() else { return println!("not connected") };
        match youtube::Client::new(jar).search(query).await {
            Err(e) => println!("search: {e}"),
            Ok(r) => {
                println!("songs: {}", r.songs.len());
                for s in r.songs.iter().take(6) {
                    println!("  {} | {} | {} | {:?} | art {}", s.title, s.artist, s.album, s.duration, s.thumbnail.is_some());
                }
                println!("albums/playlists: {}", r.collections.len());
                for c in &r.collections {
                    println!("  {:?} {} | {} | art {}", c.kind, c.title, c.subtitle, c.thumbnail.is_some());
                }
            }
        }
    });
}

/// Debug-only: `musify --yt-check` signs in from Firefox and prints what came back (counts, no names).
#[cfg(debug_assertions)]
fn yt_check() {
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().expect("runtime");
    rt.block_on(async {
        let jar = match youtube::Jar::load().ok_or(()).or_else(|_| youtube::cookies::import_firefox()) {
            Ok(j) => j,
            Err(e) => return println!("login: {e}"),
        };
        println!("login: ok ({jar:?})");
        let client = youtube::Client::new(jar);
        match client.overview().await {
            Err(e) => println!("overview: {e}"),
            Ok(o) => {
                let songs: usize = o.home.iter().map(|s| s.items.iter().filter(|i| matches!(i, youtube::Item::Song(_))).count()).sum();
                println!("library playlists: {}", o.playlists.len());
                println!("home shelves: {}, tiles: {}, song tiles: {songs}", o.home.len(), o.home.iter().map(|s| s.items.len()).sum::<usize>());
                println!("playlist ids: {:?}", o.playlists.iter().map(|c| &c.browse_id[..4.min(c.browse_id.len())]).collect::<Vec<_>>());
                let with_art = o.playlists.iter().filter(|c| c.thumbnail.is_some()).count();
                println!("playlists with art: {with_art}");
            }
        }
        match client.songs("VLLM").await {
            Err(e) => println!("liked: {e}"),
            Ok(s) => {
                let artist = s.iter().filter(|x| !x.artist.is_empty()).count();
                let album = s.iter().filter(|x| !x.album.is_empty()).count();
                let dur = s.iter().filter(|x| !x.duration.is_zero()).count();
                let thumb = s.iter().filter(|x| x.thumbnail.is_some()).count();
                println!("liked songs: {} (artist {artist}, album {album}, duration {dur}, art {thumb})", s.len());
                let http = reqwest::Client::builder().user_agent("Musify/0.1 (Linux music player)").build().unwrap();
                let (mut synced, mut plain, mut none) = (0, 0, 0);
                let mut sources = std::collections::BTreeMap::new();
                for song in s.iter().take(8) {
                    let q = lyrics::Query {
                        key: song.video_id.clone(),
                        video_id: Some(song.video_id.clone()),
                        title: song.title.clone(),
                        artist: song.artist.clone(),
                        album: song.album.clone(),
                        duration: song.duration,
                    };
                    match lyrics::fetch(http.clone(), Some(client.clone()), q).await {
                        Some(l) if l.synced => synced += 1,
                        Some(_) => plain += 1,
                        None => none += 1,
                    }
                    if let Some(l) = lyrics::fetch(http.clone(), None, lyrics::Query { key: song.video_id.clone(), video_id: None, title: String::new(), artist: String::new(), album: String::new(), duration: Duration::ZERO }).await {
                        *sources.entry(l.source.unwrap_or_default()).or_insert(0) += 1;
                    }
                }
                for host in ["googleusercontent", "ytimg"] {
                    if let Some(url) = s.iter().filter_map(|x| x.thumbnail.clone()).find(|u| u.contains(host)) {
                        let size = |b: Option<Vec<u8>>| b.and_then(|b| image::load_from_memory(&b).ok()).map(|i| (i.width(), i.height()));
                        let list = size(youtube::stream::artwork(http.clone(), url.clone()).await.map(|(b, _)| b));
                        let big = size(youtube::stream::hires(http.clone(), url).await);
                        println!("{host} art: list {list:?}, now playing {big:?}");
                    }
                }
                println!("lyrics for first 8 liked: synced {synced}, plain {plain}, none {none}; sources {sources:?}");
            }
        }
    });
}
