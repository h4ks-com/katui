//! Surface presenters: two kitty strategies, the whole-frame baseline
//! with alternating image ids and the tile-patched refinement, plus the
//! damage-tracked blocks fallback every truecolor terminal runs.

use crate::Sink;
use std::io::{self, Write as _};

use katui_proto::kitty::{
    Delete, Format, ImageId as KittyImage, Place, PlacementId as KittyPlacement, Transmit,
};
use katui_proto::probe::CellPixels;
use katui_scene::cell::diff;
use katui_scene::cell::{Attrs, Buffer, Color, Position};
use katui_scene::placement::CellRect;
use katui_scene::surface::Surface;
use katui_scene::surface::diff_tiles;

use crate::blocks::{Rgb, quarter, render_line};
use crate::cells::move_cursor;

/// The full-frame kitty surface presenter: every frame transmits the
/// whole surface compressed under one of two alternating image ids and
/// places it, deleting the previous frame's placement, so the terminal
/// never shows a half-updated image. Patched tile presenters refine
/// this with damage repaints; whole-frame stays the correctness
/// baseline.
#[derive(Debug)]
pub struct KittySurface {
    base: u32,
    parity: bool,
}

/// One whole surface as a frame image: a compressed transmit under
/// `image`, the cursor at the area origin, one placement filling `area`.
/// The caller owns the sync wrapper and any previous-placement delete.
fn write_whole_image<W: Sink>(
    bytes: &mut W,
    image: KittyImage,
    surface: &Surface,
    area: CellRect,
) -> io::Result<()> {
    let mut transmit = Transmit::new(
        image,
        Format::Rgba,
        surface.width(),
        surface.height(),
        surface.pixels(),
    );
    transmit.compressed = true;
    for frame in transmit.encode()? {
        bytes.write_all(&frame)?;
    }
    move_cursor(
        bytes,
        Position {
            x: area.x,
            y: area.y,
        },
    )?;
    let mut place = Place::new(image);
    place.placement = KittyPlacement(1);
    place.columns = u32::from(area.columns);
    place.rows = u32::from(area.rows);
    bytes.write_all(&place.encode())
}

impl KittySurface {
    /// A presenter handing out image ids from `base` upward; pick a base
    /// no other katui presenter in the session uses.
    #[must_use]
    pub fn new(base: u32) -> Self {
        Self {
            base,
            parity: false,
        }
    }

    /// Write `surface` scaled into `area` cells as one synchronized
    /// frame and return the bytes written.
    pub fn render_frame<W: Sink>(
        &mut self,
        out: &mut W,
        surface: &Surface,
        area: CellRect,
    ) -> io::Result<usize> {
        let current = self.base + u32::from(self.parity);
        let previous = self.base + u32::from(!self.parity);
        let mut bytes = Vec::new();
        bytes.write_all(b"\x1b[?2026h")?;
        write_whole_image(&mut bytes, KittyImage(current), surface, area)?;
        bytes.write_all(&Delete::Placement(KittyImage(previous), KittyPlacement(1)).encode())?;
        bytes.write_all(b"\x1b[?2026l")?;
        self.parity = !self.parity;
        let written = bytes.len();
        out.write_all(&bytes)?;
        Ok(written)
    }
}

/// The stateful blocks presenter: renders a pixel surface as
/// quarter-block cells once, then repaints only the cells whose pixels
/// changed. This is the blocks path's answer to
/// [`PatchedKitty`](PatchedKitty): a moving sprite on a static
/// background costs a handful of cursor-addressed SGR cells per frame
/// instead of a full-screen rewrite, which is what makes the fallback
/// survivable inside multiplexers.
#[derive(Debug, Default)]
pub struct BlocksSurface {
    prev: Option<Buffer>,
}

