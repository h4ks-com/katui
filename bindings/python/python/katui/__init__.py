"""Pixel apps in the terminal: a view of elements, messages that change your
state, and a runner that draws with kitty graphics, sixel or blocks."""

from typing import Any

from katui._katui import (
    Control,
    Element,
    Event,
    Image,
    Scroll,
    Span,
    Theme,
    Waker,
    __version__,
    html,
    markdown,
    run,
)

row = Element.row
column = Element.column
stack = Element.stack
text = Element.text
rich = Element.rich
image = Element.image
spacer = Element.spacer


class App:
    """A pixel UI program: state that shows itself as a view and changes on
    messages. Subclass it and pass an instance to `run`."""

    def start(self, waker: Waker) -> None:
        """Keep `waker` to send messages from other threads."""

    def view(self) -> Element:
        """The view of the current state."""
        raise NotImplementedError("return the view from App.view")

    def update(self, message: Any) -> Control | None:
        """Apply `message`. A callable message is called. `None` redraws."""
        if callable(message):
            result: Control | None = message()
            return result
        return None

    def input(self, event: Event) -> Control | None:
        """React to a key or a paste. `None` waits for the next event."""
        return None

    def theme(self, cell: tuple[int, int]) -> Theme:
        """The theme for cells of `cell` pixels, asked before every frame."""
        return Theme.dark(cell)


__all__ = [
    "App",
    "Control",
    "Element",
    "Event",
    "Image",
    "Scroll",
    "Span",
    "Theme",
    "Waker",
    "__version__",
    "column",
    "html",
    "image",
    "markdown",
    "rich",
    "row",
    "run",
    "spacer",
    "stack",
    "text",
]
