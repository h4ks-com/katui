//! Turns a computed layout into per-layer draw lists and hit regions.

use std::collections::HashSet;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;

use katui_draw::{Paint, Point, Rect, Rgba};
use katui_scene::surface::Surface;
use taffy::prelude::Size;

use crate::element::{Element, Kind};
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
}

/// An interactive box, clipped by its scroll ancestors.
pub(crate) struct Region<M> {
    rect: Rect,
    parent: Option<usize>,
    id: Option<u64>,
    on_click: Option<M>,
    on_hover: Option<M>,
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
    };
    let mut cursor = Some(found);
    while let Some(region) = cursor.and_then(|index| regions.get(index)) {
        hit.id = hit.id.or(region.id);
        hit.on_click = hit.on_click.or_else(|| region.on_click.clone());
        hit.on_hover = hit.on_hover.or_else(|| region.on_hover.clone());
        cursor = region.parent;
    }
    Some(hit)
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
}

struct Walker<'a, 'b, M> {
    laid: &'a Laid<'b>,
    text: &'a mut TextCache,
    cell: (f32, f32),
    cursor: usize,
    keys: HashSet<u64>,
    drafts: Vec<Draft>,
    regions: Vec<Region<M>>,
}

/// Walks `root` over its layout. The first draft is the implicit root
/// layer, covering the surface.
pub(crate) fn resolve<M: Clone>(
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
    };
    let scope = Scope {
        layer: 0,
        origin: Point::default(),
        clip: screen,
        cell_clip,
        hit_parent: None,
    };
    walker.walk(root, Point::default(), scope);
    Resolved {
        drafts: walker.drafts,
        regions: walker.regions,
    }
}

impl<M: Clone> Walker<'_, '_, M> {
    fn walk(&mut self, element: &Element<M>, parent: Point, scope: Scope) {
        let Some(node) = self.laid.nodes.get(self.cursor).copied() else {
            return;
        };
        self.cursor += 1;
        let Ok(layout) = self.laid.tree.layout(node) else {
            return;
        };
        let at = Point::new(parent.x + layout.location.x, parent.y + layout.location.y);
        let rect = Rect::new(at.x, at.y, layout.size.width, layout.size.height);
        let (rect, painted, scope) = self.enter_layer(element, rect, scope);
        self.paint_box(element, painted, scope);
        let scope = self.add_region(element, rect, scope);
        let unrounded = self.laid.tree.unrounded_layout(node).size;
        self.paint_leaf(element, rect, unrounded.width, scope);
        self.walk_children(element, rect, scope);
    }

    /// Opens a layer at the cell the box snaps to. Layer-relative drawing
    /// keeps the image independent of where the box sits, and the root's
    /// painted box covers the whole snapped image.
    fn enter_layer(
        &mut self,
        element: &Element<M>,
        rect: Rect,
        scope: Scope,
    ) -> (Rect, Rect, Scope) {
        let Some(key) = element.layer.filter(|key| self.keys.insert(*key)) else {
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

    fn add_region(&mut self, element: &Element<M>, rect: Rect, scope: Scope) -> Scope {
        let interactive =
            element.on_click.is_some() || element.on_hover.is_some() || element.id.is_some();
        let visible = intersect(rect, scope.clip);
        if !interactive || visible.width <= 0.0 || visible.height <= 0.0 {
            return scope;
        }
        self.regions.push(Region {
            rect: visible,
            parent: scope.hit_parent,
            id: element.id,
            on_click: element.on_click.clone(),
            on_hover: element.on_hover.clone(),
        });
        Scope {
            hit_parent: Some(self.regions.len() - 1),
            ..scope
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

    fn walk_children(&mut self, element: &Element<M>, rect: Rect, scope: Scope) {
        let Some(offset) = element.scroll else {
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
        self.push_op(scope.layer, Op::PopClip);
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
