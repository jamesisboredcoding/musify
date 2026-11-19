//! Custom-drawn pieces: the queue ribbon and generated album covers.

use iced::alignment::Vertical;
use iced::border::Radius;
use iced::mouse;
use iced::widget::canvas::{self, Action, Event, Frame, Geometry, Path, Stroke, Text};
use iced::{Color, Pixels, Point, Rectangle, Renderer, Size, Theme};

use crate::theme::{self, alpha};

pub struct Segment {
    pub title: String,
    pub artist: String,
    pub secs: f32,
    pub color: Color,
    /// Label under the segment, e.g. "in 5 min".
    pub mark: String,
    /// 1 normally; shrinks to 0 for a song leaving the front of the queue.
    pub weight: f32,
}

/// The queue drawn as one strip: each song a segment sized by its length and
/// tinted by its cover, with an amber needle at the current position.
pub struct Ribbon<M> {
    pub segments: Vec<Segment>,
    /// Index of the playing segment; anything before it is on its way out.
    pub current: usize,
    /// Seconds played of the current segment.
    pub played: f32,
    pub style: RibbonStyle,
    /// 0 → 1 as the ribbon grows in when playback starts.
    pub reveal: f32,
    pub on_click: fn(usize, f32) -> M,
}

#[derive(Clone, Copy, PartialEq)]
pub enum RibbonStyle {
    /// Thin bar in the landscape player bar: titles only.
    Compact,
    /// Title and artist in each segment.
    Labelled,
    /// Labelled, plus "in N min" marks under each segment.
    Marked,
}

const GAP: f32 = 3.0;
const PAD: f32 = 8.0;

impl<M> Ribbon<M> {
    pub fn height(style: RibbonStyle) -> f32 {
        match style {
            RibbonStyle::Compact => 22.0 + PAD * 2.0,
            RibbonStyle::Labelled => 52.0 + PAD * 2.0,
            RibbonStyle::Marked => 68.0 + PAD * 2.0 + 20.0,
        }
    }

    fn seg_height(&self) -> f32 {
        match self.style {
            RibbonStyle::Compact => 22.0,
            RibbonStyle::Labelled => 52.0,
            RibbonStyle::Marked => 68.0,
        }
    }

    /// x and width of every segment.
    fn layout(&self, width: f32) -> Vec<(f32, f32)> {
        let size = |s: &Segment| s.secs.max(1.0) * s.weight;
        let total: f32 = self.segments.iter().map(size).sum();
        let gaps: f32 = self.segments.iter().map(|s| GAP * s.weight.min(1.0)).sum::<f32>() - GAP;
        let avail = width - gaps.max(0.0);
        let mut x = 0.0;
        self.segments
            .iter()
            .map(|s| {
                let w = if s.weight < 1.0 { avail * size(s) / total } else { (avail * size(s) / total).max(4.0) };
                let r = (x, w);
                x += w + GAP * s.weight.min(1.0);
                r
            })
            .collect()
    }

    fn hit(&self, bounds: Rectangle, cursor: mouse::Cursor) -> Option<(usize, f32)> {
        let p = cursor.position_in(bounds)?;
        if p.y < PAD - 4.0 || p.y > PAD + self.seg_height() + 4.0 {
            return None;
        }
        self.layout(bounds.width)
            .iter()
            .position(|&(x, w)| p.x >= x && p.x <= x + w + GAP)
            .map(|i| {
                let (x, w) = self.layout(bounds.width)[i];
                (i, ((p.x - x) / w).clamp(0.0, 1.0))
            })
    }
}

impl<M> canvas::Program<M> for Ribbon<M> {
    type State = ();

