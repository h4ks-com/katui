//! The retained layer cache: rasterizes changed layers and emits placements.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use katui_draw::{Canvas, TextEngine};
use katui_proto::probe::CellPixels;
use katui_scene::placement::{CellRect, ImageId, PixelRect, Placement, PlacementId};
use katui_scene::surface::Surface;
use taffy::TaffyTree;
use taffy::prelude::Size;

use crate::element::Element;
use crate::layout::{Leaf, TextCache, compute, to_f32};
use crate::resolve::{CellBox, Draft, Hit, Op, Region, hit_test, resolve};

const PLACEMENT: PlacementId = PlacementId(1);

fn cell_size(cell: CellPixels) -> (u32, u32) {
    (u32::from(cell.width.max(1)), u32::from(cell.height.max(1)))
}

/// Options fixed for the life of a [`Ui`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiOptions {
    /// The first image id handed out; later ids count up from it.
    pub first_image_id: u32,
    /// The largest `(columns, rows)` one image may span. A larger layer
    /// splits into a grid of images cut on cell boundaries.
    pub max_layer_cells: Option<(u16, u16)>,
}

/// A layer size that keeps every upload inside what a multiplexer carries.
const MULTIPLEXER_LAYER_CELLS: (u16, u16) = (40, 16);

impl UiOptions {
    /// Ids from `first_image_id`, with layers split into multiplexer
    /// sized images when `multiplexed`.
    #[must_use]
    pub fn for_terminal(first_image_id: u32, multiplexed: bool) -> Self {
        Self {
            first_image_id,
            max_layer_cells: multiplexed.then_some(MULTIPLEXER_LAYER_CELLS),
        }
    }
}

impl Default for UiOptions {
    fn default() -> Self {
        Self {
            first_image_id: 1,
            max_layer_cells: None,
        }
    }
}

/// One image ready for transmission.
#[derive(Debug, Clone)]
pub struct LayerImage {
    /// The scene image id.
    pub id: ImageId,
    surface: Arc<Surface>,
}

impl LayerImage {
    /// The width in pixels.
    pub fn width(&self) -> u32 {
        self.surface.width()
    }

    /// The height in pixels.
    pub fn height(&self) -> u32 {
        self.surface.height()
    }

    /// The straight-alpha RGBA bytes, `4 * width * height` long.
    pub fn rgba(&self) -> &[u8] {
        self.surface.pixels()
    }
}

/// The result of one [`Ui::render`].
#[derive(Debug, Clone, Default)]
pub struct RenderOutput {
    /// What to show, in ascending stacking order.
    pub placements: Vec<Placement>,
    /// Every image the placements show. A presenter deletes an image when
    /// its last placement leaves, so a layer scrolling back into view
    /// needs its pixels again under the same id.
    pub images: Vec<LayerImage>,
    /// The images rasterized by this render, including tiles of a split
    /// layer that sit outside the viewport.
    pub new_images: Vec<LayerImage>,
    /// Where each visible layer sits on screen, by its key.
    pub layer_areas: HashMap<u64, CellRect>,
}

impl RenderOutput {
    /// Every placement blended in stacking order onto a transparent
    /// `size` pixel surface, the way a terminal shows them at `cell`
    /// pixels per cell: a screenshot without a terminal.
    #[must_use]
    pub fn flatten(&self, cell: CellPixels, size: (u32, u32)) -> Option<Surface> {
        let images: HashMap<ImageId, &LayerImage> =
            self.images.iter().map(|image| (image.id, image)).collect();
        let (cell_width, cell_height) = (u32::from(cell.width), u32::from(cell.height));
        let mut target = vec![0_u8; (size.0 as usize) * (size.1 as usize) * 4];
        for placement in &self.placements {
            let Some(image) = images.get(&placement.image) else {
                continue;
            };
            let (crop_x, crop_y) = placement.crop.map_or((0, 0), |crop| (crop.x, crop.y));
            let left = u32::from(placement.area.x) * cell_width;
            let top = u32::from(placement.area.y) * cell_height;
            let columns =
                (u32::from(placement.area.columns) * cell_width).min(size.0.saturating_sub(left));
            let rows =
                (u32::from(placement.area.rows) * cell_height).min(size.1.saturating_sub(top));
            for row in 0..rows {
                let from = (((row + crop_y) * image.width() + crop_x) * 4) as usize;
                let to = (((top + row) * size.0 + left) * 4) as usize;
                let length = columns as usize * 4;
                if let (Some(source), Some(destination)) = (
                    image.rgba().get(from..from + length),
                    target.get_mut(to..to + length),
                ) {
                    blend_row(destination, source);
                }
            }
        }
        Surface::from_rgba(size.0, size.1, target)
    }
}

