//! The katui ratatui demo: an image viewer built from ratatui's own
//! widgets, with the picture drawn beside them through the kitty graphics
//! protocol. It shows five generated pictures and any image files you
//! pass. Run with `just demo-ratatui`, or `just demo-ratatui photo.jpg`.
#![cfg(unix)] // the runner drives /dev/tty through katui-rt's unix layer
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]

use std::path::PathBuf;
use std::sync::Arc;

use katui_proto::input::{InputEvent, Key};
use katui_proto::probe::CellPixels;
use katui_ratatui::image::{ImageCache, pixel_area};
use katui_ratatui::run::{Control, RatatuiApp, Waker, cell_rect, run};
use katui_ratatui::{ImageFrame, ImageId, ImageSlot, RgbaImage};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Gauge, List, ListItem, ListState, Paragraph, Sparkline, Tabs};

const PATTERNS: [&str; 5] = ["Plasma", "Mandelbrot", "Sunset", "Rings", "Checkers"];
const ACCENT: Color = Color::Rgb(138, 168, 255);

fn main() -> Result<(), katui_rt::RtError> {
    let files = std::env::args().skip(1).map(PathBuf::from).collect();
    run(Viewer::new(files))
}

#[derive(Debug, Clone)]
enum Source {
    Pattern(usize),
    File(PathBuf),
}

impl Source {
    fn name(&self) -> String {
        match self {
            Self::Pattern(index) => PATTERNS.get(*index).copied().unwrap_or("?").to_owned(),
            Self::File(path) => path.file_name().map_or_else(
                || path.display().to_string(),
                |name| name.to_string_lossy().into_owned(),
            ),
        }
    }
}

struct Shown {
    key: (usize, Rect),
    id: ImageId,
    area: Rect,
    size: (u32, u32),
    pixels: Arc<Vec<u8>>,
}

struct Viewer {
    sources: Vec<Source>,
    list: ListState,
    tab: usize,
    shown: Option<Shown>,
    next_id: u32,
    files: ImageCache<usize>,
}

impl Viewer {
    fn new(files: Vec<PathBuf>) -> Self {
        let mut sources: Vec<Source> = (0..PATTERNS.len()).map(Source::Pattern).collect();
        sources.extend(files.into_iter().map(Source::File));
        Self {
            sources,
            list: ListState::default().with_selected(Some(0)),
            tab: 0,
            shown: None,
            next_id: 1,
            files: ImageCache::default(),
        }
    }

    fn selected(&self) -> usize {
        self.list.selected().unwrap_or(0)
    }

    fn failed(&self) -> bool {
        self.files.failed(&self.selected())
    }

    /// Draw the selected picture for `area` unless it is already drawn.
    fn prepare(&mut self, area: Rect, cell: CellPixels) {
        self.files.poll();
        let key = (self.selected(), area);
        if self.shown.as_ref().is_some_and(|shown| shown.key == key) {
            return;
        }
        let size = pixel_area(cell_rect(area), cell);
        let pixels = match self.sources.get(self.selected()) {
            Some(Source::Pattern(index)) => Some(Arc::new(pattern(*index, size))),
            Some(Source::File(_)) => self.files.contained(&key.0, cell_rect(area), cell),
            None => None,
        };
        self.shown = pixels.map(|pixels| {
            self.next_id += 1;
            Shown {
                key,
                id: ImageId(self.next_id),
                area,
                size,
                pixels,
            }
        });
    }

    fn histogram(&self) -> Vec<u64> {
        let mut bins = vec![0_u64; 32];
        if let Some(shown) = &self.shown {
            for &[red, green, blue, _] in shown.pixels.as_chunks::<4>().0.iter().step_by(7) {
                let light = (u32::from(red) + u32::from(green) + u32::from(blue)) / 3;
                if let Some(bin) = bins.get_mut((light / 8) as usize) {
                    *bin += 1;
                }
            }
        }
        bins
    }

    fn sidebar(&mut self, frame: &mut Frame<'_>, area: Rect) {
        let [list, details] =
            Layout::vertical([Constraint::Fill(1), Constraint::Length(9)]).areas(area);
        let items: Vec<ListItem<'_>> = self
            .sources
            .iter()
            .map(|source| ListItem::new(source.name()))
            .collect();
        let picker = List::new(items)
            .block(Block::bordered().title(" Pictures "))
            .highlight_style(
                Style::new()
                    .fg(Color::Black)
                    .bg(ACCENT)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("› ");
        frame.render_stateful_widget(picker, list, &mut self.list);

        let [info, gauge, spark] = Layout::vertical([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
        ])
        .areas(details);
        let size = self.shown.as_ref().map_or((0, 0), |shown| shown.size);
        let light = self.histogram();
        frame.render_widget(
            Paragraph::new(format!("{} x {} pixels", size.0, size.1))
                .block(Block::bordered().title(" Size ")),
            info,
        );
        frame.render_widget(
            Gauge::default()
                .block(Block::bordered().title(" Brightness "))
                .gauge_style(Style::new().fg(ACCENT))
                .ratio(brightness(&light).clamp(0.0, 1.0)),
            gauge,
        );
        frame.render_widget(
            Sparkline::default()
                .block(Block::bordered().title(" Light "))
                .style(Style::new().fg(ACCENT))
                .data(&light),
            spark,
        );
    }
}

impl RatatuiApp for Viewer {
    type Message = ();

