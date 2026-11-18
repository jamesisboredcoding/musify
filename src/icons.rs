//! Icon set from the Figma design. Drawn as SVG and tinted at render time.

use iced::widget::svg;
use iced::{Color, Element};

use crate::theme;

#[derive(Debug, Clone, Copy)]
pub enum Icon {
    Play,
    Pause,
    Prev,
    Next,
    Shuffle,
    Repeat,
    Heart,
    HeartFill,
    Search,
    Home,
    Library,
    Volume,
    Mute,
    Lyrics,
    Expand,
    Down,
    Left,
    Right,
    Minimize,
    Maximize,
    Close,
}

fn body(icon: Icon) -> &'static str {
    match icon {
        Icon::Play => r##"<path d="M7 4.6v14.8a.7.7 0 0 0 1.05.6l12-7.4a.7.7 0 0 0 0-1.2l-12-7.4A.7.7 0 0 0 7 4.6z" fill="#000" stroke="none"/>"##,
        Icon::Pause => r##"<rect x="6" y="4.5" width="4" height="15" rx="1.2" fill="#000" stroke="none"/><rect x="14" y="4.5" width="4" height="15" rx="1.2" fill="#000" stroke="none"/>"##,
        Icon::Prev => r##"<path d="M19 5.5v13a.6.6 0 0 1-.95.5L9 12.5a.6.6 0 0 1 0-1l9.05-6.5a.6.6 0 0 1 .95.5z" fill="#000" stroke="none"/><rect x="5" y="5" width="2.4" height="14" rx="1.2" fill="#000" stroke="none"/>"##,
        Icon::Next => r##"<path d="M5 5.5v13a.6.6 0 0 0 .95.5L15 12.5a.6.6 0 0 0 0-1L5.95 5A.6.6 0 0 0 5 5.5z" fill="#000" stroke="none"/><rect x="16.6" y="5" width="2.4" height="14" rx="1.2" fill="#000" stroke="none"/>"##,
        Icon::Shuffle => r##"<path d="M2 18h1.4c1.3 0 2.5-.6 3.3-1.7l6.1-8.6c.7-1.1 2-1.7 3.3-1.7H22"/><path d="m18 2 4 4-4 4"/><path d="M2 6h1.9c1.5 0 2.9.9 3.6 2.2"/><path d="M22 18h-5.9c-1.3 0-2.6-.7-3.3-1.8l-.5-.8"/><path d="m18 14 4 4-4 4"/>"##,
        Icon::Repeat => r##"<path d="m17 2 4 4-4 4"/><path d="M3 11v-1a4 4 0 0 1 4-4h14"/><path d="m7 22-4-4 4-4"/><path d="M21 13v1a4 4 0 0 1-4 4H3"/>"##,
        Icon::Heart => r##"<path d="M19 14c1.49-1.46 3-3.21 3-5.5A5.5 5.5 0 0 0 16.5 3c-1.76 0-3 .5-4.5 2-1.5-1.5-2.74-2-4.5-2A5.5 5.5 0 0 0 2 8.5c0 2.3 1.5 4.05 3 5.5l7 7Z"/>"##,
        Icon::HeartFill => r##"<path d="M19 14c1.49-1.46 3-3.21 3-5.5A5.5 5.5 0 0 0 16.5 3c-1.76 0-3 .5-4.5 2-1.5-1.5-2.74-2-4.5-2A5.5 5.5 0 0 0 2 8.5c0 2.3 1.5 4.05 3 5.5l7 7Z" fill="#000"/>"##,
        Icon::Search => r##"<circle cx="11" cy="11" r="7"/><path d="m21 21-4.3-4.3"/>"##,
        Icon::Home => r##"<path d="M3 10.5 12 3l9 7.5V20a1 1 0 0 1-1 1h-5v-6h-6v6H4a1 1 0 0 1-1-1z"/>"##,
        Icon::Library => r##"<path d="M5 4v16M10 4v16M14.5 4.5l4.5 15.5"/>"##,
        Icon::Volume => r##"<path d="M11 5 6 9H2v6h4l5 4z"/><path d="M15.5 8.5a5 5 0 0 1 0 7"/><path d="M19 5a10 10 0 0 1 0 14"/>"##,
        Icon::Mute => r##"<path d="M11 5 6 9H2v6h4l5 4z"/><path d="m22 9-6 6M16 9l6 6"/>"##,
        Icon::Lyrics => r##"<path d="M4 6h16M4 12h10M4 18h7"/>"##,
        Icon::Expand => r##"<path d="M15 3h6v6"/><path d="M9 21H3v-6"/><path d="m21 3-7 7"/><path d="m3 21 7-7"/>"##,
        Icon::Down => r##"<path d="m6 9 6 6 6-6"/>"##,
        Icon::Left => r##"<path d="m14 7-5 5 5 5"/>"##,
        Icon::Right => r##"<path d="m10 7 5 5-5 5"/>"##,
        Icon::Minimize => r##"<path d="M7 12h10"/>"##,
        Icon::Maximize => r##"<path d="M8 10V8h2M16 14v2h-2M14 8h2v2M10 16H8v-2"/>"##,
        Icon::Close => r##"<path d="m8 8 8 8M16 8l-8 8"/>"##,
    }
}

fn handle(icon: Icon) -> svg::Handle {
    let src = format!(
        r##"<svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" xmlns="http://www.w3.org/2000/svg">{}</svg>"##,
        body(icon)
    );
    svg::Handle::from_memory(src.into_bytes())
}

pub fn icon<'a, M: 'a>(icon: Icon, size: f32, color: Color) -> Element<'a, M> {
    svg(handle(icon)).width(size).height(size).style(theme::tint(color)).into()
}
