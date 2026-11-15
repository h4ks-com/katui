"""A tiny browser for local HTML pages: links open the next page, the wheel
and the arrow keys scroll, Backspace goes back."""

import sys
from pathlib import Path

import katui
from katui import Control, Event, Image, Scroll, Theme
from katui.widgets import ScrollState

SHOWCASE = Path(__file__).parents[3] / "crates/katui-app/examples/pages/showcase.html"


class Browser(katui.App):
    def __init__(self, start: Path) -> None:
        self.history = [start.resolve()]
        self.scroll = ScrollState()
        self.images: dict[str, Image] = {}

    @property
    def here(self) -> Path:
        return self.history[-1]

    def follow(self, href: str) -> None:
        target = (self.here.parent / href).resolve()
        if target.is_file():
            self.history.append(target)
            self.scroll = ScrollState()

    def image(self, src: str) -> Image | None:
        path = str((self.here.parent / src).resolve())
        if path not in self.images and Path(path).is_file():
            self.images[path] = Image.open(path)
        return self.images.get(path)

    def scrolled(self, scroll: Scroll) -> None:
        self.scroll.apply(scroll)

    def view(self) -> katui.Element:
        theme = Theme.current()
        try:
            document = katui.html(self.here.read_text(), on_link=self.follow, images=self.image)
        except OSError as error:
            document = katui.text(f"Check the path, the page did not open: {error}")
        return (
            katui.column(document)
            .scroll(self.scroll.offset)
            .on_scroll(self.scrolled)
            .scrollbar(theme.muted)
            .padding(theme.cell_height())
            .size("fill", "fill")
            .background(theme.background or theme.text)
        )

    def input(self, event: Event) -> Control | None:
        if event.key == "backspace" and len(self.history) > 1:
            self.history.pop()
            return Control.REDRAW
        return Control.REDRAW if self.scroll.handle(event, 40) else None


def app(start: Path = SHOWCASE) -> Browser:
    return Browser(start)


if __name__ == "__main__":
    katui.run(app(Path(sys.argv[1]) if len(sys.argv) > 1 else SHOWCASE))