    fn start(&mut self, waker: Waker<()>) {
        self.files.on_ready(move || {
            waker.send(());
        });
        for (index, source) in self.sources.iter().enumerate() {
            if let Source::File(path) = source {
                self.files
                    .request(index, std::fs::read(path).unwrap_or_default());
            }
        }
    }

    fn update(&mut self, (): ()) -> Control {
        Control::Redraw
    }

    fn draw(&mut self, frame: &mut Frame<'_>, cell: CellPixels) {
        let [top, body, hints] = Layout::vertical([
            Constraint::Length(3),
            Constraint::Fill(1),
            Constraint::Length(1),
        ])
        .areas(frame.area());
        let tabs = Tabs::new(["Viewer", "About"])
            .select(self.tab)
            .highlight_style(Style::new().fg(ACCENT).add_modifier(Modifier::BOLD))
            .block(Block::bordered().title(" katui + ratatui "));
        frame.render_widget(tabs, top);
        frame.render_widget(
            Line::from(vec![
                Span::styled(" ↑↓ ", Style::new().fg(Color::Black).bg(ACCENT)),
                Span::raw(" pick a picture  "),
                Span::styled(" Tab ", Style::new().fg(Color::Black).bg(ACCENT)),
                Span::raw(" switch tabs  "),
                Span::styled(" q ", Style::new().fg(Color::Black).bg(ACCENT)),
                Span::raw(" quit"),
            ]),
            hints,
        );
        if self.tab == 1 {
            self.shown = None;
            frame.render_widget(
                Paragraph::new("Pass image files to the demo to add them to the list.")
                    .wrap(ratatui::widgets::Wrap { trim: true })
                    .block(Block::bordered().title(" About ")),
                body,
            );
            return;
        }
        let [side, picture] =
            Layout::horizontal([Constraint::Length(30), Constraint::Fill(1)]).areas(body);
        let frame_block = Block::bordered().title(format!(
            " {} ",
            self.sources
                .get(self.selected())
                .map(Source::name)
                .unwrap_or_default()
        ));
        let inner = frame_block.inner(picture);
        frame.render_widget(frame_block, picture);
        self.prepare(inner, cell);
        if self.failed() {
            frame.render_widget(
                Paragraph::new("Pick another file, this one did not open."),
                inner,
            );
        } else {
            frame.render_widget(ImageSlot, inner);
        }
        self.sidebar(frame, side);
    }

    fn images(&self) -> Vec<ImageFrame<'_>> {
        let Some(shown) = self.shown.as_ref().filter(|_| self.tab == 0) else {
            return Vec::new();
        };
        RgbaImage::new(shown.size.0, shown.size.1, shown.pixels.as_slice())
            .map(|pixels| ImageFrame {
                image: shown.id,
                pixels,
                area: cell_rect(shown.area),
            })
            .into_iter()
            .collect()
    }

    fn input(&mut self, event: &InputEvent) -> Control {
        let InputEvent::Key(key, _) = event else {
            return Control::Wait;
        };
        let last = self.sources.len().saturating_sub(1);
        match key {
            Key::Down | Key::Char('j') => self.list.select(Some((self.selected() + 1).min(last))),
            Key::Up | Key::Char('k') => self.list.select(Some(self.selected().saturating_sub(1))),
            Key::Tab => self.tab = 1 - self.tab,
            Key::Char('q') => return Control::Quit,
            _ => return Control::Wait,
        }
        Control::Redraw
    }
}

fn brightness(histogram: &[u64]) -> f64 {
    let total: u64 = histogram.iter().sum();
    let weighted: u64 = histogram
        .iter()
        .enumerate()
        .map(|(index, count)| index as u64 * count)
        .sum();
    if total == 0 {
        0.0
    } else {
        weighted as f64 / total as f64 / 31.0
    }
}

/// Generated picture `index` at `width` x `height` pixels.
fn pattern(index: usize, (width, height): (u32, u32)) -> Vec<u8> {
    let mut pixels = Vec::with_capacity(width as usize * height as usize * 4);
    let aspect = width as f32 / height.max(1) as f32;
    for y in 0..height {
        for x in 0..width {
            let (across, down) = (
                x as f32 / width.max(1) as f32,
                y as f32 / height.max(1) as f32,
            );
            let color = match index {
                0 => plasma(across, down),
                1 => mandelbrot(across, down, aspect),
                2 => sunset(across, down, aspect),
                3 => rings(across, down, aspect),
                _ => checkers(x, y),
            };
            pixels.extend(color);
            pixels.push(255);
        }
    }
    pixels
}

