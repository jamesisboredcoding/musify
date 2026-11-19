//! Everything on screen. Layout follows the Figma frames:
//! landscape album view (4:2), now playing (5:84), portrait album view (6:84).

use std::time::Duration;

use iced::alignment::Horizontal;
use iced::border::Radius;
use iced::mouse::Interaction;
use iced::widget::text::Wrapping;
use iced::widget::{
    Column, Space, button, canvas, tooltip, column, container, float, image, mouse_area, opaque, row, scrollable, slider, space, stack,
    text, text_input,
};
use iced::window::Direction;
use iced::{Alignment, Color, ContentFit, Element, Font, Length, Padding, Vector};

use crate::anim::{self, hover};
use crate::canvas::{CoverArt, LevelTrail, Ribbon, RibbonStyle, Segment, Spinner, Splash};
use crate::icons::{Icon, icon};
use crate::library::{Library, ShelfItem, format_minutes, format_time};
use crate::theme::{self, AMBER, AMBER_INK, HAZE, INK, LINE, PAPER, RAISED, SLATE, alpha};
use crate::{Message, Musify, Page, Repeat, Source};

type El<'a> = Element<'a, Message>;

const RAIL: f32 = 264.0;
const RAIL_COMPACT: f32 = 76.0;
/// Height of the drag region above the page content.
const CHROME_H: f32 = 54.0;
const WASH_REACH: f32 = 0.45;

fn label<'a>(s: impl text::IntoFragment<'a>, size: f32, font: Font, color: Color) -> text::Text<'a> {
    text(s).size(size).font(font).color(color).wrapping(Wrapping::None)
}

fn icon_button<'a>(i: Icon, size: f32, side: f32, color: Color, msg: Option<Message>) -> El<'a> {
    let enabled = msg.is_some();
    let b = button(container(icon(i, size, color)).center(side))
        .width(side)
        .height(side)
        .padding(0)
        .style(theme::ghost(side / 2.0))
        .on_press_maybe(msg);
    hover(b, alpha(PAPER, if enabled { 0.08 } else { 0.0 }), side / 2.0).into()
}

/// `loading` (a clock in seconds) swaps the icon for a spinner while the song is being fetched.
fn play_button<'a>(playing: bool, loading: Option<f32>, side: f32, msg: Message) -> El<'a> {
    let inner: El = match loading {
        Some(t) => canvas(Spinner { t, color: AMBER_INK }).width(side * 0.46).height(side * 0.46).into(),
        None => icon(if playing { Icon::Pause } else { Icon::Play }, side * 0.42, AMBER_INK).into(),
    };
    button(container(inner).center(side))
        .width(side)
        .height(side)
        .padding(0)
        .style(theme::amber(side / 2.0))
        .on_press(msg)
        .into()
}

fn gap<'a>(h: f32) -> El<'a> {
    Space::new().height(h).into()
}

fn hairline<'a>() -> El<'a> {
    container(Space::new().height(1)).width(Length::Fill).style(theme::fill(LINE)).into()
}

impl Musify {
    /// Seconds since launch, for looping animations.
    fn clock(&self) -> f32 {
        self.now.saturating_duration_since(self.booted).as_secs_f32()
    }

    /// The spinner clock while the current song is still downloading.
    fn spin(&self) -> Option<f32> {
        (self.waiting.is_some() && self.waiting == self.current_track()).then(|| self.clock())
    }

