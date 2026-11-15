"""The static docs site: a front page and one page per guide, beside the
rustdoc API reference in target/doc. Run through `just site`."""

import html
import json
import posixpath
import re
import shutil
from dataclasses import dataclass
from pathlib import Path

import markdown
from llms import BLOB, REPO, SITE, guides, read

OUT = REPO / "target" / "doc"
RAW = "https://raw.githubusercontent.com/h4ks-com/katui/main/"
GITHUB = "https://github.com/h4ks-com/katui"
FONTS = REPO / "crates" / "katui-draw" / "assets"
SITE_FILES = REPO / "scripts" / "site"

INSTALL = [
    ("Rust", "cargo add katui --features app"),
    ("Python", "pip install katui"),
    ("Node.js", "npm install @mattfillipe/katui"),
    ("Go", "go get github.com/h4ks-com/katui/bindings/go"),
]


@dataclass
class Guide:
    title: str
    source: str
    summary: str

    @property
    def page(self) -> str:
        return Path(self.source).stem + ".html"

    @property
    def sentence(self) -> str:
        """The summary as a sentence of HTML, with its inline code set as code."""
        text = html.escape(self.summary[:1].upper() + self.summary[1:])
        return re.sub(r"`([^`]+)`", r"<code>\1</code>", text)


def slug(text: str) -> str:
    """GitHub's heading anchor, so links written for GitHub land here too."""
    kept = (letter for letter in text.strip().lower() if letter.isalnum() or letter in " -_")
    return "".join(kept).replace(" ", "-")


def link_target(target: str, source: str) -> str:
    """Where a link in `source` points on this site: guides stay here, the
    rest of the repository opens on GitHub."""
    if re.match(r"[a-z]+:|#", target):
        return target
    path, _, anchor = target.partition("#")
    resolved = posixpath.normpath(posixpath.join(posixpath.dirname(source), path))
    suffix = f"#{anchor}" if anchor else ""
    if resolved == "docs/README.md":
        return f"../index.html{suffix or '#guides'}"
    if resolved.startswith("docs/") and resolved.endswith(".md"):
        return Path(resolved).stem + ".html" + suffix
    if re.search(r"\.(png|webp|gif|svg|jpe?g)$", resolved):
        return RAW + resolved
    return BLOB + resolved + suffix


def local_links(text: str, source: str) -> str:
    def rewrite(prose: str) -> str:
        prose = re.sub(
            r"\]\(([^)\s]+)\)", lambda m: f"]({link_target(m.group(1), source)})", prose
        )
        return re.sub(
            r'(href|src)="([^"]+)"',
            lambda m: f'{m.group(1)}="{link_target(m.group(2), source)}"',
            prose,
        )

    # We split on code fences so links inside code samples stay as the reader would type them.
    parts = re.split(r"(^```.*?^```$)", text, flags=re.M | re.S)
    return "".join(part if part.startswith("```") else rewrite(part) for part in parts)


def render(text: str) -> tuple[str, list[dict[str, str]]]:
    """The HTML of a guide and its second-level headings."""
    # Rustdoc fence options like `rust,no_run` would hide the language from the highlighter.
    text = re.sub(r"^```(\w+)[,\w]*$", r"```\1", text, flags=re.M)
    text = re.sub(
        r"^```mermaid\n(.*?)^```$",
        lambda m: f'<pre class="mermaid">\n{html.escape(m.group(1))}</pre>',
        text,
        flags=re.M | re.S,
    )
    converter = markdown.Markdown(
        extensions=["fenced_code", "tables", "sane_lists", "codehilite", "toc"],
        extension_configs={
            "codehilite": {"guess_lang": False, "css_class": "code"},
            "toc": {"slugify": lambda value, _separator: slug(value), "toc_depth": "2-3"},
        },
    )
    body = converter.convert(text)
    headings = [
        {"id": token["id"], "name": html.unescape(token["name"])}
        for top in converter.toc_tokens  # type: ignore[attr-defined]
        for token in [top, *top["children"]]
        if token["level"] == 2
    ]
    return body, headings


def sections(guide: Guide, text: str) -> list[dict[str, str]]:
    """The search entries of a guide: one per heading, with the prose under it."""
    entries = []
    for part in re.split(r"^(?=#{1,3} )", text, flags=re.M):
        heading, _, prose = part.partition("\n")
        name = heading.lstrip("# ").strip() or guide.title
        prose = re.sub(r"```.*?```", " ", prose, flags=re.S)
        prose = re.sub(r"[`*_>#|\[\]()]", " ", prose)
        anchor = "" if heading.startswith("# ") or not heading.startswith("#") else slug(name)
        entries.append(
            {
                "guide": guide.title,
                "heading": name,
                "url": f"guides/{guide.page}" + (f"#{anchor}" if anchor else ""),
                "text": " ".join(prose.split())[:600],
            }
        )
    return entries


def header(prefix: str) -> str:
    return f"""<header class="bar">
  <a class="brand" href="{prefix}index.html" aria-label="katui home">
    <img src="{prefix}logo.svg" alt="" width="28" height="28"><span>katui</span>
  </a>
  <nav aria-label="Site">
    <a href="{prefix}index.html#guides">Guides</a>
    <a href="{prefix}katui/index.html">API</a>
    <a href="{GITHUB}">GitHub</a>
  </nav>
</header>"""


def search_box(prefix: str) -> str:
    return f"""<div class="search" data-root="{prefix}">
  <label for="search">Search the guides</label>
  <input id="search" type="search" placeholder="Search the guides" autocomplete="off">
  <ol class="results" hidden></ol>
</div>"""


