"""Rust is the source of truth for the API. Every public widget, game item,
type method, theme field, trait hook and Control and Edit variant in the
Rust crates has a counterpart in katui.h, or an entry in OMITTED saying why
C does without it. Adding something public in Rust fails this check until
the binding or the reason is written. Run it with `just c-check`."""

import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[3]
CRATES = REPO / "crates"
HEADER = (REPO / "bindings/c/include/katui.h").read_text()

RENAMED = {
    "HeldKeys": "Keys",
    "P2": "Point",
    "HtmlDefaults": "Document",
    "HtmlHost": "Document",
    "markdown_view": "markdown",
    "with_text": "new",
    "texts": "text",
    "to_surface": "to_image",
}

OMITTED = {
    "Key": "keys are named by strings like 'a', 'enter' and 'up'",
    "Theme.scope": "the app runner and the screen set the theme around every view",
    "HeldKeys.tick": "the game loop feeds the held keys",
    "HeldKeys.press": "the game loop feeds the held keys",
    "HeldKeys.release": "the game loop feeds the held keys",
    "HeldKeys.clear": "the game loop feeds the held keys",
}

FUNCTIONS = set(re.findall(r"\b(katui_\w+)\s*\(", HEADER))
TYPES = set(re.findall(r"\b(Katui\w+)\s*;", HEADER))
DEFINES = set(re.findall(r"^#define (KATUI_\w+)", HEADER, re.M))


def snake(name: str) -> str:
    return re.sub(r"(?<=[a-z0-9])(?=[A-Z])", "_", name).lower()


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


def has_export(name: str) -> bool:
    renamed = RENAMED.get(name, name)
    if re.fullmatch(r"[A-Z0-9]+(_[A-Z0-9]+)+", name):
        return f"KATUI_{name}" in DEFINES
    if name[0].isupper():
        return f"Katui{renamed}" in TYPES
    return f"katui_{renamed}" in FUNCTIONS


def missing_exports() -> list[str]:
    libs = ["katui-widgets", "katui-game", "katui-content"]
    names = [name for lib in libs for name in exports(CRATES / lib / "src/lib.rs")]
    return [name for name in names if name not in OMITTED and not has_export(name)]


METHODS = [
    ("katui-ui/src/element.rs", "Element"),
    ("katui-widgets/src/theme.rs", "Theme"),
    ("katui-widgets/src/field.rs", "TextField"),
    ("katui-widgets/src/area.rs", "TextArea"),
    ("katui-widgets/src/list.rs", "ListState"),
    ("katui-widgets/src/scroll.rs", "ScrollState"),
    ("katui-widgets/src/tree.rs", "TreeNode"),
    ("katui-widgets/src/tree.rs", "TreeState"),
    ("katui-widgets/src/focus.rs", "Focus"),
    ("katui-widgets/src/feedback.rs", "Toasts"),
    ("katui-game/src/light.rs", "Light"),
    ("katui-game/src/keys.rs", "HeldKeys"),
    ("katui-game/src/space.rs", "V3"),
    ("katui-game/src/space.rs", "Camera"),
    ("katui-game/src/space.rs", "Rng"),
    ("katui-game/src/space.rs", "Sparks"),
]


def missing_methods() -> list[str]:
    found = []
    for source, rust in METHODS:
        names = public_fns(CRATES / source, rust)
        assert names, f"no public methods of {rust} found; fix the parser or the path"
        prefix = snake(RENAMED.get(rust, rust))
        for name in names:
            method = RENAMED.get(name, name)
            wanted = {f"katui_{prefix}_{method}", f"katui_{method}" if rust == "Element" else ""}
            if f"{rust}.{name}" not in OMITTED and not wanted & FUNCTIONS:
                found.append(f"{rust}.{name}")
    return found


def missing_theme_fields() -> list[str]:
    source = (CRATES / "katui-widgets/src/theme.rs").read_text()
    struct = re.search(r"pub struct Theme \{(.*?)\n\}", source, re.S)
    assert struct
    found = []
    for field in re.findall(r"^    pub (\w+):", struct.group(1), re.M):
        wanted = [f"katui_theme_{field}"] + ([f"katui_theme_set_{field}"] if field != "cell" else [])
        found += [name for name in wanted if name not in FUNCTIONS]
    return found


def callback_fields(struct: str) -> list[str]:
    block = re.search(rf"typedef struct {struct} \{{(.*?)\}} {struct};", HEADER, re.S)
    assert block, f"struct {struct} not found in katui.h"
    return re.findall(r"\(\*(\w+)\)", block.group(1))


def missing_hooks() -> list[str]:
    found = []
    for source, trait, struct in [
        ("katui-app/src/lib.rs", "UiApp", "KatuiApp"),
        ("katui-game/src/arcade.rs", "Game", "KatuiGame"),
    ]:
        fields = callback_fields(struct)
        found += [f"{trait}.{hook}" for hook in trait_fns(CRATES / source, trait) if hook not in fields]
    return found


def missing_variants() -> list[str]:
    found = []
    for source, enum in [("katui-rt/src/app.rs", "Control"), ("katui-widgets/src/field.rs", "Edit")]:
        block = re.search(rf"pub enum {enum} \{{(.*?)\n\}}", (CRATES / source).read_text(), re.S)
        assert block
        for variant in re.findall(r"^    (\w+),", block.group(1), re.M):
            if f"KATUI_{enum.upper()}_{variant.upper()}" not in DEFINES:
                found.append(f"{enum}::{variant}")
    return found


def main() -> int:
    checks = {
        "exports": missing_exports(),
        "methods": missing_methods(),
        "theme fields": missing_theme_fields(),
        "trait hooks": missing_hooks(),
        "variants": missing_variants(),
    }
    failed = {kind: names for kind, names in checks.items() if names}
    for kind, names in failed.items():
        print(f"katui.h lacks these Rust {kind}: {', '.join(names)}", file=sys.stderr)
    if failed:
        print("bind them in bindings/c/src, or list them in OMITTED with a reason", file=sys.stderr)
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