fn blend_row(target: &mut [u8], source: &[u8]) {
    let pixels = target.as_chunks_mut::<4>().0.iter_mut();
    for (to, &[red, green, blue, alpha]) in pixels.zip(source.as_chunks::<4>().0) {
        let weight = u32::from(alpha);
        let mix = |over: u8, under: u8| {
            ((u32::from(over) * weight + u32::from(under) * (255 - weight)) / 255) as u8
        };
        *to = [
            mix(red, to[0]),
            mix(green, to[1]),
            mix(blue, to[2]),
            to[3].max(alpha),
        ];
    }
}

struct Tile {
    image: LayerImage,
    columns: u32,
    rows: u32,
    column: u32,
    row: u32,
}

struct CachedLayer {
    hash: u64,
    tiles: Vec<Tile>,
    // We hold the source images so their addresses, which the hash covers,
    // cannot be reused by different pixels while this entry lives.
    _sources: Vec<Arc<Surface>>,
}

/// The stateful view layer: owns text shaping, the layer cache and the
/// hit regions of the last layout.
pub struct Ui<M> {
    text: TextCache,
    tree: TaffyTree<Leaf>,
    layers: HashMap<Option<u64>, CachedLayer>,
    regions: Vec<Region<M>>,
    next_image: u32,
    max_layer_cells: Option<(u32, u32)>,
}

impl<M: Clone> Ui<M> {
    /// A view layer shaping text through `engine`.
    pub fn new(engine: TextEngine, options: UiOptions) -> Self {
        Self {
            text: TextCache::new(engine),
            tree: TaffyTree::new(),
            layers: HashMap::new(),
            regions: Vec::new(),
            next_image: options.first_image_id,
            max_layer_cells: options
                .max_layer_cells
                .map(|(columns, rows)| (u32::from(columns.max(1)), u32::from(rows.max(1)))),
        }
    }

    /// Lays `root` out over a surface of `size_px`, rasterizes the layers
    /// whose content changed and returns the placements for this frame.
    pub fn render(
        &mut self,
        root: &Element<M>,
        size_px: (u32, u32),
        cell: CellPixels,
    ) -> RenderOutput {
        let surface = Size {
            width: to_f32(size_px.0),
            height: to_f32(size_px.1),
        };
        let (cell_width, cell_height) = cell_size(cell);
        let cell_f32 = (to_f32(cell_width), to_f32(cell_height));
        self.text.begin_render();
        let Some(laid) = compute(root, surface, cell_f32, &mut self.tree, &mut self.text) else {
            return RenderOutput::default();
        };
        let resolved = resolve(root, &laid, &mut self.text, surface, cell_f32);
        self.regions = resolved.regions;
        let output = self.present(&resolved.drafts, (cell_width, cell_height));
        self.text.end_render();
        output
    }

    /// The innermost interactive element at surface pixel (`x`, `y`) on
    /// the last layout.
    pub fn hit_test(&self, x: f32, y: f32) -> Option<Hit<M>> {
        hit_test(&self.regions, x, y)
    }

    /// The whole pixels of the layer `key` as last rendered, its tiles
    /// joined back together.
    pub fn layer_surface(&self, key: u64) -> Option<Surface> {
        let tiles = &self.layers.get(&Some(key))?.tiles;
        let first = tiles.first()?;
        let cell_width = first.image.width() / first.columns.max(1);
        let cell_height = first.image.height() / first.rows.max(1);
        let columns = tiles.iter().map(|tile| tile.column + tile.columns).max()?;
        let rows = tiles.iter().map(|tile| tile.row + tile.rows).max()?;
        let mut whole = Surface::new(columns * cell_width, rows * cell_height);
        for tile in tiles {
            whole.blit(
                &tile.image.surface,
                tile.column * cell_width,
                tile.row * cell_height,
            );
        }
        Some(whole)
    }

