//! Turns a computed layout into per-layer draw lists and hit regions.

use std::collections::HashSet;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;

use katui_draw::{Paint, Point, Rect, Rgba};
use katui_scene::surface::Surface;
use taffy::NodeId;
use taffy::prelude::Size;

use crate::element::{Element, Kind, PressHandler, Scroll, ScrollHandler};
use crate::layout::{Laid, TextCache, TextKey, text_key};

/// A half-open box of terminal cells, negative while off screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CellBox {
    pub(crate) left: i32,
    pub(crate) top: i32,
    pub(crate) right: i32,
    pub(crate) bottom: i32,
}

impl CellBox {
    pub(crate) fn intersect(self, other: Self) -> Self {
        Self {
            left: self.left.max(other.left),
            top: self.top.max(other.top),
            right: self.right.min(other.right),
            bottom: self.bottom.min(other.bottom),
        }
    }

    pub(crate) fn is_empty(self) -> bool {
        self.left >= self.right || self.top >= self.bottom
    }

    fn inside(rect: Rect, cell: (f32, f32)) -> Self {
        Self {
            left: (rect.x / cell.0).ceil() as i32,
            top: (rect.y / cell.1).ceil() as i32,
            right: ((rect.x + rect.width) / cell.0).floor() as i32,
            bottom: ((rect.y + rect.height) / cell.1).floor() as i32,
        }
    }
}

/// One drawing step in layer-local pixels.
pub(crate) enum Op {
    Fill {
        rect: Rect,
        radius: f32,
        paint: Paint,
    },
    Stroke {
        rect: Rect,
        radius: f32,
        width: f32,
        paint: Paint,
    },
    Image {
        surface: Arc<Surface>,
        rect: Rect,
        radius: f32,
    },
    Text {
        key: TextKey,
        origin: Point,
        paint: Paint,
    },
    PushClip(Rect),
    PopClip,
}

fn hash_floats(hasher: &mut DefaultHasher, values: &[f32]) {
    for value in values {
        value.to_bits().hash(hasher);
    }
}

fn hash_rect(hasher: &mut DefaultHasher, rect: Rect) {
    hash_floats(hasher, &[rect.x, rect.y, rect.width, rect.height]);
}

fn hash_color(hasher: &mut DefaultHasher, color: Rgba) {
    [color.r, color.g, color.b, color.a].hash(hasher);
}

fn hash_paint(hasher: &mut DefaultHasher, paint: &Paint) {
    match paint {
        Paint::Solid(color) => hash_color(hasher, *color),
        Paint::Linear { from, to, stops } => {
            hash_floats(hasher, &[from.x, from.y, to.x, to.y]);
            for stop in stops {
                hash_floats(hasher, &[stop.offset]);
                hash_color(hasher, stop.color);
            }
        }
    }
}

impl Op {
    fn hash_into(&self, hasher: &mut DefaultHasher) {
        match self {
            Self::Fill {
                rect,
                radius,
                paint,
            } => {
                0_u8.hash(hasher);
                hash_rect(hasher, *rect);
                hash_floats(hasher, &[*radius]);
                hash_paint(hasher, paint);
            }
            Self::Stroke {
                rect,
                radius,
                width,
                paint,
            } => {
                1_u8.hash(hasher);
                hash_rect(hasher, *rect);
                hash_floats(hasher, &[*radius, *width]);
                hash_paint(hasher, paint);
            }
            Self::Image {
                surface,
                rect,
                radius,
            } => {
                2_u8.hash(hasher);
                Arc::as_ptr(surface).hash(hasher);
                hash_rect(hasher, *rect);
                hash_floats(hasher, &[*radius]);
            }
            Self::Text { key, origin, paint } => {
                3_u8.hash(hasher);
                key.hash(hasher);
                hash_floats(hasher, &[origin.x, origin.y]);
                hash_paint(hasher, paint);
            }
            Self::PushClip(rect) => {
                4_u8.hash(hasher);
                hash_rect(hasher, *rect);
            }
            Self::PopClip => 5_u8.hash(hasher),
        }
    }
}

/// One layer before rasterizing. `key` is `None` for the implicit root
/// layer holding content outside every explicit layer.
pub(crate) struct Draft {
    pub(crate) key: Option<u64>,
    pub(crate) cells: CellBox,
    pub(crate) clip: CellBox,
    pub(crate) ops: Vec<Op>,
    pub(crate) inked: bool,
}