MERMAID = """<script type="module">
import mermaid from "https://cdn.jsdelivr.net/npm/mermaid@11.4.1/dist/mermaid.esm.min.mjs";
const light = matchMedia("(prefers-color-scheme: light)").matches;
mermaid.initialize({
  startOnLoad: true,
  theme: light ? "default" : "dark",
  themeVariables: { fontFamily: getComputedStyle(document.body).fontFamily, fontSize: "16px" },
});
</script>"""


def page(title: str, prefix: str, body: str, description: str) -> str:
    diagrams = MERMAID if 'class="mermaid"' in body else ""
    return f"""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{html.escape(title)}</title>
<meta name="description" content="{html.escape(description)}">
<link rel="icon" href="{prefix}logo.svg" type="image/svg+xml">
<link rel="stylesheet" href="{prefix}site.css">
<script src="{prefix}site.js" defer></script>
{diagrams}
</head>
<body>
{header(prefix)}
{body}
<footer class="foot">
  <a href="{GITHUB}">Source on GitHub</a>
  <a href="{prefix}katui/index.html">API reference</a>
  <a href="{prefix}llms.txt">llms.txt</a>
  <span>MIT or Apache 2.0</span>
</footer>
</body>
</html>
"""


def guide_list(all_guides: list[Guide], prefix: str, current: Guide | None) -> str:
    items = "\n".join(
        f'<li{" aria-current=\"page\"" if guide is current else ""}>'
        f'<a href="{prefix}guides/{guide.page}">{html.escape(guide.title)}</a></li>'
        for guide in all_guides
    )
    return f'<ol class="guides">\n{items}\n</ol>'


def neighbours(all_guides: list[Guide], index: int) -> str:
    links = []
    if index > 0:
        before = all_guides[index - 1]
        links.append(f'<a rel="prev" href="{before.page}">Previous: {before.title}</a>')
    if index + 1 < len(all_guides):
        after = all_guides[index + 1]
        links.append(f'<a rel="next" href="{after.page}">Next: {after.title}</a>')
    return f'<nav class="turn" aria-label="Guides">{"".join(links)}</nav>'


def guide_page(all_guides: list[Guide], index: int) -> str:
    guide = all_guides[index]
    body, headings = render(local_links(read(guide.source), guide.source))
    outline = "\n".join(
        f'<li><a href="#{entry["id"]}">{html.escape(entry["name"])}</a></li>' for entry in headings
    )
    main = f"""<div class="reader">
<aside class="side">
{search_box("../")}
{guide_list(all_guides, "../", guide)}
</aside>
<main class="prose" id="content">
{body}
{neighbours(all_guides, index)}
</main>
<nav class="outline" aria-label="On this page">
<p>On this page</p>
<ol>
{outline}
</ol>
</nav>
</div>"""
    return page(f"{guide.title} · katui", "../", main, guide.summary)


def front_page(all_guides: list[Guide]) -> str:
    install = "\n".join(
        f"<dt>{name}</dt><dd><code>{html.escape(command)}</code></dd>"
        for name, command in INSTALL
    )
    listed = "\n".join(
        f'<li><a href="guides/{guide.page}">{html.escape(guide.title)}</a>'
        f"<p>{guide.sentence}</p></li>"
        for guide in all_guides
    )
    main = f"""<main class="front">
<section class="hero">
  <h1>Real pixels in your terminal.</h1>
  <p>katui is a terminal UI library that draws apps, games and video as images
  through the kitty graphics protocol, from Rust, Python, Node.js, Go and C.
  Terminals without it get sixel images or quarter-block characters.</p>
  <figure class="pane">
    <img src="demos/shardrift.webp" width="800" height="450"
      alt="Shardrift, a katui game: a ship firing through a nebula and splitting crystal rocks">
  </figure>
</section>
<section id="install">
  <h2>Install</h2>
  <dl class="install">
{install}
<dt>C</dt><dd>Download the header and libraries from the
<a href="{GITHUB}/releases/latest">latest release</a>.</dd>
  </dl>
</section>
<section id="guides">
  <h2>Guides</h2>
  {search_box("")}
  <ol class="guide-list">
{listed}
  </ol>
</section>
</main>"""
    return page("katui", "", main, "Real pixels in your terminal, from Rust, Python, Node.js, Go and C.")


def copy_assets() -> None:
    (OUT / "fonts").mkdir(parents=True, exist_ok=True)
    (OUT / "demos").mkdir(parents=True, exist_ok=True)
    for name in ["AtkinsonHyperlegibleNext[wght].ttf", "AtkinsonHyperlegibleMono[wght].ttf"]:
        shutil.copy(FONTS / name, OUT / "fonts" / name.replace("[wght]", ""))
    shutil.copy(REPO / "assets" / "logo.svg", OUT / "logo.svg")
    shutil.copy(REPO / "assets" / "demos" / "shardrift.webp", OUT / "demos" / "shardrift.webp")
    for name in ["site.css", "site.js"]:
        shutil.copy(SITE_FILES / name, OUT / name)


def main() -> int:
    all_guides = [Guide(title, source, summary) for title, source, summary in guides()]
    (OUT / "guides").mkdir(parents=True, exist_ok=True)
    copy_assets()
    entries = []
    for index, guide in enumerate(all_guides):
        (OUT / "guides" / guide.page).write_text(guide_page(all_guides, index))
        entries.extend(sections(guide, read(guide.source)))
    (OUT / "index.html").write_text(front_page(all_guides))
    (OUT / "search.json").write_text(json.dumps(entries))
    print(f"wrote the site for {len(all_guides)} guides to {OUT} ({SITE})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
