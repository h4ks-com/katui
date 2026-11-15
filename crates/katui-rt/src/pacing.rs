//! Output pacing: writes frames in step with what the terminal has
//! actually consumed, so nothing queues up behind a slow terminal.

use std::io;
use std::time::Duration;

use katui_present::session;
use katui_proto::kitty::{self, IdSpace};
use katui_proto::probe::{Capabilities, ProbeSource};
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
const QUERY_IDS_UNTIL: u32 = 0x00f0_0000;

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
    /// Wait for the device attributes answer after every frame, for a
    /// terminal without kitty graphics that answered the probe.
    DeviceAttributes,
}

impl Pacing {
    /// The pacing `caps` can support.
    #[must_use]
    pub fn for_terminal(caps: &Capabilities) -> Self {
        let answers = caps.trust == ProbeSource::Probed;
        match (caps.kitty, caps.multiplexed()) {
            (false, false) if answers => Self::DeviceAttributes,
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
    space: IdSpace,
}

impl Pacer {
    pub(crate) fn new(pacing: Pacing, multiplexed: bool, space: IdSpace) -> Self {
        Self {
            configured: pacing,
            multiplexed,
            pacing,
            next_query: QUERY_IDS_FROM,
            space,
        }
    }

    /// Go back to the configured pacing: a pane that lost focus stops
    /// getting answers, which says nothing about the terminal.
    pub(crate) fn restore(&mut self) {
        self.pacing = self.configured;
    }

    /// Whether the next frame waits for the terminal's answer.
    pub(crate) fn asks(&self) -> bool {
        self.pacing != Pacing::Timer
    }

    fn take_query_id(&mut self) -> u32 {
        let local = self.next_query;
        self.next_query = if local + 1 < QUERY_IDS_UNTIL {
            local + 1
        } else {
            QUERY_IDS_FROM
        };
        self.space.image(local).0
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
        if self.pacing == Pacing::DeviceAttributes {
            return self.deliver_with_device_attributes(out, frame, pump);
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

    fn deliver_with_device_attributes<W: io::Write, M: Send + 'static>(
        &mut self,
        out: &mut W,
        frame: &[u8],
        pump: &mut EventPump<M>,
    ) -> io::Result<bool> {
        out.write_all(frame)?;
        out.write_all(DEVICE_ATTRIBUTES_QUERY)?;
        out.flush()?;
        let answering = pump.await_device_attributes(ANSWER_WAIT);
        if !answering {
            self.pacing = Pacing::Timer;
        }
        Ok(answering)
    }
}

const DEVICE_ATTRIBUTES_QUERY: &[u8] = b"\x1b[c";

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
        let answering_plain = Capabilities {
            trust: katui_proto::probe::ProbeSource::Probed,
            ..Capabilities::default()
        };
        assert_eq!(
            Pacing::for_terminal(&answering_plain),
            Pacing::DeviceAttributes,
            "a terminal that answered the probe paces on device attributes"
        );
    }

    #[test]
    fn a_multiplexed_pacer_asks_with_ids_of_its_own_space() {
        let space = katui_proto::kitty::IdSpace::of_process(4242);
        let mut pacer = super::Pacer::new(Pacing::Batches, true, space);
        let first = pacer.take_query_id();
        assert_eq!(first >> 24, space.image(0).0 >> 24);
        assert_ne!(pacer.take_query_id(), first);
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