impl Draft {
    pub(crate) fn columns(&self) -> u32 {
        self.cells
            .right
            .saturating_sub(self.cells.left)
            .max(0)
            .unsigned_abs()
    }

    pub(crate) fn rows(&self) -> u32 {
        self.cells
            .bottom
            .saturating_sub(self.cells.top)
            .max(0)
            .unsigned_abs()
    }

    pub(crate) fn is_visible(&self) -> bool {
        self.inked && !self.cells.intersect(self.clip).is_empty()
    }

    pub(crate) fn content_hash(&self, cell: (u32, u32)) -> u64 {
        let mut hasher = DefaultHasher::new();
        (self.columns(), self.rows(), cell).hash(&mut hasher);
        for op in &self.ops {
            op.hash_into(&mut hasher);
        }
        hasher.finish()
    }
}

/// What a pointer position resolved to on the last layout. Each field holds
/// the innermost value on the path from the element under the pointer up
/// to the root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit<M> {
    /// The innermost [`Element::id`].
    pub id: Option<u64>,
    /// The innermost [`Element::on_click`] message.
    pub on_click: Option<M>,
    /// The innermost [`Element::on_hover`] message.
    pub on_hover: Option<M>,
    /// The innermost [`Element::on_leave`] message.
    pub on_leave: Option<M>,
    /// The message for one turn of the wheel up, from the innermost
    /// element that can still scroll.
    pub on_wheel_up: Option<M>,
    /// The message for one turn of the wheel down.
    pub on_wheel_down: Option<M>,
    /// Which hoverable element is under the pointer, comparable between
    /// hit tests of one layout, so a change means the pointer moved to
    /// another element.
    pub hover_target: Option<usize>,
}

/// An interactive box, clipped by its scroll ancestors.
pub(crate) struct Region<M> {
    rect: Rect,
    full: Rect,
    parent: Option<usize>,
    id: Option<u64>,
    on_click: Option<M>,
    on_hover: Option<M>,
    on_leave: Option<M>,
    on_wheel: Option<(M, M)>,
    press: Option<(PressHandler<M>, bool)>,
    grip: bool,
}

impl<M> Region<M> {
    fn plain(rect: Rect, full: Rect, parent: Option<usize>) -> Self {
        Self {
            rect,
            full,
            parent,
            id: None,
            on_click: None,
            on_hover: None,
            on_leave: None,
            on_wheel: None,
            press: None,
            grip: false,
        }
    }
}

fn contains(rect: Rect, x: f32, y: f32) -> bool {
    x >= rect.x && y >= rect.y && x < rect.x + rect.width && y < rect.y + rect.height
}

fn intersect(a: Rect, b: Rect) -> Rect {
    let left = a.x.max(b.x);
    let top = a.y.max(b.y);
    Rect::new(
        left,
        top,
        (a.x + a.width).min(b.x + b.width) - left,
        (a.y + a.height).min(b.y + b.height) - top,
    )
}

pub(crate) fn hit_test<M: Clone>(regions: &[Region<M>], x: f32, y: f32) -> Option<Hit<M>> {
    let found = regions
        .iter()
        .rposition(|region| contains(region.rect, x, y))?;
    let mut hit = Hit {
        id: None,
        on_click: None,
        on_hover: None,
        on_leave: None,
        on_wheel_up: None,
        on_wheel_down: None,
        hover_target: None,
    };
    let mut cursor = Some(found);
    while let Some(index) = cursor {
        let Some(region) = regions.get(index) else {
            break;
        };
        hit.id = hit.id.or(region.id);
        hit.on_click = hit.on_click.or_else(|| region.on_click.clone());
        hit.on_hover = hit.on_hover.or_else(|| region.on_hover.clone());
        hit.on_leave = hit.on_leave.or_else(|| region.on_leave.clone());
        if hit.hover_target.is_none() && (region.on_hover.is_some() || region.on_leave.is_some()) {
            hit.hover_target = Some(index);
        }
        if hit.on_wheel_up.is_none()
            && let Some((up, down)) = &region.on_wheel
        {
            hit.on_wheel_up = Some(up.clone());
            hit.on_wheel_down = Some(down.clone());
        }
        cursor = region.parent;
    }
    Some(hit)
}