    fn update(&self, _: &mut (), event: &Event, bounds: Rectangle, cursor: mouse::Cursor) -> Option<Action<M>> {
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let (i, f) = self.hit(bounds, cursor)?;
                let offset = i.checked_sub(self.current)?;
                Some(Action::publish((self.on_click)(offset, f)).and_capture())
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) => Some(Action::request_redraw()),
            _ => None,
        }
    }

    fn draw(&self, _: &(), renderer: &Renderer, _: &Theme, bounds: Rectangle, cursor: mouse::Cursor) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let h = self.seg_height();
        let radius: f32 = if self.style == RibbonStyle::Compact { 4.0 } else { 8.0 };
        let hovered = self.hit(bounds, cursor).map(|(i, _)| i);
        let secs = self.reveal * 0.9;
        // Stagger fits the segment count so the last one finishes growing when the reveal ends.
        let step = (0.45 / self.segments.len().max(1) as f32).min(0.07);
        let reveal = self.reveal;
        let grow = move |i: usize| if reveal >= 1.0 { 1.0 } else { crate::anim::ease_out((secs - step * i as f32) / 0.45) };

        // Empty groove: visible when idle, and what the segments grow over.
        let idle = if self.segments.is_empty() { 1.0 } else { 1.0 - crate::anim::ease_out(secs / 0.5) };
        if idle > 0.0 {
            frame.fill(
                &Path::rounded_rectangle(Point::new(0.0, PAD), Size::new(bounds.width, h), radius.into()),
                alpha(theme::LINE, 0.55 * idle),
            );
        }

        for (i, ((x, full_w), seg)) in self.layout(bounds.width).into_iter().zip(&self.segments).enumerate() {
            let g = grow(i);
            let w = full_w * g;
            if w < 0.5 {
                continue;
            }
            if i < self.current {
                // Leaving: fully played, fading as it shrinks.
                frame.fill(
                    &Path::rounded_rectangle(Point::new(x, PAD), Size::new(w, h), radius.min(w / 2.0).into()),
                    alpha(seg.color, seg.weight),
                );
                continue;
            }
            let base = (if i == self.current { 0.4 } else { 0.24 } + if hovered == Some(i) { 0.12 } else { 0.0 }) * g;
            frame.fill(&Path::rounded_rectangle(Point::new(x, PAD), Size::new(w, h), radius.min(w / 2.0).into()), alpha(seg.color, base));

            if i == self.current {
                let pw = (w * self.played / seg.secs.max(1.0)).clamp(0.0, w);
                let r = Radius { top_left: radius, bottom_left: radius, top_right: 0.0, bottom_right: 0.0 };
                let r = if pw >= w - 0.5 { Radius::from(radius) } else { r };
                frame.fill(&Path::rounded_rectangle(Point::new(x, PAD), Size::new(pw, h), r), seg.color);
            }

            if w > 48.0 {
                // with_clip doesn't translate: coordinates below stay in canvas space.
                frame.with_clip(Rectangle::new(Point::new(x, PAD), Size::new(w - 8.0, h)), |f| {
                    let lead = (if i == self.current { 1.0 } else { 0.85 }) * g;
                    match self.style {
                        RibbonStyle::Compact => f.fill_text(Text {
                            content: seg.title.clone(),
                            position: Point::new(x + 8.0, PAD + h / 2.0),
                            color: alpha(theme::PAPER, lead * 0.85),
                            size: Pixels(12.5),
                            font: theme::BODY,
                            align_y: Vertical::Center,
                            ..Text::default()
                        }),
                        _ => {
                            let top = if self.style == RibbonStyle::Marked { 15.0 } else { 9.0 };
                            f.fill_text(Text {
                                content: seg.title.clone(),
                                position: Point::new(x + 11.0, PAD + top),
                                color: alpha(theme::PAPER, lead),
                                size: Pixels(14.0),
                                font: theme::STRONG,
                                ..Text::default()
                            });
                            f.fill_text(Text {
                                content: seg.artist.clone(),
                                position: Point::new(x + 11.0, PAD + top + 19.0),
                                color: alpha(theme::PAPER, lead * 0.65),
                                size: Pixels(12.5),
                                font: theme::BODY,
                                ..Text::default()
                            });
                        }
                    }
                });
            }

            if self.style == RibbonStyle::Marked && !seg.mark.is_empty() && w > 40.0 {
                frame.fill_text(Text {
                    content: seg.mark.clone(),
                    position: Point::new(x, PAD + h + 10.0),
                    color: alpha(if i == self.current { theme::AMBER } else { theme::HAZE }, g),
                    size: Pixels(12.5),
                    font: theme::BODY,
                    ..Text::default()
                });
            }
        }

        if let Some(seg) = self.segments.get(self.current) {
            let (x, w) = self.layout(bounds.width)[self.current];
            let px = x + (w * self.played / seg.secs.max(1.0)).clamp(0.0, w);
            let knob = if self.style == RibbonStyle::Compact { 5.0 } else { 7.0 };
            let needle = crate::anim::ease_out((secs - 0.3) / 0.35);
            frame.fill(
                &Path::rounded_rectangle(Point::new(px - 1.5, PAD - 6.0), Size::new(3.0, h + 12.0), 1.5.into()),
                alpha(theme::AMBER, needle),
            );
            frame.fill(&Path::circle(Point::new(px, (PAD - 6.0).max(knob) - (1.0 - needle) * 6.0), knob), alpha(theme::AMBER, needle));
        }

        vec![frame.into_geometry()]
    }

    fn mouse_interaction(&self, _: &(), bounds: Rectangle, cursor: mouse::Cursor) -> mouse::Interaction {
        if self.hit(bounds, cursor).is_some() { mouse::Interaction::Pointer } else { mouse::Interaction::default() }
    }
}

