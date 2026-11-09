//! Presenters: layer 1 of the stack in ARCHITECTURE.md. A presenter is the
//! only place that produces bytes; it consumes a scene diff and writes to a
//! [`Sink`], which is stdout, a buffer, a socket or a test harness.

mod anchor;
pub mod blocks;
pub mod cells;
pub mod kitty;
pub mod select;
pub mod session;
pub mod surface;

/// Where a presenter writes its output: the presenters' one output seam.
/// Every presenter entry point in this crate bounds on [`Sink`], not on
/// [`std::io::Write`] directly, so the write path always goes through
/// this contract; a bare blanket impl keeps every writer a sink, so
/// tests capture output in a `Vec<u8>` and stdout needs no wrapping.
pub trait Sink: std::io::Write {}

impl<T: std::io::Write> Sink for T {}

#[cfg(test)]
mod tests {
    use super::Sink;

    fn write_greeting(sink: &mut dyn Sink) -> std::io::Result<()> {
        sink.write_all(b"\x1b_Ga=q\x1b\\")
    }

    #[test]
    fn any_writer_is_a_sink() {
        let mut buffer: Vec<u8> = Vec::new();
        write_greeting(&mut buffer).expect("writing to a Vec cannot fail");
        assert_eq!(buffer, b"\x1b_Ga=q\x1b\\");
    }
}
