//! The element tree an application rebuilds from its state on every change.

use std::sync::Arc;

use katui_draw::{Paint, Rgba, TextSpan};
use katui_scene::surface::Surface;

/// How an element sizes along one axis.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Length {
    /// An exact size in pixels.
    Px(f32),
    /// All the space the parent offers.
    Fill,
    /// The size of the element's content.
    #[default]
    Content,
}

/// Where children sit along an axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Align {
    /// Against the start edge.
    #[default]
    Start,
    /// In the middle.
    Center,
    /// Against the end edge.
    End,
}

#[derive(Debug, Clone)]
pub(crate) struct Style {
    pub(crate) padding: (f32, f32),
    pub(crate) gap: f32,
    pub(crate) width: Length,
    pub(crate) height: Length,
    pub(crate) grow: f32,
    pub(crate) align: Align,
    pub(crate) justify: Align,
    pub(crate) background: Option<Paint>,
    pub(crate) radius: f32,
    pub(crate) border: Option<(f32, Paint)>,
    pub(crate) color: Rgba,
    pub(crate) font_size: f32,
    pub(crate) weight: u16,
    pub(crate) spans: Option<Arc<[TextSpan]>>,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            padding: (0.0, 0.0),
            gap: 0.0,
            width: Length::Content,
            height: Length::Content,
            grow: 0.0,
            align: Align::Start,
            justify: Align::Start,
            background: None,
            radius: 0.0,
            border: None,
            color: Rgba::solid(255, 255, 255),
            font_size: 16.0,
            weight: 400,
            spans: None,
        }
    }
}

#[derive(Clone)]
pub(crate) enum Kind {
    Row,
    Column,
    Stack,
    Text(Arc<str>),
    Image(Arc<Surface>),
    Spacer,
}

/// Where a scrolling element stands: its offset and the largest offset its
/// content allows, both in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Scroll {
    /// How far the content is scrolled.
    pub offset: f32,
    /// The furthest the content can scroll.
    pub max: f32,
}

/// A message built from where the pointer is, as fractions of a box.
pub(crate) type PressHandler<M> = Arc<dyn Fn(f32, f32) -> M>;

/// A message built from a new scroll position.
pub(crate) type ScrollHandler<M> = Arc<dyn Fn(Scroll) -> M>;

/// One node of the view: a container, text, an image or a spacer.
#[derive(Clone)]
pub struct Element<M> {
    pub(crate) kind: Kind,
    pub(crate) style: Style,
    pub(crate) children: Vec<Element<M>>,
    pub(crate) layer: Option<u64>,
    pub(crate) scroll: Option<f32>,
    pub(crate) reveal: Option<u64>,
    pub(crate) on_click: Option<M>,
    pub(crate) on_hover: Option<M>,
    pub(crate) span_clicks: Vec<(usize, M)>,
    pub(crate) id: Option<u64>,
    pub(crate) on_leave: Option<M>,
    pub(crate) on_scroll: Option<ScrollHandler<M>>,
    pub(crate) on_press: Option<(PressHandler<M>, bool)>,
    pub(crate) grip: bool,
    pub(crate) popup: Option<(f32, f32)>,
    pub(crate) opaque: bool,
    pub(crate) scrollbar: Option<Rgba>,
}

impl<M> Element<M> {
    fn new(kind: Kind, children: Vec<Self>) -> Self {
        Self {
            kind,
            style: Style::default(),
            children,
            layer: None,
            scroll: None,
            reveal: None,
            on_click: None,
            on_hover: None,
            span_clicks: Vec::new(),
            id: None,
            on_leave: None,
            on_scroll: None,
            on_press: None,
            grip: false,
            popup: None,
            opaque: false,
            scrollbar: None,
        }
    }

    /// Children side by side, left to right.
    pub fn row(children: impl IntoIterator<Item = Self>) -> Self {
        Self::new(Kind::Row, children.into_iter().collect())
    }

    /// Children stacked top to bottom.
    pub fn column(children: impl IntoIterator<Item = Self>) -> Self {
        Self::new(Kind::Column, children.into_iter().collect())
    }

