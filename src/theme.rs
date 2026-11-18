//! Musify's design tokens and widget styles, mirrored from the Figma file
//! (collection "Musify", mode "Dusk").

use iced::font::{Family, Weight};
use iced::widget::{button, container, scrollable, slider, svg, text_input};
use iced::{Background, Border, Color, Font, Shadow, Theme, Vector};

pub const fn rgb(hex: u32) -> Color {
    Color {
        r: ((hex >> 16) & 0xff) as f32 / 255.0,
        g: ((hex >> 8) & 0xff) as f32 / 255.0,
        b: (hex & 0xff) as f32 / 255.0,
        a: 1.0,
    }
}

pub const INK: Color = rgb(0x17162A);
pub const SLATE: Color = rgb(0x1F1E35);
pub const RAISED: Color = rgb(0x2A2845);
pub const LINE: Color = rgb(0x35334F);
pub const PAPER: Color = rgb(0xEEE6D8);
pub const HAZE: Color = rgb(0x9590B0);
pub const AMBER: Color = rgb(0xF4B03E);
pub const AMBER_INK: Color = rgb(0x2A1D05);
pub const ROSE: Color = rgb(0xE07A8F);

/// Fallback album colors when a cover has no art to sample.
pub const ALBUM_COLORS: [Color; 8] = [
    rgb(0x3E8E8A),
    rgb(0xC4607A),
    rgb(0xC9963A),
    rgb(0x6B5BC2),
    rgb(0x4A78B8),
    rgb(0x6E8B4E),
    rgb(0xA0479A),
    rgb(0x8F8AAD),
];

pub fn alpha(c: Color, a: f32) -> Color {
    Color { a: c.a * a, ..c }
}

const fn font(family: &'static str, weight: Weight) -> Font {
    Font {
        family: Family::Name(family),
        weight,
        stretch: iced::font::Stretch::Normal,
        style: iced::font::Style::Normal,
    }
}

pub const BODY: Font = font("Onest", Weight::Normal);
pub const MEDIUM: Font = font("Onest", Weight::Medium);
pub const STRONG: Font = font("Onest", Weight::Semibold);
pub const DISPLAY_M: Font = font("Bricolage Grotesque", Weight::Semibold);
pub const DISPLAY_L: Font = font("Bricolage Grotesque", Weight::Bold);
pub const DISPLAY_XL: Font = font("Bricolage Grotesque", Weight::ExtraBold);

pub const FONT_FILES: [&[u8]; 6] = [
    include_bytes!("../assets/fonts/onest-400.ttf"),
    include_bytes!("../assets/fonts/onest-500.ttf"),
    include_bytes!("../assets/fonts/onest-600.ttf"),
    include_bytes!("../assets/fonts/bricolage-600.ttf"),
    include_bytes!("../assets/fonts/bricolage-700.ttf"),
    include_bytes!("../assets/fonts/bricolage-800.ttf"),
];

pub fn theme() -> Theme {
    Theme::custom(
        "Musify Dusk",
        iced::theme::Palette {
            background: INK,
            text: PAPER,
            primary: AMBER,
            success: rgb(0x6E8B4E),
            warning: AMBER,
            danger: ROSE,
        },
    )
}

// ---------- containers ----------

pub fn window(radius: f32) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(INK.into()),
        border: Border { radius: radius.into(), ..Default::default() },
        text_color: Some(PAPER),
        ..Default::default()
    }
}

pub fn fill(color: Color) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style { background: Some(color.into()), ..Default::default() }
}

pub fn rounded(color: Color, radius: f32) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(color.into()),
        border: Border { radius: radius.into(), ..Default::default() },
        ..Default::default()
    }
}

/// The part of `wash(tint, 0.45, ..)` that sits below the chrome bar, at strength `k`.
/// Used to fade page content without a visible box.
pub fn veil(tint: Color, k: f32, area: f32, top: f32) -> impl Fn(&Theme) -> container::Style {
    move |_| {
        let reach = 0.45 * area;
        let start = 0.55 * (1.0 - top / reach).max(0.0);
        let end = ((reach - top) / (area - top)).clamp(0.01, 1.0);
        let g = iced::gradient::Linear::new(iced::Radians(std::f32::consts::PI))
            .add_stop(0.0, alpha(tint, start * k))
            .add_stop(end, alpha(tint, 0.0));
        container::Style { background: Some(Background::Gradient(g.into())), ..Default::default() }
    }
}

/// Cover glow on Now playing: grows and warms with the music's loudness.
pub fn glow(color: Color, level: f32, radius: f32) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        border: Border { radius: radius.into(), ..Default::default() },
        shadow: Shadow {
            color: Color { a: 0.3 + 0.4 * level, ..crate::anim::mix(Color::BLACK, color, 0.35 + 0.5 * level) },
            offset: Vector::new(0.0, 18.0),
            blur_radius: 48.0 + 90.0 * level,
        },
        ..Default::default()
    }
}

pub fn capsule(_: &Theme) -> container::Style {
    container::Style {
        background: Some(alpha(INK, 0.55).into()),
        border: Border { radius: 15.0.into(), width: 1.0, color: alpha(PAPER, 0.08) },
        ..Default::default()
    }
}