    fn present(&mut self, drafts: &[Draft], cell: (u32, u32)) -> RenderOutput {
        let mut output = RenderOutput::default();
        for (index, draft) in drafts.iter().enumerate() {
            if !draft.is_visible() {
                continue;
            }
            self.refresh(draft, cell, &mut output);
            let z = i32::try_from(index).map_or(i32::MAX, |index| index.saturating_add(1));
            if let Some(layer) = self.layers.get(&draft.key) {
                emit(layer, draft, cell, z, &mut output);
            }
            let visible = draft.cells.intersect(draft.clip);
            if let Some(key) = draft.key
                && !visible.is_empty()
            {
                output.layer_areas.insert(key, cell_rect(visible));
            }
        }
        let live: HashSet<Option<u64>> = drafts.iter().map(|draft| draft.key).collect();
        self.layers.retain(|key, _| live.contains(key));
        output
    }

    fn refresh(&mut self, draft: &Draft, cell: (u32, u32), output: &mut RenderOutput) {
        let hash = draft.content_hash(cell);
        if self
            .layers
            .get(&draft.key)
            .is_some_and(|layer| layer.hash == hash)
        {
            return;
        }
        let Some(surface) = self.rasterize(draft, cell) else {
            return;
        };
        let tiles: Vec<Tile> = self
            .cut(surface, draft, cell)
            .into_iter()
            .map(|(region, surface)| Tile {
                image: LayerImage {
                    id: self.fresh_id(),
                    surface: Arc::new(surface),
                },
                columns: region.columns,
                rows: region.rows,
                column: region.column,
                row: region.row,
            })
            .collect();
        output
            .new_images
            .extend(tiles.iter().map(|tile| tile.image.clone()));
        let sources = draft
            .ops
            .iter()
            .filter_map(|op| match op {
                Op::Image { surface, .. } => Some(Arc::clone(surface)),
                _ => None,
            })
            .collect();
        self.layers.insert(
            draft.key,
            CachedLayer {
                hash,
                tiles,
                _sources: sources,
            },
        );
    }

    fn fresh_id(&mut self) -> ImageId {
        let id = ImageId(self.next_image);
        self.next_image = self.next_image.saturating_add(1);
        id
    }

    fn rasterize(&self, draft: &Draft, cell: (u32, u32)) -> Option<Surface> {
        let width = draft.columns().checked_mul(cell.0)?;
        let height = draft.rows().checked_mul(cell.1)?;
        let mut canvas = Canvas::new(width, height).ok()?;
        for op in &draft.ops {
            self.draw(&mut canvas, op);
        }
        canvas.to_surface().ok()
    }

    fn draw(&self, canvas: &mut Canvas, op: &Op) {
        match op {
            Op::Fill {
                rect,
                radius,
                paint,
            } => canvas.fill_round_rect(*rect, *radius, paint),
            Op::Stroke {
                rect,
                radius,
                width,
                paint,
            } => canvas.stroke_round_rect(*rect, *radius, *width, paint),
            Op::Image {
                surface,
                rect,
                radius,
            } => canvas.draw_surface(surface, *rect, *radius),
            Op::Text { key, origin, paint } => {
                if let Some(layout) = self.text.get(key) {
                    canvas.draw_text(layout, *origin, paint);
                }
            }
            Op::PushClip(rect) => canvas.push_clip(*rect),
            Op::PopClip => canvas.pop_clip(),
        }
    }

    fn cut(&self, surface: Surface, draft: &Draft, cell: (u32, u32)) -> Vec<(CellArea, Surface)> {
        let layer = CellArea {
            column: 0,
            row: 0,
            columns: draft.columns(),
            rows: draft.rows(),
        };
        match self.max_layer_cells {
            Some(max) if layer.columns > max.0 || layer.rows > max.1 => split(&layer, max)
                .into_iter()
                .filter_map(|tile| Some((tile, crop(&surface, &tile, cell)?)))
                .collect(),
            _ => vec![(layer, surface)],
        }
    }
}

