"""Every example drawn after a little use, for the screenshots in examples/README.md."""

from pathlib import Path

import browse
import comet
import counter
import help as help_page
import todo
import widgets
from conftest import shot
from katui import Event
from katui.testing import Screen, frame

WIDTH, HEIGHT = 900, 560


def screenshot(name: str) -> Path:
    return shot(f"example-{name}")


def test_counter_screenshot() -> None:
    app = counter.app()
    for _ in range(3):
        app.add()
    Screen(app, WIDTH, HEIGHT).render().save(screenshot("counter"))


def test_todo_screenshot() -> None:
    app = todo.app()
    screen = Screen(app, WIDTH, HEIGHT)
    for task in ["buy bread", "call the bank"]:
        screen.type_text(task)
        screen.send(Event.press("enter"))
    screen.type_text("walk the dog")
    screen.render().save(screenshot("todo"))


def test_widgets_screenshot() -> None:
    app = widgets.app()
    app.size = 0
    app.busy = True
    Screen(app, WIDTH, HEIGHT).render().save(screenshot("widgets"))


def test_help_screenshot() -> None:
    Screen(help_page.app(), WIDTH, HEIGHT).render().save(screenshot("help"))


def test_browse_screenshot() -> None:
    Screen(browse.app(), WIDTH, HEIGHT).render().save(screenshot("browse"))


def test_comet_screenshot() -> None:
    game = comet.new_game()
    frame(game, 360, 224, steps=40, held=["right", "down"])
    game.press(" ")
    frame(game, 360, 224, steps=20, held=["right"]).save(screenshot("comet"))