    /// What Now playing says under the artist: fetching, an error, or paused.
    fn playback_status(&self) -> Option<El<'_>> {
        if let Some(n) = &self.notice {
            return Some(label(n, 14.0, theme::BODY, theme::ROSE).into());
        }
        if self.spin().is_some() {
            return Some(
                row![canvas(Spinner { t: self.clock(), color: AMBER }).width(16).height(16), label("Getting song…", 14.0, theme::STRONG, AMBER)]
                    .spacing(10)
                    .align_y(Alignment::Center)
                    .into(),
            );
        }
        (self.cursor.is_some() && !self.playing).then(|| {
            container(
                row![icon(Icon::Pause, 12.0, PAPER), label("Paused", 12.5, theme::STRONG, PAPER)].spacing(6).align_y(Alignment::Center),
            )
            .padding([4, 10])
            .style(theme::rounded(alpha(PAPER, 0.12), 999.0))
            .into()
        })
    }

    pub fn portrait(&self) -> bool {
        self.size.width < 1000.0 || self.size.height > self.size.width
    }

    fn radius(&self) -> f32 {
        if self.maximized || self.fullscreen { 0.0 } else { 12.0 }
    }

    pub fn view(&self) -> El<'_> {
        let r = self.radius();
        let content: El = match &self.library {
            None => Space::new().width(Length::Fill).height(Length::Fill).into(),
            Some(lib) => {
                let portrait = self.portrait();
                let rail = if portrait { self.rail_compact(lib) } else { self.rail(lib) };
                let body = row![rail, self.main(lib, portrait)].height(Length::Fill);
                let bar = if portrait { self.dock(lib) } else { self.player_bar(lib) };
                let app: El = column![body, hairline(), bar].into();
                // Now playing rises over the library as a sheet.
                let p = self.now_playing_progress();
                if p > 0.001 && self.current_track().is_some() {
                    let sheet = float(opaque(self.now_playing_view(lib)))
                        .translate(move |_, viewport| Vector::new(0.0, (1.0 - p) * viewport.height));
                    stack![app, sheet].into()
                } else {
                    app
                }
            }
        };
        let framed = container(content).width(Length::Fill).height(Length::Fill).style(theme::window(r)).clip(true);
        let mut layers = stack![framed];
        if !self.splash_done() {
            let t = self.now.saturating_duration_since(self.booted).as_secs_f32();
            let exit = self.splash_exit.map(|e| e.linear(self.now)).unwrap_or(0.0);
            let splash = canvas(Splash { t, exit, loading: self.library.is_none() }).width(Length::Fill).height(Length::Fill);
            let splash = container(splash).style(theme::corners(Color::TRANSPARENT, Radius::from(r))).clip(true);
            layers = layers.push(mouse_area(splash).on_press(Message::Drag));
        }
        if !self.maximized && !self.fullscreen {
            layers = layers.push(resize_edges());
        }
        layers.into()
    }

    // ---------- chrome ----------

    /// The drag region: drag to move, double-click to maximize. Buttons inside capture their own clicks.
    fn chrome_bar<'a>(&'a self, left: El<'a>, center: Option<El<'a>>) -> El<'a> {
        let bar = row![
            left,
            space::horizontal(),
            center.unwrap_or_else(|| space::horizontal().width(0).into()),
            space::horizontal(),
            window_controls(),
        ]
        .align_y(Alignment::Center)
        .padding(Padding { top: 14.0, right: 20.0, bottom: 10.0, left: 24.0 });
        mouse_area(bar).on_press(Message::Drag).on_double_click(Message::ToggleFullscreen).into()
    }

    fn history(&self) -> El<'_> {
        let cell = |i, msg: Option<Message>, radius: Radius| {
            let color = if msg.is_some() { PAPER } else { alpha(HAZE, 0.6) };
            let fill = if msg.is_some() { alpha(PAPER, 0.10) } else { Color::TRANSPARENT };
            hover(
                button(container(icon(i, 18.0, color)).center_x(36).center_y(30))
                    .padding(0)
                    .style(theme::chrome_cell)
                    .on_press_maybe(msg),
                fill,
                radius,
            )
        };
        container(
            row![
                cell(Icon::Left, (!self.back.is_empty()).then_some(Message::Back), CAPSULE_LEFT),
                capsule_divider(),
                cell(Icon::Right, (!self.forward.is_empty()).then_some(Message::Forward), CAPSULE_RIGHT),
            ]
            .align_y(Alignment::Center),
        )
        .style(theme::capsule)
        .clip(true)
        .into()
    }

    // ---------- rail ----------

    fn rail<'a>(&'a self, lib: &'a Library) -> El<'a> {
        let mark = mouse_area(
            row![wordmark_disc(18.0), label("musify", 24.0, theme::DISPLAY_M, PAPER)]
                .spacing(8)
                .align_y(Alignment::Center)
                .padding(Padding { top: 4.0, right: 0.0, bottom: 14.0, left: 6.0 }),
        )
        .on_press(Message::Drag)
        .on_double_click(Message::ToggleFullscreen);

        let search = container(
            row![
                icon(Icon::Search, 18.0, HAZE),
                text_input("Songs, artists, albums", &self.query)
                    .on_input(Message::Search)
                    .style(theme::search)
                    .font(theme::BODY)
                    .size(14)
                    .padding(0),
            ]
            .spacing(10)
            .align_y(Alignment::Center),
        )
        .padding([0, 12])
        .height(40)
        .center_y(40)
        .style(theme::rounded(INK, 8.0));

        let nav = |i, name, page: Page| {
            let active = self.page == page && self.query.is_empty();
            let color = if active { PAPER } else { HAZE };
            hover(
                button(
                    row![icon(i, 20.0, color), label(name, 14.0, if active { theme::STRONG } else { theme::BODY }, color)]
                        .spacing(12)
                        .align_y(Alignment::Center),
                )
                .width(Length::Fill)
                .padding([9, 12])
                .style(theme::row(active))
                .on_press(Message::Open(page)),
                alpha(RAISED, 0.6),
                8.0,
            )
        };

        let liked_count = self.source(Source::Liked).len();
        let mut shelf = Column::new().spacing(2).push(spine(
            "Liked songs",
            songs(liked_count),
            AMBER,
            self.page == Page::Liked,
            Message::Open(Page::Liked),
        ));
        let section = |name| row![label(name, 12.5, theme::STRONG, alpha(HAZE, 0.8))].padding(Padding { top: 14.0, right: 0.0, bottom: 4.0, left: 12.0 });
        shelf = shelf.push(section("YouTube Music")).push(self.youtube_status(false));
        for (i, a) in lib.youtube_collections().filter(|(_, c)| c.saved) {
            shelf = shelf.push(spine(&a.title, collection_detail(a), a.cover.color, self.page == Page::Collection(i), Message::Open(Page::Collection(i))));
        }
        if lib.has_local() {
            shelf = shelf.push(section("On this computer"));
            for (i, a) in lib.local_albums() {
                shelf = shelf.push(spine(&a.title, a.subtitle.clone(), a.cover.color, self.page == Page::Collection(i), Message::Open(Page::Collection(i))));
            }
        }

        let content = column![
            mark,
            search,
            gap(6.0),
            nav(Icon::Home, "Home", Page::Home),
            nav(Icon::Library, "All songs", Page::Songs),
            gap(14.0),
            row![label("Your shelf", 14.0, theme::STRONG, HAZE)].padding(Padding { top: 0.0, right: 0.0, bottom: 4.0, left: 12.0 }),
            scrollable(shelf).style(theme::scroll).height(Length::Fill),
        ]
        .spacing(4)
        .padding(Padding { top: 18.0, right: 16.0, bottom: 16.0, left: 16.0 });

        container(content)
            .width(RAIL)
            .height(Length::Fill)
            .style(theme::corners(SLATE, Radius { top_left: self.radius(), ..Default::default() }))
            .into()
    }

    fn rail_compact<'a>(&'a self, lib: &'a Library) -> El<'a> {
        let nav = |i, page: Page| {
            let active = self.page == page && self.query.is_empty();
            hover(
                button(container(icon(i, 20.0, if active { PAPER } else { HAZE })).center(44))
                    .width(44)
                    .height(44)
                    .padding(0)
                    .style(theme::row(active))
                    .on_press(Message::Open(page)),
                alpha(RAISED, 0.6),
                8.0,
            )
        };
        // Each shelf item is its cover (Liked songs a heart), with its name on hover.
        let tile = |art: El<'a>, name: String, detail: String, selected: bool, msg: Message| -> El<'a> {
            let ring = container(art).padding(2).style(move |_: &iced::Theme| container::Style {
                border: iced::Border { radius: 9.0.into(), width: 2.0, color: if selected { PAPER } else { Color::TRANSPARENT } },
                ..Default::default()
            });
            let tip = container(
                column![label(name, 13.5, theme::MEDIUM, PAPER), label(detail, 12.0, theme::BODY, HAZE)].spacing(1),
            )
            .padding([6, 10])
            .style(theme::rounded(RAISED, 6.0));
            tooltip(
                hover(button(ring).padding(0).style(theme::plain).on_press(msg), alpha(PAPER, 0.08), 9.0),
                tip,
                tooltip::Position::Right,
            )
            .gap(10)
            .into()
        };
        let liked_art: El = container(icon(Icon::HeartFill, 20.0, AMBER_INK)).center(44).style(theme::rounded(AMBER, 6.0)).into();

        let mut shelf = Column::new().spacing(6).align_x(Alignment::Center);
        shelf = shelf.push(tile(
            liked_art,
            "Liked songs".into(),
            songs(self.source(Source::Liked).len()),
            self.page == Page::Liked,
            Message::Open(Page::Liked),
        ));
        for (i, a) in lib.collections.iter().enumerate().filter(|(_, c)| c.saved) {
            shelf = shelf.push(tile(
                cover_art(&a.cover, 44.0),
                a.title.clone(),
                if a.browse_id.is_some() { collection_detail(a) } else { a.subtitle.clone() },
                self.page == Page::Collection(i),
                Message::Open(Page::Collection(i)),
            ));
        }

        let content = column![
            mouse_area(container(wordmark_disc(22.0)).center_x(Length::Fill).padding([4, 0]))
                .on_press(Message::Drag)
                .on_double_click(Message::ToggleFullscreen),
            gap(10.0),
            nav(Icon::Home, Page::Home),
            nav(Icon::Library, Page::Songs),
            container(Space::new().width(28).height(1)).style(theme::fill(LINE)),
            scrollable(shelf).style(theme::scroll).height(Length::Fill),
        ]
        .spacing(8)
        .align_x(Alignment::Center)
        .padding([20, 0]);

        container(content)
            .width(RAIL_COMPACT)
            .height(Length::Fill)
            .style(theme::corners(SLATE, Radius { top_left: self.radius(), ..Default::default() }))
            .into()
    }

    // ---------- main area ----------

    fn main<'a>(&'a self, lib: &'a Library, portrait: bool) -> El<'a> {
        let pad = if portrait { 28.0 } else { 32.0 };
        let page: El = if !self.query.trim().is_empty() {
            self.list_page(lib, format!("Results for “{}”", self.query.trim()), Source::Search, pad)
        } else {
            match self.page {
                Page::Collection(a) if a < lib.collections.len() => self.album_page(lib, a, portrait, pad),
                Page::Songs => self.list_page(lib, "All songs".into(), Source::Songs, pad),
                Page::Liked => self.list_page(lib, "Liked songs".into(), Source::Liked, pad),
                _ => self.home_page(lib, portrait, pad),
            }
        };
        let tint = self.tint();
        // New pages rise 18px and fade up from the background.
        let e = self.page_tween.eased(self.now);
        let page: El = if e < 1.0 {
            let lifted = container(page).width(Length::Fill).padding(Padding { top: 18.0 * (1.0 - e), ..Padding::ZERO });
            // A veil painted like the background (ink + the same wash), so only the content appears to fade.
            let k = 0.9 * (1.0 - e);
            let area = self.size.height - if portrait { 190.0 } else { 113.0 };
            let veil = stack![
                container(Space::new()).width(Length::Fill).height(Length::Fill).style(theme::fill(alpha(INK, k))),
                container(Space::new()).width(Length::Fill).height(Length::Fill).style(theme::veil(tint, k, area, CHROME_H)),
            ];
            stack![lifted, veil].into()
        } else {
            page
        };
        let left = if portrait {
            // The rail search is hidden in portrait, so search lives in the drag region.
            row![self.history(), compact_search(&self.query)].spacing(12).align_y(Alignment::Center).into()
        } else {
            self.history()
        };
        container(column![self.chrome_bar(left, None), scrollable(page).style(theme::scroll).height(Length::Fill)])
            .width(Length::Fill)
            .height(Length::Fill)
            .style(theme::wash(tint, WASH_REACH, Radius { top_right: self.radius(), ..Default::default() }))
            .into()
    }

    fn album_page<'a>(&'a self, lib: &'a Library, a: usize, portrait: bool, pad: f32) -> El<'a> {
        let album = &lib.collections[a];
        let meta = if album.loaded {
            format!(
                "{}{}, {}",
                album.year.map(|y| format!("{y}, ")).unwrap_or_default(),
                songs(album.tracks.len()),
                format_minutes(lib.duration(a))
            )
        } else {
            String::new()
        };
        let byline = row![
            container(Space::new().width(24).height(24)).style(theme::rounded(alpha(album.cover.color, 0.9), 12.0)),
            label(&album.subtitle, 14.0, theme::STRONG, PAPER),
            label(meta, 14.0, theme::BODY, alpha(PAPER, 0.7)),
        ]
        .spacing(10)
        .align_y(Alignment::Center);

        let content_w = self.size.width - if portrait { RAIL_COMPACT } else { RAIL } - pad * 2.0;
        let title_size = if portrait { 72.0 } else { 76.0 };
        let title = text(&album.title).size(title_size).font(theme::DISPLAY_XL).color(PAPER).line_height(0.98);
        let hero: El = if portrait {
            column![cover(&album.cover, 300.0), column![title, byline].spacing(14)].spacing(28).into()
        } else {
            let cover_side = 220.0;
            row![
                cover(&album.cover, cover_side),
                column![title, byline].spacing(14).width(Length::Fixed((content_w - cover_side - 32.0).max(200.0))),
            ]
            .spacing(32)
            .align_y(Alignment::End)
            .into()
        };

        let this_album_playing = self.cursor.is_some() && self.queue_source == Some(Source::Collection(a));
        let play_msg = if this_album_playing { Message::TogglePlay } else { Message::PlayFrom(Source::Collection(a), 0) };
        let actions = row![
            play_button(this_album_playing && self.playing, None, 56.0, play_msg),
            icon_button(Icon::Shuffle, 22.0, 40.0, AMBER, Some(Message::PlayShuffled(Source::Collection(a)))),
        ]
        .spacing(14)
        .align_y(Alignment::Center);

        column![
            container(hero).padding(Padding { top: 6.0, right: pad, bottom: 28.0, left: pad }),
            container(actions).padding(Padding { top: 0.0, right: pad, bottom: 12.0, left: pad }),
            if album.loaded {
                self.track_list(lib, Source::Collection(a), album.is_youtube())
            } else {
                container(label("Loading songs…", 14.0, theme::BODY, HAZE)).padding([8.0, pad]).into()
            },
        ]
        .into()
    }

    fn list_page<'a>(&'a self, lib: &'a Library, title: String, source: Source, pad: f32) -> El<'a> {
        let tracks = self.source(source);
        let total: Duration = tracks.iter().map(|&t| lib.tracks[t].duration).sum();
        let heading = column![
            text(title).size(40).font(theme::DISPLAY_L).color(PAPER),
            label(format!("{}, {}", songs(tracks.len()), format_minutes(total)), 14.0, theme::BODY, alpha(PAPER, 0.7)),
        ]
        .spacing(8);
        let searching = source == Source::Search && self.searching;
        let body: El = if tracks.is_empty() {
            let hint = match source {
                Source::Liked => "Songs you like show up here. Click the heart next to any song to add it.",
                Source::Search if searching => "",
                Source::Search => "Nothing matches. Try part of a song title, artist, or album name.",
                _ => "No songs yet.",
            };
            container(text(hint).size(14).color(HAZE)).padding([8.0, pad]).into()
        } else {
            column![
                container(
                    row![
                        play_button(false, None, 56.0, Message::PlayFrom(source, 0)),
                        icon_button(Icon::Shuffle, 22.0, 40.0, AMBER, Some(Message::PlayShuffled(source))),
                    ]
                    .spacing(14)
                    .align_y(Alignment::Center)
                )
                .padding(Padding { top: 0.0, right: pad, bottom: 12.0, left: pad }),
                self.track_list(lib, source, true),
            ]
            .into()
        };
        let extra = (source == Source::Search).then(|| self.search_extras(lib, pad));
        Column::new()
            .push(container(heading).padding(Padding { top: 10.0, right: pad, bottom: 24.0, left: pad }))
            .extend(extra)
            .push(body)
            .into()
    }

    /// Above the search's song list: whether YouTube is still looking, and the albums and playlists it found.
    fn search_extras<'a>(&'a self, lib: &'a Library, pad: f32) -> El<'a> {
        let mut col = Column::new().spacing(16).padding(Padding { top: 0.0, right: pad, bottom: 24.0, left: pad });
        if self.searching {
            col = col.push(
                row![canvas(Spinner { t: self.clock(), color: AMBER }).width(16).height(16), label("Searching YouTube Music…", 14.0, theme::STRONG, AMBER)]
                    .spacing(10)
                    .align_y(Alignment::Center),
            );
        } else if let Some(e) = &self.search_error {
            col = col.push(label(e, 14.0, theme::BODY, theme::ROSE));
        } else if self.yt != crate::YtStatus::Ready {
            col = col.push(label("Connect YouTube Music to search all of it, not only your library.", 14.0, theme::BODY, HAZE));
        }
        if self.found.query == self.query.trim() && !self.found.collections.is_empty() {
            let mut tiles = row![].spacing(20);
            for &c in &self.found.collections {
                let a = &lib.collections[c];
                tiles = tiles.push(tile(&a.cover, &a.title, a.subtitle.clone(), 150.0, Message::Open(Page::Collection(c))));
            }
            col = col.push(label("Albums and playlists", 20.0, theme::DISPLAY_M, PAPER)).push(
                scrollable(tiles)
                    .direction(scrollable::Direction::Horizontal(scrollable::Scrollbar::new().width(4).scroller_width(4).spacing(10)))
                    .style(theme::scroll),
            );
            col = col.push(Space::new().height(4)).push(label("Songs", 20.0, theme::DISPLAY_M, PAPER));
        }
        col.into()
    }

    fn home_page<'a>(&'a self, lib: &'a Library, portrait: bool, pad: f32) -> El<'a> {
        let width = self.size.width - if portrait { RAIL_COMPACT } else { RAIL } - pad * 2.0 - 12.0;
        let mut page = Column::new().spacing(36).padding(Padding { top: 10.0, right: pad, bottom: 40.0, left: pad });

        if lib.shelves.is_empty() {
            page = page.push(text("Home").size(40).font(theme::DISPLAY_L).color(PAPER));
            if self.yt != crate::YtStatus::Ready {
                page = page.push(self.youtube_status(true));
            }
        }

        // YouTube Music's home rows, each scrolling sideways.
        for shelf in &lib.shelves {
            let mut tiles = row![].spacing(20);
            for item in &shelf.items {
                tiles = tiles.push(match *item {
                    ShelfItem::Collection(c) => {
                        let col = &lib.collections[c];
                        tile(&col.cover, &col.title, col.subtitle.clone(), 168.0, Message::Open(Page::Collection(c)))
                    }
                    ShelfItem::Song(t) => {
                        let tr = &lib.tracks[t];
                        tile(&tr.cover, &tr.title, tr.artist.clone(), 168.0, Message::PlaySong(t))
                    }
                });
            }
            page = page.push(
                column![
                    text(&shelf.title).size(26).font(theme::DISPLAY_M).color(PAPER),
                    scrollable(tiles)
                        .direction(scrollable::Direction::Horizontal(scrollable::Scrollbar::new().width(4).scroller_width(4).spacing(10)))
                        .style(theme::scroll),
                ]
                .spacing(14),
            );
        }

        if lib.has_local() {
            let spacing = 24.0;
            let cols = (((width + spacing) / (180.0 + spacing)).floor() as usize).max(2);
            let side = ((width - spacing * (cols - 1) as f32) / cols as f32).floor();
            let albums: Vec<_> = lib.local_albums().collect();
            let mut grid = Column::new().spacing(28);
            for chunk in albums.chunks(cols) {
                let mut r = row![].spacing(spacing);
                for &(i, a) in chunk {
                    let sub = match a.year {
                        Some(y) => format!("{}, {y}", a.subtitle),
                        None => a.subtitle.clone(),
                    };
                    r = r.push(tile(&a.cover, &a.title, sub, side, Message::Open(Page::Collection(i))));
                }
                grid = grid.push(r);
            }
            let count: usize = albums.iter().map(|(_, a)| a.tracks.len()).sum();
            page = page.push(
                column![
                    column![
                        text("On this computer").size(if lib.shelves.is_empty() { 40 } else { 26 }).font(theme::DISPLAY_L).color(PAPER),
                        label(format!("{} albums, {}", albums.len(), songs(count)), 14.0, theme::BODY, alpha(PAPER, 0.7)),
                    ]
                    .spacing(8),
                    grid,
                ]
                .spacing(24),
            );
        }
        page.into()
    }

    /// Connect button and sign-in progress. `large` is the version on an empty Home page.
    fn youtube_status(&self, large: bool) -> El<'_> {
        use crate::YtStatus::*;
        let (line, action): (String, Option<(&str, Message)>) = match &self.yt {
            Off => (
                "Sign in to bring in your playlists, liked songs and mixes. Musify uses your Firefox login, or opens a Firefox window to sign in.".into(),
                Some(("Connect YouTube Music", Message::YtConnect)),
            ),
            Connecting => ("Looking for your YouTube login in Firefox…".into(), None),
            SigningIn => ("Sign in to YouTube Music in the Firefox window, then close it. Musify picks up your login when it closes.".into(), None),
            Loading => ("Loading your library…".into(), None),
            Ready => return Space::new().into(),
            Failed(e) => (e.clone(), Some(("Try again", Message::YtConnect))),
        };
        let color = if matches!(self.yt, Failed(_)) { theme::ROSE } else { HAZE };
        let mut col = Column::new().spacing(10).push(text(line).size(if large { 15.0 } else { 12.5 }).color(color));
        if let Some((label_text, msg)) = action {
            col = col.push(
                button(label(label_text, if large { 15.0 } else { 13.0 }, theme::STRONG, AMBER_INK))
                    .padding(if large { Padding::from([12, 22]) } else { Padding::from([7, 14]) })
                    .style(theme::amber(20.0))
                    .on_press(msg),
            );
        }
        container(col).padding(if large { Padding::ZERO } else { Padding { top: 0.0, right: 8.0, bottom: 8.0, left: 12.0 } }).max_width(520).into()
    }

    fn track_list<'a>(&'a self, lib: &'a Library, source: Source, show_album: bool) -> El<'a> {
        let tracks = self.source(source);
        let current = self.current_track();
        let columns = row![
            container(label("#", 12.5, theme::BODY, HAZE)).center_x(24),
            label("Title", 12.5, theme::BODY, HAZE).width(Length::FillPortion(3)),
        ]
        .extend(show_album.then(|| label("Album", 12.5, theme::BODY, HAZE).width(Length::FillPortion(2)).into()))
        .push(Space::new().width(32))
        .push(label("Length", 12.5, theme::BODY, HAZE))
        .spacing(16)
        .padding(Padding { top: 0.0, right: 20.0, bottom: 8.0, left: 16.0 });

        let mut list = Column::new().spacing(2).push(columns).push(hairline()).push(gap(6.0));
        for (i, &t) in tracks.iter().enumerate() {
            let track = &lib.tracks[t];
            let is_current = current == Some(t);
            let lead: El = if is_current {
                container(eq_bars(self.now.saturating_duration_since(self.booted).as_secs_f32(), self.playing)).center_x(24).into()
            } else {
                container(label((i + 1).to_string(), 14.0, theme::BODY, HAZE)).center_x(24).into()
            };
            let liked = self.is_liked(t);
            let heart = hover(button(container(icon(if liked { Icon::HeartFill } else { Icon::Heart }, 16.0, if liked { AMBER } else { alpha(HAZE, 0.7) })).center(32))
                .width(32)
                .height(32)
                .padding(0)
                .style(theme::ghost(16.0))
                .on_press(Message::ToggleLike(t)), alpha(PAPER, 0.08), 16.0);
            let row_content = row![
                lead,
                column![
                    label(&track.title, 15.0, theme::MEDIUM, if is_current { AMBER } else { PAPER }),
                    label(&track.artist, 12.5, theme::BODY, HAZE),
                ]
                .spacing(2)
                .width(Length::FillPortion(3))
                .clip(true),
            ]
            .extend(show_album.then(|| {
                container(label(&track.album_title, 14.0, theme::BODY, HAZE))
                    .width(Length::FillPortion(2))
                    .clip(true)
                    .into()
            }))
            .push(heart)
            .push(label(format_time(track.duration), 14.0, theme::BODY, HAZE))
            .spacing(16)
            .align_y(Alignment::Center);
            list = list.push(hover(
                button(row_content)
                    .width(Length::Fill)
                    .padding(Padding { top: 6.0, right: 20.0, bottom: 6.0, left: 16.0 })
                    .style(theme::row(is_current))
                    .on_press(Message::PlayFrom(source, i)),
                alpha(RAISED, 0.6),
                8.0,
            ));
        }
        container(list).padding(Padding { top: 0.0, right: 20.0, bottom: 32.0, left: 20.0 }).into()
    }

    // ---------- player ----------

    fn ribbon<'a>(&'a self, lib: &'a Library, style: RibbonStyle) -> El<'a> {
        let mut segments = Vec::new();
        let mut current = 0;
        if let Some((t, tween)) = self.ribbon_leave
            && self.cursor.is_some()
        {
            let tr = &lib.tracks[t];
            segments.push(Segment {
                title: tr.title.clone(),
                artist: tr.artist.clone(),
                secs: tr.duration.as_secs_f32(),
                color: tr.cover.color,
                mark: String::new(),
                weight: 1.0 - anim::ease_in_out(tween.linear(self.now)),
            });
            current = 1;
        }
        if let Some(c) = self.cursor {
            let mut ahead = Duration::ZERO;
            let limit = Duration::from_secs(45 * 60);
            for (k, &t) in self.queue.iter().enumerate().skip(c).take(12) {
                let tr = &lib.tracks[t];
                if k > c && ahead > limit {
                    break;
                }
                let mark = if k == c { "now".into() } else { format!("in {}", format_minutes(ahead)) };
                segments.push(Segment {
                    title: tr.title.clone(),
                    artist: tr.artist.clone(),
                    secs: tr.duration.as_secs_f32(),
                    color: tr.cover.color,
                    mark,
                    weight: 1.0,
                });
                ahead += if k == c { tr.duration.saturating_sub(self.position) } else { tr.duration };
            }
        }
        if segments.len() <= current {
            segments.clear();
            current = 0;
        }
        let reveal = self.ribbon_reveal.linear(self.now);
        canvas(Ribbon { segments, current, played: self.position.as_secs_f32(), style, reveal, on_click: Message::Ribbon })
            .width(Length::Fill)
            .height(Ribbon::<Message>::height(style))
            .into()
    }

    /// Fades the time labels in with the ribbon.
    fn reveal(&self) -> f32 {
        if self.cursor.is_none() { 0.0 } else { anim::ease_out((self.ribbon_reveal.linear(self.now) - 0.35) / 0.65) }
    }

    fn queue_left_text(&self, lib: &Library) -> String {
        if self.cursor.is_none() {
            return String::new();
        }
        format!("{} left in queue", format_minutes(self.queue_left(lib)))
    }

    fn queue_left(&self, lib: &Library) -> Duration {
        let Some(c) = self.cursor else { return Duration::ZERO };
        let rest: Duration = self.queue.iter().skip(c + 1).map(|&t| lib.tracks[t].duration).sum();
        rest + self.current_duration().saturating_sub(self.position)
    }

    fn controls(&self, big: bool) -> El<'_> {
        let s = if big { 1.25 } else { 1.0 };
        let repeat_color = if self.repeat == Repeat::Off { HAZE } else { AMBER };
        let repeat: El = if self.repeat == Repeat::One {
            stack![
                icon_button(Icon::Repeat, 18.0 * s, 32.0 * s, repeat_color, Some(Message::CycleRepeat)),
                container(label("1", 9.0, theme::STRONG, AMBER_INK))
                    .center(12)
                    .style(theme::rounded(AMBER, 6.0)),
            ]
            .into()
        } else {
            icon_button(Icon::Repeat, 18.0 * s, 32.0 * s, repeat_color, Some(Message::CycleRepeat))
        };
        row![
            icon_button(Icon::Shuffle, 18.0 * s, 32.0 * s, if self.shuffle { AMBER } else { HAZE }, Some(Message::ToggleShuffle)),
            icon_button(Icon::Prev, 20.0 * s, 32.0 * s, PAPER, Some(Message::Prev)),
            play_button(self.playing, self.spin(), if big { 60.0 } else { 40.0 }, Message::TogglePlay),
            icon_button(Icon::Next, 20.0 * s, 32.0 * s, PAPER, Some(Message::Next)),
            repeat,
        ]
        .spacing(if big { 22.0 } else { 18.0 })
        .align_y(Alignment::Center)
        .into()
    }

    fn now_playing_card<'a>(&'a self, lib: &'a Library) -> El<'a> {
        let Some(t) = self.current_track() else {
            return column![label("Nothing playing", 15.0, theme::MEDIUM, PAPER), label("Pick a song to start", 12.5, theme::BODY, HAZE)]
                .spacing(3)
                .into();
        };
        let track = &lib.tracks[t];
        let liked = self.is_liked(t);
        let sub: El = match (&self.notice, self.waiting == Some(t)) {
            (Some(n), _) => label(n, 12.5, theme::BODY, theme::ROSE).into(),
            (None, true) => row![
                canvas(Spinner { t: self.clock(), color: AMBER }).width(12).height(12),
                label("Getting song…", 12.5, theme::BODY, AMBER),
            ]
            .spacing(6)
            .align_y(Alignment::Center)
            .into(),
            (None, false) => label(&track.artist, 12.5, theme::BODY, HAZE).into(),
        };
        row![
            button(cover(&track.cover, 56.0)).padding(0).style(theme::plain).on_press(Message::ToggleNowPlaying),
            column![label(&track.title, 15.0, theme::MEDIUM, PAPER), sub].spacing(3).width(Length::Fill).clip(true),
            icon_button(if liked { Icon::HeartFill } else { Icon::Heart }, 18.0, 32.0, if liked { AMBER } else { HAZE }, Some(Message::ToggleLike(t))),
        ]
        .spacing(14)
        .align_y(Alignment::Center)
        .into()
    }

    /// Turns the Discord "Listening to" status on and off.
    fn discord_toggle(&self) -> El<'_> {
        let ready = crate::discord::Discord::configured();
        let on = self.discord.is_some();
        let tip = match (ready, on) {
            (false, _) => "Discord status isn't set up in this build",
            (true, true) => "Showing on Discord. Click to stop",
            (true, false) => "Show what you're playing on Discord",
        };
        tooltip(
            icon_button(Icon::Broadcast, 18.0, 32.0, if on { AMBER } else { HAZE }, ready.then_some(Message::ToggleDiscord)),
            container(label(tip, 12.5, theme::BODY, PAPER)).padding([5, 9]).style(theme::rounded(RAISED, 6.0)),
            tooltip::Position::Top,
        )
        .gap(6)
        .into()
    }

    fn output(&self) -> El<'_> {
        let vol_icon = if self.muted || self.volume == 0.0 { Icon::Mute } else { Icon::Volume };
        row![
            icon_button(Icon::Lyrics, 18.0, 32.0, if self.now_playing { AMBER } else { HAZE }, self.cursor.map(|_| Message::ToggleNowPlaying)),
            icon_button(Icon::Expand, 17.0, 32.0, HAZE, self.cursor.map(|_| Message::ToggleNowPlaying)),
            self.discord_toggle(),
            icon_button(vol_icon, 18.0, 32.0, HAZE, Some(Message::ToggleMute)),
            slider(0.0..=1.0, if self.muted { 0.0 } else { self.volume }, Message::Volume).step(0.01_f32).width(88).style(theme::volume),
        ]
        .spacing(4)
        .align_y(Alignment::Center)
        .into()
    }

    /// Landscape player bar (Figma 4:2).
    fn player_bar<'a>(&'a self, lib: &'a Library) -> El<'a> {
        let r = self.radius();
        // The ribbon row is always there (an empty groove when idle) so nothing jumps when music starts.
        // Time labels get fixed widths so the ribbon doesn't resize as the digits change.
        let a = self.reveal();
        let elapsed = container(label(if self.cursor.is_some() { format_time(self.position) } else { String::new() }, 12.5, theme::BODY, alpha(PAPER, a)))
            .width(44)
            .align_x(Horizontal::Right);
        let left = container(label(self.queue_left_text(lib), 12.5, theme::BODY, alpha(HAZE, a))).width(150);
        let transport = column![
            self.controls(false),
            row![elapsed, self.ribbon(lib, RibbonStyle::Compact), left].spacing(12).align_y(Alignment::Center),
        ]
        .spacing(2)
        .align_x(Alignment::Center)
        .width(Length::Fill);

        container(
            row![
                container(self.now_playing_card(lib)).width(300),
                transport,
                container(self.output()).width(300).align_x(Horizontal::Right),
            ]
            .spacing(32)
            .align_y(Alignment::Center),
        )
        .padding([0, 20])
        .height(112)
        .center_y(112)
        .style(theme::corners(SLATE, Radius { bottom_left: r, bottom_right: r, ..Default::default() }))
        .into()
    }

    /// Portrait dock (Figma 6:84): two rows, ribbon with titles.
    fn dock<'a>(&'a self, lib: &'a Library) -> El<'a> {
        let r = self.radius();
        container(
            column![
                row![
                    container(self.now_playing_card(lib)).width(Length::FillPortion(1)),
                    self.controls(false),
                    container(self.output()).width(Length::FillPortion(1)).align_x(Horizontal::Right),
                ]
                .spacing(16)
                .align_y(Alignment::Center),
                self.ribbon(lib, RibbonStyle::Labelled),
                row![
                    label(
                        if self.cursor.is_some() {
                            format!("{} of {}", format_time(self.position), format_time(self.current_duration()))
                        } else {
                            String::new()
                        },
                        12.5,
                        theme::BODY,
                        alpha(PAPER, self.reveal())
                    ),
                    space::horizontal(),
                    label(self.queue_left_text(lib), 12.5, theme::BODY, alpha(HAZE, self.reveal())),
                ],
            ]
            .spacing(6),
        )
        .padding(Padding { top: 18.0, right: 24.0, bottom: 18.0, left: 24.0 })
        .style(theme::corners(SLATE, Radius { bottom_left: r, bottom_right: r, ..Default::default() }))
        .into()
    }

    // ---------- now playing ----------

    fn now_playing_view<'a>(&'a self, lib: &'a Library) -> El<'a> {
        let t = self.current_track().unwrap_or_default();
        let track = &lib.tracks[t];
        let portrait = self.portrait();
        let from = self.playing_from(lib);
        let liked = self.is_liked(t);

        let back = button(
            row![icon(Icon::Down, 18.0, PAPER), label("Library", 14.0, theme::STRONG, PAPER)].spacing(8).align_y(Alignment::Center),
        )
        .padding(Padding { top: 6.0, right: 14.0, bottom: 6.0, left: 10.0 })
        .style(theme::pill)
        .on_press(Message::ToggleNowPlaying);
        let source = label(format!("Playing from {from}"), 12.5, theme::BODY, alpha(PAPER, 0.7));
        let header = self.chrome_bar(back.into(), (!portrait).then(|| source.into()));

        let title = row![
            text(&track.title).size(40).font(theme::DISPLAY_L).color(PAPER),
            icon_button(if liked { Icon::HeartFill } else { Icon::Heart }, 22.0, 36.0, if liked { AMBER } else { HAZE }, Some(Message::ToggleLike(t))),
        ]
        .spacing(16)
        .align_y(Alignment::Center);
        let words = column![
            Column::new()
                .push(title)
                .push(label(if track.album_title.is_empty() { track.artist.clone() } else { format!("{}, from {}", track.artist, track.album_title) }, 14.0, theme::BODY, alpha(PAPER, 0.7)))
                .extend(self.playback_status().map(|s| column![Space::new().height(4), s].into()))
                .spacing(8),
            self.lyrics_view(),
        ]
        .spacing(36);

        let h = self.size.height;
        let w = self.size.width;
        let stage: El = if portrait {
            let side = (w - 96.0).min(h * 0.4).max(200.0);
            column![container(self.pulsing_cover(&track.cover, side)).center_x(Length::Fill), words].spacing(40).padding([10, 48]).into()
        } else {
            let side = (h - 360.0).min(w * 0.36).clamp(220.0, 560.0);
            container(row![self.pulsing_cover(&track.cover, side), words].spacing(72).align_y(Alignment::Center))
                .padding([0, 88])
                .center_y(Length::Fill)
                .into()
        };

        let device = if self.audio.is_silent() { "No sound device: playing silently" } else { "System output" };
        let dock = column![
            row![
                container(if self.spin().is_some() {
                    label("Getting song…", 14.0, theme::STRONG, AMBER)
                } else {
                    label(format!("{} of {}", format_time(self.position), format_time(self.current_duration())), 14.0, theme::STRONG, PAPER)
                })
                    .width(Length::FillPortion(1)),
                self.controls(true),
                container(
                    row![
                        label(device, 12.5, theme::BODY, HAZE),
                        icon_button(if self.muted || self.volume == 0.0 { Icon::Mute } else { Icon::Volume }, 18.0, 32.0, HAZE, Some(Message::ToggleMute)),
                        slider(0.0..=1.0, if self.muted { 0.0 } else { self.volume }, Message::Volume).step(0.01_f32).width(110).style(theme::volume),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                )
                .width(Length::FillPortion(1))
                .align_x(Horizontal::Right),
            ]
            .align_y(Alignment::Center),
            self.ribbon(lib, RibbonStyle::Marked),
        ]
        .spacing(12)
        .padding(Padding { top: 0.0, right: 48.0, bottom: 28.0, left: 48.0 });

        // Solid ink underneath: the sheet slides over the library and must hide it.
        let washed = container(column![header, container(stage).height(Length::Fill), dock])
            .width(Length::Fill)
            .height(Length::Fill)
            .style(theme::wash_strength(track.cover.color, 0.7, Radius::from(self.radius()), 0.5 + 0.22 * self.level));
        container(washed)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(theme::rounded(INK, self.radius()))
            .into()
    }

    /// The Now playing cover: breathes and glows with the music, with a loudness trail underneath.
    fn pulsing_cover<'a>(&'a self, art: &crate::library::Cover, side: f32) -> El<'a> {
        let l = self.level;
        let radius = (side * 0.03).max(3.0);
        let sharp = crate::library::Cover { image: art.hires.clone().or_else(|| art.image.clone()), ..art.clone() };
        let glowing = container(cover_art(&sharp, side)).style(theme::glow(art.color, l, radius));
        let trail = canvas(LevelTrail { levels: self.levels.iter().copied().collect(), color: art.color }).width(side).height(40);
        column![float(glowing).scale(1.0 + 0.03 * l), trail].spacing(26).align_x(Alignment::Center).into()
    }

    fn lyrics_view(&self) -> El<'_> {
        if self.lyrics_loading {
            return label("Looking for lyrics…", 26.0, theme::DISPLAY_M, alpha(PAPER, 0.35)).into();
        }
        let Some(lyrics) = &self.lyrics else {
            return column![
                label("No lyrics for this song", 26.0, theme::DISPLAY_M, alpha(PAPER, 0.4)),
                text("Musify looks on LRCLIB and YouTube Music, or reads an .lrc file saved next to a local song with the same name.")
                    .size(14)
                    .color(HAZE),
            ]
            .spacing(10)
            .max_width(520)
            .into();
        };
        let credit = |l: &crate::lyrics::Lyrics| -> El<'_> {
            match &l.source {
                Some(src) => label(format!("Lyrics from {src}"), 12.5, theme::BODY, alpha(HAZE, 0.8)).into(),
                None => Space::new().into(),
            }
        };
        if !lyrics.synced {
            // No timing: a calm page you can scroll, rather than a moving highlight.
            let mut page = Column::new().spacing(10);
            for (_, line) in &lyrics.lines {
                page = page.push(if line.is_empty() {
                    Element::from(Space::new().height(10))
                } else {
                    text(line).size(22).font(theme::DISPLAY_M).color(alpha(PAPER, 0.82)).into()
                });
            }
            return column![scrollable(page.padding(Padding { right: 16.0, ..Padding::ZERO })).style(theme::scroll).height(Length::Fixed(360.0)), credit(lyrics)]
                .spacing(14)
                .into();
        }
        let cur = lyrics.current(self.position);
        // f is the "focus" line, gliding from the previous line to the new one.
        let e = anim::ease_out(self.lyric_tween.linear(self.now));
        let to = cur.map(|c| c as f32).unwrap_or(-1.0);
        let from = self.lyric_from.map(|c| c as f32).unwrap_or(to - 1.0);
        let f = if (to - from).abs() <= 2.0 { anim::lerp(from, to, e) } else { to };
        let start = cur.unwrap_or(0).saturating_sub(2);
        // The window starts one line lower than before the change, so begin a line down and rise into place.
        let rise = ((to - f) * 46.0).clamp(0.0, 92.0);
        let mut col = Column::new().spacing(14).padding(Padding { top: rise, ..Padding::ZERO });
        for (i, (_, line)) in lyrics.lines.iter().enumerate().skip(start).take(7) {
            let d = i as f32 - f;
            let focus = (1.0 - d.abs()).clamp(0.0, 1.0);
            let a = if d < 0.0 { (0.3 + 0.7 * (1.0 + d)).clamp(0.3, 1.0) } else { (1.0 - 0.4 * d).max(0.22) };
            col = col.push(text(line).size(26.0 + 6.0 * focus).font(theme::DISPLAY_M).color(alpha(PAPER, a)));
        }
        column![container(col).height(Length::Fixed(360.0)).clip(true), credit(lyrics)].spacing(14).into()
    }
}