fn byte(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0) as u8
}

fn plasma(u: f32, v: f32) -> [u8; 3] {
    let wave = (u * 9.0).sin()
        + (v * 7.0).sin()
        + ((u + v) * 6.0).sin()
        + ((u * u + v * v).sqrt() * 12.0).sin();
    let t = wave / 8.0 + 0.5;
    [
        byte(0.5 + 0.5 * (t * 6.3).sin()),
        byte(0.3 + 0.3 * (t * 6.3 + 2.0).sin()),
        byte(0.6 + 0.4 * (t * 6.3 + 4.0).sin()),
    ]
}

fn mandelbrot(across: f32, down: f32, aspect: f32) -> [u8; 3] {
    let (start_real, start_imaginary) = ((across - 0.65) * 3.0 * aspect / 1.5, (down - 0.5) * 2.4);
    let (mut real, mut imaginary, mut steps) = (0.0_f32, 0.0_f32, 0);
    while real * real + imaginary * imaginary < 4.0 && steps < 80 {
        (real, imaginary) = (
            real * real - imaginary * imaginary + start_real,
            2.0 * real * imaginary + start_imaginary,
        );
        steps += 1;
    }
    if steps == 80 {
        return [10, 10, 24];
    }
    let escape = steps as f32 / 80.0;
    [byte(escape * 3.0), byte(escape * 1.6), byte(0.4 + escape)]
}

fn sunset(u: f32, v: f32, aspect: f32) -> [u8; 3] {
    let sun = (((u - 0.5) * aspect).powi(2) + (v - 0.62).powi(2)).sqrt();
    if v > 0.7 {
        return [byte(0.1 + (1.0 - v)), byte(0.05), byte(0.2 + (v - 0.7))];
    }
    if sun < 0.14 {
        return [255, byte(0.75 - sun), byte(0.3)];
    }
    [
        byte(0.9 * v + 0.2),
        byte(0.3 * v + 0.1),
        byte(0.5 - 0.3 * v),
    ]
}

fn rings(u: f32, v: f32, aspect: f32) -> [u8; 3] {
    let distance = (((u - 0.5) * aspect).powi(2) + (v - 0.5).powi(2)).sqrt();
    let band = (distance * 40.0).sin() * 0.5 + 0.5;
    [
        byte(band * 0.6 + 0.2),
        byte(band * 0.9),
        byte(1.0 - band * 0.5),
    ]
}

fn checkers(x: u32, y: u32) -> [u8; 3] {
    if (x / 24 + y / 24).is_multiple_of(2) {
        [236, 233, 245]
    } else {
        [56, 52, 78]
    }
}

#[cfg(test)]
mod tests {
    use katui_draw::{
        Canvas, Paint, Point, Rect as PixelRect, Rgba, TextEngine, TextSpan, TextStyle,
        bundled_fonts,
    };
    use katui_proto::probe::CellPixels;
    use katui_scene::surface::Surface;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;
    use ratatui::style::{Color, Modifier};

    use super::{RatatuiApp as _, Viewer};

    const CELL: CellPixels = CellPixels {
        width: 9,
        height: 19,
    };
    const BACKGROUND: Rgba = Rgba::solid(24, 24, 32);
    const FOREGROUND: Rgba = Rgba::solid(220, 220, 230);

    fn rgba(color: Color, reset: Rgba) -> Rgba {
        match color {
            Color::Rgb(red, green, blue) => Rgba::solid(red, green, blue),
            Color::Black => Rgba::solid(0, 0, 0),
            _ => reset,
        }
    }

    fn rasterize(buffer: &Buffer) -> Canvas {
        let (cell_width, cell_height) = (f32::from(CELL.width), f32::from(CELL.height));
        let width = u32::from(buffer.area.width);
        let mut canvas = Canvas::new(
            width * u32::from(CELL.width),
            u32::from(buffer.area.height) * u32::from(CELL.height),
        )
        .expect("a positive size");
        canvas.clear(BACKGROUND);
        let mut engine = TextEngine::new(bundled_fonts()).expect("the bundled fonts parse");
        let style = TextStyle {
            line_height: cell_height,
            ..TextStyle::new(15.0)
        };
        for (index, cell) in (0_u32..).zip(&buffer.content) {
            let at = Point::new(
                (index % width) as f32 * cell_width,
                (index / width) as f32 * cell_height,
            );
            let background = rgba(cell.bg, BACKGROUND);
            canvas.fill_rect(
                PixelRect::new(at.x, at.y, cell_width, cell_height),
                &Paint::Solid(background),
            );
            let ink = Paint::Solid(rgba(cell.fg, FOREGROUND));
            let symbol = cell.symbol().chars().next().unwrap_or(' ');
            if !draw_shape(&mut canvas, symbol, at, &ink) && symbol != ' ' {
                let span = TextSpan {
                    text: cell.symbol().to_owned(),
                    bold: cell.modifier.contains(Modifier::BOLD),
                    mono: true,
                    ..TextSpan::default()
                };
                let layout = engine.layout_spans(&[span], &style, None);
                canvas.draw_text(&layout, at, &ink);
            }
        }
        canvas
    }

