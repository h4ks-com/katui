//! Taffy trees built from element trees, and the text shaping cache their
//! measuring runs through.

use std::collections::HashMap;
use std::sync::Arc;

use katui_draw::{TextEngine, TextLayout, TextSpan, TextStyle};
use taffy::prelude::*;
use taffy::{Overflow, Point as TaffyPoint};

use crate::element::{Align, Element, Kind, Length};

#[allow(clippy::cast_precision_loss)] // Pixel extents stay far below 2^24, so we convert them exactly.
pub(crate) fn to_f32(value: u32) -> f32 {
    value as f32
}

fn to_u32(value: f32) -> u32 {
    (value as i32).max(0).unsigned_abs()
}

/// Identity of one shaping result: the text, its look and its wrap width.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct TextKey {
    text: Arc<str>,
    size_bits: u32,
    weight: u16,
    wrap: Option<u32>,
    spans: Option<Arc<[TextSpan]>>,
}

impl TextKey {
    pub(crate) fn wrapped_at(&self, width: f32) -> Self {
        Self {
            wrap: Some(to_u32(width.round())),
            ..self.clone()
        }
    }

    fn line_height(&self) -> f32 {
        TextStyle::new(f32::from_bits(self.size_bits)).line_height
    }
}

struct CachedText {
    layout: TextLayout,
    generation: u64,
}

/// Shaped text keyed by text, style and width. We drop entries the latest
/// render never asked for.
pub(crate) struct TextCache {
    engine: TextEngine,
    entries: HashMap<TextKey, CachedText>,
    generation: u64,
    shapes: u64,
}

impl TextCache {
    pub(crate) fn new(engine: TextEngine) -> Self {
        Self {
            engine,
            entries: HashMap::new(),
            generation: 0,
            shapes: 0,
        }
    }

    #[cfg(test)]
    pub(crate) fn shape_count(&self) -> u64 {
        self.shapes
    }

    pub(crate) fn begin_render(&mut self) {
        self.generation += 1;
    }

    pub(crate) fn end_render(&mut self) {
        let generation = self.generation;
        self.entries
            .retain(|_, entry| entry.generation == generation);
    }

    pub(crate) fn get(&self, key: &TextKey) -> Option<&TextLayout> {
        self.entries.get(key).map(|entry| &entry.layout)
    }

    pub(crate) fn shape(&mut self, key: &TextKey) -> &TextLayout {
        let Self {
            engine,
            entries,
            generation,
            shapes,
        } = self;
        let entry = entries.entry(key.clone()).or_insert_with(|| {
            *shapes += 1;
            let style = TextStyle {
                weight: key.weight,
                ..TextStyle::new(f32::from_bits(key.size_bits))
            };
            CachedText {
                layout: match &key.spans {
                    Some(spans) => engine.layout_spans(spans, &style, key.wrap.map(to_f32)),
                    None => engine.layout(&key.text, &style, key.wrap.map(to_f32)),
                },
                generation: *generation,
            }
        });
        entry.generation = *generation;
        &entry.layout
    }

    fn measure(
        &mut self,
        key: &TextKey,
        known: Size<Option<f32>>,
        available: Size<AvailableSpace>,
    ) -> Size<f32> {
        let wrap = match (known.width, available.width) {
            (Some(width), _) | (None, AvailableSpace::Definite(width)) => Some(width),
            (None, AvailableSpace::MinContent) => {
                return Size {
                    width: 0.0,
                    height: known.height.unwrap_or_else(|| key.line_height()),
                };
            }
            (None, AvailableSpace::MaxContent) => None,
        };
        let shaped = match wrap {
            Some(width) => key.wrapped_at(width),
            None => key.clone(),
        };
        let (width, height) = self.shape(&shaped).size();
        Size {
            width: known.width.unwrap_or_else(|| width.ceil()),
            height: known.height.unwrap_or_else(|| height.ceil()),
        }
    }
}

/// What a taffy leaf measures.
pub(crate) enum Leaf {
    Text(TextKey),
    Image(f32, f32),
}

/// An image's size: what the layout fixed, the other side from its aspect,
/// and with neither fixed its own size, narrowed to fit the space.
fn measure_image(
    known: Size<Option<f32>>,
    available: Size<AvailableSpace>,
    width: f32,
    height: f32,
) -> Size<f32> {
    let aspect = height / width.max(1.0);
    let room = match available.width {
        AvailableSpace::Definite(room) => room,
        _ => f32::INFINITY,
    };
    let width = width.min(room);
    let height = width * aspect;
    match (known.width, known.height) {
        (Some(width), Some(height)) => Size { width, height },
        (Some(width), None) => Size {
            width,
            height: width * aspect,
        },
        (None, Some(height)) => Size {
            width: height / aspect.max(f32::EPSILON),
            height,
        },
        (None, None) => Size { width, height },
    }
}