// ---------- small pieces ----------

fn collection_detail(c: &crate::library::Collection) -> String {
    use crate::library::Kind;
    match (c.kind, c.loaded) {
        (_, true) => songs(c.tracks.len()),
        (Kind::Liked, false) => "Your likes on YouTube".into(),
        (Kind::Mix, false) => "Mix".into(),
        _ => c.subtitle.split(" • ").last().unwrap_or("Playlist").to_string(),
    }
}

/// Cover tile with a title and one line under it.
fn tile<'a>(art: &crate::library::Cover, title: &str, sub: String, side: f32, msg: Message) -> El<'a> {
    button(
        column![
            cover(art, side),
            gap(4.0),
            label(title.to_string(), 15.0, theme::MEDIUM, PAPER),
            label(sub, 12.5, theme::BODY, HAZE),
        ]
        .spacing(6)
        .width(side)
        .clip(true),
    )
    .padding(0)
    .style(theme::plain)
    .on_press(msg)
    .into()
}

fn songs(n: usize) -> String {
    if n == 1 { "1 song".into() } else { format!("{n} songs") }
}

fn wordmark_disc<'a>(side: f32) -> El<'a> {
    let hole = side / 3.0;
    container(container(Space::new().width(hole).height(hole)).style(theme::rounded(SLATE, hole / 2.0)))
        .center(side)
        .style(theme::rounded(AMBER, side / 2.0))
        .into()
}