    /// Children overlapping in one box, later children on top.
    pub fn stack(children: impl IntoIterator<Item = Self>) -> Self {
        Self::new(Kind::Stack, children.into_iter().collect())
    }

    /// Text wrapped to the width its parent offers.
    pub fn text(text: impl Into<Arc<str>>) -> Self {
        Self::new(Kind::Text(text.into()), Vec::new())
    }

    /// Text with its own look per stretch, wrapping as one run.
    #[must_use]
    pub fn rich(spans: Vec<TextSpan>) -> Self {
        let text: String = spans.iter().map(|span| span.text.as_str()).collect();
        let mut element = Self::new(Kind::Text(text.into()), Vec::new());
        element.style.spans = Some(spans.into());
        element
    }

    /// An image, at its natural size unless a width or height is set.
    /// Layers compare images by `Arc` identity, so an unchanged image keeps
    /// the same `Arc` between renders.
    #[must_use]
    pub fn image(surface: Arc<Surface>) -> Self {
        Self::new(Kind::Image(surface), Vec::new())
    }

    /// Empty space that grows to fill its parent's main axis.
    #[must_use]
    pub fn spacer() -> Self {
        let mut element = Self::new(Kind::Spacer, Vec::new());
        element.style.grow = 1.0;
        element
    }

    /// Marks this subtree as one retained image identified by `key`.
    /// Non-layer content of an enclosing layer draws under this layer. The
    /// first layer with a key in a tree wins; a later duplicate draws
    /// inside its enclosing layer.
    #[must_use]
    pub fn layer(mut self, key: u64) -> Self {
        self.layer = Some(key);
        self
    }

    /// Clips children to this box and offsets them by `offset_px` along
    /// the main axis, a positive offset moving content toward the start.
    #[must_use]
    pub fn scroll(mut self, offset_px: f32) -> Self {
        self.scroll = Some(offset_px);
        self
    }

    /// Scroll this scrolling element so its descendant with [`Self::id`]
    /// `id` starts at the top, as far as the content allows. The handler
    /// from [`Self::on_scroll`] receives the new position, so the app
    /// stores it there and stops asking.
    #[must_use]
    pub fn reveal(mut self, id: u64) -> Self {
        self.reveal = Some(id);
        self
    }

    /// The message [`crate::Hit`] carries for a click on this element.
    #[must_use]
    pub fn on_click(mut self, message: M) -> Self {
        self.on_click = Some(message);
        self
    }

    /// The message [`crate::Hit`] carries for a click on the glyphs of span
    /// `span` of a rich text, such as a link inside a paragraph.
    #[must_use]
    pub fn on_span_click(mut self, span: usize, message: M) -> Self {
        self.span_clicks.push((span, message));
        self
    }

    /// The message [`crate::Hit`] carries for a hover over this element.
    #[must_use]
    pub fn on_hover(mut self, message: M) -> Self {
        self.on_hover = Some(message);
        self
    }

    /// The message sent when the pointer leaves this element after
    /// hovering it.
    #[must_use]
    pub fn on_leave(mut self, message: M) -> Self {
        self.on_leave = Some(message);
        self
    }

    /// The message for a new scroll position of a [`Self::scroll`]
    /// element. The view clamps the offset to the content, turns the
    /// mouse wheel into new positions, and draws a scrollbar the pointer
    /// can drag.
    #[must_use]
    pub fn on_scroll(mut self, message: impl Fn(Scroll) -> M + 'static) -> Self {
        self.on_scroll = Some(Arc::new(message));
        self
    }

    /// The message for a press at fractions `(x, y)` of this box, from 0.0
    /// at the top left to 1.0 at the bottom right. Dragging after the
    /// press keeps sending messages, with the fractions clamped to the box.
    #[must_use]
    pub fn on_press(mut self, message: impl Fn(f32, f32) -> M + 'static) -> Self {
        self.on_press = Some((Arc::new(message), false));
        self
    }