/// The press handler a press at (`x`, `y`) starts, with the box its
/// fractions count in: the innermost one, skipping handlers that need a
/// grip when the press missed every grip.
pub(crate) fn press_target<M>(
    regions: &[Region<M>],
    x: f32,
    y: f32,
) -> Option<(Rect, PressHandler<M>)> {
    let found = regions
        .iter()
        .rposition(|region| contains(region.rect, x, y))?;
    let mut on_grip = false;
    let mut cursor = Some(found);
    while let Some(region) = cursor.and_then(|index| regions.get(index)) {
        on_grip |= region.grip;
        if let Some((handler, needs_grip)) = &region.press
            && (on_grip || !needs_grip)
        {
            return Some((region.full, Arc::clone(handler)));
        }
        cursor = region.parent;
    }
    None
}

/// Where the walk stands: the enclosing layer, its origin on the surface,
/// the active clips and the nearest interactive ancestor.
#[derive(Clone, Copy)]
struct Scope {
    layer: usize,
    origin: Point,
    clip: Rect,
    cell_clip: CellBox,
    hit_parent: Option<usize>,
}

pub(crate) struct Resolved<M> {
    pub(crate) drafts: Vec<Draft>,
    pub(crate) regions: Vec<Region<M>>,
    pub(crate) corrections: Vec<M>,
}

/// The layer keys popups without their own key draw under, far above any
/// key an application picks by hand.
const POPUP_KEYS: u64 = 1 << 62;

/// How many cells of text one turn of the wheel scrolls.
const WHEEL_LINES: f32 = 3.0;

/// A popup waiting for the main walk to finish: the element, its parent's
/// origin and the index of its layout node.
type Lifted<'r, M> = (&'r Element<M>, Point, usize);

struct Walker<'a, 'b, 'r, M> {
    laid: &'a Laid<'b>,
    text: &'a mut TextCache,
    cell: (f32, f32),
    cursor: usize,
    keys: HashSet<u64>,
    drafts: Vec<Draft>,
    regions: Vec<Region<M>>,
    corrections: Vec<M>,
    lifted: Vec<Lifted<'r, M>>,
    lifting: bool,
}

/// How many layout nodes `element` and its subtree occupy.
fn node_count<M>(element: &Element<M>) -> usize {
    match element.kind {
        Kind::Row | Kind::Column | Kind::Stack => {
            1 + element.children.iter().map(node_count).sum::<usize>()
        }
        _ => 1,
    }
}

/// Walks `root` over its layout. The first draft is the implicit root
/// layer, covering the surface.
pub(crate) fn resolve<M: Clone + 'static>(
    root: &Element<M>,
    laid: &Laid<'_>,
    text: &mut TextCache,
    surface: Size<f32>,
    cell: (f32, f32),
) -> Resolved<M> {
    let screen = Rect::new(0.0, 0.0, surface.width, surface.height);
    let cell_clip = CellBox::inside(screen, cell);
    let mut walker = Walker {
        laid,
        text,
        cell,
        cursor: 0,
        keys: HashSet::new(),
        drafts: vec![Draft {
            key: None,
            cells: CellBox {
                left: 0,
                top: 0,
                right: (surface.width / cell.0).ceil() as i32,
                bottom: (surface.height / cell.1).ceil() as i32,
            },
            clip: cell_clip,
            ops: Vec::new(),
            inked: false,
        }],
        regions: Vec::new(),
        corrections: Vec::new(),
        lifted: Vec::new(),
        lifting: false,
    };
    let scope = Scope {
        layer: 0,
        origin: Point::default(),
        clip: screen,
        cell_clip,
        hit_parent: None,
    };
    walker.walk(root, Point::default(), scope);
    let mut next = 0;
    while let Some((element, parent, cursor)) = walker.lifted.get(next).copied() {
        next += 1;
        walker.cursor = cursor;
        walker.lifting = true;
        walker.walk(element, parent, scope);
    }
    Resolved {
        drafts: walker.drafts,
        regions: walker.regions,
        corrections: walker.corrections,
    }
}

