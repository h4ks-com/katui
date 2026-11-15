"""Rust is the source of truth for the API. `bindings/rust-api.json` lists
every public widget, game item, type method, app and game hook, and Control
and Edit variant of the Rust crates; each has a Python counterpart here, or
an entry in OMITTED saying why Python does without it."""

import json
from types import ModuleType

import katui
import katui.game
import katui.widgets
import pytest
from conftest import REPO

API = json.loads((REPO / "bindings" / "rust-api.json").read_text())

RENAMED = {
    "list": "list_view",
    "markdown_view": "markdown",
    "HeldKeys": "Keys",
    "v3": "V3",
    "new": "__init__",
    "with_text": "__init__",
    "leaf": "__init__",
    "branch": "__init__",
    "is": "is_",
    "len": "__len__",
    "push_for": "push",
    "to_surface": "to_image",
    "set_text": "text",
}

OMITTED = {
    "Orientation": "split takes the strings 'horizontal' and 'vertical'",
    "Blend": "drawing calls take the strings 'over' and 'add'",
    "P2": "points are (x, y) tuples",
    "Rgb": "light colors are (r, g, b) float tuples",
    "Key": "keys are named by strings like 'a', 'enter' and 'up'",
    "HtmlDefaults": "html and markdown take font_size, color, link and tint arguments",
    "HtmlHost": "html and markdown take on_link and images arguments",
    "scope": "the app runner and Screen set the theme around every view",
    "tick": "the game loop feeds the held keys",
    "press": "the game loop feeds the held keys",
    "release": "the game loop feeds the held keys",
    "clear": "the game loop feeds the held keys",
}

MODULES: dict[str, ModuleType] = {
    "katui-widgets": katui.widgets,
    "katui-game": katui.game,
    "katui-content": katui,
}

CLASSES: dict[str, object] = {
    "Element": katui.Element,
    "Theme": katui.Theme,
    "TextField": katui.widgets.TextField,
    "TextArea": katui.widgets.TextArea,
    "ListState": katui.widgets.ListState,
    "ScrollState": katui.widgets.ScrollState,
    "TreeNode": katui.widgets.TreeNode,
    "TreeState": katui.widgets.TreeState,
    "Focus": katui.widgets.Focus,
    "Toasts": katui.widgets.Toasts,
    "Light": katui.game.Light,
    "HeldKeys": katui.game.Keys,
    "V3": katui.game.V3,
    "Camera": katui.game.Camera,
    "Rng": katui.game.Rng,
    "Sparks": katui.game.Sparks,
}

HOOKS: dict[str, type] = {"UiApp": katui.App, "Game": katui.game.Game}

VARIANTS: dict[str, type] = {"Control": katui.Control, "Edit": katui.widgets.Edit}


def missing(names: list[str], target: object) -> list[str]:
    return [
        name
        for name in names
        if name not in OMITTED and not hasattr(target, RENAMED.get(name, name))
    ]


@pytest.mark.parametrize("crate", API["exports"])
def test_every_rust_export_has_a_python_name(crate: str) -> None:
    assert missing(API["exports"][crate], MODULES[crate]) == []


@pytest.mark.parametrize("rust", API["methods"])
def test_every_rust_method_has_a_python_one(rust: str) -> None:
    assert missing(API["methods"][rust], CLASSES[rust]) == []


def test_every_theme_field_is_a_python_attribute() -> None:
    assert missing(API["fields"]["Theme"], katui.Theme.dark()) == []


@pytest.mark.parametrize("trait", API["hooks"])
def test_every_hook_of_a_rust_trait_is_a_python_method(trait: str) -> None:
    assert missing(API["hooks"][trait], HOOKS[trait]) == []


@pytest.mark.parametrize("enum", API["variants"])
def test_every_rust_variant_has_a_python_constant(enum: str) -> None:
    python = VARIANTS[enum]
    assert [v for v in API["variants"][enum] if not hasattr(python, v.upper())] == []


def test_the_package_version_is_the_workspace_version() -> None:
    assert katui.__version__ == API["version"]
