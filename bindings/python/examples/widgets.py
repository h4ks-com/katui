"""The widget tour: a menu, a file tree beside a page of controls, a table,
a note, toasts, a dialog, and themes and sizes that switch at runtime."""

import time

import katui
from katui import Control, Event, Theme
from katui.widgets import (
    Column,
    Edit,
    Focus,
    ListState,
    Scroll,
    ScrollState,
    TextArea,
    Toasts,
    TreeNode,
    TreeState,
    button,
    dialog,
    heading,
    hint,
    key_hints,
    label,
    menu,
    panel,
    primary_button,
    radio,
    scroll_view,
    select,
    slider,
    spinner,
    split,
    switch,
    table,
    text_area,
    toasts,
    tooltip,
    tree,
)

THEMES = ["Dusk", "Forest", "Paper"]
SIZES = ["Small", "Medium", "Large"]
SCALES = [0.85, 1.0, 1.2]
FILE_MENU = ["Count the note's lines", "Clear the note", "Quit"]
TOOLS = [["ripgrep", "search", "41k"], ["fzf", "finder", "62k"], ["bat", "pager", "47k"]]
FILES = [
    TreeNode(1, "src", [TreeNode(2, "main.py"), TreeNode(3, "widgets.py")]),
    TreeNode(4, "README.md"),
]


def forest(cell: tuple[int, int]) -> Theme:
    theme = Theme.dark(cell)
    theme.background = (18, 28, 22)
    theme.raised = (28, 42, 33)
    theme.highlight = (42, 64, 50)
    theme.border = (58, 84, 66)
    theme.accent = (120, 214, 150)
    return theme


class Tour(katui.App):
    def __init__(self) -> None:
        self.popup: str | None = None
        self.palette = 0
        self.size = 1
        self.busy = False
        self.volume = 0.4
        self.tip = False
        self.sidebar = 0.25
        self.table = ListState(len(TOOLS), 4)
        self.note = TextArea(4)
        self.page = ScrollState()
        self.files = TreeState()
        self.files.toggle(1)
        self.toasts = Toasts()
        self.focus = Focus(["note", "table"])
        self.started = time.monotonic()

    def theme(self, cell: tuple[int, int]) -> Theme:
        theme = [Theme.dark, forest, Theme.light][self.palette](cell)
        return theme.scaled(SCALES[self.size])

    def start(self, waker: katui.Waker) -> None:
        waker.every(0.25, self.tick)

    def tick(self) -> Control:
        expired = self.toasts.expire()
        return Control.REDRAW if self.busy or expired else Control.WAIT

    def toggle(self, popup: str) -> None:
        self.popup = None if self.popup == popup else popup

    def file(self, index: int) -> None:
        self.popup = "quit" if index == 2 else None
        if index == 0:
            self.toasts.push(f"The note has {len(self.note.text.splitlines())} lines")
        elif index == 1:
            self.note.take()

    def pick_theme(self, index: int) -> None:
        self.palette, self.popup = index, None

    def controls(self) -> katui.Element:
        theme = Theme.current()
        working = (
            katui.row(spinner(time.monotonic() - self.started), label("Turn off to stop"))
            .gap(theme.cell_width())
            .align("center")
            if self.busy
            else label("Turn on the switch to start working").color(theme.muted)
        )
        return panel(
            heading("Settings"),
            hint("Theme"),
            select(
                THEMES,
                self.palette,
                self.popup == "themes",
                lambda: self.toggle("themes"),
                self.pick_theme,
            ),
            hint("Size"),
            radio(SIZES, self.size, lambda index: setattr(self, "size", index)),
            switch(
                "Work in the background", self.busy, lambda: setattr(self, "busy", not self.busy)
            ),
            working,
            hint(f"Volume {self.volume * 100:.0f}%"),
            slider(self.volume, lambda value: setattr(self, "volume", value)),
            tooltip(
                primary_button("Count lines", lambda: self.file(0)),
                "Count the lines in the note",
                self.tip,
                lambda: setattr(self, "tip", True),
                lambda: setattr(self, "tip", False),
            ),
        ).width("fill")

    def data(self) -> katui.Element:
        return panel(
            heading("Tools"),
            table(
                [Column("Name", 12), Column("Kind", 10), Column("Stars")],
                TOOLS,
                self.table,
                self.table.select,
            ),
            hint("Note"),
            text_area(self.note, "Write a note", self.focus.is_("note")),
        ).width("fill")

    def view(self) -> katui.Element:
        theme = Theme.current()
        bar = (
            katui.row(
                menu(
                    "File", FILE_MENU, self.popup == "file", lambda: self.toggle("file"), self.file
                ),
                katui.spacer(),
                key_hints(
                    [("Tab", "switch field"), ("Esc", "close the popup"), ("Ctrl+C", "quit")]
                ),
            )
            .align("center")
            .padding_xy(theme.cell_width(), theme.cell_height() * 0.3)
            .width("fill")
            .background(theme.raised or theme.text)
        )
        sidebar = (
            katui.column(tree(FILES, self.files, self.files.toggle, self.files.select))
            .padding(theme.cell_height() * 0.5)
            .size("fill", "fill")
            .background(theme.raised or theme.text)
        )
        page = scroll_view(self.page, [self.controls(), self.data()], self.scrolled).padding(
            theme.cell_height()
        )
        body = split(sidebar, page, self.sidebar, lambda at: setattr(self, "sidebar", at))
        quit_dialog = None
        if self.popup == "quit":
            quit_dialog = dialog(
                "Quit the tour?",
                label("Quit now to discard the note.").color(theme.muted),
                [button("Stay", self.dismiss), primary_button("Quit", lambda: Control.QUIT)],
                self.dismiss,
            )
        return (
            katui.column(bar, body, toasts(self.toasts), quit_dialog)
            .size("fill", "fill")
            .background(theme.background or theme.text)
        )

    def scrolled(self, scroll: Scroll) -> None:
        self.page.apply(scroll)

    def dismiss(self) -> None:
        self.popup = None

    def input(self, event: Event) -> Control | None:
        if event.key == "escape":
            self.popup = None
            return Control.REDRAW
        if self.focus.handle(event):
            return Control.REDRAW
        if self.focus.is_("note"):
            handled = self.note.handle(event) != Edit.IGNORED
        else:
            handled = self.table.handle(event)
        return Control.REDRAW if handled or self.page.handle(event, 40) else None


def app() -> Tour:
    return Tour()


if __name__ == "__main__":
    katui.run(app())