    /// The up, down, left and right arms of a box drawing character.
    fn arms(symbol: char) -> Option<[bool; 4]> {
        Some(match symbol {
            '─' => [false, false, true, true],
            '│' => [true, true, false, false],
            '┌' | '╭' => [false, true, false, true],
            '┐' | '╮' => [false, true, true, false],
            '└' | '╰' => [true, false, false, true],
            '┘' | '╯' => [true, false, true, false],
            _ => return None,
        })
    }

    /// We draw box lines, block eighths and arrows as shapes because the
    /// bundled fonts have no glyphs for them, the way terminals draw them.
    fn draw_shape(canvas: &mut Canvas, symbol: char, at: Point, ink: &Paint) -> bool {
        let (width, height) = (f32::from(CELL.width), f32::from(CELL.height));
        let (middle_x, middle_y) = (at.x + (width / 2.0).floor(), at.y + (height / 2.0).floor());
        let code = u32::from(symbol);
        if let Some([up, down, left, right]) = arms(symbol) {
            let lines = [
                (up, PixelRect::new(middle_x, at.y, 1.0, height / 2.0)),
                (
                    down,
                    PixelRect::new(middle_x, middle_y, 1.0, height / 2.0 + 1.0),
                ),
                (left, PixelRect::new(at.x, middle_y, width / 2.0, 1.0)),
                (
                    right,
                    PixelRect::new(middle_x, middle_y, width / 2.0 + 1.0, 1.0),
                ),
            ];
            for (_, line) in lines.iter().filter(|(drawn, _)| *drawn) {
                canvas.fill_rect(*line, ink);
            }
        } else if (0x2581..=0x2588).contains(&code) {
            let filled = height * (code - 0x2580) as f32 / 8.0;
            canvas.fill_rect(
                PixelRect::new(at.x, at.y + height - filled, width, filled),
                ink,
            );
        } else if (0x2589..=0x258F).contains(&code) {
            let filled = width * (0x2590 - code) as f32 / 8.0;
            canvas.fill_rect(PixelRect::new(at.x, at.y, filled, height), ink);
        } else if symbol == '↑' || symbol == '↓' {
            for row in 0..5_u8 {
                let half = f32::from(if symbol == '↑' { row } else { 4 - row });
                let top = middle_y - 2.0 + f32::from(row);
                canvas.fill_rect(
                    PixelRect::new(middle_x - half, top, half * 2.0 + 1.0, 1.0),
                    ink,
                );
            }
        } else {
            return false;
        }
        true
    }

    #[test]
    fn the_sidebar_reports_the_picture_it_shows() {
        let mut viewer = Viewer::new(Vec::new());
        viewer.list.select(Some(1));
        let mut terminal = Terminal::new(TestBackend::new(100, 30)).expect("a test terminal");
        terminal
            .draw(|frame| viewer.draw(frame, CELL))
            .expect("the viewer draws");
        let shown = viewer.shown.as_ref().expect("the mandelbrot is drawn");
        let buffer = terminal.backend().buffer();
        let text: String = buffer.content.iter().map(|cell| cell.symbol()).collect();
        let size = format!("{} x {} pixels", shown.size.0, shown.size.1);
        assert!(text.contains(&size), "the sidebar reads {size}");

        let mut canvas = rasterize(buffer);
        let picture = Surface::from_rgba(shown.size.0, shown.size.1, shown.pixels.to_vec())
            .expect("exact pixels");
        let area = PixelRect::new(
            f32::from(shown.area.x) * f32::from(CELL.width),
            f32::from(shown.area.y) * f32::from(CELL.height),
            shown.size.0 as f32,
            shown.size.1 as f32,
        );
        canvas.draw_surface(&picture, area, 0.0);
        let surface = canvas.to_surface().expect("whole pixels");
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/screenshots");
        std::fs::create_dir_all(dir).expect("the target directory is writable");
        image::save_buffer(
            format!("{dir}/viewer.png"),
            surface.pixels(),
            surface.width(),
            surface.height(),
            image::ColorType::Rgba8,
        )
        .expect("the png writer saves under target");
    }
}