impl BlocksSurface {
    /// A fresh presenter with no previous frame.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Write one frame's cells and return the bytes written. A fresh or
    /// resized frame repaints every cell cursor-addressed; a later frame
    /// repaints only the cells whose pixels changed, inside synchronized
    /// output. An unchanged frame writes nothing.
    pub fn render_frame<W: Sink>(&mut self, out: &mut W, surface: &Surface) -> io::Result<usize> {
        let next = cells_for_surface(surface);
        let changed: Vec<(Position, katui_scene::cell::Cell)> = match self.prev.as_ref() {
            Some(prev) if prev.size() == next.size() => diff(prev, &next)
                .into_iter()
                .map(|(position, cell)| (position, cell.clone()))
                .collect(),
            Some(_) | None => all_cells(&next)
                .into_iter()
                .map(|(position, cell)| (position, cell.clone()))
                .collect(),
        };
        self.prev = Some(next);
        if changed.is_empty() {
            return Ok(0);
        }
        let mut bytes = Vec::new();
        bytes.write_all(b"\x1b[?2026h\x1b[H\x1b[2J")?;
        for (position, cell) in &changed {
            crate::cells::render_cell(&mut bytes, *position, cell)?;
        }
        bytes.write_all(b"\x1b[0m\x1b[?2026l")?;
        let written = bytes.len();
        out.write_all(&bytes)?;
        Ok(written)
    }
}

/// Every cell of a buffer, row-major.
fn all_cells(buffer: &Buffer) -> Vec<(Position, &katui_scene::cell::Cell)> {
    let mut cells = Vec::new();
    for row in 0..buffer.size().rows {
        for column in 0..buffer.size().columns {
            if let Some(cell) = buffer.get(Position { x: column, y: row }) {
                cells.push((Position { x: column, y: row }, cell));
            }
        }
    }
    cells
}

/// Classify every cell of a surface into the quarter-block grid.
fn cells_for_surface(surface: &Surface) -> Buffer {
    let columns = u16::try_from(surface.width() / 2).unwrap_or(u16::MAX);
    let rows = u16::try_from(surface.height() / 2).unwrap_or(u16::MAX);
    let mut buffer = Buffer::blank(katui_scene::CellSize { columns, rows });
    for row in 0..rows {
        for column in 0..columns {
            let pixel = |dx: u32, dy: u32| -> Rgb {
                let x = u32::from(column) * 2 + dx;
                let y = u32::from(row) * 2 + dy;
                surface
                    .get(katui_scene::surface::PixelPos::new(x, y))
                    .map_or(Rgb { r: 0, g: 0, b: 0 }, |color| Rgb {
                        r: color.r,
                        g: color.g,
                        b: color.b,
                    })
            };
            let block = quarter([[pixel(0, 0), pixel(1, 0)], [pixel(0, 1), pixel(1, 1)]]);
            let cell = katui_scene::cell::Cell {
                symbol: block.glyph.to_string(),
                fg: Color::Rgb(block.fg.r, block.fg.g, block.fg.b),
                bg: Color::Rgb(block.bg.r, block.bg.g, block.bg.b),
                attrs: Attrs::EMPTY,
            };
            buffer.set(Position { x: column, y: row }, cell);
        }
    }
    buffer
}

/// Write a surface as quarter-block lines from the home cursor, the
/// pixel path for terminals without a graphics protocol. Alpha is
/// ignored: block glyphs carry no transparency.
pub fn render_blocks<W: Sink>(out: &mut W, surface: &Surface) -> io::Result<usize> {
    let mut bytes = Vec::new();
    bytes.write_all(b"\x1b[H")?;
    let mut y = 0;
    while y + 1 < surface.height() {
        let top = pixel_row(surface, y);
        let bottom = pixel_row(surface, y + 1);
        render_line(&mut bytes, &top, &bottom)?;
        bytes.write_all(b"\x1b[K\r\n")?;
        y += 2;
    }
    let written = bytes.len();
    out.write_all(&bytes)?;
    Ok(written)
}

fn pixel_row(surface: &Surface, y: u32) -> Vec<Rgb> {
    (0..surface.width())
        .filter_map(|x| {
            surface
                .get(katui_scene::surface::PixelPos::new(x, y))
                .map(|pixel| Rgb {
                    r: pixel.r,
                    g: pixel.g,
                    b: pixel.b,
                })
        })
        .collect()
}

/// The patched kitty surface presenter: transmits the whole surface once,
/// then per frame re-places only the tiles whose pixels changed, as crop
/// placements of the transmitted image. A frame with more than 64
/// damaged tiles, a resize, or anything that invalidates the base image
/// falls back to one whole retransmit, which is cheaper than hundreds of
/// placements. An unchanged frame writes nothing.
///
/// The tile side is `cell.width * cell.height` pixels, a multiple of both
/// cell axes, so every tile maps to whole cells and patch edges never
/// tear. This is the bandwidth workhorse: a moving sprite on a static
/// background costs a handful of small placements per frame.
#[derive(Debug)]
pub struct PatchedKitty {
    base: u32,
    cell: CellPixels,
    prev: Option<Surface>,
}

