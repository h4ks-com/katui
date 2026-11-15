"""Write llms.txt and llms-full.txt into target/doc, the site GitHub Pages
publishes, so coding agents can read the guides and the API in one place.

llms.txt lists the guides from docs/README.md, the API reference of each
crate from the crate table in docs/architecture.md, and the binding
READMEs. llms-full.txt holds README.md, every guide in the order
docs/README.md gives, and the public Rust API from bindings/rust-api.json."""

import json
import posixpath
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
OUT = REPO / "target" / "doc"
BLOB = "https://github.com/matheusfillipe/katui/blob/main/"
SITE = "https://matheusfillipe.github.io/katui/"
BINDING_CRATES = {"katui-c", "katui-python", "katui-node", "katui-bindings"}
BINDINGS = [
    ("Python", "bindings/python/README.md", "the `katui` package on PyPI"),
    ("Node.js", "bindings/node/README.md", "the `@matheusfillipe/katui` package on npm"),
    ("Go", "bindings/go/README.md", "the Go module over the C library"),
    ("C", "bindings/c/README.md", "the C library behind `katui.h`"),
]
EXAMPLES = [
    ("Rust app examples", "crates/katui-app/examples", "the to-do list, the widget tour"),
    ("Rust game examples", "crates/katui-game/examples", "the games and the video player"),
    ("Python examples", "bindings/python/examples", "the same apps and a game in Python"),
    ("Node.js examples", "bindings/node/examples", "the same apps and a game in TypeScript"),
    ("Go examples", "bindings/go/examples", "a counter, a to-do list, a help screen, a game"),
    ("C examples", "bindings/c/examples", "a counter, a to-do list and a game"),
]

SUMMARY = (
    "katui is a terminal UI library for Rust, Python, Node.js, Go and C that draws real "
    "pixels through the kitty graphics protocol, with sixel or quarter-block characters "
    "on other terminals."
)
NOTES = f"""\
- Pick the entry point by what you build: `katui::app::run` with a `UiApp` (a `view` and \
an `update`) for pixel UI apps, `katui::game::play` with a `Game` (`step` and `render`) \
for games, `katui::content::html` and `markdown_view` for documents inside a view, \
`katui::ratatui` to add images to a ratatui app, and `katui::rt::media::VideoStream` for video.
- Rust features: `app` (runner and widgets), `game`, `html`, `ratatui`. Python: \
`pip install katui`. Node.js: `npm install @matheusfillipe/katui`. Go: \
`go get github.com/matheusfillipe/katui/bindings/go`.
- Every binding has a headless `Screen` and a game `frame` for tests without a terminal.
- `KATUI_GRAPHICS=kitty|sixel|blocks` overrides the detected backend. Inside tmux, set \
`allow-passthrough on` and the true color terminal feature, as the README shows.
- An agent skill for katui lives in [skills/katui]({BLOB}skills/katui/SKILL.md).
"""


def read(path: str) -> str:
    return (REPO / path).read_text()


def guides() -> list[tuple[str, str, str]]:
    """The (title, path, description) of each guide, in the order docs/README.md lists them."""
    found = re.findall(
        r"^\d+\. \[([^]]+)\]\(([^)]+\.md)\): (.*?)(?=^\d+\.|^$)",
        read("docs/README.md"),
        re.M | re.S,
    )
    return [(title, f"docs/{path}", " ".join(text.split())) for title, path, text in found]


def crates() -> list[tuple[str, str]]:
    """The (crate, description) rows of the crate table in docs/architecture.md."""
    rows = re.findall(r"^\| `(katui[\w-]*)` \| (.+?) \|$", read("docs/architecture.md"), re.M)
    return [(name, text) for name, text in rows if name not in BINDING_CRATES]


def item(title: str, url: str, text: str) -> str:
    return f"- [{title}]({url}): {text}"


def llms() -> str:
    sections = [
        f"# katui\n\n> {SUMMARY}\n\n{NOTES.strip()}",
        "## Guides\n\n"
        + "\n".join(item(title, BLOB + path, text) for title, path, text in guides()),
        "## API reference\n\n"
        + "\n".join(
            item(f"`{name}`", f"{SITE}{name.replace('-', '_')}/", text) for name, text in crates()
        ),
        "## Bindings\n\n"
        + "\n".join(item(name, BLOB + path, text) for name, path, text in BINDINGS),
        "## Optional\n\n"
        + "\n".join(
            [
                item("Full text", f"{SITE}llms-full.txt", "the README, every guide and the API"),
                item("README", f"{BLOB}README.md", "install, a first program, terminals, demos"),
                *(item(name, BLOB + path, text) for name, path, text in EXAMPLES),
            ]
        ),
    ]
    return "\n\n".join(sections) + "\n"


def absolute_links(text: str, path: str) -> str:
    """`text` with its relative links and image sources pointing at GitHub."""
    folder = posixpath.dirname(path)

    def resolve(target: str) -> str:
        if re.match(r"[a-z]+:|#", target):
            return target
        return BLOB + posixpath.normpath(posixpath.join(folder, target))

    def rewrite(prose: str) -> str:
        prose = re.sub(r"\]\(([^)\s]+)\)", lambda m: f"]({resolve(m.group(1))})", prose)
        return re.sub(
            r'(href|src)="([^"]+)"', lambda m: f'{m.group(1)}="{resolve(m.group(2))}"', prose
        )

    # We split on code fences so links inside code samples stay as the reader would type them.
    parts = re.split(r"(^```.*?^```$)", text, flags=re.M | re.S)
    return "".join(part if part.startswith("```") else rewrite(part) for part in parts)


def api_overview() -> str:
    api = json.loads(read("bindings/rust-api.json"))
    lines = [f"# Public Rust API\n\nVersion {api['version']}. Reference: {SITE}katui/\n"]
    headings = [
        ("exports", "Re-exports of `{}`"),
        ("methods", "Methods of `{}`"),
        ("fields", "Fields of `{}`"),
        ("hooks", "Methods of the `{}` trait"),
        ("variants", "Variants of `{}`"),
    ]
    for section, heading in headings:
        for name, names in api[section].items():
            lines.append(f"- {heading.format(name)}: {', '.join(names)}")
    return "\n".join(lines) + "\n"


def llms_full() -> str:
    paths = ["README.md", *(path for _, path, _ in guides())]
    parts = [f"Source: {BLOB}{path}\n\n{absolute_links(read(path), path)}" for path in paths]
    return "\n\n---\n\n".join([*parts, api_overview()])


def main() -> int:
    OUT.mkdir(parents=True, exist_ok=True)
    (OUT / "llms.txt").write_text(llms())
    (OUT / "llms-full.txt").write_text(llms_full())
    return 0


if __name__ == "__main__":
    sys.exit(main())
