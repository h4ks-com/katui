"""Messages are checked statically. Each `# type: ignore` line must be an
error: mypy's strict mode and ty's unused-ignore rule fail on one that is
not, so a weaker type here breaks `just py-lint`."""

import re
from enum import Enum
from typing import Literal, Never, assert_type

import katui
import katui.widgets
import pytest
from conftest import BINDINGS
from katui import Element, Waker, html, markdown
from katui.testing import Screen
from katui.widgets import (
    Focus,
    ScrollState,
    TextField,
    button,
    focusable_button,
    label,
    panel,
    scroll_view,
    slider,
    tabs,
    text_field,
)


class Message(Enum):
    SAVE = "save"
    QUIT = "quit"


class Editor(katui.App[Message]):
    def view(self) -> Element[Message]:
        return panel(
            label("Notes"),
            button("Save", Message.SAVE),
            button("Quit", Message.QUIT),
        )

    def update(self, message: Message) -> katui.Control | None:
        assert_type(message, Message)
        return None


class Wrong(katui.App[Message]):
    def view(self) -> Element[Message]:
        return panel(button("Save", "save"))  # type: ignore


def test_a_widget_carries_the_message_it_sends() -> None:
    save: Element[Message] = button("Save", Message.SAVE)
    leave: Element[Message] = focusable_button("Leave", Message.QUIT, True)
    stray: Element[Message] = focusable_button("Leave", "leave", True)  # type: ignore
    assert leave is not None and stray is not None
    nothing: Element[Never] = label("x")
    assert save is not None and nothing is not None
    assert Screen(Editor()).render().width > 0


def test_handlers_widen_and_callbacks_see_their_argument_types() -> None:
    clicked: Element[Message] = label("x").on_click(Message.SAVE)
    assert clicked is not None
    picked = tabs(["a", "b"], 0, lambda index: index * 2)
    assert_type(picked, Element[int])
    scrolled = scroll_view(ScrollState(), [button("x", Message.QUIT)], lambda s: s.offset)
    assert_type(scrolled, Element[Message | float])


def wrong_callbacks() -> None:
    slider(0.5, lambda value: value.upper())  # type: ignore
    text_field("text")  # type: ignore


def test_options_are_literal_types_and_the_engine_rejects_others() -> None:
    with pytest.raises(ValueError, match="align"):
        label("x").align("centre")  # type: ignore
    with pytest.raises(ValueError, match="length"):
        label("x").width("full")  # type: ignore
    assert text_field(TextField()) is not None


def test_documents_send_hrefs_unless_on_link_makes_another_message() -> None:
    assert_type(markdown("[a](b)"), Element[str])
    routed: Element[tuple[str, str]] = html(
        '<a href="x">x</a>', on_link=lambda href: ("open", href)
    )
    assert routed is not None


def wakers_and_focus(waker: Waker[Message]) -> None:
    waker.send(Message.SAVE)
    waker.send("stop")  # type: ignore
    focus = Focus[Literal["note", "table"]](["note", "table"])
    assert_type(focus.current(), Literal["note", "table"])
    focus.is_("list")  # type: ignore


def test_the_public_api_has_no_any() -> None:
    typed = BINDINGS / "python" / "katui"
    for path in [*typed.glob("*.py"), *typed.glob("*.pyi")]:
        assert not re.search(r"\bAny\b", path.read_text()), f"{path.name} uses Any"
