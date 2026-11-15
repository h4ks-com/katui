# HTML and markdown

This guide shows a document inside a katui app: a help screen written in
markdown, with links you can click. The same renderer takes HTML with CSS,
so you can style documents the way you would on the web.

Add the `app` and `html` features:

```sh
cargo add katui --git https://github.com/h4ks-com/katui --features app,html
```

## A help screen

```rust,no_run
use std::sync::Arc;

use katui::app::{Control, UiApp, run};
use katui::content::{HtmlDefaults, HtmlHost, markdown_view};
use katui::scene::surface::Surface;
use katui::ui::Element;
use katui::widgets::{Theme, hint, page};

const HELP: &str = "\
# Help

Press **Ctrl+C** to quit. Read [the keys](keys) or [the credits](credits).

- Arrows move
- Enter opens
";

#[derive(Default)]
struct Help {
    opened: Option<String>,
}

impl HtmlHost<String> for Help {
    fn link(&self, href: &str) -> String {
        href.to_owned()
    }

    fn image(&self, _src: &str) -> Option<Arc<Surface>> {
        None
    }
}

impl UiApp for Help {
    type Message = String;

    fn view(&self) -> Element<String> {
        let theme = Theme::current();
        let defaults = HtmlDefaults {
            font_size: theme.body(),
            color: theme.text,
            link: theme.accent,
            tint: theme.muted,
        };
        let opened = match &self.opened {
            Some(page) => format!("You opened {page}"),
            None => "Click a link".to_owned(),
        };
        page([markdown_view(HELP, &defaults, self), hint(opened)])
    }

    fn update(&mut self, page: String) -> Control {
        self.opened = Some(page);
        Control::Redraw
    }
}

fn main() -> Result<(), katui::rt::RtError> {
    run(Help::default())
}
```

## The host

The renderer asks your app two things through `HtmlHost`:

- `link` turns a link's `href` into one of your messages. Clicking the link
  sends it to `update`, and what a link does is up to you: open another
  page, switch a tab, or run a command.
- `image` returns the pixels for an image's `src`, or `None` to leave it
  out. The renderer does no file or network reads of its own, so you decide
  where images come from. Decode them once and keep the `Arc<Surface>`,
  since the same `Arc` tells katui the image has not changed.

`HtmlDefaults` sets the body text size and the colors a document starts
with, before its own CSS applies. Taking them from the theme makes the
document match the rest of your app.

## HTML

`katui::content::html(source, &defaults, host)` works the same way with an
HTML document. It supports:

- Block and flex layout: `display: flex`, `flex-direction`, `gap`,
  `justify-content`, `align-items`, `padding`, `margin`, `width` and
  `height`.
- Text: headings, paragraphs, lists, quotes, `pre` and `code` in a
  monospace font, bold, italic, underline, strikethrough, links, and
  `font-size`, `font-weight`, `font-style`, `font-family: monospace`,
  `text-align`, the `align` attribute and `color`.
- Tables, with every column of a row the same width, and `hr` rules.
- Boxes: `background`, `border` and `border-radius`.
- `<style>` blocks and `style` attributes, with selectors by type, class
  and id, and specificity that follows CSS.
- `display: none` hides an element.
- Images through your host.

It does not run JavaScript, and floats are not supported. It is
meant for help pages, release notes and documents you write, not for
browsing the web.

`markdown_view` turns markdown into HTML and renders it the same way, so
everything above applies to markdown too. It reads GitHub's markdown:
tables, task lists, strikethrough, footnotes and alerts such as
`> [!NOTE]`. Run `just demo-md` for a reader of a whole folder of markdown
files.

## Scrolling

A long document can be taller than the screen. Keep a `ScrollState` for it
as in [Scrolling](ui-apps.md#scrolling). Run `just demo-browse` to see a
scrolling page viewer.

Headings carry the id of their GitHub slug, and elements with an `id`
attribute carry the id of that. `anchor_id("#install")` is the id a link to
`#install` reaches; pass it to `.reveal` on the scrolling element around
the document to jump there.
