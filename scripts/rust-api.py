"""Write the public Rust API every binding must cover as JSON, from the
crate sources: the re-exports of the widget, game and content crates, the
public methods of the types the bindings wrap, the theme's fields, the app
and game hooks, the Control and Edit variants, and the version.

`just api` writes bindings/rust-api.json. Each binding's sync test reads
that file, and `just api-check` fails when it is stale."""

import json
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
CRATES = REPO / "crates"

EXPORTS = ["katui-widgets", "katui-game", "katui-content"]
TYPES = [
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
FIELDS = [("katui-widgets/src/theme.rs", "Theme")]
HOOKS = [("katui-app/src/lib.rs", "UiApp"), ("katui-game/src/arcade.rs", "Game")]
VARIANTS = [("katui-rt/src/app.rs", "Control"), ("katui-widgets/src/field.rs", "Edit")]


def source(path: str) -> str:
    return (CRATES / path).read_text()


def exports(crate: str) -> list[str]:
    """The names a crate's lib.rs re-exports with `pub use`."""
    names = []
    for block in re.findall(r"^pub use [\w:]+::(\{[^}]*\}|\w+);", source(f"{crate}/src/lib.rs"), re.M):
        names += re.findall(r"\w+", block)
    return names


def block_end(text: str, open_at: int) -> int:
    """Where the block whose `{` sits at `open_at` closes."""
    depth = 0
    for at in range(open_at, len(text)):
        depth += {"{": 1, "}": -1}.get(text[at], 0)
        if depth == 0:
            return at
    return len(text)


def methods(path: str, type_name: str) -> list[str]:
    """The `pub fn` names in the inherent impl blocks of `type_name`."""
    text = source(path)
    names = []
    pattern = rf"^impl(?:<[^{{]*?>)?\s+{type_name}(?:<[^>]*>)?\s*\{{"
    for start in re.finditer(pattern, text, re.M):
        body = text[start.end() : block_end(text, start.end() - 1)]
        names += re.findall(r"^    pub (?:const )?fn (\w+)", body, re.M)
    return names


def fields(path: str, type_name: str) -> list[str]:
    struct = re.search(rf"pub struct {type_name} \{{(.*?)\n\}}", source(path), re.S)
    return re.findall(r"^    pub (\w+):", struct.group(1), re.M) if struct else []


def hooks(path: str, trait: str) -> list[str]:
    block = re.search(rf"^pub trait {trait}\b.*?^\}}", source(path), re.M | re.S)
    return re.findall(r"^    fn (\w+)", block.group(0), re.M) if block else []


def variants(path: str, enum: str) -> list[str]:
    block = re.search(rf"pub enum {enum} \{{(.*?)\n\}}", source(path), re.S)
    return re.findall(r"^    (\w+),", block.group(1), re.M) if block else []


def version() -> str:
    found = re.search(r'^version = "([^"]+)"', (REPO / "Cargo.toml").read_text(), re.M)
    return found.group(1) if found else ""


def api() -> dict[str, object]:
    return {
        "version": version(),
        "exports": {crate: exports(crate) for crate in EXPORTS},
        "methods": {name: methods(path, name) for path, name in TYPES},
        "fields": {name: fields(path, name) for path, name in FIELDS},
        "hooks": {name: hooks(path, name) for path, name in HOOKS},
        "variants": {name: variants(path, name) for path, name in VARIANTS},
    }


def empty_sections(found: dict[str, object]) -> list[str]:
    """Every list that came out empty, which means a path or a pattern broke."""
    return [
        f"{section}.{name}"
        for section, table in found.items()
        if isinstance(table, dict)
        for name, names in table.items()
        if not names
    ]


def main() -> int:
    found = api()
    broken = empty_sections(found)
    if broken:
        print(f"found nothing for {', '.join(broken)}; fix the path or the pattern", file=sys.stderr)
        return 1
    print(json.dumps(found, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