impl<'r, M: Clone + 'static> Walker<'_, '_, 'r, M> {
    fn walk(&mut self, element: &'r Element<M>, parent: Point, scope: Scope) {
        let lifted = std::mem::take(&mut self.lifting);
        if element.popup.is_some() && !lifted {
            self.lifted.push((element, parent, self.cursor));
            self.cursor += node_count(element);
            return;
        }
        let Some(node) = self.laid.nodes.get(self.cursor).copied() else {
            return;
        };
        self.cursor += 1;
        let Ok(layout) = self.laid.tree.layout(node) else {
            return;
        };
        let at = Point::new(parent.x + layout.location.x, parent.y + layout.location.y);
        let rect = Rect::new(at.x, at.y, layout.size.width, layout.size.height);
        let forced = lifted.then(|| self.popup_key(element));
        let (rect, painted, scope) = self.enter_layer(element, rect, scope, forced);
        self.paint_box(element, painted, scope);
        let scroll = self.scroll_of(element, node, rect);
        let scope = self.add_region(element, rect, scope, scroll);
        let unrounded = self.laid.tree.unrounded_layout(node).size;
        self.paint_leaf(element, rect, unrounded.width, scope);
        self.walk_children(element, rect, scope, scroll);
    }

    fn popup_key(&self, element: &Element<M>) -> u64 {
        element
            .layer
            .unwrap_or(POPUP_KEYS + self.lifted.len() as u64)
    }

    /// The clamped scroll position of a scrolling element, sending its
    /// handler the clamped position when the requested one ran past the
    /// content.
    fn scroll_of(&mut self, element: &Element<M>, node: NodeId, rect: Rect) -> Option<Scroll> {
        let requested = element.scroll?;
        let is_row = matches!(element.kind, Kind::Row);
        let layout = self.laid.tree.layout(node).ok()?;
        let children = self.laid.tree.children(node).ok()?;
        let reach = children
            .iter()
            .zip(&element.children)
            .filter(|(_, child)| child.popup.is_none())
            .filter_map(|(child, _)| self.laid.tree.layout(*child).ok())
            .map(|child| {
                if is_row {
                    child.location.x + child.size.width
                } else {
                    child.location.y + child.size.height
                }
            })
            .fold(0.0_f32, f32::max);
        let (end_padding, viewport) = if is_row {
            (layout.padding.right + layout.border.right, rect.width)
        } else {
            (layout.padding.bottom + layout.border.bottom, rect.height)
        };
        let max = (reach + end_padding - viewport).max(0.0);
        let offset = requested.clamp(0.0, max);
        if (requested - offset).abs() > 0.5
            && let Some(handler) = &element.on_scroll
        {
            self.corrections.push(handler(Scroll { offset, max }));
        }
        Some(Scroll { offset, max })
    }

    /// Opens a layer at the cell the box snaps to. Layer-relative drawing
    /// keeps the image independent of where the box sits, and the root's
    /// painted box covers the whole snapped image.
    fn enter_layer(
        &mut self,
        element: &Element<M>,
        rect: Rect,
        scope: Scope,
        forced: Option<u64>,
    ) -> (Rect, Rect, Scope) {
        let Some(key) = forced
            .or(element.layer)
            .filter(|key| self.keys.insert(*key))
        else {
            return (rect, rect, scope);
        };
        let (cell_width, cell_height) = self.cell;
        let column = (rect.x / cell_width).floor();
        let row = (rect.y / cell_height).floor();
        let columns = (rect.width / cell_width).ceil();
        let rows = (rect.height / cell_height).ceil();
        let origin = Point::new(column * cell_width, row * cell_height);
        self.drafts.push(Draft {
            key: Some(key),
            cells: CellBox {
                left: column as i32,
                top: row as i32,
                right: (column + columns) as i32,
                bottom: (row + rows) as i32,
            },
            clip: scope.cell_clip,
            ops: Vec::new(),
            inked: false,
        });
        let snapped = Rect::new(origin.x, origin.y, rect.width, rect.height);
        let painted = Rect::new(origin.x, origin.y, columns * cell_width, rows * cell_height);
        let layer = self.drafts.len() - 1;
        (
            snapped,
            painted,
            Scope {
                layer,
                origin,
                ..scope
            },
        )
    }

    fn local(rect: Rect, scope: Scope) -> Rect {
        Rect::new(
            rect.x - scope.origin.x,
            rect.y - scope.origin.y,
            rect.width,
            rect.height,
        )
    }

    fn push_op(&mut self, layer: usize, op: Op) {
        if let Some(draft) = self.drafts.get_mut(layer) {
            draft.inked |= !matches!(op, Op::PushClip(_) | Op::PopClip);
            draft.ops.push(op);
        }
    }

    fn paint_box(&mut self, element: &Element<M>, rect: Rect, scope: Scope) {
        let style = &element.style;
        let local = Self::local(rect, scope);
        let radius = style.radius;
        if let Some(paint) = &style.background {
            self.push_op(
                scope.layer,
                Op::Fill {
                    rect: local,
                    radius,
                    paint: anchored(paint, local),
                },
            );
        }
        if let Some((width, paint)) = &style.border {
            self.push_op(
                scope.layer,
                Op::Stroke {
                    rect: local,
                    radius,
                    width: *width,
                    paint: anchored(paint, local),
                },
            );
        }
    }

    fn add_region(
        &mut self,
        element: &Element<M>,
        rect: Rect,
        scope: Scope,
        scroll: Option<Scroll>,
    ) -> Scope {
        let on_wheel = self.wheel(element, scroll);
        let interactive = element.on_click.is_some()
            || element.on_hover.is_some()
            || element.on_leave.is_some()
            || element.on_press.is_some()
            || element.id.is_some()
            || element.grip
            || element.opaque
            || on_wheel.is_some();
        let visible = intersect(rect, scope.clip);
        if !interactive || visible.width <= 0.0 || visible.height <= 0.0 {
            return scope;
        }
        self.regions.push(Region {
            id: element.id,
            on_click: element.on_click.clone(),
            on_hover: element.on_hover.clone(),
            on_leave: element.on_leave.clone(),
            on_wheel,
            press: element.on_press.clone(),
            grip: element.grip,
            ..Region::plain(visible, rect, scope.hit_parent)
        });
        Scope {
            hit_parent: Some(self.regions.len() - 1),
            ..scope
        }
    }

    /// The wheel messages of a scrolling element, while it can scroll.
    fn wheel(&self, element: &Element<M>, scroll: Option<Scroll>) -> Option<(M, M)> {
        let scroll = scroll.filter(|scroll| scroll.max > 0.0)?;
        let handler = element.on_scroll.as_ref()?;
        let step = if matches!(element.kind, Kind::Row) {
            self.cell.0 * WHEEL_LINES * 2.0
        } else {
            self.cell.1 * WHEEL_LINES
        };
        let to = |offset: f32| {
            handler(Scroll {
                offset: offset.clamp(0.0, scroll.max),
                max: scroll.max,
            })
        };
        Some((to(scroll.offset - step), to(scroll.offset + step)))
    }

    /// One region per line of each clickable span, above the text's own
    /// region so a click on the span wins.
    fn add_span_regions(
        &mut self,
        element: &Element<M>,
        key: &TextKey,
        origin: Point,
        scope: Scope,
    ) {
        let Some(layout) = self.text.get(key) else {
            return;
        };
        for (span, message) in &element.span_clicks {
            for glyphs in layout.span_boxes(*span) {
                let rect = Rect::new(
                    origin.x + glyphs.x,
                    origin.y + glyphs.y,
                    glyphs.width,
                    glyphs.height,
                );
                let visible = intersect(rect, scope.clip);
                if visible.width > 0.0 && visible.height > 0.0 {
                    self.regions.push(Region {
                        on_click: Some(message.clone()),
                        ..Region::plain(visible, rect, scope.hit_parent)
                    });
                }
            }
        }
    }

    fn paint_leaf(&mut self, element: &Element<M>, rect: Rect, exact_width: f32, scope: Scope) {
        let style = &element.style;
        let border = style.border.as_ref().map_or(0.0, |(width, _)| *width);
        let (pad_x, pad_y) = (style.padding.0 + border, style.padding.1 + border);
        let local = Self::local(rect, scope);
        let inner = Rect::new(
            local.x + pad_x,
            local.y + pad_y,
            local.width - pad_x * 2.0,
            local.height - pad_y * 2.0,
        );
        match &element.kind {
            Kind::Text(text) => {
                let key = text_key(element, text).wrapped_at(exact_width - pad_x * 2.0);
                self.text.shape(&key);
                self.add_span_regions(
                    element,
                    &key,
                    Point::new(rect.x + pad_x, rect.y + pad_y),
                    scope,
                );
                self.push_op(
                    scope.layer,
                    Op::Text {
                        key,
                        origin: Point::new(inner.x, inner.y),
                        paint: Paint::Solid(style.color),
                    },
                );
            }
            Kind::Image(surface) => self.push_op(
                scope.layer,
                Op::Image {
                    surface: Arc::clone(surface),
                    rect: inner,
                    radius: style.radius,
                },
            ),
            _ => {}
        }
    }

    fn walk_children(
        &mut self,
        element: &'r Element<M>,
        rect: Rect,
        scope: Scope,
        scroll: Option<Scroll>,
    ) {
        let Some(Scroll { offset, max }) = scroll else {
            let origin = Point::new(rect.x, rect.y);
            for child in &element.children {
                self.walk(child, origin, scope);
            }
            return;
        };
        let is_row = matches!(element.kind, Kind::Row);
        let origin = if is_row {
            Point::new(rect.x - offset, rect.y)
        } else {
            Point::new(rect.x, rect.y - offset)
        };
        self.push_op(scope.layer, Op::PushClip(Self::local(rect, scope)));
        let inner = Scope {
            clip: intersect(scope.clip, rect),
            cell_clip: scope.cell_clip.intersect(CellBox::inside(rect, self.cell)),
            ..scope
        };
        for child in &element.children {
            self.walk(child, origin, inner);
        }
        if max > 0.0
            && let Some(handler) = &element.on_scroll
        {
            self.scrollbar(element, handler, rect, scope, Scroll { offset, max });
        }
        self.push_op(scope.layer, Op::PopClip);
    }

    /// The thumb along the end edge of a scrolling element, and a track
    /// region that drags it.
    fn scrollbar(
        &mut self,
        element: &Element<M>,
        handler: &ScrollHandler<M>,
        rect: Rect,
        scope: Scope,
        scroll: Scroll,
    ) {
        let is_row = matches!(element.kind, Kind::Row);
        let thickness = (self.cell.0 * 0.6).clamp(3.0, 8.0);
        let length = if is_row { rect.width } else { rect.height };
        let thumb = (length * length / (length + scroll.max))
            .max(thickness * 2.0)
            .min(length);
        let along = scroll.offset / scroll.max * (length - thumb);
        let (track, thumb_rect) = if is_row {
            let y = rect.y + rect.height - thickness;
            (
                Rect::new(rect.x, y, rect.width, thickness),
                Rect::new(rect.x + along, y, thumb, thickness),
            )
        } else {
            let x = rect.x + rect.width - thickness;
            (
                Rect::new(x, rect.y, thickness, rect.height),
                Rect::new(x, rect.y + along, thickness, thumb),
            )
        };
        let color = element.scrollbar.unwrap_or(Rgba {
            r: 255,
            g: 255,
            b: 255,
            a: 90,
        });
        self.push_op(
            scope.layer,
            Op::Fill {
                rect: Self::local(thumb_rect, scope),
                radius: thickness / 2.0,
                paint: Paint::Solid(color),
            },
        );
        let handler = Arc::clone(handler);
        let max = scroll.max;
        let travel = (length - thumb).max(1.0);
        let press: PressHandler<M> = Arc::new(move |x, y| {
            let at = if is_row { x } else { y } * length;
            let offset = ((at - thumb / 2.0) / travel).clamp(0.0, 1.0) * max;
            handler(Scroll { offset, max })
        });
        let visible = intersect(track, scope.clip);
        if visible.width > 0.0 && visible.height > 0.0 {
            self.regions.push(Region {
                press: Some((press, false)),
                ..Region::plain(visible, track, scope.hit_parent)
            });
        }
    }
}

fn anchored(paint: &Paint, rect: Rect) -> Paint {
    match paint {
        Paint::Solid(color) => Paint::Solid(*color),
        Paint::Linear { from, to, stops } => Paint::Linear {
            from: Point::new(from.x + rect.x, from.y + rect.y),
            to: Point::new(to.x + rect.x, to.y + rect.y),
            stops: stops.clone(),
        },
    }
}