impl PatchedKitty {
    /// A presenter for surfaces drawn at `cell` pixel resolution, handing
    /// out image ids from `base` upward.
    #[must_use]
    pub fn new(base: u32, cell: CellPixels) -> Self {
        Self {
            base,
            cell,
            prev: None,
        }
    }

    /// Write one frame into `area` cells and return the bytes written.
    pub fn render_frame<W: Sink>(
        &mut self,
        out: &mut W,
        surface: &Surface,
        area: CellRect,
    ) -> io::Result<usize> {
        let fresh_start = self.prev.as_ref().is_none_or(|prev| {
            prev.width() != surface.width() || prev.height() != surface.height()
        });
        let tile = self.tile_side();
        let damage = match self.prev.as_ref() {
            Some(prev) if !fresh_start => diff_tiles(prev, surface, tile),
            _ => Vec::new(),
        };
        let patching = !fresh_start && damage.len() <= 64;

        let mut bytes = Vec::new();
        if !patching {
            self.full_frame(&mut bytes, surface, area)?;
        } else if damage.is_empty() {
            self.prev = Some(surface.clone());
            return Ok(0);
        } else {
            bytes.write_all(b"\x1b[?2026h")?;
            for rect in &damage {
                self.patch(&mut bytes, *rect, area)?;
            }
            bytes.write_all(b"\x1b[?2026l")?;
        }
        self.prev = Some(surface.clone());
        let written = bytes.len();
        out.write_all(&bytes)?;
        Ok(written)
    }

    fn tile_side(&self) -> u32 {
        u32::from(self.cell.width) * u32::from(self.cell.height)
    }

    fn full_frame(&self, bytes: &mut Vec<u8>, surface: &Surface, area: CellRect) -> io::Result<()> {
        bytes.write_all(b"\x1b[?2026h")?;
        write_whole_image(bytes, KittyImage(self.base), surface, area)?;
        bytes.write_all(b"\x1b[?2026l")?;
        Ok(())
    }

    /// One damaged tile as a crop placement of the base image: the source
    /// rectangle selects the tile's pixels, the cell rectangle says where
    /// they land, the stable placement id makes re-paints overwrite in
    /// place.
    fn patch(
        &self,
        bytes: &mut Vec<u8>,
        rect: katui_scene::surface::PixelRect,
        area: CellRect,
    ) -> io::Result<()> {
        let columns = rect.width.div_ceil(u32::from(self.cell.width)) as u16;
        let rows = rect.height.div_ceil(u32::from(self.cell.height)) as u16;
        let origin_x = area
            .x
            .saturating_add((rect.x / u32::from(self.cell.width)) as u16);
        let origin_y = area
            .y
            .saturating_add((rect.y / u32::from(self.cell.height)) as u16);
        move_cursor(
            bytes,
            Position {
                x: origin_x,
                y: origin_y,
            },
        )?;
        let tile_index = (rect.x / self.tile_side(), rect.y / self.tile_side());
        let mut place = Place::new(KittyImage(self.base));
        place.placement = KittyPlacement(2 + tile_index.0 + tile_index.1 * 4096);
        place.columns = u32::from(columns);
        place.rows = u32::from(rows);
        place.crop = Some(katui_proto::kitty::PixelRect {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: rect.height,
        });
        bytes.write_all(&place.encode())
    }
}

#[cfg(test)]
mod tests {

    use super::{KittySurface, render_blocks};
    use katui_scene::placement::CellRect;
    use katui_scene::surface::{Rgba, Surface};

    fn solid(width: u32, height: u32, color: Rgba) -> Surface {
        let mut surface = Surface::new(width, height);
        for y in 0..height {
            for x in 0..width {
                surface.set(katui_scene::surface::PixelPos::new(x, y), color);
            }
        }
        surface
    }

    fn area() -> CellRect {
        CellRect {
            x: 2,
            y: 1,
            columns: 20,
            rows: 10,
        }
    }

