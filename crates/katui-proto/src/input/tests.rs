#[test]
fn every_key_name_reads_back_as_its_key() {
    let keys = [Key::Char('a'), Key::Char(' '), Key::Function(12)]
        .into_iter()
        .chain(super::NAMED_KEYS.iter().map(|(_, key)| *key));
    for key in keys {
        assert_eq!(Key::from_name(&key.name()), Some(key), "{key:?}");
    }
    assert_eq!(Key::from_name("space"), Some(Key::Char(' ')));
    assert_eq!(Key::from_name("nothing"), None);
    assert_eq!(Key::from_name("fx"), None);
}

use super::{Button, InputEvent, InputParser, Key, Mods, MouseAction, MouseEvent, Pointer};
use crate::probe::CellPixels;

fn parse(bytes: &[u8]) -> Vec<InputEvent> {
    InputParser::default().feed(bytes)
}

#[test]
fn a_report_beyond_the_grid_switches_the_pointer_to_pixels() {
    let cell = CellPixels {
        width: 7,
        height: 17,
    };
    let click = |x, y| MouseEvent {
        action: MouseAction::Press(Button::Left),
        x,
        y,
        mods: Mods::NONE,
    };
    let mut pointer = Pointer::default();
    pointer.resize(80, 25);
    pointer.observe(&InputEvent::Mode(1016, false));
    assert_eq!(pointer.locate(&click(2, 1), cell), (17.5, 25.5));
    assert_eq!(
        pointer.locate(&click(85, 26), cell),
        (598.5, 450.5),
        "a cell report just past a grid that grew stays in cells"
    );
    assert_eq!(pointer.locate(&click(210, 159), cell), (210.0, 159.0));
    assert_eq!(
        pointer.locate(&click(2, 1), cell),
        (2.0, 1.0),
        "later reports stay in pixels"
    );
    let mut honored = Pointer::default();
    honored.observe(&InputEvent::Mode(1016, true));
    assert_eq!(honored.locate(&click(2, 1), cell), (2.0, 1.0));
    let retina = CellPixels {
        width: 14,
        height: 34,
    };
    let mut scaled = Pointer::new(Some(cell));
    scaled.resize(80, 25);
    assert_eq!(
        scaled.locate(&click(211, 92), retina),
        (422.0, 184.0),
        "reports in points land on the double density surface"
    );
}

const CTRL: Mods = Mods {
    shift: false,
    alt: false,
    ctrl: true,
};

fn plain(pressed: Key) -> InputEvent {
    InputEvent::Key(pressed, Mods::NONE)
}

#[test]
fn key_event_types_separate_releases_from_presses() {
    let mut parser = InputParser::default();
    assert_eq!(
        parser.feed(b"\x1b[97;1:1u\x1b[97;1:2u\x1b[97;1:3u\x1b[1;1:3A"),
        vec![
            InputEvent::Key(Key::Char('a'), Mods::NONE),
            InputEvent::Key(Key::Char('a'), Mods::NONE),
            InputEvent::KeyUp(Key::Char('a'), Mods::NONE),
            InputEvent::KeyUp(Key::Up, Mods::NONE),
        ]
    );
}

#[test]
fn plain_text_and_utf8_become_characters() {
    assert_eq!(
        parse("aé🔥".as_bytes()),
        vec![
            plain(Key::Char('a')),
            plain(Key::Char('é')),
            plain(Key::Char('🔥'))
        ]
    );
}

#[test]
fn a_character_split_across_reads_waits_for_its_tail() {
    let mut parser = InputParser::default();
    let bytes = "🔥".as_bytes();
    assert_eq!(parser.feed(&bytes[..2]), vec![]);
    assert_eq!(parser.feed(&bytes[2..]), vec![plain(Key::Char('🔥'))]);
}

#[test]
fn control_bytes_are_named_keys_or_ctrl_chords() {
    assert_eq!(
        parse(b"\r\t\x7f\x03"),
        vec![
            plain(Key::Enter),
            plain(Key::Tab),
            plain(Key::Backspace),
            InputEvent::Key(Key::Char('c'), CTRL),
        ]
    );
}

#[test]
fn arrows_and_editing_keys_parse_in_every_form() {
    assert_eq!(
        parse(b"\x1b[A\x1bOB\x1b[3~\x1b[5~\x1b[H"),
        vec![
            plain(Key::Up),
            plain(Key::Down),
            plain(Key::Delete),
            plain(Key::PageUp),
            plain(Key::Home),
        ]
    );
}

#[test]
fn modified_arrows_report_their_modifiers() {
    assert_eq!(parse(b"\x1b[1;5C"), vec![InputEvent::Key(Key::Right, CTRL)]);
}

#[test]
fn kitty_keyboard_form_reports_keys_and_modifiers() {
    assert_eq!(
        parse(b"\x1b[27u\x1b[13;2u\x1b[97;5u"),
        vec![
            plain(Key::Escape),
            InputEvent::Key(
                Key::Enter,
                Mods {
                    shift: true,
                    ..Mods::NONE
                }
            ),
            InputEvent::Key(Key::Char('a'), CTRL),
        ]
    );
}

