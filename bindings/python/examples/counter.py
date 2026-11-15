"""A click counter, the smallest katui program."""

import katui
from katui.widgets import label, panel, primary_button


class Counter(katui.App[katui.Action]):
    def __init__(self) -> None:
        self.count = 0

    def view(self) -> katui.Element[katui.Action]:
        return panel(
            label(f"Clicked {self.count} times"),
            primary_button("Add one", self.add),
        ).align("center")

    def add(self) -> None:
        self.count += 1


def app() -> Counter:
    return Counter()


if __name__ == "__main__":
    katui.run(app())
