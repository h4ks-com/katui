//! Output pacing: writes frames in step with what the terminal has
//! actually consumed, so nothing queues up behind a slow terminal.

use std::io;
use std::time::Duration;

use katui_present::session;
use katui_proto::kitty;
use katui_proto::probe::Capabilities;
use katui_scene::CellSize;

use crate::events::EventPump;

/// A batch size well inside what a multiplexer buffers: rmux 0.10 drops
/// graphics commands past about 64 KB in flight, and 32 KB batches still
/// lost the odd tile.
const MULTIPLEXER_BATCH: usize = 16 * 1024;

/// Bytes per pane cell in one batch. tmux discards its whole output once
/// it holds eight bytes per cell of the client terminal, which is at
/// least as big as our pane, so we keep each batch at half of that.
const BATCH_BYTES_PER_CELL: usize = 4;

/// The first graphics query id a run waits on; each query takes its own
/// id, because a second client attached to a multiplexer answers too and
/// its answer must not stand in for the one being waited on.
const QUERY_IDS_FROM: u32 = 1_000_000;

/// How long a run waits for one answer before it stops asking.
const ANSWER_WAIT: Duration = Duration::from_millis(500);

/// How a run keeps its output in step with the terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Pacing {
    /// The frame clock alone, for terminals that answer no queries.
    #[default]
    Timer,
    /// Wait for the terminal after every frame.
    EachFrame,
    /// Wait after every batch of commands, sized for a multiplexer.
    Batches,
}

impl Pacing {
    /// The pacing `caps` can support.
    #[must_use]
    pub fn for_terminal(caps: &Capabilities) -> Self {
        match (caps.kitty, caps.multiplexed()) {
            (false, _) => Self::Timer,
            (true, false) => Self::EachFrame,
            (true, true) => Self::Batches,
        }
    }
}

/// Delivers frames under a [`Pacing`], dropping to the timer when the
/// terminal stops answering.
#[derive(Debug)]
pub(crate) struct Pacer {
    configured: Pacing,
    multiplexed: bool,
    pacing: Pacing,
    next_query: u32,
}

impl Pacer {
    pub(crate) fn new(pacing: Pacing, multiplexed: bool) -> Self {
        Self {
            configured: pacing,
            multiplexed,
            pacing,
            next_query: QUERY_IDS_FROM,
        }
    }

    /// Go back to the configured pacing: a pane that lost focus stops
    /// getting answers, which says nothing about the terminal.
    pub(crate) fn restore(&mut self) {
        self.pacing = self.configured;
    }

    fn take_query_id(&mut self) -> u32 {
        let query = self.next_query;
        self.next_query = self.next_query.wrapping_add(1).max(QUERY_IDS_FROM);
        query
    }

    /// Write `frame`; `false` when this frame lost the terminal's answers.
    /// The frame is always written whole.
    pub(crate) fn deliver<W: io::Write, M: Send + 'static>(
        &mut self,
        out: &mut W,
        frame: &[u8],
        size: CellSize,
        pump: &mut EventPump<M>,
    ) -> io::Result<bool> {
        if frame.is_empty() {
            return Ok(true);
        }
        let parts = if self.pacing == Pacing::Batches {
            kitty::split_at_commands(frame, batch_limit(size))
        } else {
            vec![frame]
        };
        let mut answering = true;
        for part in parts {
            let mut batch = part.to_vec();
            let query = (self.pacing != Pacing::Timer && answering).then(|| self.take_query_id());
            if let Some(query) = query {
                session::terminal_query(&mut batch, query)?;
            }
            if self.multiplexed {
                batch = kitty::wrap_for_passthrough(&batch);
            }
            out.write_all(&batch)?;
            out.flush()?;
            if let Some(query) = query {
                answering = pump.await_answer(query, ANSWER_WAIT);
            }
        }
        if !answering {
            self.pacing = Pacing::Timer;
        }
        Ok(answering)
    }
}

fn batch_limit(size: CellSize) -> usize {
    let cells = usize::from(size.columns) * usize::from(size.rows);
    MULTIPLEXER_BATCH.min(cells * BATCH_BYTES_PER_CELL)
}

#[cfg(test)]
mod tests {
    use katui_proto::probe::{Capabilities, Identity};
    use katui_scene::CellSize;

    use super::Pacing;

    #[test]
    fn multiplexers_get_batches_and_plain_terminals_no_queries() {
        let plain = Capabilities::default();
        assert_eq!(Pacing::for_terminal(&plain), Pacing::Timer);
        let kitty = Capabilities {
            kitty: true,
            ..Capabilities::default()
        };
        assert_eq!(Pacing::for_terminal(&kitty), Pacing::EachFrame);
        let muxed = Capabilities {
            identity: Some(Identity {
                name: "rmux".to_owned(),
                version: None,
            }),
            ..kitty
        };
        assert_eq!(Pacing::for_terminal(&muxed), Pacing::Batches);
    }

    #[test]
    fn a_small_pane_gets_batches_under_the_multiplexer_discard_limit() {
        let small = CellSize {
            columns: 80,
            rows: 24,
        };
        assert!(super::batch_limit(small) * 2 <= 80 * 24 * 8);
        let large = CellSize {
            columns: 300,
            rows: 90,
        };
        assert_eq!(super::batch_limit(large), super::MULTIPLEXER_BATCH);
    }
}