fn capsule_divider<'a>() -> El<'a> {
    container(Space::new().width(1).height(14)).style(theme::fill(alpha(PAPER, 0.1))).into()
}

/// Hover shapes for the end cells of a chrome capsule, matching its 15px corners.
const CAPSULE_LEFT: Radius = Radius { top_left: 15.0, bottom_left: 15.0, top_right: 0.0, bottom_right: 0.0 };
const CAPSULE_RIGHT: Radius = Radius { top_left: 0.0, bottom_left: 0.0, top_right: 15.0, bottom_right: 15.0 };

/// Minimize, maximize, close: one capsule (Figma component "Window controls").
fn window_controls<'a>() -> El<'a> {
    let cell = |i, close: bool, msg, radius: Radius| {
        let fill = if close { theme::ROSE } else { alpha(PAPER, 0.10) };
        hover(
            button(container(icon(i, 18.0, if close { PAPER } else { HAZE })).center_x(36).center_y(30))
                .padding(0)
                .style(theme::chrome_cell)
                .on_press(msg),
            fill,
            radius,
        )
    };
    container(
        row![
            cell(Icon::Minimize, false, Message::Minimize, CAPSULE_LEFT),
            capsule_divider(),
            cell(Icon::Maximize, false, Message::ToggleMaximize, Radius::default()),
            capsule_divider(),
            cell(Icon::Close, true, Message::Close, CAPSULE_RIGHT),
        ]
        .align_y(Alignment::Center),
    )
    .style(theme::capsule)
    .clip(true)
    .into()
}

