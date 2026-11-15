"""Rust is the source of truth for the API. Every public widget, game item,
type method and app hook in the Rust crates has a Python counterpart, or an
entry in OMITTED saying why Python does without it. Adding something public
in Rust fails this test until the binding or the reason is written."""

import re
from pathlib import Path
from types import ModuleType

import katui
import katui.game
import katui.widgets
import pytest
from conftest import REPO

CRATES = REPO / "crates"

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


def exports(lib: Path) -> list[str]:
    """The names a crate's lib.rs re-exports with `pub use`."""
    names = []
    for block in re.findall(r"^pub use [\w:]+::(\{[^}]*\}|\w+);", lib.read_text(), re.M):
        names += re.findall(r"\w+", block)
    return names


def public_fns(source: Path, type_name: str) -> list[str]:
    """The `pub fn` names in the inherent impl blocks of `type_name`."""
    text = source.read_text()
    names = []
    pattern = rf"^impl(?:<[^{{]*?>)?\s+{type_name}(?:<[^>]*>)?\s*\{{"
    for start in re.finditer(pattern, text, re.M):
        depth, at = 0, start.end() - 1
        for at in range(start.end() - 1, len(text)):
            depth += {"{": 1, "}": -1}.get(text[at], 0)
            if depth == 0:
                break
        body = text[start.end() : at]
        names += re.findall(r"^    pub (?:const )?fn (\w+)", body, re.M)
    return names


def trait_fns(source: Path, trait: str) -> list[str]:
    block = re.search(rf"^pub trait {trait}\b.*?^\}}", source.read_text(), re.M | re.S)
    assert block, f"trait {trait} not found in {source}"
    return re.findall(r"^    fn (\w+)", block.group(0), re.M)


def missing(names: list[str], target: object) -> list[str]:
    return [
        name
        for name in names
        if name not in OMITTED and not hasattr(target, RENAMED.get(name, name))
    ]


@pytest.mark.parametrize(
    ("lib", "module"),
    [
        ("katui-widgets/src/lib.rs", katui.widgets),
        ("katui-game/src/lib.rs", katui.game),
        ("katui-content/src/lib.rs", katui),
    ],
)
def test_every_rust_export_has_a_python_name(lib: str, module: ModuleType) -> None:
    assert missing(exports(CRATES / lib), module) == []


METHODS = [
    ("katui-ui/src/element.rs", "Element", katui.Element),
    ("katui-widgets/src/theme.rs", "Theme", katui.Theme),
    ("katui-widgets/src/field.rs", "TextField", katui.widgets.TextField),
    ("katui-widgets/src/area.rs", "TextArea", katui.widgets.TextArea),
    ("katui-widgets/src/list.rs", "ListState", katui.widgets.ListState),
    ("katui-widgets/src/scroll.rs", "ScrollState", katui.widgets.ScrollState),
    ("katui-widgets/src/tree.rs", "TreeNode", katui.widgets.TreeNode),
    ("katui-widgets/src/tree.rs", "TreeState", katui.widgets.TreeState),
    ("katui-widgets/src/focus.rs", "Focus", katui.widgets.Focus),
    ("katui-widgets/src/feedback.rs", "Toasts", katui.widgets.Toasts),
    ("katui-game/src/light.rs", "Light", katui.game.Light),
    ("katui-game/src/keys.rs", "HeldKeys", katui.game.Keys),
    ("katui-game/src/space.rs", "V3", katui.game.V3),
    ("katui-game/src/space.rs", "Camera", katui.game.Camera),
    ("katui-game/src/space.rs", "Rng", katui.game.Rng),
    ("katui-game/src/space.rs", "Sparks", katui.game.Sparks),
]


@pytest.mark.parametrize(("source", "rust", "python"), METHODS, ids=[m[1] for m in METHODS])
def test_every_rust_method_has_a_python_one(source: str, rust: str, python: type) -> None:
    names = public_fns(CRATES / source, rust)
    assert names, f"no public methods of {rust} found; fix the parser or the path"
    assert missing(names, python) == []


def test_every_theme_field_is_a_python_attribute() -> None:
    struct = re.search(
        r"pub struct Theme \{(.*?)\n\}", (CRATES / "katui-widgets/src/theme.rs").read_text(), re.S
    )
    assert struct
    fields = re.findall(r"^    pub (\w+):", struct.group(1), re.M)
    assert missing(fields, katui.Theme.dark()) == []


@pytest.mark.parametrize(
    ("source", "trait", "python"),
    [
        ("katui-app/src/lib.rs", "UiApp", katui.App),
        ("katui-game/src/arcade.rs", "Game", katui.game.Game),
    ],
)
def test_every_hook_of_a_rust_trait_is_a_python_method(
    source: str, trait: str, python: type
) -> None:
    assert missing(trait_fns(CRATES / source, trait), python) == []


@pytest.mark.parametrize(
    ("source", "enum", "python"),
    [
        ("katui-rt/src/app.rs", "Control", katui.Control),
        ("katui-widgets/src/field.rs", "Edit", katui.widgets.Edit),
    ],
)
def test_every_rust_variant_has_a_python_constant(source: str, enum: str, python: type) -> None:
    block = re.search(rf"pub enum {enum} \{{(.*?)\n\}}", (CRATES / source).read_text(), re.S)
    assert block
    variants = re.findall(r"^    (\w+),", block.group(1), re.M)
    assert [v for v in variants if not hasattr(python, v.upper())] == []


def test_the_package_version_is_the_workspace_version() -> None:
    workspace = (REPO / "Cargo.toml").read_text()
    version = re.search(r'^version = "([^"]+)"', workspace, re.M)
    assert version
    assert katui.__version__ == version.group(1)
