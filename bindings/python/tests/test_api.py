"""The bindings' behavior: messages, widgets, themes, documents, errors."""

from collections.abc import Callable

import katui
import katui.game
import pytest
from katui import Control, Event, Image, Theme
from katui.game import V3, Camera, Light, Rng, srgb
from katui.testing import Screen, frame
from katui.widgets import (
    Edit,
    Focus,
    ListState,
    TextField,
    Toasts,
    TreeNode,
    TreeState,
    button,
    label,
    page,
    panel,
    slider,
    tabs,
)


class Recorder(katui.App[object]):
    def __init__(self, view: Callable[[Recorder], katui.Element[object]]) -> None:
        self.build = view
        self.messages: list[object] = []

    def view(self) -> katui.Element[object]:
        return self.build(self)

    def update(self, message: object) -> Control | None:
        self.messages.append(message)
        return None


def test_a_button_hands_its_message_to_update_as_is() -> None:
    app = Recorder(lambda app: panel(button("Go", ("go", 1))))
    screen = Screen(app, 300, 120)
    screen.render()
    screen.click(40, 40)
    assert app.messages == [("go", 1)]


def test_a_slider_calls_its_function_with_the_value() -> None:
    app = Recorder(lambda app: page(slider(0.5, lambda value: ("volume", round(value, 1)))))
    screen = Screen(app, 400, 100)
    screen.render()
    screen.click(300, 30)
    match app.messages[0]:
        case ("volume", float(value)):
            assert value > 0.5
        case other:
            raise AssertionError(f"the slider sent {other!r}")


def test_tabs_send_the_index_picked() -> None:
    app = Recorder(lambda app: page(tabs(["One", "Two"], 0, lambda index: index)))
    screen = Screen(app, 400, 100)
    screen.render()
    screen.click(110, 30)
    assert app.messages == [1]


def test_the_default_update_calls_a_callable_message() -> None:
    calls: list[int] = []

    class Plain(katui.App[object]):
        def view(self) -> katui.Element[object]:
            return panel(button("Go", lambda: calls.append(1)))

    screen = Screen(Plain(), 300, 120)
    screen.render()
    screen.click(40, 40)
    assert calls == [1]


def test_widgets_draw_with_the_apps_theme() -> None:
    class Paper(katui.App[object]):
        def view(self) -> katui.Element[object]:
            return page(label(str(Theme.current().cell)))

        def theme(self, cell: tuple[int, int]) -> Theme:
            return Theme.light(cell)

    image = Screen(Paper(), 200, 100).render()
    background = Theme.light().background
    assert background is not None
    assert image.pixel(199, 99) == background


def test_a_theme_takes_colors_as_tuples_or_hex() -> None:
    theme = Theme.dark()
    theme.accent = "#ff8800"
    theme.raised = (1, 2, 3)
    assert theme.accent == (255, 136, 0, 255)
    assert theme.raised == (1, 2, 3, 255)
    assert theme.scaled(2).cell == (20, 40)
    with pytest.raises(ValueError, match="rrggbb"):
        theme.accent = "#12"


def test_an_error_in_view_reaches_the_caller() -> None:
    class Broken(katui.App[object]):
        def view(self) -> katui.Element[object]:
            raise RuntimeError("view broke")

    with pytest.raises(RuntimeError, match="view broke"):
        Screen(Broken()).render()


def test_an_error_in_update_reaches_the_caller() -> None:
    class Broken(katui.App[object]):
        def view(self) -> katui.Element[object]:
            return panel(button("Go", "go"))

        def update(self, message: object) -> Control | None:
            raise KeyError(message)

    screen = Screen(Broken(), 300, 120)
    screen.render()
    with pytest.raises(KeyError):
        screen.click(40, 40)