pub fn cover_shadow(radius: f32) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        border: Border { radius: radius.into(), ..Default::default() },
        shadow: Shadow {
            color: Color { a: 0.4, ..Color::BLACK },
            offset: Vector::new(0.0, 16.0),
            blur_radius: 40.0,
        },
        ..Default::default()
    }
}

/// Cover-tinted wash behind the top of a page.
pub fn wash(tint: Color, reach: f32, radius: iced::border::Radius) -> impl Fn(&Theme) -> container::Style {
    wash_strength(tint, reach, radius, 0.55)
}

pub fn wash_strength(tint: Color, reach: f32, radius: iced::border::Radius, strength: f32) -> impl Fn(&Theme) -> container::Style {
    move |_| {
        let g = iced::gradient::Linear::new(iced::Radians(std::f32::consts::PI))
            .add_stop(0.0, alpha(tint, strength))
            .add_stop(reach, alpha(INK, 0.0));
        container::Style {
            background: Some(Background::Gradient(g.into())),
            border: Border { radius, ..Default::default() },
            ..Default::default()
        }
    }
}

// ---------- buttons ----------

fn btn(bg: Option<Color>, radius: f32) -> button::Style {
    button::Style {
        background: bg.map(Background::Color),
        text_color: PAPER,
        border: Border { radius: radius.into(), ..Default::default() },
        ..Default::default()
    }
}

/// Round icon button. The hover fill comes from `anim::hover`, so it fades instead of snapping.
pub fn ghost(radius: f32) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, s| match s {
        button::Status::Pressed => btn(Some(alpha(PAPER, 0.06)), radius),
        _ => btn(None, radius),
    }
}

pub fn amber(radius: f32) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, s| {
        let c = match s {
            button::Status::Hovered => rgb(0xF7C267),
            button::Status::Pressed => rgb(0xE09E2E),
            _ => AMBER,
        };
        button::Style { text_color: AMBER_INK, ..btn(Some(c), radius) }
    }
}

/// List rows and nav items; `selected` keeps the raised fill.
pub fn row(selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, s| {
        let bg = match (selected, s) {
            (true, _) => Some(RAISED),
            (false, button::Status::Pressed) => Some(alpha(RAISED, 0.4)),
            _ => None,
        };
        btn(bg, 8.0)
    }
}

pub fn spine(selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, s| {
        let bg = match (selected, s) {
            (true, _) => Some(alpha(RAISED, 0.8)),
            (false, button::Status::Pressed) => Some(alpha(RAISED, 0.35)),
            _ => None,
        };
        btn(bg, 6.0)
    }
}

/// A cell inside the window-controls capsule. Hover fill fades in via `anim::hover`.
pub fn chrome_cell(_: &Theme, _: button::Status) -> button::Style {
    btn(None, 0.0)
}

/// A container with only some corners rounded, for panels that meet the window edge.
pub fn corners(color: Color, radius: iced::border::Radius) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(color.into()),
        border: Border { radius, ..Default::default() },
        ..Default::default()
    }
}

pub fn pill(_: &Theme, s: button::Status) -> button::Style {
    let hovered = matches!(s, button::Status::Hovered | button::Status::Pressed);
    button::Style {
        background: Some(if hovered { alpha(RAISED, 0.9) } else { alpha(INK, 0.55) }.into()),
        text_color: PAPER,
        border: Border { radius: 15.0.into(), width: 1.0, color: alpha(PAPER, 0.08) },
        ..Default::default()
    }
}

pub fn plain(_: &Theme, _: button::Status) -> button::Style {
    btn(None, 0.0)
}

// ---------- inputs ----------

pub fn search(_: &Theme, _: text_input::Status) -> text_input::Style {
    text_input::Style {
        background: Background::Color(Color::TRANSPARENT),
        border: Border::default(),
        icon: HAZE,
        placeholder: HAZE,
        value: PAPER,
        selection: alpha(AMBER, 0.35),
    }
}

pub fn volume(_: &Theme, s: slider::Status) -> slider::Style {
    let hovered = matches!(s, slider::Status::Hovered | slider::Status::Dragged);
    slider::Style {
        rail: slider::Rail {
            backgrounds: ((if hovered { AMBER } else { PAPER }).into(), LINE.into()),
            width: 4.0,
            border: Border { radius: 2.0.into(), ..Default::default() },
        },
        handle: slider::Handle {
            shape: slider::HandleShape::Circle { radius: if hovered { 6.0 } else { 0.0 } },
            background: PAPER.into(),
            border_width: 0.0,
            border_color: Color::TRANSPARENT,
        },
    }
}

pub fn scroll(_: &Theme, _: scrollable::Status) -> scrollable::Style {
    let rail = scrollable::Rail {
        background: None,
        border: Border::default(),
        scroller: scrollable::Scroller {
            background: alpha(HAZE, 0.25).into(),
            border: Border { radius: 4.0.into(), ..Default::default() },
        },
    };
    scrollable::Style {
        container: container::Style::default(),
        vertical_rail: rail,
        horizontal_rail: rail,
        gap: None,
        auto_scroll: scrollable::AutoScroll {
            background: SLATE.into(),
            border: Border::default(),
            shadow: Shadow::default(),
            icon: PAPER,
        },
    }
}

pub fn tint(color: Color) -> impl Fn(&Theme, svg::Status) -> svg::Style {
    move |_, _| svg::Style { color: Some(color) }
}