#[derive(Clone, Copy)]
enum Parent {
    Root(Size<f32>),
    Row,
    Column,
    Stack,
}

/// A computed taffy tree; `nodes` lists node ids in element pre-order.
pub(crate) struct Laid<'a> {
    pub(crate) tree: &'a TaffyTree<Leaf>,
    pub(crate) nodes: Vec<NodeId>,
}

/// Lays `root` out at `surface` pixels. The root always fills the surface.
pub(crate) fn compute<'a, M>(
    root: &Element<M>,
    surface: Size<f32>,
    cell: (f32, f32),
    tree: &'a mut TaffyTree<Leaf>,
    text: &mut TextCache,
) -> Option<Laid<'a>> {
    tree.clear();
    let mut builder = Builder {
        tree: &mut *tree,
        nodes: Vec::new(),
        layers: Vec::new(),
    };
    let top = builder.build(root, Parent::Root(surface), false).ok()?;
    let Builder { nodes, layers, .. } = builder;
    let available = Size {
        width: AvailableSpace::Definite(surface.width),
        height: AvailableSpace::Definite(surface.height),
    };
    run(tree, top, available, text).ok()?;
    if round_layers_to_cells(tree, &layers, cell).ok()? {
        run(tree, top, available, text).ok()?;
    }
    Some(Laid { tree, nodes })
}

fn run(
    tree: &mut TaffyTree<Leaf>,
    top: NodeId,
    available: Size<AvailableSpace>,
    text: &mut TextCache,
) -> taffy::TaffyResult<()> {
    tree.compute_layout_with_measure(top, available, |input, _, leaf, style| {
        taffy::compute_leaf_layout(
            input,
            style,
            |_, _| 0.0,
            |known, available| match leaf.as_deref() {
                Some(Leaf::Text(key)) => text.measure(key, known, available),
                Some(Leaf::Image(width, height)) => {
                    measure_image(known, available, *width, *height)
                }
                None => known.unwrap_or(Size::ZERO),
            },
        )
    })
}

/// Fixes each layer to its whole-cell size. We feed the rounded size back
/// through layout because stacked layers would overlap by up to one cell
/// when only their images rounded up.
fn round_layers_to_cells(
    tree: &mut TaffyTree<Leaf>,
    layers: &[NodeId],
    cell: (f32, f32),
) -> taffy::TaffyResult<bool> {
    let mut changed = false;
    for layer in layers {
        let size = tree.layout(*layer)?.size;
        let width = (size.width / cell.0).ceil() * cell.0;
        let height = (size.height / cell.1).ceil() * cell.1;
        if (width - size.width).abs() < 0.01 && (height - size.height).abs() < 0.01 {
            continue;
        }
        let mut style = tree.style(*layer)?.clone();
        style.size = Size {
            width: length(width),
            height: length(height),
        };
        style.flex_basis = Dimension::auto();
        tree.set_style(*layer, style)?;
        changed = true;
    }
    Ok(changed)
}

struct Builder<'a> {
    tree: &'a mut TaffyTree<Leaf>,
    nodes: Vec<NodeId>,
    layers: Vec<NodeId>,
}

impl Builder<'_> {
    fn build<M>(
        &mut self,
        element: &Element<M>,
        parent: Parent,
        scrolled: bool,
    ) -> taffy::TaffyResult<NodeId> {
        let slot = self.nodes.len();
        self.nodes.push(NodeId::new(0));
        let style = style_of(element, parent, scrolled);
        let node = match &element.kind {
            Kind::Text(text) => self
                .tree
                .new_leaf_with_context(style, Leaf::Text(text_key(element, text)))?,
            Kind::Image(surface) => self.tree.new_leaf_with_context(
                style,
                Leaf::Image(to_f32(surface.width()), to_f32(surface.height())),
            )?,
            Kind::Spacer => self.tree.new_leaf(style)?,
            Kind::Row | Kind::Column | Kind::Stack => {
                let this = parent_of(&element.kind);
                let children = element
                    .children
                    .iter()
                    .map(|child| self.build(child, this, element.scroll.is_some()))
                    .collect::<Result<Vec<_>, _>>()?;
                self.tree.new_with_children(style, &children)?
            }
        };
        if let Some(entry) = self.nodes.get_mut(slot) {
            *entry = node;
        }
        if element.layer.is_some() {
            self.layers.push(node);
        }
        Ok(node)
    }
}

fn parent_of(kind: &Kind) -> Parent {
    match kind {
        Kind::Row => Parent::Row,
        Kind::Stack => Parent::Stack,
        _ => Parent::Column,
    }
}