def test_a_control_return_quits() -> None:
    class Quitter(katui.App[object]):
        def view(self) -> katui.Element[object]:
            return panel(button("Quit", lambda: Control.QUIT))

    screen = Screen(Quitter(), 300, 120)
    screen.render()
    screen.click(40, 40)
    assert screen.quit


def test_a_markdown_link_sends_its_href_by_default() -> None:
    app = Recorder(lambda app: page(katui.markdown("[open the keys](keys)")))
    screen = Screen(app, 400, 120)
    screen.render()
    for x in range(20, 120, 10):
        screen.click(x, 30)
        if app.messages:
            break
    assert app.messages[:1] == ["keys"]


def test_html_takes_images_from_a_dict() -> None:
    red = Image.from_rgba(2, 2, bytes([255, 0, 0, 255] * 4))
    source = '<img src="red.png" style="width: 40px; height: 40px">'
    app = Recorder(lambda app: katui.column(katui.html(source, images={"red.png": red})))
    image = Screen(app, 100, 100).render()
    assert image.pixel(10, 10) == (255, 0, 0, 255)


def test_children_skip_none_and_open_lists() -> None:
    app = Recorder(lambda app: katui.column(None, [label("a"), label("b")], label("c")))
    Screen(app, 100, 100).render()


def test_text_fields_and_lists_take_events() -> None:
    field = TextField("ab")
    assert field.handle(Event.press("c")) == Edit.CHANGED
    assert field.handle(Event.press("enter")) == Edit.SUBMITTED
    assert field.take() == "abc"
    rows = ListState(5, 3)
    assert rows.handle(Event.press("down"))
    assert rows.selected == 1
    assert rows.in_view() == (0, 3)
    with pytest.raises(ValueError, match="name a key"):
        Event.press("nothing")


def test_focus_moves_between_any_objects() -> None:
    focus = Focus(["note", "table"])
    assert focus.is_("note")
    assert focus.handle(Event.press("tab"))
    assert focus.current() == "table"


def test_trees_toasts_and_events_keep_their_state() -> None:
    roots = [TreeNode(1, "src", [TreeNode(2, "main.py")])]
    state = TreeState()
    state.handle(Event.press("down"), roots)
    assert state.selected == 1
    toasts = Toasts()
    toasts.push("saved", 0.0)
    assert toasts.expire()
    assert toasts.is_empty()
    paste = Event.paste("hello")
    assert (paste.kind, paste.text, paste.key) == ("paste", "hello", None)


def test_games_draw_through_frame() -> None:
    class Dot(katui.game.Game):
        def __init__(self) -> None:
            self.x = 10.0

        def step(self, keys: katui.game.Keys, dt: float) -> None:
            if keys.is_held("right"):
                self.x += 100 * dt

        def render(self, light: Light) -> None:
            light.rect((self.x, 10), 6, 6, srgb(255, 255, 255))

    dot = Dot()
    image = frame(dot, 80, 40, steps=6, held=["right"])
    assert dot.x == pytest.approx(20.0)
    assert image.pixel(22, 12) != image.pixel(70, 30)


def test_an_error_in_a_game_reaches_the_caller() -> None:
    class Broken(katui.game.Game):
        def render(self, light: Light) -> None:
            raise ValueError("render broke")

    with pytest.raises(ValueError, match="render broke"):
        frame(Broken())


def test_world_math_matches_the_engine() -> None:
    offset = (1.0, 1.0, 1.0)
    assert V3(1, 2, 3) + offset == V3(2, 3, 4)
    assert V3(1, 0, 0).cross((0, 1, 0)) == V3(0, 0, 1)
    assert list(V3(3, 0, 4).normalized()) == pytest.approx([0.6, 0, 0.8])
    camera = Camera((50, 50), 100, 10)
    assert camera.project((0, 0, 0)) == ((50.0, 50.0), 10.0)
    assert camera.project((0, 0, -20)) is None
    first, second = Rng(3), Rng(3)
    assert first.range(0, 1) == second.range(0, 1)