fn compact_search(query: &str) -> El<'_> {
    container(
        row![
            icon(Icon::Search, 16.0, HAZE),
            text_input("Search", query).on_input(Message::Search).style(theme::search).size(13).padding(0).width(180),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .padding([0, 12])
    .center_y(30)
    .style(theme::capsule)
    .into()
}

fn spine<'a>(name: &'a str, detail: String, color: Color, selected: bool, msg: Message) -> El<'a> {
    hover(button(
        row![
            container(Space::new().width(5).height(36)).style(theme::rounded(color, 2.5)),
            column![label(name, 15.0, theme::MEDIUM, PAPER), label(detail, 12.5, theme::BODY, HAZE)].spacing(1).clip(true),
        ]
        .spacing(12)
        .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .padding([6, 10])
    .style(theme::spine(selected))
    .on_press(msg), alpha(RAISED, 0.5), 6.0)
    .into()
}

fn eq_bars<'a>(secs: f32, playing: bool) -> El<'a> {
    let t = secs * 5.5;
    let bar = |phase: f32, rest: f32| {
        let h = if playing { 4.0 + 12.0 * (t + phase).sin().abs() } else { rest };
        container(Space::new().width(3).height(h)).style(theme::rounded(AMBER, 1.5))
    };
    row![bar(0.0, 10.0), bar(1.7, 16.0), bar(3.1, 7.0)].spacing(2).align_y(Alignment::End).height(18).into()
}

