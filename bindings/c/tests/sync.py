"""Rust is the source of truth for the API. Every public widget, game item,
type method, theme field, trait hook and Control and Edit variant listed in
bindings/rust-api.json has a counterpart in katui.h, or an entry in OMITTED
saying why C does without it. Run it with `just c-check`."""

import json
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[3]
API = json.loads((REPO / "bindings/rust-api.json").read_text())
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





def has_export(name: str) -> bool:
    renamed = RENAMED.get(name, name)
    if re.fullmatch(r"[A-Z0-9]+(_[A-Z0-9]+)+", name):
        return f"KATUI_{name}" in DEFINES
    if name[0].isupper():
        return f"Katui{renamed}" in TYPES
    return f"katui_{renamed}" in FUNCTIONS


def missing_exports() -> list[str]:
    names = [name for crate in API["exports"].values() for name in crate]
    return [name for name in names if name not in OMITTED and not has_export(name)]




def missing_methods() -> list[str]:
    found = []
    for rust, names in API["methods"].items():
        prefix = snake(RENAMED.get(rust, rust))
        for name in names:
            method = RENAMED.get(name, name)
            wanted = {f"katui_{prefix}_{method}", f"katui_{method}" if rust == "Element" else ""}
            if f"{rust}.{name}" not in OMITTED and not wanted & FUNCTIONS:
                found.append(f"{rust}.{name}")
    return found


def missing_theme_fields() -> list[str]:
    found = []
    for field in API["fields"]["Theme"]:
        wanted = [f"katui_theme_{field}"] + ([f"katui_theme_set_{field}"] if field != "cell" else [])
        found += [name for name in wanted if name not in FUNCTIONS]
    return found


def callback_fields(struct: str) -> list[str]:
    block = re.search(rf"typedef struct {struct} \{{(.*?)\}} {struct};", HEADER, re.S)
    assert block, f"struct {struct} not found in katui.h"
    return re.findall(r"\(\*(\w+)\)", block.group(1))


def missing_hooks() -> list[str]:
    found = []
    for trait, struct in [("UiApp", "KatuiApp"), ("Game", "KatuiGame")]:
        fields = callback_fields(struct)
        found += [f"{trait}.{hook}" for hook in API["hooks"][trait] if hook not in fields]
    return found


def missing_variants() -> list[str]:
    found = []
    for enum, variants in API["variants"].items():
        for variant in variants:
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