#[test]
fn sgr_mouse_reports_press_release_motion_and_wheel() {
    let at = |action, x, y| {
        InputEvent::Mouse(MouseEvent {
            action,
            x,
            y,
            mods: Mods::NONE,
        })
    };
    assert_eq!(
        parse(b"\x1b[<0;11;21M\x1b[<0;11;21m\x1b[<35;5;6M\x1b[<32;7;8M\x1b[<64;1;1M\x1b[<65;1;1M"),
        vec![
            at(MouseAction::Press(Button::Left), 10, 20),
            at(MouseAction::Release(Button::Left), 10, 20),
            at(MouseAction::Move, 4, 5),
            at(MouseAction::Drag(Button::Left), 6, 7),
            at(MouseAction::WheelUp, 0, 0),
            at(MouseAction::WheelDown, 0, 0),
        ]
    );
}

#[test]
fn a_paste_arrives_as_one_block_even_across_reads() {
    let mut parser = InputParser::default();
    assert_eq!(parser.feed(b"\x1b[200~hello\x1b[A"), vec![]);
    assert_eq!(
        parser.feed(b" world\x1b[201~q"),
        vec![
            InputEvent::Paste("hello\x1b[A world".to_owned()),
            plain(Key::Char('q'))
        ]
    );
}

#[test]
fn a_paste_that_stalls_stays_one_paste() {
    let mut parser = InputParser::default();
    assert_eq!(parser.feed(b"\x1b[200~one\n"), vec![]);
    assert_eq!(parser.idle(), None);
    assert_eq!(
        parser.feed(b"two\x1b[201~"),
        vec![InputEvent::Paste("one\ntwo".to_owned())]
    );
}

#[test]
fn keypad_enter_sends_and_other_functional_keys_type_nothing() {
    assert_eq!(parse(b"\x1b[57414u"), vec![plain(Key::Enter)]);
    assert_eq!(parse(b"\x1b[57376u"), vec![]);
}

#[test]
fn focus_and_mode_reports_parse() {
    assert_eq!(
        parse(b"\x1b[I\x1b[O\x1b[?1016;1$y\x1b[?1016;0$y"),
        vec![
            InputEvent::Focus(true),
            InputEvent::Focus(false),
            InputEvent::Mode(1016, true),
            InputEvent::Mode(1016, false),
        ]
    );
}

#[test]
fn graphics_answers_report_their_image_even_when_they_fail() {
    assert_eq!(
        parse(b"\x1b_Gi=29;OK\x1b\\\x1b_Gi=30,r=1;ENOENT:gone\x1b\\"),
        vec![
            InputEvent::GraphicsAnswer(29),
            InputEvent::GraphicsAnswer(30)
        ]
    );
}

#[test]
fn a_legacy_back_tab_is_shift_tab() {
    let back = Mods {
        shift: true,
        ..Mods::NONE
    };
    assert_eq!(parse(b"\x1b[Z"), vec![InputEvent::Key(Key::Tab, back)]);
}

#[test]
fn a_device_attributes_answer_is_one_event_and_no_key() {
    assert_eq!(parse(b"\x1b[?62;4;22c"), vec![InputEvent::DeviceAttributes]);
}

#[test]
fn a_q_inside_an_answer_is_no_key() {
    assert_eq!(
        parse(b"\x1b_Gi=5;qqq\x1b\\"),
        vec![InputEvent::GraphicsAnswer(5)]
    );
}

#[test]
fn a_lone_escape_is_the_esc_key_once_input_goes_quiet() {
    let mut parser = InputParser::default();
    assert_eq!(parser.feed(b"\x1b"), vec![]);
    assert_eq!(parser.idle(), Some(plain(Key::Escape)));
    assert_eq!(parser.idle(), None);
}

#[test]
fn escape_right_before_an_answer_is_still_the_esc_key() {
    assert_eq!(
        parse(b"\x1b\x1b_Gi=7;OK\x1b\\"),
        vec![plain(Key::Escape), InputEvent::GraphicsAnswer(7)]
    );
}

#[test]
fn an_alt_chord_reports_alt_and_cannot_wedge_the_parser() {
    assert_eq!(
        parse(b"\x1bx"),
        vec![InputEvent::Key(
            Key::Char('x'),
            Mods {
                alt: true,
                ..Mods::NONE
            }
        )]
    );
    let mut parser = InputParser::default();
    assert_eq!(parser.feed(b"\x1b]"), vec![]);
    assert_eq!(parser.idle(), None, "an open string is no Esc key");
    assert_eq!(parser.feed(b"q"), vec![plain(Key::Char('q'))]);
}

#[test]
fn arrow_keys_never_read_as_quit_keys() {
    assert!(parse(b"\x1b[A").iter().all(|event| !event.is_quit_key()));
}

#[test]
fn the_quit_keys_are_q_escape_and_ctrl_c() {
    assert!(plain(Key::Char('q')).is_quit_key());
    assert!(plain(Key::Escape).is_quit_key());
    assert!(InputEvent::Key(Key::Char('c'), CTRL).is_quit_key());
    assert!(!plain(Key::Char('c')).is_quit_key());
}

#[test]
fn random_byte_streams_never_panic() {
    let mut rng = crate::fuzz::XorShift::new(0x1234_5678_9abc_def0);
    for _ in 0..3000 {
        let mut parser = InputParser::default();
        for _ in 0..4 {
            let length = (rng.next() % 64) as usize;
            let mut bytes: Vec<u8> = (0..length).map(|_| rng.byte()).collect();
            if rng.next().is_multiple_of(2) {
                let at = (rng.next() as usize) % (length + 1);
                bytes.splice(at..at, b"\x1b[<".iter().copied());
            }
            let _ = parser.feed(&bytes);
        }
        let _ = parser.idle();
    }
}
