//! The katui gallery: styled cells through the ratatui adapter, and a
//! generated gradient over the kitty graphics protocol when the terminal
//! speaks it. Run with `just demo-gallery`.
#![cfg(unix)] // the demos drive /dev/tty through katui-rt's unix layer

use std::io::{self, Write as _};

use katui_proto::probe::Capabilities;
use katui_ratatui::KatuiBackend;
use katui_scene::placement::{CellRect, ImageId};
use ratatui::Terminal;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Widget as _};

fn main() -> io::Result<()> {
    let size = katui_rt::tty::size().map_err(io::Error::other)?;
    let (columns, rows) = (size.columns, size.rows);
    let caps = katui_rt::tty::probe_default();
    // the stage hides the cursor for the run and restores it after; the
    // backend draws through its own stdout handle, the same terminal
    let stage =
        katui_rt::session::Stage::enter(io::stdout(), katui_rt::session::Teardown::default())
            .map_err(io::Error::other)?;
    let mut terminal =
        Terminal::new(KatuiBackend::new(columns, rows, io::stdout())).map_err(io::Error::other)?;
    terminal
        .draw(|frame| {
            Paragraph::new(vec![
                Line::from(Span::styled(
                    "katui gallery",
                    Style::new()
                        .fg(Color::LightGreen)
                        .add_modifier(Modifier::BOLD),
                )),
                Line::from(Span::styled(
                    "cells, styles and one image, through the katui scene",
                    Style::new().fg(Color::Gray),
                )),
                Line::from(format!(
                    "terminal: {}",
                    match &caps.identity {
                        Some(identity) => match &identity.version {
                            Some(version) => format!("{} {}", identity.name, version),
                            None => identity.name.clone(),
                        },
                        None => "unknown".to_owned(),
                    }
                )),
                Line::from(format!(
                    "graphics: {}",
                    match katui_present::select::select(&caps) {
                        katui_present::select::Backend::Kitty => "kitty protocol",
                        katui_present::select::Backend::Sixel => "sixel",
                        katui_present::select::Backend::Blocks => "blocks only",
                    }
                )),
            ])
            .block(Block::bordered())
            .render(frame.area(), frame.buffer_mut());
        })
        .map_err(io::Error::other)?;

    draw_gradient(&mut terminal, &caps, columns, rows);

    let mut out = io::stdout();
    out.write_all(format!("\x1b[{rows};1H\rkatui gallery: press enter to leave ").as_bytes())?;
    out.flush()?;
    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    let mut out = stage.leave().map_err(io::Error::other)?;
    out.write_all(b"\n")?;
    out.flush()
}

/// A horizontal gradient across the bottom half, through the image
/// pipeline: generated pixels, one transmit, one placement.
fn draw_gradient(
    terminal: &mut Terminal<KatuiBackend<io::Stdout>>,
    caps: &Capabilities,
    columns: u16,
    rows: u16,
) {
    let Some(cell) = caps.cell else {
        return;
    };
    let area_columns = columns.saturating_sub(2);
    let area_rows = (rows / 2).saturating_sub(1);
    if area_columns == 0 || area_rows == 0 {
        return;
    }
    let width = u32::from(area_columns) * u32::from(cell.width);
    let height = u32::from(area_rows) * u32::from(cell.height);
    let pixels = gradient(width, height);
    let area = CellRect {
        x: 1,
        y: rows / 2,
        columns: area_columns,
        rows: area_rows,
    };
    let result = terminal
        .backend_mut()
        .draw_image(ImageId(1), &pixels, width, height, area);
    let _ = result
        .map_err(io::Error::other)
        .and_then(|written| io::stdout().flush().map(|()| written));
}

/// One frame of pixel-perfect color stops, the cheap way to see the
/// protocol working.
#[allow(clippy::cast_sign_loss)]
// shares stay inside 0..=1 and both stops sit inside 0..=255, so the
// rounded mix can never leave the u8 range
fn gradient(width: u32, height: u32) -> Vec<u8> {
    const FROM: (u8, u8, u8) = (30, 30, 160);
    const MIDDLE: (u8, u8, u8) = (160, 30, 130);
    const TO: (u8, u8, u8) = (240, 140, 40);
    let channel = |from: u8, to: u8, share: f64| -> u8 {
        (f64::from(from) + (f64::from(to) - f64::from(from)) * share).round() as u8
    };
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        let row = f64::from(y) / f64::from(height.max(1));
        for x in 0..width {
            let along = f64::from(x) / f64::from(width.max(1));
            let (from, to, share) = if along < 0.5 {
                (FROM, MIDDLE, along * 2.0)
            } else {
                (MIDDLE, TO, (along - 0.5) * 2.0)
            };
            let lift = |color: u8| channel(color, 255, row * 0.25);
            pixels.extend_from_slice(&[
                lift(channel(from.0, to.0, share)),
                lift(channel(from.1, to.1, share)),
                lift(channel(from.2, to.2, share)),
                255,
            ]);
        }
    }
    pixels
}