    #[test]
    fn frames_alternate_ids_and_delete_the_previous_frame() {
        let surface = solid(4, 4, Rgba::solid(255, 0, 0));
        let mut presenter = KittySurface::new(90);
        let mut first = Vec::new();
        presenter
            .render_frame(&mut first, &surface, area())
            .expect("writing to a Vec cannot fail");
        let text = String::from_utf8_lossy(&first);
        assert!(text.contains("i=90,"), "the first frame uses the base id");
        assert!(text.contains("d=i,i=91"), "the other id's placement clears");
        assert!(text.contains("o=z"), "frames travel compressed");

        let mut second = Vec::new();
        presenter
            .render_frame(&mut second, &surface, area())
            .expect("writing to a Vec cannot fail");
        let text = String::from_utf8_lossy(&second);
        assert!(text.contains("i=91,"), "the second frame alternates");
        assert!(
            text.contains("d=i,i=90"),
            "the first frame's placement clears"
        );
    }

    #[test]
    fn frames_stay_inside_synchronized_output() {
        let surface = solid(2, 2, Rgba::solid(0, 255, 0));
        let mut presenter = KittySurface::new(10);
        let mut out = Vec::new();
        presenter
            .render_frame(&mut out, &surface, area())
            .expect("writing to a Vec cannot fail");
        let text = String::from_utf8_lossy(&out);
        assert!(text.starts_with("\x1b[?2026h"));
        assert!(text.ends_with("\x1b[?2026l"));
    }

    #[test]
    fn blocks_render_a_solid_surface_as_full_cells() {
        let surface = solid(4, 2, Rgba::solid(255, 0, 0));
        let mut out = Vec::new();
        render_blocks(&mut out, &surface).expect("writing to a Vec cannot fail");
        let text = String::from_utf8_lossy(&out);
        assert!(text.starts_with("\x1b[H"));
        assert!(text.contains("\x1b[38;2;255;0;0m"));
        assert!(text.contains('█'));
        assert!(text.contains("\x1b[K\r\n"));
    }

    mod diffed_blocks {
        use super::super::BlocksSurface;
        use super::solid;
        use katui_scene::surface::Rgba;

        #[test]
        fn an_identical_frame_writes_nothing() {
            let mut presenter = BlocksSurface::new();
            let surface = solid(16, 16, Rgba::solid(30, 40, 50));
            let mut first = Vec::new();
            presenter
                .render_frame(&mut first, &surface)
                .expect("writing to a Vec cannot fail");
            let mut second = Vec::new();
            let written = presenter
                .render_frame(&mut second, &surface)
                .expect("writing to a Vec cannot fail");
            assert_eq!(written, 0, "no pixel change means no bytes");
            assert!(second.is_empty());
        }

        #[test]
        fn a_small_change_costs_a_handful_of_cells() {
            let mut presenter = BlocksSurface::new();
            let mut surface = solid(32, 32, Rgba::solid(10, 10, 10));
            let mut first = Vec::new();
            presenter
                .render_frame(&mut first, &surface)
                .expect("writing to a Vec cannot fail");
            surface.set(
                katui_scene::surface::PixelPos::new(20, 20),
                Rgba::solid(255, 0, 0),
            );
            let mut second = Vec::new();
            let written = presenter
                .render_frame(&mut second, &surface)
                .expect("writing to a Vec cannot fail");
            assert!(
                written < first.len() / 8,
                "one pixel change is far cheaper than a full frame"
            );
            let text = String::from_utf8_lossy(&second);
            assert!(
                text.matches("\x1b[").count() < 10,
                "a couple of cells repaint, not the screen"
            );
        }

        #[test]
        fn a_size_change_repaints_everything() {
            let mut presenter = BlocksSurface::new();
            let mut first = Vec::new();
            presenter
                .render_frame(&mut first, &solid(16, 16, Rgba::solid(9, 9, 9)))
                .expect("writing to a Vec cannot fail");
            let mut second = Vec::new();
            let written = presenter
                .render_frame(&mut second, &solid(32, 32, Rgba::solid(9, 9, 9)))
                .expect("writing to a Vec cannot fail");
            assert!(written > 0, "a resize repaints the whole grid");
        }
    }

    mod patched {
        use super::super::PatchedKitty;
        use katui_proto::probe::CellPixels;
        use katui_scene::placement::CellRect;
        use katui_scene::surface::{Rgba, Surface};