fn cover_art<'a>(art: &crate::library::Cover, side: f32) -> El<'a> {
    let radius = (side * 0.03).max(3.0);
    match &art.image {
        Some(h) => image(h.clone()).width(side).height(side).content_fit(ContentFit::Cover).border_radius(radius).into(),
        None => canvas(CoverArt { color: art.color, seed: art.seed }).width(side).height(side).into(),
    }
}

fn cover<'a>(art: &crate::library::Cover, side: f32) -> El<'a> {
    let radius = (side * 0.03).max(3.0);
    let img = cover_art(art, side);
    if side >= 120.0 { container(img).style(theme::cover_shadow(radius)).into() } else { img }
}

/// Invisible 6px border that resizes the undecorated window.
fn resize_edges<'a>() -> El<'a> {
    const E: f32 = 6.0;
    const C: f32 = 12.0;
    let edge = |w: Length, h: Length, d: Direction, cursor: Interaction| -> El<'a> {
        mouse_area(Space::new().width(w).height(h)).interaction(cursor).on_press(Message::DragResize(d)).into()
    };
    use Interaction::*;
    use Length::{Fill, Fixed};
    column![
        row![
            edge(Fixed(C), Fixed(E), Direction::NorthWest, ResizingDiagonallyDown),
            edge(Fill, Fixed(E), Direction::North, ResizingVertically),
            edge(Fixed(C), Fixed(E), Direction::NorthEast, ResizingDiagonallyUp),
        ],
        row![
            edge(Fixed(E), Fill, Direction::West, ResizingHorizontally),
            Space::new().width(Fill).height(Fill),
            edge(Fixed(E), Fill, Direction::East, ResizingHorizontally),
        ]
        .height(Fill),
        row![
            edge(Fixed(C), Fixed(E), Direction::SouthWest, ResizingDiagonallyUp),
            edge(Fill, Fixed(E), Direction::South, ResizingVertically),
            edge(Fixed(C), Fixed(E), Direction::SouthEast, ResizingDiagonallyDown),
        ],
    ]
    .into()
}