/// Generated cover for albums without art, in the style of the Figma mock covers.
pub struct CoverArt {
    pub color: Color,
    pub seed: u64,
}

impl<M> canvas::Program<M> for CoverArt {
    type State = ();

    fn draw(&self, _: &(), renderer: &Renderer, _: &Theme, bounds: Rectangle, _: mouse::Cursor) -> Vec<Geometry> {
        let s = bounds.width.min(bounds.height);
        let r = (s * 0.03).max(3.0);
        let mut f = Frame::new(renderer, bounds.size());
        let paper = theme::PAPER;
        let rect = |x: f32, y: f32, w: f32, h: f32, rad: f32| Path::rounded_rectangle(Point::new(x * s, y * s), Size::new(w * s, h * s), rad.into());

        match self.seed % 4 {
            0 => {
                f.fill(&rect(0.0, 0.0, 1.0, 1.0, r), self.color);
                f.fill(&Path::circle(Point::new(s * 0.5, s * 0.45), s * 0.25), alpha(paper, 0.92));
                let sea = Radius { top_left: 0.0, top_right: 0.0, bottom_left: r, bottom_right: r };
                f.fill(&Path::rounded_rectangle(Point::new(0.0, s * 0.58), Size::new(s, s * 0.42), sea), alpha(theme::INK, 0.6));
                for (x, y, w) in [(0.18, 0.68, 0.5), (0.3, 0.76, 0.32), (0.22, 0.85, 0.44), (0.38, 0.92, 0.16)] {
                    f.fill(&rect(x, y, w, 0.018, 2.0), alpha(paper, 0.5));
                }
            }
            1 => {
                f.fill(&rect(0.0, 0.0, 1.0, 1.0, r), theme::rgb(0x1B1A2C));
                for (i, w) in [0.62, 0.8, 0.4, 0.72, 0.52, 0.3].into_iter().enumerate() {
                    f.fill(&rect(0.1, 0.12 + i as f32 * 0.13, w, 0.09, s * 0.045), alpha(self.color, 1.0 - i as f32 * 0.1));
                }
            }
            2 => {
                f.fill(&rect(0.0, 0.0, 1.0, 1.0, r), self.color);
                let c = Point::new(s * 0.56, s * 0.56);
                for d in [0.8, 0.6, 0.4] {
                    f.stroke(&Path::circle(c, s * d / 2.0), Stroke::default().with_color(alpha(paper, 0.55)).with_width((s * 0.012).max(1.0)));
                }
                f.fill(&Path::circle(c, s * 0.11), alpha(paper, 0.9));
            }
            _ => {
                f.fill(&rect(0.0, 0.0, 1.0, 1.0, r), self.color);
                f.fill(&Path::circle(Point::new(s * 0.34, s * 0.66), s * 0.27), alpha(theme::INK, 0.7));
                f.fill(&rect(0.62, 0.16, 0.22, 0.22, 0.0), alpha(paper, 0.9));
            }
        }
        vec![f.into_geometry()]
    }
}