/// A rectangle of cells inside a layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CellArea {
    column: u32,
    row: u32,
    columns: u32,
    rows: u32,
}

fn split(layer: &CellArea, max: (u32, u32)) -> Vec<CellArea> {
    let mut tiles = Vec::new();
    for row in (0..layer.rows).step_by(max.1 as usize) {
        for column in (0..layer.columns).step_by(max.0 as usize) {
            tiles.push(CellArea {
                column,
                row,
                columns: max.0.min(layer.columns - column),
                rows: max.1.min(layer.rows - row),
            });
        }
    }
    tiles
}

fn crop(surface: &Surface, area: &CellArea, cell: (u32, u32)) -> Option<Surface> {
    let stride = surface.width().checked_mul(4)? as usize;
    let left = (area.column * cell.0 * 4) as usize;
    let right = left + (area.columns * cell.0 * 4) as usize;
    let top = (area.row * cell.1) as usize;
    let rows = (area.rows * cell.1) as usize;
    let mut pixels = Vec::with_capacity((right - left) * rows);
    for row in surface.pixels().chunks_exact(stride).skip(top).take(rows) {
        pixels.extend_from_slice(row.get(left..right)?);
    }
    Surface::from_rgba(area.columns * cell.0, area.rows * cell.1, pixels)
}

fn emit(layer: &CachedLayer, draft: &Draft, cell: (u32, u32), z: i32, output: &mut RenderOutput) {
    for tile in &layer.tiles {
        let left = draft.cells.left.saturating_add_unsigned(tile.column);
        let top = draft.cells.top.saturating_add_unsigned(tile.row);
        let area = CellBox {
            left,
            top,
            right: left.saturating_add_unsigned(tile.columns),
            bottom: top.saturating_add_unsigned(tile.rows),
        };
        let visible = area.intersect(draft.clip);
        if visible.is_empty() {
            continue;
        }
        output.placements.push(Placement {
            image: tile.image.id,
            id: PLACEMENT,
            area: cell_rect(visible),
            crop: (visible != area).then(|| crop_of(area, visible, cell)),
            z,
        });
        output.images.push(tile.image.clone());
    }
}

fn cell_rect(visible: CellBox) -> CellRect {
    CellRect {
        x: visible.left.unsigned_abs() as u16,
        y: visible.top.unsigned_abs() as u16,
        columns: (visible.right - visible.left).unsigned_abs() as u16,
        rows: (visible.bottom - visible.top).unsigned_abs() as u16,
    }
}

fn crop_of(area: CellBox, visible: CellBox, cell: (u32, u32)) -> PixelRect {
    PixelRect {
        x: (visible.left - area.left).unsigned_abs() * cell.0,
        y: (visible.top - area.top).unsigned_abs() * cell.1,
        width: (visible.right - visible.left).unsigned_abs() * cell.0,
        height: (visible.bottom - visible.top).unsigned_abs() * cell.1,
    }
}

#[cfg(test)]
mod tests {
    use katui_draw::TextEngine;

    use super::{CellPixels, Ui, UiOptions};
    use crate::element::{Element, Length};

    const TEXT_FONT: &[u8] =
        include_bytes!("../../katui-draw/assets/AtkinsonHyperlegibleNext[wght].ttf");

    #[test]
    fn a_second_render_shapes_no_text() {
        let engine = TextEngine::new(vec![TEXT_FONT.to_vec()]).expect("the bundled font parses");
        let mut ui: Ui<u32> = Ui::new(engine, UiOptions::default());
        let root = Element::column([Element::text("message")]).width(Length::Fill);
        let cell = CellPixels {
            width: 10,
            height: 20,
        };
        ui.render(&root, (400, 240), cell);
        let shaped = ui.text.shape_count();
        assert!(shaped > 0);
        ui.render(&root, (400, 240), cell);
        assert_eq!(ui.text.shape_count(), shaped);
    }
}
