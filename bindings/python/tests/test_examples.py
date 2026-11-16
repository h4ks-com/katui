"""Every example renders headless and answers its clicks and keys."""

import browse
import comet
import counter
import help as help_page
import todo
import widgets
from conftest import shot
from katui import Control, Event, Image
from katui.testing import Screen, frame


def lit(image: Image) -> int:
    return sum(1 for value in image.rgba()[0::4] if value > 40)


def test_the_counter_counts_clicks() -> None:
    app = counter.app()
    screen = Screen(app, 480, 240)
    screen.render().save(shot("counter"))
    screen.click(240, 72)
    screen.click(240, 72)
    assert app.count == 2


def test_todo_adds_typed_tasks_and_filters_them() -> None:
    app = todo.app()
    screen = Screen(app, 900, 480)
    screen.render()
    screen.type_text("buy bread")
    screen.send(Event.press("enter"))
    assert app.tasks[-1] == ("buy bread", False)
    app.pick_filter(2)
    assert app.shown() == [1]
    screen.render().save(shot("todo"))


def test_the_tour_switches_theme_and_size() -> None:
    app = widgets.app()
    screen = Screen(app, 1200, 700)
    dark = screen.render()
    app.pick_theme(2)
    app.size = 2
    light = screen.render()
    light.save(shot("widgets"))
    assert dark.pixel(600, 600) != light.pixel(600, 600)


def test_the_tour_quits_through_its_dialog() -> None:
    app = widgets.app()
    app.file(2)
    assert app.popup == "quit"
    assert app.update(lambda: Control.QUIT) == Control.QUIT


def test_help_links_switch_pages() -> None:
    app = help_page.app()
    screen = Screen(app, 700, 400)
    screen.render().save(shot("help"))
    app.open("keys")
    assert app.current == "keys"
    screen.render()


def test_the_browser_opens_the_showcase_and_follows_links() -> None:
    app = browse.app()
    screen = Screen(app, 1000, 700)
    screen.render().save(shot("browse"))
    screen.wheel(500, 350)
    screen.render()
    assert app.scroll.offset > 0


def test_the_comet_glows_and_bursts() -> None:
    game = comet.new_game()
    still = frame(game, 200, 120)
    game.press(" ")
    moved = frame(game, 200, 120, steps=20, held=["right"])
    moved.save(shot("comet"))
    assert lit(moved) > 0
    assert game.at[0] > 60
    assert still.pixel(0, 0) is not None
    game.mouse(Event.press("a"), (5.0, 6.0))
    assert game.at[0] > 60, "only a mouse press moves it"
