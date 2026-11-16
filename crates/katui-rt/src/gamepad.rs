//! Game controllers. The terminal never sees them, so we read them from the
//! system on a thread of our own and hand out snapshots in the standard
//! layout of the browser Gamepad API.

use std::sync::mpsc;
use std::time::Duration;

use gilrs::{Axis, Button, Gilrs};

/// How long the reading thread waits for an event before it checks again.
const POLL: Duration = Duration::from_millis(50);

/// A button, numbered as the standard gamepad layout numbers it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub enum PadButton {
    /// The bottom face button: A, or cross on a `PlayStation` pad.
    South = 0,
    /// The right face button: B, or circle.
    East = 1,
    /// The left face button: X, or square.
    West = 2,
    /// The top face button: Y, or triangle.
    North = 3,
    LeftBumper = 4,
    RightBumper = 5,
    LeftTrigger = 6,
    RightTrigger = 7,
    Select = 8,
    Start = 9,
    LeftStick = 10,
    RightStick = 11,
    Up = 12,
    Down = 13,
    Left = 14,
    Right = 15,
    /// The logo button in the middle.
    Home = 16,
}

/// A stick axis, numbered as the standard gamepad layout numbers it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub enum PadAxis {
    LeftX = 0,
    LeftY = 1,
    RightX = 2,
    RightY = 3,
}

/// The gilrs button behind each standard button, in standard order.
const BUTTONS: [Button; 17] = [
    Button::South,
    Button::East,
    Button::West,
    Button::North,
    Button::LeftTrigger,
    Button::RightTrigger,
    Button::LeftTrigger2,
    Button::RightTrigger2,
    Button::Select,
    Button::Start,
    Button::LeftThumb,
    Button::RightThumb,
    Button::DPadUp,
    Button::DPadDown,
    Button::DPadLeft,
    Button::DPadRight,
    Button::Mode,
];

/// The gilrs axis behind each standard axis, and whether we flip it so
/// down and right are positive.
const AXES: [(Axis, bool); 4] = [
    (Axis::LeftStickX, false),
    (Axis::LeftStickY, true),
    (Axis::RightStickX, false),
    (Axis::RightStickY, true),
];

/// One controller at one moment.
#[derive(Debug, Clone, PartialEq)]
pub struct Pad {
    /// The controller's slot, which stays the same while it is connected.
    pub index: usize,
    /// What the controller calls itself.
    pub name: String,
    /// How far each button is down, from 0 to 1, in [`PadButton`] order.
    pub buttons: [f32; 17],
    /// Each stick axis from -1 to 1, down and right positive, in
    /// [`PadAxis`] order.
    pub axes: [f32; 4],
}

impl Pad {
    /// How far `button` is down, from 0 to 1.
    #[must_use]
    pub fn button(&self, button: PadButton) -> f32 {
        self.buttons.get(button as usize).copied().unwrap_or(0.0)
    }

    /// Whether `button` is down past half way.
    #[must_use]
    pub fn pressed(&self, button: PadButton) -> bool {
        self.button(button) > 0.5
    }

    /// Where `axis` stands, from -1 to 1.
    #[must_use]
    pub fn axis(&self, axis: PadAxis) -> f32 {
        self.axes.get(axis as usize).copied().unwrap_or(0.0)
    }
}

/// Why controllers cannot be read.
#[derive(Debug, thiserror::Error)]
#[error("cannot read game controllers: {0}")]
pub struct GamepadError(String);

/// Read controllers on a thread of their own and call `on_change` with
/// every connected one whenever any of them changes, until it returns
/// `false`.
pub fn watch<F>(on_change: F) -> Result<(), GamepadError>
where
    F: FnMut(&[Pad]) -> bool + Send + 'static,
{
    let (started, outcome) = mpsc::channel();
    std::thread::spawn(move || {
        // gilrs stays on the thread that made it, since some platforms
        // tie it to one thread
        match Gilrs::new() {
            Ok(gilrs) => {
                let _ = started.send(Ok(()));
                read(gilrs, on_change);
            }
            Err(error) => {
                let _ = started.send(Err(GamepadError(error.to_string())));
            }
        }
    });
    outcome
        .recv()
        .unwrap_or_else(|_| Err(GamepadError("the reading thread stopped".to_owned())))
}

fn read<F: FnMut(&[Pad]) -> bool>(mut gilrs: Gilrs, mut on_change: F) {
    let mut last = Vec::new();
    loop {
        let mut changed = false;
        while gilrs.next_event_blocking(Some(POLL)).is_some() {
            changed = true;
        }
        if !changed {
            continue;
        }
        let pads = snapshot(&gilrs);
        if pads != last {
            if !on_change(&pads) {
                return;
            }
            last = pads;
        }
    }
}

fn snapshot(gilrs: &Gilrs) -> Vec<Pad> {
    gilrs
        .gamepads()
        .filter(|(_, pad)| pad.is_connected())
        .map(|(id, pad)| Pad {
            index: usize::from(id),
            name: pad.name().to_owned(),
            buttons: BUTTONS.map(|button| {
                pad.button_data(button)
                    .map_or(0.0, gilrs::ev::state::ButtonData::value)
            }),
            axes: AXES.map(|(axis, flip)| {
                let value = pad.value(axis);
                if flip { -value } else { value }
            }),
        })
        .collect()
}
