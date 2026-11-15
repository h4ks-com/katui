"""A help screen written in Markdown, its links switching pages."""

import katui
from katui.widgets import hint, page

PAGES = {
    "start": """\
# Help

Press **Ctrl+C** to quit. Read [the keys](keys) or [the credits](credits).

- Arrows move
- Enter opens
""",
    "keys": "# Keys\n\n- **Up** and **Down** move\n- **Enter** opens\n\n[Back](start)",
    "credits": "# Credits\n\nDrawn by katui in real pixels.\n\n[Back](start)",
}


class Help(katui.App):
    def __init__(self) -> None:
        self.current = "start"

    def open(self, name: str) -> None:
        self.current = name if name in PAGES else "start"

    def view(self) -> katui.Element:
        return page(
            katui.markdown(PAGES[self.current], on_link=self.open),
            hint(f"You are on the {self.current} page"),
        )


def app() -> Help:
    return Help()


if __name__ == "__main__":
    katui.run(app())