        // 8x8 px tiles with a 2x4 cell: 2*4=8, a multiple of both axes
        fn cell() -> CellPixels {
            CellPixels {
                width: 2,
                height: 4,
            }
        }

        fn area() -> CellRect {
            CellRect {
                x: 0,
                y: 0,
                columns: 16,
                rows: 16,
            }
        }

        fn plain(width: u32, height: u32) -> Surface {
            let mut surface = Surface::new(width, height);
            for y in 0..height {
                for x in 0..width {
                    surface.set(
                        katui_scene::surface::PixelPos::new(x, y),
                        Rgba::solid(10, 10, 30),
                    );
                }
            }
            surface
        }

        #[test]
        fn the_first_frame_transmits_the_whole_surface() {
            let mut presenter = PatchedKitty::new(90, cell());
            let mut out = Vec::new();
            presenter
                .render_frame(&mut out, &plain(64, 64), area())
                .expect("writing to a Vec cannot fail");
            let text = String::from_utf8_lossy(&out);
            assert!(text.contains("a=t"), "the whole image travels");
            assert!(text.contains("o=z"), "compressed");
            assert!(text.contains("a=p,i=90"), "and is placed");
        }

        #[test]
        fn an_identical_frame_writes_nothing() {
            let mut presenter = PatchedKitty::new(90, cell());
            let surface = plain(64, 64);
            let mut first = Vec::new();
            presenter
                .render_frame(&mut first, &surface, area())
                .expect("writing to a Vec cannot fail");
            let mut second = Vec::new();
            let written = presenter
                .render_frame(&mut second, &surface, area())
                .expect("writing to a Vec cannot fail");
            assert_eq!(written, 0, "no damage means no bytes");
            assert!(second.is_empty());
        }

        #[test]
        fn a_small_change_patches_only_its_tile() {
            let mut presenter = PatchedKitty::new(90, cell());
            let mut surface = plain(64, 64);
            let mut warmup = Vec::new();
            presenter
                .render_frame(&mut warmup, &surface, area())
                .expect("writing to a Vec cannot fail");
            surface.set(
                katui_scene::surface::PixelPos::new(40, 40),
                Rgba::solid(255, 255, 255),
            );
            let mut out = Vec::new();
            presenter
                .render_frame(&mut out, &surface, area())
                .expect("writing to a Vec cannot fail");
            let text = String::from_utf8_lossy(&out);
            assert!(!text.contains("a=t"), "no retransmit");
            assert!(text.contains("a=p,i=90"), "a crop placement runs");
            assert!(text.contains("x=40,y=40"), "the crop names its tile");
        }

        #[test]
        fn a_whole_surface_change_falls_back_to_one_retransmit() {
            // 128x128 at 8px tiles is 256 tiles: everything dirty blows
            // the patch budget
            let mut presenter = PatchedKitty::new(90, cell());
            let mut surface = plain(128, 128);
            let big_area = CellRect {
                x: 0,
                y: 0,
                columns: 32,
                rows: 32,
            };
            let mut warmup = Vec::new();
            presenter
                .render_frame(&mut warmup, &surface, big_area)
                .expect("writing to a Vec cannot fail");
            for y in 0..128u32 {
                for x in 0..128u32 {
                    surface.set(
                        katui_scene::surface::PixelPos::new(x, y),
                        Rgba::solid(200, 40, 40),
                    );
                }
            }
            let mut out = Vec::new();
            presenter
                .render_frame(&mut out, &surface, big_area)
                .expect("writing to a Vec cannot fail");
            let text = String::from_utf8_lossy(&out);
            assert!(
                text.contains("a=t"),
                "everything dirty means one full frame"
            );
        }

        #[test]
        fn a_size_change_falls_back_to_one_retransmit() {
            let mut presenter = PatchedKitty::new(90, cell());
            let mut out = Vec::new();
            presenter
                .render_frame(&mut out, &plain(64, 64), area())
                .expect("writing to a Vec cannot fail");
            let mut resized = Vec::new();
            presenter
                .render_frame(&mut resized, &plain(32, 32), area())
                .expect("writing to a Vec cannot fail");
            let text = String::from_utf8_lossy(&resized);
            assert!(text.contains("a=t"), "a resize retransmits whole");
        }
    }
}
