//! The katui page viewer: a local HTML file through the HTML and CSS
//! subset, with scrolling and links between local pages. Run with
//! `just demo-browse` for the bundled pages, or pass a path to a page.
#![cfg(unix)] // the runner drives /dev/tty through katui-rt's unix layer

use std::cell::RefCell;
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use katui_app::{Control, UiApp, run};
use katui_content::{HtmlDefaults, HtmlHost, html};
use katui_draw::Rgba;
use katui_proto::input::{InputEvent, Key};
use katui_scene::surface::Surface;
use katui_ui::{Element, Length};
use katui_widgets::{Theme, label};

const SCROLL_STEP: f32 = 48.0;

fn main() -> Result<(), katui_rt::RtError> {
    let page = std::env::args().nth(1).map_or_else(
        || {
            PathBuf::from(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/examples/pages/index.html"
            ))
        },
        PathBuf::from,
    );
    run(Browser::open(page))
}

struct Browser {
    page: PathBuf,
    source: io::Result<String>,
    images: RefCell<HashMap<String, Option<Arc<Surface>>>>,
    scroll: f32,
    history: Vec<PathBuf>,
}

impl Browser {
    fn open(page: PathBuf) -> Self {
        let source = read_page(&page);
        Self {
            page,
            source,
            images: RefCell::default(),
            scroll: 0.0,
            history: Vec::new(),
        }
    }

    fn follow(&mut self, href: &str) {
        let Some(target) = local_target(&self.page, href) else {
            return;
        };
        let previous = std::mem::replace(self, Self::open(target));
        self.history = previous.history;
        self.history.push(previous.page);
    }

    fn back(&mut self) {
        if let Some(page) = self.history.pop() {
            let history = std::mem::take(&mut self.history);
            *self = Self::open(page);
            self.history = history;
        }
    }
}

/// The local file `href` points at from `page`, for relative links. A
/// link with a scheme, `https:` or `mailto:` for one, stays out.
fn local_target(page: &Path, href: &str) -> Option<PathBuf> {
    let path = href
        .split(['#', '?'])
        .next()
        .filter(|path| !path.is_empty())?;
    let scheme = path
        .split_once(':')
        .is_some_and(|(scheme, _)| !scheme.contains('/'));
    if scheme {
        return None;
    }
    Some(page.parent().unwrap_or(Path::new(".")).join(path))
}

/// The text of `page`, which has to be a regular file: a device such as
/// `/dev/zero` would never finish reading.
fn read_page(page: &Path) -> io::Result<String> {
    if !std::fs::metadata(page)?.is_file() {
        return Err(io::Error::other("the path is not a regular file"));
    }
    std::fs::read_to_string(page)
}

fn decode(path: &Path) -> Option<Arc<Surface>> {
    let rgba = image::open(path).ok()?.to_rgba8();
    Surface::from_rgba(rgba.width(), rgba.height(), rgba.into_raw()).map(Arc::new)
}

impl HtmlHost<String> for Browser {
    fn link(&self, href: &str) -> String {
        href.to_owned()
    }

    fn image(&self, src: &str) -> Option<Arc<Surface>> {
        self.images
            .borrow_mut()
            .entry(src.to_owned())
            .or_insert_with(|| {
                local_target(&self.page, src)
                    .filter(|path| path.is_file())
                    .and_then(|path| decode(&path))
            })
            .clone()
    }
}

impl UiApp for Browser {
    type Message = String;

    fn view(&self, theme: &Theme) -> Element<String> {
        let defaults = HtmlDefaults {
            font_size: theme.body(),
            color: theme.text,
            link: theme.accent,
            tint: Rgba::solid(45, 42, 60),
        };
        let page = match &self.source {
            Ok(source) => html(source, &defaults, self),
            Err(error) => label(
                format!("Check the path, the page did not open: {error}"),
                theme.body(),
                theme.danger,
            ),
        };
        Element::column([page])
            .scroll(self.scroll)
            .padding(theme.cell_height())
            .size(Length::Fill, Length::Fill)
            .background(theme.background.clone())
    }

    fn update(&mut self, href: String) -> Control {
        self.follow(&href);
        Control::Redraw
    }

    fn input(&mut self, event: &InputEvent) -> Control {
        let InputEvent::Key(key, _) = event else {
            return Control::Wait;
        };
        match key {
            Key::Down | Key::Char('j') => self.scroll += SCROLL_STEP,
            Key::Up | Key::Char('k') => self.scroll = (self.scroll - SCROLL_STEP).max(0.0),
            Key::PageDown | Key::Char(' ') => self.scroll += SCROLL_STEP * 8.0,
            Key::PageUp => self.scroll = (self.scroll - SCROLL_STEP * 8.0).max(0.0),
            Key::Home => self.scroll = 0.0,
            Key::Backspace | Key::Left => self.back(),
            Key::Char('q') => return Control::Quit,
            _ => return Control::Wait,
        }
        Control::Redraw
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use katui_app::UiApp as _;
    use katui_draw::{TextEngine, bundled_fonts};
    use katui_ui::{CellPixels, Ui, UiOptions};
    use katui_widgets::Theme;

    use super::{Browser, local_target};

    const CELL: CellPixels = CellPixels {
        width: 9,
        height: 19,
    };

    #[test]
    fn the_start_page_renders_and_its_link_opens_the_next_page() {
        let start = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/pages/index.html");
        let mut browser = Browser::open(start);
        assert!(browser.source.is_ok(), "the bundled page opens");
        let engine = TextEngine::new(bundled_fonts()).expect("the bundled fonts parse");
        let mut ui: Ui<String> = Ui::new(engine, UiOptions::default());
        let size = (9 * 100, 19 * 36);
        let output = ui.render(&browser.view(&Theme::dark(CELL)), size, CELL);
        let surface = output.flatten(CELL, size).expect("whole pixels");
        let path = format!(
            "{}/../../target/katui-browse.png",
            env!("CARGO_MANIFEST_DIR")
        );
        image::save_buffer(
            path,
            surface.pixels(),
            surface.width(),
            surface.height(),
            image::ExtendedColorType::Rgba8,
        )
        .expect("the target directory is writable");
        browser.update("about.html".to_owned());
        assert!(browser.page.ends_with("about.html"));
        assert!(browser.source.is_ok(), "the linked page opens");
        browser.back();
        assert!(
            browser.page.ends_with("index.html"),
            "back returns to the start"
        );
    }

    #[test]
    fn relative_links_resolve_beside_the_page_and_remote_ones_stay_out() {
        let page = Path::new("docs/index.html");
        assert_eq!(
            local_target(page, "guide.html#top"),
            Some(Path::new("docs/guide.html").to_path_buf())
        );
        assert_eq!(local_target(page, "https://example.com"), None);
        assert_eq!(local_target(page, "#section"), None);
        assert_eq!(local_target(page, "mailto:someone@example.com"), None);
    }
}
