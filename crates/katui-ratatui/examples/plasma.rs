//! The katui plasma: a quarter-block animation through the blocks
//! presenter, the pixel path every truecolor terminal understands. Run
//! with `just demo-plasma`; it plays for a few seconds and leaves.

use std::io::{self, Write as _};
use std::time::Duration;

use katui_present::blocks::{Rgb, render_line};

fn main() -> io::Result<()> {
    let (columns, rows) = katui_rt::tty::size().map_err(io::Error::other)?;
    let mut out = io::stdout();
    out.write_all(b"\x1b[?25l")?;
    for tick in 0..150u32 {
        let frame = frame(tick, columns, rows);
        out.write_all(b"\x1b[H")?;
        for line in frame {
            out.write_all(&line)?;
            out.write_all(b"\r\n")?;
        }
        out.flush()?;
        std::thread::sleep(Duration::from_millis(33));
    }
    out.write_all(b"\x1b[?25h")?;
    out.flush()
}

/// One terminal line per entry: two pixel rows classified into quarter
/// blocks, the plasma's value driving the color.
fn frame(tick: u32, columns: u16, rows: u16) -> Vec<Vec<u8>> {
    let width = f64::from(columns) * 2.0;
    let height = f64::from(rows) * 2.0;
    let time = f64::from(tick) * 0.12;
    let mut lines = Vec::new();
    for cell_row in 0..rows {
        let mut top = Vec::new();
        let mut bottom = Vec::new();
        for cell_column in 0..columns {
            for (dy, sink) in [(0.0, &mut top), (1.0, &mut bottom)] {
                for dx in [0.0, 1.0] {
                    let x = f64::from(cell_column) * 2.0 + dx;
                    let y = f64::from(cell_row) * 2.0 + dy;
                    let value = plasma(x / width, y / height, time);
                    sink.push(palette(value));
                }
            }
        }
        let mut line = Vec::new();
        let _ = render_line(&mut line, &top, &bottom);
        lines.push(line);
    }
    lines
}

/// Classic interference plasma: three waves summed, the result kept in
/// zero to one.
fn plasma(x: f64, y: f64, time: f64) -> f64 {
    let horizontal = (x * 6.0 + time).sin();
    let vertical = (y * 6.0 - time * 0.7).sin();
    let diagonal = ((x + y) * 5.0 + time * 1.3).sin();
    let cross = (((x - y) * 4.0 - time).sin() + horizontal).sin();
    f64::midpoint((horizontal + vertical + diagonal + cross) / 4.0, 1.0)
}

/// Deep blue to hot pink through orange, the value steering the stop.
#[allow(clippy::cast_sign_loss)]
// the value is clamped and both color stops sit inside 0..=255, so the
// rounded mix can never leave the u8 range
fn palette(value: f64) -> Rgb {
    let value = value.clamp(0.0, 1.0);
    let mix = |from: f64, to: f64| (from + (to - from) * value).round() as u8;
    Rgb {
        r: mix(20.0, 255.0),
        g: mix(30.0, 90.0),
        b: mix(120.0, 60.0),
    }
}
