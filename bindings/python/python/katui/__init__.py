"""Pixel apps in the terminal: a view of elements, messages that change your
state, and a runner that draws with kitty graphics, sixel or blocks."""

from collections.abc import Callable

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
    anchor_id,
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

type Action = Callable[[], Control | None] | None
"""The message of an app that acts in its callbacks: a callable the default
`update` calls, whose result decides what happens next, or `None`, when a
value callback like a slider's already did the work. Such apps subclass
`App[Action]`."""


class App[M]:
    """A pixel UI program whose clicks, keys and timers send messages of type
    `M`: state that shows itself as a view and changes on messages. Subclass
    it and pass an instance to `run`."""

    def start(self, waker: Waker[M]) -> None:
        """Keep `waker` to send messages from other threads."""

    def view(self) -> Element[M]:
        """The view of the current state."""
        raise NotImplementedError("return the view from App.view")

    def update(self, message: M) -> Control | None:
        """Apply `message`. A callable message is called. `None` redraws."""
        if callable(message):
            result = message()
            return result if isinstance(result, Control) else None
        return None

    def input(self, event: Event) -> Control | None:
        """React to a key or a paste. `None` waits for the next event."""
        return None

    def theme(self, cell: tuple[int, int]) -> Theme:
        """The theme for cells of `cell` pixels, asked before every frame."""
        return Theme.dark(cell)


__all__ = [
    "Action",
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
    "anchor_id",
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
