//! Game controllers. The terminal never sees them, so we read them from the
//! system on a thread of our own and hand out snapshots in the standard
//! layout of the browser Gamepad API.

use std::sync::mpsc;
use std::time::Duration;

use gilrs::{Axis, Button, Gilrs};
use katui_proto::input::Key;

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

/// How far a stick leans before it counts as a D-pad press.
const STICK_DEAD_ZONE: f32 = 0.5;

/// A menu layout: the D-pad and left stick are the arrows, the bottom face
/// button is enter and the right face button is escape.
pub const MENU_KEYS: [(PadButton, Key); 6] = [
    (PadButton::Up, Key::Up),
    (PadButton::Down, Key::Down),
    (PadButton::Left, Key::Left),
    (PadButton::Right, Key::Right),
    (PadButton::South, Key::Enter),
    (PadButton::East, Key::Escape),
];

/// The keys `pads` hold under `layout`, which names the key each button
/// stands for, in layout order and each once. The left stick leaning
/// toward a D-pad direction holds that D-pad button.
#[must_use]
pub fn held_keys(pads: &[Pad], layout: &[(PadButton, Key)]) -> Vec<Key> {
    let mut keys = Vec::new();
    for (button, key) in layout {
        if pads.iter().any(|pad| holds(pad, *button)) && !keys.contains(key) {
            keys.push(*key);
        }
    }
    keys
}

/// The keys that went down between the `before` and `now` snapshots
/// under `layout`, for menus that move one step per press.
#[must_use]
pub fn pressed_keys(before: &[Pad], now: &[Pad], layout: &[(PadButton, Key)]) -> Vec<Key> {
    let held_before = held_keys(before, layout);
    held_keys(now, layout)
        .into_iter()
        .filter(|key| !held_before.contains(key))
        .collect()
}

fn holds(pad: &Pad, button: PadButton) -> bool {
    let leans = |axis, sign: f32| pad.axis(axis) * sign > STICK_DEAD_ZONE;
    pad.pressed(button)
        || match button {
            PadButton::Up => leans(PadAxis::LeftY, -1.0),
            PadButton::Down => leans(PadAxis::LeftY, 1.0),
            PadButton::Left => leans(PadAxis::LeftX, -1.0),
            PadButton::Right => leans(PadAxis::LeftX, 1.0),
            _ => false,
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
        if gilrs.next_event_blocking(Some(POLL)).is_none() {
            continue;
        }
        // a controller in use reports without pause, so we take only the
        // events already queued and never wait for a quiet moment
        while gilrs.next_event().is_some() {}
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

#[cfg(test)]
mod tests {
    use katui_proto::input::Key;

    use super::{MENU_KEYS, Pad, PadButton, held_keys, pressed_keys};

    fn pad(buttons: &[PadButton], axes: [f32; 4]) -> Pad {
        let mut pad = Pad {
            index: 0,
            name: "pad".to_owned(),
            buttons: [0.0; 17],
            axes,
        };
        for button in buttons {
            pad.buttons[*button as usize] = 1.0;
        }
        pad
    }

    #[test]
    fn the_stick_leans_like_the_d_pad_and_each_key_shows_once() {
        let pads = [
            pad(&[PadButton::South], [-0.9, 0.2, 0.0, 0.0]),
            pad(&[PadButton::Left], [0.0; 4]),
        ];
        assert_eq!(
            held_keys(&pads, &MENU_KEYS),
            vec![Key::Left, Key::Enter],
            "a small lean down stays in the dead zone"
        );
    }

    #[test]
    fn a_menu_sees_a_held_button_once() {
        let idle = [pad(&[], [0.0; 4])];
        let east = [pad(&[PadButton::East], [0.0; 4])];
        assert_eq!(pressed_keys(&idle, &east, &MENU_KEYS), vec![Key::Escape]);
        assert!(pressed_keys(&east, &east, &MENU_KEYS).is_empty());
    }
}
