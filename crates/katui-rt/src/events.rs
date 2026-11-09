//! The event pump: terminal input on its own thread and messages from
//! the application's own threads, merged into one queue.

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use katui_proto::input::{InputEvent, InputParser};

use crate::pane::{self, PaneView};
use crate::tty::RawInput;

/// How long a reader waits for input before checking for shutdown, and
/// the silence after which a lone escape counts as the Esc key.
const INPUT_POLL: Duration = Duration::from_millis(50);

/// How long a finishing pump keeps raw mode for input still travelling,
/// such as the release of the key that quit, and the silence that ends
/// that wait sooner.
const LATE_INPUT_LIMIT: Duration = Duration::from_millis(600);
const LATE_INPUT_SILENCE: Duration = Duration::from_millis(150);

/// One item off the event queue.
#[derive(Debug, PartialEq, Eq)]
pub enum Signal<M> {
    /// Something the terminal reported.
    Input(InputEvent),
    /// A message from one of the application's threads.
    Message(M),
    /// What our multiplexer pane can show changed.
    Pane(PaneView),
}

/// Lets any thread put a message on the event queue.
#[derive(Debug)]
pub struct Waker<M>(pub(crate) Sender<Signal<M>>);

impl<M> Clone for Waker<M> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<M> Waker<M> {
    /// Queue `message`; `false` once the event loop has ended.
    pub fn send(&self, message: M) -> bool {
        self.0.send(Signal::Message(message)).is_ok()
    }
}

/// The merged event queue and the thread that reads the terminal.
#[derive(Debug)]
pub struct EventPump<M> {
    sender: Sender<Signal<M>>,
    receiver: Receiver<Signal<M>>,
    queued: VecDeque<Signal<M>>,
    stop: Arc<AtomicBool>,
    reader: Option<JoinHandle<()>>,
}

impl<M: Send + 'static> EventPump<M> {
    /// A pump reading the controlling terminal, in raw mode until
    /// [`EventPump::finish`]. With no terminal it carries messages only.
    #[must_use]
    pub fn start() -> Self {
        let (sender, receiver) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let reader = RawInput::open().ok().map(|input| {
            let (sender, stop) = (sender.clone(), stop.clone());
            std::thread::spawn(move || read_terminal(input, &stop, &sender))
        });
        Self {
            sender,
            receiver,
            queued: VecDeque::new(),
            stop,
            reader,
        }
    }

    /// Report what the multiplexer `program` lets our pane show.
    pub fn watch_pane_visibility(&self, program: &str) {
        let sender = self.sender.clone();
        pane::watch(program.to_owned(), self.stop.clone(), move |view| {
            sender.send(Signal::Pane(view)).is_ok()
        });
    }

    /// Whether a terminal is being read.
    #[must_use]
    pub fn reads_terminal(&self) -> bool {
        self.reader.is_some()
    }

    /// A handle other threads use to queue messages.
    #[must_use]
    pub fn waker(&self) -> Waker<M> {
        Waker(self.sender.clone())
    }

    /// The next item, waiting at most `wait`.
    pub fn next(&mut self, wait: Duration) -> Option<Signal<M>> {
        if let Some(signal) = self.queued.pop_front() {
            return Some(signal);
        }
        self.receiver.recv_timeout(wait).ok()
    }

    /// Wait for the graphics answer naming `image`, holding everything
    /// else for later; `false` when no answer arrives within `wait`.
    pub fn await_answer(&mut self, image: u32, wait: Duration) -> bool {
        let deadline = Instant::now() + wait;
        while let Ok(signal) = self
            .receiver
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        {
            match signal {
                Signal::Input(InputEvent::GraphicsAnswer(answered)) if answered == image => {
                    return true;
                }
                Signal::Input(InputEvent::GraphicsAnswer(_)) => {}
                other => self.queued.push_back(other),
            }
        }
        false
    }

    /// Stop reading and leave raw mode once input goes quiet, so that
    /// answers and key releases still on their way never reach the shell.
    pub fn finish(self) {
        let deadline = Instant::now() + LATE_INPUT_LIMIT;
        while Instant::now() < deadline && self.receiver.recv_timeout(LATE_INPUT_SILENCE).is_ok() {}
    }
}

impl<M> Drop for EventPump<M> {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.reader.take() {
            let _ = handle.join();
        }
    }
}

fn read_terminal<M>(mut input: RawInput, stop: &AtomicBool, sender: &Sender<Signal<M>>) {
    let mut parser = InputParser::default();
    while !stop.load(Ordering::Relaxed) {
        let events = match input.read_chunk(INPUT_POLL) {
            Some(bytes) => parser.feed(&bytes),
            None => parser.idle().into_iter().collect(),
        };
        for event in events {
            let _ = sender.send(Signal::Input(event));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use katui_proto::input::{InputEvent, Key, Mods};

    use super::{EventPump, Signal};

    const NO_WAIT: Duration = Duration::from_millis(0);

    fn pump() -> EventPump<u32> {
        let (sender, receiver) = std::sync::mpsc::channel();
        EventPump {
            sender,
            receiver,
            queued: std::collections::VecDeque::new(),
            stop: std::sync::Arc::default(),
            reader: None,
        }
    }

    fn answer(image: u32) -> Signal<u32> {
        Signal::Input(InputEvent::GraphicsAnswer(image))
    }

    #[test]
    fn a_waker_delivers_from_another_thread() {
        let mut events = pump();
        let waker = events.waker();
        std::thread::spawn(move || waker.send(7))
            .join()
            .expect("the sender thread cannot panic");
        assert_eq!(events.next(NO_WAIT), Some(Signal::Message(7)));
        assert_eq!(events.next(NO_WAIT), None);
    }

    #[test]
    fn waiting_for_an_answer_keeps_other_events_in_order() {
        let mut events = pump();
        let key = InputEvent::Key(Key::Char('x'), Mods::NONE);
        events
            .sender
            .send(Signal::Input(key.clone()))
            .expect("the receiver is alive");
        events
            .sender
            .send(answer(5))
            .expect("the receiver is alive");
        events
            .sender
            .send(Signal::Message(9))
            .expect("the receiver is alive");
        assert!(events.await_answer(5, NO_WAIT));
        assert_eq!(events.next(NO_WAIT), Some(Signal::Input(key)));
        assert_eq!(events.next(NO_WAIT), Some(Signal::Message(9)));
    }

    #[test]
    fn unrelated_events_cannot_extend_the_wait_forever() {
        let mut events = pump();
        for _ in 0..3 {
            events
                .sender
                .send(Signal::Message(1))
                .expect("the receiver is alive");
        }
        let started = std::time::Instant::now();
        assert!(!events.await_answer(5, Duration::from_millis(20)));
        assert!(started.elapsed() < Duration::from_secs(2));
        assert_eq!(events.queued.len(), 3, "the events wait their turn");
    }

    #[test]
    fn a_stale_answer_cannot_stand_in_for_the_awaited_one() {
        let mut events = pump();
        events
            .sender
            .send(answer(4))
            .expect("the receiver is alive");
        assert!(!events.await_answer(5, NO_WAIT));
    }
}