pub(crate) fn text_key<M>(element: &Element<M>, text: &Arc<str>) -> TextKey {
    TextKey {
        text: Arc::clone(text),
        size_bits: element.style.font_size.to_bits(),
        weight: element.style.weight,
        wrap: None,
        spans: element.style.spans.clone(),
    }
}

fn items(align: Align) -> AlignItems {
    match align {
        Align::Start => AlignItems::START,
        Align::Center => AlignItems::CENTER,
        Align::End => AlignItems::END,
    }
}

fn content(align: Align) -> JustifyContent {
    match align {
        Align::Start => JustifyContent::START,
        Align::Center => JustifyContent::CENTER,
        Align::End => JustifyContent::END,
    }
}

fn dimension(length: Length) -> Dimension {
    match length {
        Length::Px(px) => Dimension::length(px),
        Length::Fill | Length::Content => Dimension::auto(),
    }
}

fn style_of<M>(element: &Element<M>, parent: Parent, scrolled: bool) -> Style {
    let element_style = &element.style;
    let is_stack = matches!(element.kind, Kind::Stack);
    let is_row = matches!(element.kind, Kind::Row);
    let border = element_style
        .border
        .as_ref()
        .map_or(0.0, |(width, _)| *width);
    let mut style = Style {
        // we position every box so a popup lands relative to its parent
        position: Position::Relative,
        display: if is_stack {
            Display::Grid
        } else {
            Display::Flex
        },
        flex_direction: if is_row {
            FlexDirection::Row
        } else {
            FlexDirection::Column
        },
        gap: length(element_style.gap),
        padding: Rect {
            left: length(element_style.padding.0),
            right: length(element_style.padding.0),
            top: length(element_style.padding.1),
            bottom: length(element_style.padding.1),
        },
        border: length(border),
        size: Size {
            width: dimension(element_style.width),
            height: dimension(element_style.height),
        },
        flex_grow: element_style.grow,
        flex_shrink: if scrolled { 0.0 } else { 1.0 },
        align_items: items(element_style.align),
        justify_items: items(element_style.justify),
        justify_content: content(element_style.justify),
        ..Style::default()
    };
    if element.scroll.is_some() {
        style.overflow = if is_row {
            TaffyPoint {
                x: Overflow::Hidden,
                y: Overflow::Visible,
            }
        } else {
            TaffyPoint {
                x: Overflow::Visible,
                y: Overflow::Hidden,
            }
        };
    }
    if matches!(element.kind, Kind::Image(_)) {
        style.max_size.width = LengthPercentageAuto::percent(1.0);
    }
    if is_stack {
        // we use one flexible track so overlapping children share the whole box
        style.grid_template_columns = vec![fr(1.0)];
        style.grid_template_rows = vec![fr(1.0)];
    }
    apply_fill(&mut style, element, parent);
    if let Some((x, y)) = element.popup {
        float(&mut style, element, x, y);
    }
    style
}

/// Takes a popup out of the flow at `x`, `y` from its parent, stretching a
/// [`Length::Fill`] side to the parent's far edge.
fn float<M>(style: &mut Style, element: &Element<M>, x: f32, y: f32) {
    let stretch = |fill: bool| if fill { length(0.0) } else { auto() };
    style.position = Position::Absolute;
    style.inset = Rect {
        left: length(x),
        top: length(y),
        right: stretch(element.style.width == Length::Fill),
        bottom: stretch(element.style.height == Length::Fill),
    };
    style.flex_grow = 0.0;
    style.align_self = None;
    style.justify_self = None;
    // we size a content popup to its natural width, since its parent is
    // often an empty anchor too narrow to wrap its text in
    if element.style.width == Length::Content {
        style.size.width = Dimension::max_content();
    }
}

fn apply_fill<M>(style: &mut Style, element: &Element<M>, parent: Parent) {
    let width = element.style.width == Length::Fill;
    let height = element.style.height == Length::Fill;
    match parent {
        Parent::Root(surface) => {
            style.size = Size {
                width: length(surface.width),
                height: length(surface.height),
            };
        }
        Parent::Row => {
            fill_main(style, width);
            fill_cross(style, height);
        }
        Parent::Column => {
            fill_main(style, height);
            fill_cross(style, width);
        }
        Parent::Stack => {
            style.grid_row = line(1);
            style.grid_column = line(1);
            fill_cross(style, height);
            if width {
                style.justify_self = Some(AlignSelf::STRETCH);
            }
        }
    }
}

fn fill_main(style: &mut Style, fill: bool) {
    if fill {
        style.flex_grow = style.flex_grow.max(1.0);
        style.flex_basis = Dimension::length(0.0);
    }
}

fn fill_cross(style: &mut Style, fill: bool) {
    if fill {
        style.align_self = Some(AlignSelf::STRETCH);
    }
}