/// Startup animation. The queue ribbon assembles segment by segment, the needle
/// sweeps across lighting them up, and the wordmark settles in above it.
pub struct Splash {
    /// Seconds since launch.
    pub t: f32,
    /// 0 → 1 while the splash leaves.
    pub exit: f32,
    pub loading: bool,
}

impl Splash {
    /// When the intro has played through and the app may take over.
    pub const INTRO: f32 = 1.75;
    pub const EXIT_MS: u64 = 520;
}

impl<M> canvas::Program<M> for Splash {
    type State = ();

    fn draw(&self, _: &(), renderer: &Renderer, _: &Theme, bounds: Rectangle, _: mouse::Cursor) -> Vec<Geometry> {
        use crate::anim::{ease_in_out, ease_out};
        let mut f = Frame::new(renderer, bounds.size());
        let fade = 1.0 - ease_out(self.exit);
        let lift = -16.0 * ease_out(self.exit);
        f.fill_rectangle(Point::ORIGIN, bounds.size(), alpha(theme::INK, fade));

        let cx = bounds.width / 2.0;
        let cy = bounds.height / 2.0 + lift;
        let w = (bounds.width * 0.5).clamp(260.0, 520.0);
        let h = 18.0;
        let left = cx - w / 2.0;
        let top = cy + 34.0;

        // Segments sized like a real queue.
        let secs = [252.0, 185.0, 321.0, 214.0, 176.0, 240.0, 198.0];
        let total: f32 = secs.iter().sum();
        let avail = w - GAP * (secs.len() - 1) as f32;
        let sweep = ease_in_out((self.t - 0.55) / 1.0);
        let needle = left + w * sweep;
        let mut x = left;
        for (i, s) in secs.iter().enumerate() {
            let full = avail * s / total;
            let grow = ease_out((self.t - 0.08 * i as f32) / 0.45);
            let seg_w = full * grow;
            let color = theme::ALBUM_COLORS[i % theme::ALBUM_COLORS.len()];
            if seg_w > 0.5 {
                f.fill(&Path::rounded_rectangle(Point::new(x, top), Size::new(seg_w, h), 4.0.into()), alpha(color, 0.3 * grow * fade));
                let lit = (needle - x).clamp(0.0, seg_w);
                if lit > 0.5 {
                    let r = if lit >= seg_w - 0.5 {
                        Radius::from(4.0)
                    } else {
                        Radius { top_left: 4.0, bottom_left: 4.0, top_right: 0.0, bottom_right: 0.0 }
                    };
                    f.fill(&Path::rounded_rectangle(Point::new(x, top), Size::new(lit, h), r), alpha(color, fade));
                }
            }
            x += full + GAP;
        }

        // Needle: drops in, sweeps, then breathes while the library is still loading.
        let needle_in = ease_out((self.t - 0.45) / 0.3);
        if needle_in > 0.0 {
            let breathe = if self.loading && self.t > 1.6 { 0.6 + 0.4 * ((self.t - 1.6) * 4.0).cos() } else { 1.0 };
            let a = needle_in * fade * breathe;
            let drop = (1.0 - needle_in) * -10.0;
            f.fill(&Path::rounded_rectangle(Point::new(needle - 1.5, top - 7.0 + drop), Size::new(3.0, h + 14.0), 1.5.into()), alpha(theme::AMBER, a));
            f.fill(&Path::circle(Point::new(needle, top - 9.0 + drop), 6.0), alpha(theme::AMBER, a));
        }

        // Wordmark: spinning record + name, rising into place.
        let word = ease_out((self.t - 0.3) / 0.55);
        if word > 0.0 {
            let a = word * fade;
            let y = cy - 26.0 + (1.0 - word) * 12.0;
            let disc_r = 19.0;
            let name_w = 150.0;
            let gx = cx - (disc_r * 2.0 + 12.0 + name_w) / 2.0;
            let dc = Point::new(gx + disc_r, y);
            f.fill(&Path::circle(dc, disc_r), alpha(theme::AMBER, a));
            f.fill(&Path::circle(dc, disc_r * 0.33), alpha(theme::INK, a));
            // A glint on the rim so the spin reads.
            let angle = self.t * 5.0;
            let gr = disc_r * 0.68;
            f.fill(&Path::circle(Point::new(dc.x + gr * angle.cos(), dc.y + gr * angle.sin()), 2.6), alpha(theme::AMBER_INK, a * 0.55));
            f.fill_text(Text {
                content: "musify".into(),
                position: Point::new(gx + disc_r * 2.0 + 12.0, y),
                color: alpha(theme::PAPER, a),
                size: Pixels(46.0),
                font: theme::DISPLAY_M,
                align_y: Vertical::Center,
                ..Text::default()
            });
        }

        if self.loading && self.t > 1.6 {
            let a = ease_out((self.t - 1.6) / 0.4) * fade;
            f.fill_text(Text {
                content: "Reading your music…".into(),
                position: Point::new(cx, top + h + 26.0),
                color: alpha(theme::HAZE, a),
                size: Pixels(13.0),
                font: theme::BODY,
                align_x: iced::alignment::Horizontal::Center.into(),
                ..Text::default()
            });
        }

        vec![f.into_geometry()]
    }
}

