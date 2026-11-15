//! Small value controls: radio buttons, a switch and a slider.

use katui_draw::Paint;
use katui_proto::input::{InputEvent, Key};
use katui_ui::{Align, Element, Length};

use crate::theme::Theme;
use crate::view::styled;

/// A column of radio buttons titled `options`, `selected` filled, each
/// sending `choose(index)` when clicked.
pub fn radio<M>(options: &[&str], selected: usize, choose: impl Fn(usize) -> M) -> Element<M> {
    let theme = Theme::current();
    let side = theme.body();
    let rows = options.iter().enumerate().map(|(index, option)| {
        let chosen = index == selected;
        let dot = chosen.then(|| {
            Element::row([])
                .size(Length::Px(side * 0.5), Length::Px(side * 0.5))
                .radius(side * 0.25)
                .background(Paint::Solid(theme.accent))
        });
        let ring = Element::column(dot)
            .size(Length::Px(side), Length::Px(side))
            .radius(side / 2.0)
            .border(1.5, theme.outline(chosen))
            .align(Align::Center)
            .justify(Align::Center);
        Element::row([ring, styled(*option, theme.body(), theme.text)])
            .gap(theme.cell_width())
            .align(Align::Center)
            .on_click(choose(index))
    });
    Element::column(rows).gap(theme.cell_height() * 0.3)
}

/// A switch with `text` beside it, on or off, sending `message` when
/// clicked.
pub fn switch<M>(text: &str, on: bool, message: M) -> Element<M> {
    let theme = Theme::current();
    let height = theme.body();
    let knob = Element::row([])
        .size(Length::Px(height - 4.0), Length::Px(height - 4.0))
        .radius((height - 4.0) / 2.0)
        .background(Paint::Solid(if on { theme.on_accent } else { theme.text }));
    let (track, justify) = if on {
        (Paint::Solid(theme.accent), Align::End)
    } else {
        (theme.highlight.clone(), Align::Start)
    };
    let pill = Element::row([knob])
        .size(Length::Px(height * 1.8), Length::Px(height))
        .padding(2.0)
        .radius(height / 2.0)
        .background(track)
        .align(Align::Center)
        .justify(justify);
    Element::row([pill, styled(text, theme.body(), theme.text)])
        .gap(theme.cell_width())
        .align(Align::Center)
        .on_click(message)
}

/// A slider at `value`, from 0.0 to 1.0, that fills its parent's width.
/// Pressing or dragging it sends `message` with the value under the
/// pointer.
pub fn slider<M>(value: f32, message: impl Fn(f32) -> M + 'static) -> Element<M> {
    let theme = Theme::current();
    let value = value.clamp(0.0, 1.0);
    let height = theme.body();
    let track = height * 0.3;
    let filled = Element::row([])
        .height(Length::Px(track))
        .grow(value)
        .radius(track / 2.0)
        .background(Paint::Solid(theme.accent));
    let knob = Element::row([])
        .size(Length::Px(height), Length::Px(height))
        .radius(height / 2.0)
        .background(Paint::Solid(theme.text));
    let rest = Element::row([])
        .height(Length::Px(track))
        .grow(1.0 - value)
        .radius(track / 2.0)
        .background(theme.highlight.clone());
    Element::row([filled, knob, rest])
        .align(Align::Center)
        .height(Length::Px(height))
        .width(Length::Fill)
        .on_press(move |x, _| message(x))
}

/// The value after `event` moves a slider at `value` by `step`: the left
/// and right arrows step, home and end jump to the ends. `None` when
/// `event` is none of those keys.
#[must_use]
pub fn nudge(value: f32, event: &InputEvent, step: f32) -> Option<f32> {
    let InputEvent::Key(key, _) = event else {
        return None;
    };
    let moved = match key {
        Key::Left => value - step,
        Key::Right => value + step,
        Key::Home => 0.0,
        Key::End => 1.0,
        _ => return None,
    };
    Some(moved.clamp(0.0, 1.0))
}

#[cfg(test)]
mod tests {
    use katui_proto::input::{InputEvent, Key, Mods};

    use super::nudge;

    #[test]
    fn arrows_step_and_the_ends_clamp() {
        let key = |key| InputEvent::Key(key, Mods::NONE);
        assert_eq!(nudge(0.5, &key(Key::Right), 0.1), Some(0.6));
        assert_eq!(nudge(0.95, &key(Key::Right), 0.1), Some(1.0));
        assert_eq!(nudge(0.5, &key(Key::Home), 0.1), Some(0.0));
        assert_eq!(nudge(0.5, &key(Key::Up), 0.1), None);
    }
}
