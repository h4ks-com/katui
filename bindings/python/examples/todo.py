"""A to-do list: type a task, press Enter to add it, Tab to tick the selected one."""

import katui
from katui import Event, Theme
from katui.widgets import (
    Edit,
    ListState,
    TextField,
    checkbox,
    hint,
    list_view,
    page,
    panel,
    progress,
    tabs,
    text_field,
    title,
)

FILTERS = ["All", "Active", "Done"]
VISIBLE = 10


class Todo(katui.App):
    def __init__(self) -> None:
        self.tasks: list[tuple[str, bool]] = [("water the plants", False), ("fix the bike", True)]
        self.draft = TextField()
        self.filter = 0
        self.selection = ListState(len(self.tasks), VISIBLE)

    def shown(self) -> list[int]:
        wanted = {1: False, 2: True}.get(self.filter)
        return [index for index, (_, done) in enumerate(self.tasks) if wanted in (None, done)]

    def toggle(self, index: int) -> None:
        text, done = self.tasks[index]
        self.tasks[index] = (text, not done)

    def pick_filter(self, index: int) -> None:
        self.filter = index
        self.selection.resize(len(self.shown()), VISIBLE)

    def view(self) -> katui.Element:
        shown = self.shown()
        done = sum(1 for _, finished in self.tasks if finished)

        def row(position: int, selected: bool) -> katui.Element:
            index = shown[position]
            text, finished = self.tasks[index]
            return checkbox(text, finished, lambda: self.toggle(index))

        body = list_view(self.selection, row) if shown else hint("Type a task and press Enter")
        card = panel(
            title("Tasks"),
            tabs(FILTERS, self.filter, self.pick_filter),
            text_field(self.draft, "Add a task"),
            body,
            progress(done / len(self.tasks) if self.tasks else 0.0),
            hint("Press Up and Down to pick a task, Tab to toggle it"),
        ).width(Theme.current().columns(60))
        return page(card).align("center")

    def input(self, event: Event) -> katui.Control | None:
        if self.selection.handle(event):
            return katui.Control.REDRAW
        shown = self.shown()
        if event.key == "tab" and shown:
            self.toggle(shown[self.selection.selected])
            return katui.Control.REDRAW
        edit = self.draft.handle(event)
        if edit == Edit.SUBMITTED and self.draft.text.strip():
            self.tasks.append((self.draft.take().strip(), False))
            self.selection.resize(len(self.shown()), VISIBLE)
        return None if edit == Edit.IGNORED else katui.Control.REDRAW


def app() -> Todo:
    return Todo()


if __name__ == "__main__":
    katui.run(app())