    /// Like [`Self::on_press`], but only a press on a descendant marked
    /// [`Self::grip`] starts it, such as a divider between two panes.
    #[must_use]
    pub fn on_grip_press(mut self, message: impl Fn(f32, f32) -> M + 'static) -> Self {
        self.on_press = Some((Arc::new(message), true));
        self
    }

    /// Marks the part of an [`Self::on_grip_press`] ancestor that starts
    /// its drag.
    #[must_use]
    pub fn grip(mut self) -> Self {
        self.grip = true;
        self
    }

    /// Takes this element out of the layout and draws it above everything
    /// else, at `x`, `y` pixels from its parent's top left. Its size comes
    /// from its content, or from the parent's edges for a [`Length::Fill`]
    /// side. Popups, menus and dialogs use it.
    #[must_use]
    pub fn popup(mut self, x: f32, y: f32) -> Self {
        self.popup = Some((x, y));
        self
    }

    /// Stops clicks on this box from reaching what lies under it or the
    /// handlers of its ancestors.
    #[must_use]
    pub fn opaque(mut self) -> Self {
        self.opaque = true;
        self
    }

    /// The color of the scrollbar a scrolling element draws.
    #[must_use]
    pub fn scrollbar(mut self, color: Rgba) -> Self {
        self.scrollbar = Some(color);
        self
    }

    /// An application id that [`crate::Hit`] reports for this element.
    #[must_use]
    pub fn id(mut self, id: u64) -> Self {
        self.id = Some(id);
        self
    }

    /// Equal padding on all four sides.
    #[must_use]
    pub fn padding(self, all: f32) -> Self {
        self.padding_xy(all, all)
    }

    /// Padding of `x` left and right and `y` top and bottom.
    #[must_use]
    pub fn padding_xy(mut self, x: f32, y: f32) -> Self {
        self.style.padding = (x, y);
        self
    }

    /// Space between children.
    #[must_use]
    pub fn gap(mut self, gap: f32) -> Self {
        self.style.gap = gap;
        self
    }

    /// The width, [`Length::Content`] by default.
    #[must_use]
    pub fn width(mut self, width: Length) -> Self {
        self.style.width = width;
        self
    }

    /// The height, [`Length::Content`] by default.
    #[must_use]
    pub fn height(mut self, height: Length) -> Self {
        self.style.height = height;
        self
    }

    #[must_use]
    pub fn size(self, width: Length, height: Length) -> Self {
        self.width(width).height(height)
    }

    /// The share of the parent's free space this element takes.
    #[must_use]
    pub fn grow(mut self, grow: f32) -> Self {
        self.style.grow = grow;
        self
    }

    /// Child placement across the main axis; vertical in a stack.
    #[must_use]
    pub fn align(mut self, align: Align) -> Self {
        self.style.align = align;
        self
    }

    /// Child placement along the main axis; horizontal in a stack.
    #[must_use]
    pub fn justify(mut self, justify: Align) -> Self {
        self.style.justify = justify;
        self
    }

    /// The background; gradient points are relative to the element's
    /// top-left corner.
    #[must_use]
    pub fn background(mut self, paint: Paint) -> Self {
        self.style.background = Some(paint);
        self
    }

    /// The corner radius of the background, border and image.
    #[must_use]
    pub fn radius(mut self, radius: f32) -> Self {
        self.style.radius = radius;
        self
    }

    /// A border of `width` just inside the box. The border insets the
    /// content by `width`.
    #[must_use]
    pub fn border(mut self, width: f32, paint: Paint) -> Self {
        self.style.border = Some((width, paint));
        self
    }

    /// The text color, white by default.
    #[must_use]
    pub fn color(mut self, color: Rgba) -> Self {
        self.style.color = color;
        self
    }

    /// The text size in pixels.
    #[must_use]
    pub fn font_size(mut self, size: f32) -> Self {
        self.style.font_size = size;
        self
    }

    /// The text weight, 400 regular and 700 bold.
    #[must_use]
    pub fn weight(mut self, weight: u16) -> Self {
        self.style.weight = weight;
        self
    }
}
