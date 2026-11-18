//! Motion: small tweens driven by the frame clock, and a hover-fade wrapper widget.

use std::time::{Duration, Instant};

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::{Operation, Tree, tree};
use iced::advanced::{Clipboard, Shell, Widget, overlay};
use iced::border::Radius;
use iced::{Border, Color, Element, Event, Length, Rectangle, Size, Vector, mouse, window};

pub fn ease_out(t: f32) -> f32 {
    1.0 - (1.0 - t.clamp(0.0, 1.0)).powi(3)
}

pub fn ease_in_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 { 4.0 * t * t * t } else { 1.0 - (-2.0 * t + 2.0).powi(3) / 2.0 }
}

pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

pub fn mix(a: Color, b: Color, t: f32) -> Color {
    Color { r: lerp(a.r, b.r, t), g: lerp(a.g, b.g, t), b: lerp(a.b, b.b, t), a: lerp(a.a, b.a, t) }
}

/// A one-shot 0 → 1 timeline.
#[derive(Debug, Clone, Copy)]
pub struct Tween {
    start: Instant,
    duration: Duration,
}

impl Tween {
    pub fn new(start: Instant, millis: u64) -> Self {
        Self { start, duration: Duration::from_millis(millis) }
    }

    /// A tween that has already finished.
    pub fn done() -> Self {
        Self { start: Instant::now() - Duration::from_secs(60), duration: Duration::from_millis(1) }
    }

    pub fn linear(&self, now: Instant) -> f32 {
        (now.saturating_duration_since(self.start).as_secs_f32() / self.duration.as_secs_f32()).clamp(0.0, 1.0)
    }

    pub fn eased(&self, now: Instant) -> f32 {
        ease_out(self.linear(now))
    }

    pub fn running(&self, now: Instant) -> bool {
        self.linear(now) < 1.0
    }
}

// ---------- hover fade ----------

/// Fades a rounded background in under its content on hover, instead of snapping.
pub struct HoverFade<'a, M> {
    content: Element<'a, M>,
    color: Color,
    radius: Radius,
}

pub fn hover<'a, M>(content: impl Into<Element<'a, M>>, color: Color, radius: impl Into<Radius>) -> HoverFade<'a, M> {
    HoverFade { content: content.into(), color, radius: radius.into() }
}

#[derive(Default)]
struct State {
    hovered: bool,
    from: f32,
    changed: Option<Instant>,
}

const FADE_IN: f32 = 0.12;
const FADE_OUT: f32 = 0.24;

impl State {
    fn level(&self, now: Instant) -> f32 {
        let Some(at) = self.changed else { return if self.hovered { 1.0 } else { 0.0 } };
        let secs = now.saturating_duration_since(at).as_secs_f32();
        if self.hovered {
            lerp(self.from, 1.0, ease_out(secs / FADE_IN))
        } else {
            lerp(self.from, 0.0, ease_out(secs / FADE_OUT))
        }
    }

    fn animating(&self, now: Instant) -> bool {
        let l = self.level(now);
        if self.hovered { l < 1.0 } else { l > 0.0 }
    }
}

impl<M> Widget<M, iced::Theme, iced::Renderer> for HoverFade<'_, M> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
    }

    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &iced::Renderer, limits: &layout::Limits) -> layout::Node {
        self.content.as_widget_mut().layout(&mut tree.children[0], renderer, limits)
    }

    fn operate(&mut self, tree: &mut Tree, layout: Layout<'_>, renderer: &iced::Renderer, operation: &mut dyn Operation) {
        self.content.as_widget_mut().operate(&mut tree.children[0], layout, renderer, operation);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, M>,
        viewport: &Rectangle,
    ) {
        let now = Instant::now();
        let state = tree.state.downcast_mut::<State>();
        let over = cursor.is_over(layout.bounds());
        if over != state.hovered {
            state.from = state.level(now);
            state.hovered = over;
            state.changed = Some(now);
            shell.request_redraw();
        } else if let Event::Window(window::Event::RedrawRequested(at)) = event {
            if state.animating(*at) {
                shell.request_redraw();
            }
        }
        self.content.as_widget_mut().update(&mut tree.children[0], event, layout, cursor, renderer, clipboard, shell, viewport);
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(&tree.children[0], layout, cursor, viewport, renderer)
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        theme: &iced::Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        use iced::advanced::Renderer as _;
        let level = tree.state.downcast_ref::<State>().level(Instant::now());
        if level > 0.001 {
            renderer.fill_quad(
                renderer::Quad {
                    bounds: layout.bounds(),
                    border: Border { radius: self.radius, ..Default::default() },
                    ..Default::default()
                },
                Color { a: self.color.a * level, ..self.color },
            );
        }
        self.content.as_widget().draw(&tree.children[0], renderer, theme, style, layout, cursor, viewport);
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, M, iced::Theme, iced::Renderer>> {
        self.content.as_widget_mut().overlay(&mut tree.children[0], layout, renderer, viewport, translation)
    }
}

impl<'a, M: 'a> From<HoverFade<'a, M>> for Element<'a, M> {
    fn from(h: HoverFade<'a, M>) -> Self {
        Element::new(h)
    }
}