/// Recent loudness as a row of bars, newest on the right. Sits under the Now playing cover.
pub struct LevelTrail {
    pub levels: Vec<f32>,
    pub color: Color,
}

impl<M> canvas::Program<M> for LevelTrail {
    type State = ();

    fn draw(&self, _: &(), renderer: &Renderer, _: &Theme, bounds: Rectangle, _: mouse::Cursor) -> Vec<Geometry> {
        let mut f = Frame::new(renderer, bounds.size());
        let n = self.levels.len().max(1);
        let gap = 3.0;
        let w = ((bounds.width - gap * (n - 1) as f32) / n as f32).max(1.0);
        for (i, &v) in self.levels.iter().enumerate() {
            let h = 3.0 + (bounds.height - 3.0) * v;
            let age = i as f32 / n as f32; // older bars fade
            let a = (0.18 + 0.82 * v) * (0.35 + 0.65 * age);
            f.fill(
                &Path::rounded_rectangle(Point::new(i as f32 * (w + gap), (bounds.height - h) / 2.0), Size::new(w, h), (w / 2.0).into()),
                alpha(crate::anim::mix(self.color, theme::PAPER, 0.35), a),
            );
        }
        vec![f.into_geometry()]
    }
}

/// A rotating arc for "getting the song": clearly busy, unlike a paused play button.
pub struct Spinner {
    /// Seconds since anything; only its fraction matters.
    pub t: f32,
    pub color: Color,
}

impl<M> canvas::Program<M> for Spinner {
    type State = ();

    fn draw(&self, _: &(), renderer: &Renderer, _: &Theme, bounds: Rectangle, _: mouse::Cursor) -> Vec<Geometry> {
        use iced::Radians;
        use std::f32::consts::TAU;
        let mut f = Frame::new(renderer, bounds.size());
        let width = (bounds.width * 0.11).max(2.0);
        let r = bounds.width.min(bounds.height) / 2.0 - width;
        let center = f.center();
        // The arc grows and shrinks while the whole thing turns, like a breathing loop.
        let turn = self.t * TAU * 0.9;
        let phase = (self.t * 1.3).fract();
        let sweep = 0.25 + 0.5 * (0.5 - 0.5 * (phase * TAU).cos());
        f.stroke(
            &Path::circle(center, r),
            Stroke::default().with_width(width).with_color(alpha(self.color, 0.18)),
        );
        let arc = Path::new(|p| {
            p.arc(canvas::path::Arc {
                center,
                radius: r,
                start_angle: Radians(turn),
                end_angle: Radians(turn + sweep * TAU),
            })
        });
        f.stroke(
            &arc,
            Stroke::default().with_width(width).with_color(self.color).with_line_cap(canvas::LineCap::Round),
        );
        vec![f.into_geometry()]
    }
}
