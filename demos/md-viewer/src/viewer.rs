//! The reader: a sidebar with the folder's markdown files and a search
//! over them, beside the open page.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;

use katui::app::{Control, UiApp};
use katui::content::{HtmlDefaults, HtmlHost, markdown_view};
use katui::draw::Rgba;
use katui::proto::input::{InputEvent, Key, Mods};
use katui::scene::surface::Surface;
use katui::ui::{Align, CellPixels, Element, Length, Scroll};
use katui::widgets::{
    Edit, Orientation, ScrollState, TextField, Theme, key_hints, split, text_field,
};

use crate::library::{Hit, Library};

/// The widest a page gets, in terminal columns, so lines stay readable.
const PAGE_COLUMNS: f32 = 100.0;
const SIDEBAR: f32 = 0.24;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Pane {
    Files,
    Page,
    Search,
}

#[derive(Debug, Clone)]
pub(crate) enum Message {
    Pick(usize),
    Hit(usize),
    Link(String),
    PageScrolled(Scroll),
    FilesScrolled(Scroll),
    Resized(f32),
}

pub(crate) struct Viewer {
    pub library: Library,
    pub pane: Pane,
    /// The entry the sidebar highlights.
    pub selected: Option<usize>,
    /// The file on the page.
    pub page: Option<usize>,
    open: HashSet<usize>,
    history: Vec<usize>,
    page_scroll: ScrollState,
    files_scroll: ScrollState,
    query: TextField,
    pub hits: Vec<Hit>,
    hit: usize,
    sidebar: f32,
    light: bool,
    pub status: Option<String>,
    cell: Cell<CellPixels>,
    images: RefCell<HashMap<String, Option<Arc<Surface>>>>,
}

impl Viewer {
    pub(crate) fn new(library: Library, first: Option<usize>) -> Self {
        let mut viewer = Self {
            library,
            pane: Pane::Files,
            selected: None,
            page: None,
            open: HashSet::new(),
            history: Vec::new(),
            page_scroll: ScrollState::default(),
            files_scroll: ScrollState::default(),
            query: TextField::default(),
            hits: Vec::new(),
            hit: 0,
            sidebar: SIDEBAR,
            light: false,
            status: None,
            cell: Cell::new(CellPixels {
                width: 10,
                height: 20,
            }),
            images: RefCell::default(),
        };
        if let Some(first) = first.or_else(|| viewer.library.front_page()) {
            viewer.show(first);
        }
        viewer
    }

    /// Put `id` on the page, and remember the page it replaces for back.
    pub(crate) fn navigate(&mut self, id: usize) {
        if let Some(previous) = self.page.filter(|&previous| previous != id) {
            self.history.push(previous);
        }
        self.show(id);
    }

    fn show(&mut self, id: usize) {
        self.page = Some(id);
        self.page_scroll = ScrollState::default();
        self.images.borrow_mut().clear();
        let mut parent = self.library.parent(id);
        while let Some(folder) = parent {
            self.open.insert(folder);
            parent = self.library.parent(folder);
        }
        self.select(id);
    }

    fn back(&mut self) {
        if let Some(previous) = self.history.pop() {
            self.show(previous);
        }
    }

    fn select(&mut self, id: usize) {
        self.selected = Some(id);
        let index = self.rows().iter().position(|&row| row == id).unwrap_or(0);
        self.reveal(index, self.row_height());
    }

    /// Scroll the sidebar so its row `index`, `height` pixels tall, is in
    /// view.
    fn reveal(&mut self, index: usize, height: f32) {
        let Some(viewport) = self.files_viewport() else {
            return;
        };
        let top = pixels(index) * height;
        let offset = self.files_scroll.offset();
        let target = if top < offset {
            top
        } else if top + height > offset + viewport {
            top + height - viewport
        } else {
            offset
        };
        self.files_scroll.scroll_by(target - offset);
    }

    /// The entries the sidebar lists, top to bottom: every top entry and
    /// the insides of the open folders.
    pub(crate) fn rows(&self) -> Vec<usize> {
        let mut rows = Vec::new();
        let mut stack: Vec<usize> = self.library.top.iter().rev().copied().collect();
        while let Some(id) = stack.pop() {
            rows.push(id);
            if let Some(entry) = self.library.entry(id).filter(|_| self.open.contains(&id)) {
                stack.extend(entry.children.iter().rev());
            }
        }
        rows
    }

    fn step(&mut self, by: isize) {
        let rows = self.rows();
        let at = self
            .selected
            .and_then(|id| rows.iter().position(|&row| row == id));
        let next = at.map_or(0, |at| {
            at.saturating_add_signed(by)
                .min(rows.len().saturating_sub(1))
        });
        if let Some(&id) = rows.get(next) {
            self.select(id);
        }
    }

    fn select_edge(&mut self, last: bool) {
        let rows = self.rows();
        let edge = if last { rows.last() } else { rows.first() };
        if let Some(&id) = edge {
            self.select(id);
        }
    }

    /// Open the selected folder, or show the selected file; `enter` also
    /// moves to the page.
    fn forward(&mut self, enter: bool) {
        let Some(id) = self.selected else {
            return;
        };
        let Some(entry) = self.library.entry(id) else {
            return;
        };
        if !entry.is_folder() {
            self.navigate(id);
            if enter {
                self.pane = Pane::Page;
            }
        } else if self.open.insert(id) {
            self.select(id);
        } else if let Some(&first) = entry.children.first() {
            self.select(first);
        }
    }

    /// Close the selected folder, or move to the folder it sits in.
    fn backward(&mut self) {
        let Some(id) = self.selected else {
            return;
        };
        if self.open.remove(&id) {
            self.select(id);
        } else if let Some(parent) = self.library.parent(id) {
            self.select(parent);
        }
    }

    fn pick(&mut self, id: usize) {
        if self.library.is_folder(id) {
            if !self.open.remove(&id) {
                self.open.insert(id);
            }
            self.select(id);
        } else {
            self.navigate(id);
        }
        self.pane = Pane::Files;
    }

    fn open_hit(&mut self, index: usize) {
        let Some(hit) = self.hits.get(index) else {
            return;
        };
        let entry = hit.entry;
        self.leave_search();
        self.navigate(entry);
        self.pane = Pane::Page;
    }

    fn leave_search(&mut self) {
        self.query.take();
        self.hits.clear();
        self.hit = 0;
        self.pane = Pane::Files;
        self.files_scroll = ScrollState::default();
        if let Some(id) = self.selected {
            self.select(id);
        }
    }

    fn move_hit(&mut self, by: isize) {
        self.hit = self
            .hit
            .saturating_add_signed(by)
            .min(self.hits.len().saturating_sub(1));
        self.reveal(self.hit, self.hit_height());
    }

    /// Follow a link on the page: another markdown file opens here, and
    /// anything else opens in the program the system picks for it.
    pub(crate) fn follow(&mut self, href: &str) {
        let path = href.split(['#', '?']).next().unwrap_or_default();
        if path.is_empty() {
            return;
        }
        if has_scheme(path) {
            self.open_outside(href);
            return;
        }
        let Some(target) = self.resolve(path) else {
            self.status = Some(format!("Check the link, {path} is outside the folder"));
            return;
        };
        if let Some(id) = self.library.find(&target) {
            self.navigate(id);
        } else if self.library.root.join(&target).exists() {
            self.open_outside(&self.library.root.join(&target).to_string_lossy());
        } else {
            self.status = Some(format!(
                "Check the link, {} does not exist",
                target.display()
            ));
        }
    }

    /// `path` from the open page, as a path from the root.
    fn resolve(&self, path: &str) -> Option<PathBuf> {
        let base = self
            .page
            .and_then(|id| self.library.entry(id))
            .and_then(|entry| entry.path.parent())
            .unwrap_or(Path::new(""));
        let joined = if let Some(absolute) = path.strip_prefix('/') {
            PathBuf::from(absolute)
        } else {
            base.join(path)
        };
        normalize(&joined)
    }

    fn open_outside(&mut self, target: &str) {
        let opener = if cfg!(target_os = "macos") {
            "open"
        } else {
            "xdg-open"
        };
        let spawned = Command::new(opener)
            .arg(target)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        self.status = Some(match spawned {
            Ok(_) => format!("See {target} in its own program"),
            Err(error) => format!("Open {target} by hand, {opener} failed: {error}"),
        });
    }

    fn row_height(&self) -> f32 {
        (f32::from(self.cell.get().height) * 1.35).round()
    }

    fn hit_height(&self) -> f32 {
        self.row_height() * 1.7
    }

    fn bar_height(&self) -> f32 {
        (f32::from(self.cell.get().height) * 1.6).round()
    }

    fn search_height(&self) -> f32 {
        (f32::from(self.cell.get().height) * 2.6).round()
    }

    /// The terminal's size in pixels; a test has no terminal.
    fn screen(&self) -> Option<(f32, f32)> {
        let size = katui::rt::tty::size().ok()?;
        let cell = self.cell.get();
        Some((
            f32::from(size.columns) * f32::from(cell.width),
            f32::from(size.rows) * f32::from(cell.height),
        ))
    }

    fn files_viewport(&self) -> Option<f32> {
        let (_, height) = self.screen()?;
        Some(height - self.bar_height() - self.search_height() - self.list_padding() * 2.0)
    }

    fn list_padding(&self) -> f32 {
        (f32::from(self.cell.get().height) * 0.4).round()
    }

    fn page_viewport(&self) -> f32 {
        self.screen()
            .map_or(400.0, |(_, height)| height - self.bar_height())
    }

    fn line(&self) -> f32 {
        f32::from(self.cell.get().height) * 2.0
    }
}

/// `path` with its `.` and `..` parts applied, or `None` when it climbs
/// out of the root.
fn normalize(path: &Path) -> Option<PathBuf> {
    let mut parts: Vec<&std::ffi::OsStr> = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => parts.push(part),
            Component::ParentDir => {
                parts.pop()?;
            }
            Component::CurDir | Component::RootDir | Component::Prefix(_) => {}
        }
    }
    Some(parts.iter().collect())
}

fn has_scheme(href: &str) -> bool {
    href.split_once(':')
        .is_some_and(|(scheme, _)| !scheme.contains('/'))
}

#[allow(clippy::cast_precision_loss)] // a sidebar never lists 2^24 rows
fn pixels(index: usize) -> f32 {
    index as f32
}

fn decode(path: &Path) -> Option<Arc<Surface>> {
    let rgba = image::open(path).ok()?.to_rgba8();
    Surface::from_rgba(rgba.width(), rgba.height(), rgba.into_raw()).map(Arc::new)
}

impl HtmlHost<Message> for Viewer {
    fn link(&self, href: &str) -> Message {
        Message::Link(href.to_owned())
    }

    fn image(&self, src: &str) -> Option<Arc<Surface>> {
        if has_scheme(src) {
            return None;
        }
        self.images
            .borrow_mut()
            .entry(src.to_owned())
            .or_insert_with(|| {
                let path = self.library.root.join(self.resolve(src)?);
                path.is_file().then(|| decode(&path)).flatten()
            })
            .clone()
    }
}

/// The view, apart from the state changes above.
impl Viewer {
    fn sidebar(&self, theme: &Theme) -> Element<Message> {
        let searching = self.pane == Pane::Search;
        let search = Element::column([text_field(&self.query, "Press / to search", searching)])
            .padding_xy(theme.cell_width(), self.list_padding())
            .height(Length::Px(self.search_height()))
            .justify(Align::Center)
            .width(Length::Fill);
        let rows: Vec<Element<Message>> = if searching {
            self.hit_rows(theme)
        } else {
            self.rows()
                .into_iter()
                .map(|id| self.file_row(id, theme))
                .collect()
        };
        let list = Element::column(rows)
            .scroll(self.files_scroll.offset())
            .on_scroll(Message::FilesScrolled)
            .scrollbar(theme.muted)
            .padding_xy(theme.cell_width() * 0.5, self.list_padding())
            .size(Length::Fill, Length::Fill);
        Element::column([search, list])
            .size(Length::Fill, Length::Fill)
            .background(theme.raised.clone())
    }

    fn file_row(&self, id: usize, theme: &Theme) -> Element<Message> {
        let Some(entry) = self.library.entry(id) else {
            return Element::row([]);
        };
        let marker = match (entry.is_folder(), self.open.contains(&id)) {
            (false, _) => "",
            (true, false) => "+",
            (true, true) => "\u{2212}",
        };
        let color = if self.page == Some(id) {
            theme.accent
        } else {
            theme.text
        };
        let indent = Element::row([]).width(theme.columns(1.5 * pixels(entry.depth)));
        let marker = Element::row([text(marker, theme.body(), theme.muted).weight(700)])
            .width(theme.columns(1.8))
            .justify(Align::Center);
        let name = text(&entry.name, theme.body(), color);
        let name = if entry.is_folder() {
            name.weight(700)
        } else {
            name
        };
        let row = Element::row([indent, marker, name])
            .align(Align::Center)
            .padding_xy(theme.cell_width() * 0.5, 0.0)
            .radius(theme.radius())
            .size(Length::Fill, Length::Px(self.row_height()))
            .on_click(Message::Pick(id));
        self.highlight(
            row,
            self.selected == Some(id) && self.pane != Pane::Search,
            theme,
        )
    }

    fn hit_rows(&self, theme: &Theme) -> Vec<Element<Message>> {
        if self.hits.is_empty() {
            let hint = if self.query.text().trim().is_empty() {
                "Type to search the file names and their text"
            } else {
                "Try other words, nothing matches"
            };
            return vec![text(hint, theme.small(), theme.muted).padding(theme.cell_width())];
        }
        self.hits
            .iter()
            .enumerate()
            .filter_map(|(index, hit)| {
                let entry = self.library.entry(hit.entry)?;
                let detail = hit
                    .line
                    .clone()
                    .unwrap_or_else(|| entry.path.display().to_string());
                let row = Element::column([
                    text(&entry.name, theme.body(), theme.text).weight(700),
                    text(&detail, theme.small(), theme.muted),
                ])
                .justify(Align::Center)
                .padding_xy(theme.cell_width(), 0.0)
                .radius(theme.radius())
                .size(Length::Fill, Length::Px(self.hit_height()))
                .on_click(Message::Hit(index));
                Some(self.highlight(row, index == self.hit, theme))
            })
            .collect()
    }

    fn highlight(&self, row: Element<Message>, selected: bool, theme: &Theme) -> Element<Message> {
        match (selected, self.pane) {
            (false, _) => row,
            (true, Pane::Files | Pane::Search) => row.background(theme.highlight.clone()),
            (true, Pane::Page) => row.border(1.0, theme.highlight.clone()),
        }
    }

    fn document(&self, theme: &Theme) -> Element<Message> {
        let content = match self
            .page
            .and_then(|id| self.library.entry(id))
            .and_then(|entry| entry.text.as_deref())
        {
            Some(source) => {
                let defaults = HtmlDefaults {
                    font_size: theme.body(),
                    color: theme.text,
                    link: theme.accent,
                    tint: self.tint(),
                };
                markdown_view(source, &defaults, self)
            }
            None => text(
                "Run md-viewer in a folder with markdown files",
                theme.body(),
                theme.muted,
            ),
        };
        let gutter = theme.cell_width() * 3.0;
        let width = self.screen().map_or(Length::Fill, |(width, _)| {
            let pane = width * (1.0 - self.sidebar) - gutter * 2.0;
            Length::Px(pane.min(theme.cell_width() * PAGE_COLUMNS))
        });
        Element::column([content.width(width)])
            .align(Align::Center)
            .scroll(self.page_scroll.offset())
            .on_scroll(Message::PageScrolled)
            .scrollbar(theme.muted)
            .padding_xy(gutter, theme.cell_height())
            .size(Length::Fill, Length::Fill)
    }

    fn tint(&self) -> Rgba {
        if self.light {
            Rgba::solid(234, 236, 240)
        } else {
            Rgba::solid(44, 41, 60)
        }
    }

    fn status_bar(&self, theme: &Theme) -> Element<Message> {
        let page = self.page.and_then(|id| self.library.entry(id));
        let left = match (&self.status, page) {
            (Some(status), _) => text(status, theme.small(), theme.accent),
            (None, Some(entry)) => text(
                &entry.path.display().to_string(),
                theme.small(),
                theme.muted,
            ),
            (None, None) => Element::row([]),
        };
        let hints: &[(&str, &str)] = match self.pane {
            Pane::Files => &[
                ("j k", "move"),
                ("l", "open"),
                ("h", "close"),
                ("/", "search"),
                ("Tab", "read the page"),
                ("q", "quit"),
            ],
            Pane::Page => &[
                ("j k", "scroll"),
                ("d u", "half page"),
                ("Bksp", "go back"),
                ("/", "search"),
                ("Tab", "pick a file"),
                ("t", "switch theme"),
            ],
            Pane::Search => &[
                ("\u{2191} \u{2193}", "pick"),
                ("Enter", "open"),
                ("Esc", "stop searching"),
            ],
        };
        Element::row([left, Element::spacer(), key_hints(hints)])
            .align(Align::Center)
            .padding_xy(theme.cell_width(), 0.0)
            .size(Length::Fill, Length::Px(self.bar_height()))
            .background(theme.raised.clone())
    }
}

fn text(content: &str, size: f32, color: Rgba) -> Element<Message> {
    Element::text(content.to_owned())
        .font_size(size)
        .color(color)
}

/// The keys, one pane at a time.
impl Viewer {
    fn files_key(&mut self, key: Key) -> bool {
        match key {
            Key::Char('j') | Key::Down => self.step(1),
            Key::Char('k') | Key::Up => self.step(-1),
            Key::Char('g') | Key::Home => self.select_edge(false),
            Key::Char('G') | Key::End => self.select_edge(true),
            Key::Char('l') | Key::Right => self.forward(false),
            Key::Enter => self.forward(true),
            Key::Char('h') | Key::Left => self.backward(),
            _ => return false,
        }
        true
    }

    fn page_key(&mut self, key: Key, mods: Mods) -> bool {
        let page = self.page_viewport();
        let by = match (key, mods.ctrl) {
            (Key::Char('j'), false) => self.line(),
            (Key::Char('k'), false) => -self.line(),
            (Key::Char('d'), _) => page / 2.0,
            (Key::Char('u'), _) => -page / 2.0,
            (Key::Char(' ' | 'f'), false) => page * 0.9,
            (Key::Char('b'), false) => -page * 0.9,
            (Key::Char('g'), false) => -f32::MAX,
            (Key::Char('G'), false) => f32::MAX / 2.0,
            (Key::Char('h'), false) => {
                self.pane = Pane::Files;
                return true;
            }
            (Key::Backspace, _) => {
                self.back();
                return true;
            }
            _ => return false,
        };
        self.page_scroll.scroll_by(by);
        true
    }

    fn search_input(&mut self, event: &InputEvent) -> Control {
        if let InputEvent::Key(key, mods) = event {
            match (key, mods.ctrl) {
                (Key::Escape, _) => self.leave_search(),
                (Key::Enter, _) => self.open_hit(self.hit),
                (Key::Down, _) | (Key::Char('n'), true) => self.move_hit(1),
                (Key::Up, _) | (Key::Char('p'), true) => self.move_hit(-1),
                _ => return self.type_query(event),
            }
            return Control::Redraw;
        }
        self.type_query(event)
    }

    fn type_query(&mut self, event: &InputEvent) -> Control {
        match self.query.handle(event) {
            Edit::Ignored => Control::Wait,
            Edit::Changed => {
                self.hits = self.library.search(self.query.text());
                self.hit = 0;
                self.files_scroll = ScrollState::default();
                Control::Redraw
            }
            Edit::Moved | Edit::Submitted => Control::Redraw,
        }
    }

    /// The keys every pane but the search shares.
    fn shared_key(&mut self, key: Key) -> Option<Control> {
        match key {
            Key::Char('q') => return Some(Control::Quit),
            Key::Char('/') => {
                self.pane = Pane::Search;
                self.files_scroll = ScrollState::default();
            }
            Key::Tab => {
                self.pane = if self.pane == Pane::Files {
                    Pane::Page
                } else {
                    Pane::Files
                };
            }
            Key::Char('t') => self.light = !self.light,
            _ => return None,
        }
        Some(Control::Redraw)
    }
}

impl UiApp for Viewer {
    type Message = Message;

    fn view(&self) -> Element<Message> {
        let theme = Theme::current();
        let body = split(
            self.sidebar(&theme),
            self.document(&theme),
            self.sidebar,
            Orientation::Horizontal,
            Message::Resized,
        );
        Element::column([body, self.status_bar(&theme)])
            .size(Length::Fill, Length::Fill)
            .background(theme.background.clone())
    }

    fn update(&mut self, message: Message) -> Control {
        match message {
            Message::Pick(id) => self.pick(id),
            Message::Hit(index) => self.open_hit(index),
            Message::Link(href) => self.follow(&href),
            Message::PageScrolled(scroll) => self.page_scroll.apply(scroll),
            Message::FilesScrolled(scroll) => self.files_scroll.apply(scroll),
            Message::Resized(fraction) => self.sidebar = fraction,
        }
        Control::Redraw
    }

    fn input(&mut self, event: &InputEvent) -> Control {
        self.status = None;
        if self.pane == Pane::Search {
            return self.search_input(event);
        }
        if self.pane == Pane::Page && self.page_scroll.handle(event, self.line()) {
            return Control::Redraw;
        }
        let InputEvent::Key(key, mods) = *event else {
            return Control::Wait;
        };
        if let Some(control) = self.shared_key(key) {
            return control;
        }
        let handled = match self.pane {
            Pane::Files => self.files_key(key),
            Pane::Page => self.page_key(key, mods),
            Pane::Search => false,
        };
        if handled {
            Control::Redraw
        } else {
            Control::Wait
        }
    }

    fn theme(&self, cell: CellPixels) -> Theme {
        self.cell.set(cell);
        if self.light {
            Theme::light(cell)
        } else {
            Theme::dark(cell)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use katui::app::UiApp as _;
    use katui::proto::input::{InputEvent, Key, Mods};

    use super::{Pane, Viewer, normalize};
    use crate::library::Library;

    fn viewer() -> Viewer {
        let library = Library::scan(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."));
        Viewer::new(library, None)
    }

    fn press(viewer: &mut Viewer, keys: &str) {
        for key in keys.chars() {
            let key = match key {
                '\n' => Key::Enter,
                '\t' => Key::Tab,
                '\u{1b}' => Key::Escape,
                other => Key::Char(other),
            };
            viewer.input(&InputEvent::Key(key, Mods::NONE));
        }
    }

    fn page_name(viewer: &Viewer) -> String {
        viewer
            .page
            .map(|id| viewer.library.entries[id].path.display().to_string())
            .unwrap_or_default()
    }

    #[test]
    fn it_opens_the_readme_and_walks_the_tree_with_vi_keys() {
        let mut viewer = viewer();
        assert_eq!(page_name(&viewer), "README.md");
        press(&mut viewer, "g");
        let first = viewer.selected;
        press(&mut viewer, "l");
        let opened = viewer
            .selected
            .is_some_and(|id| !viewer.library.entries[id].is_folder());
        assert!(
            opened || viewer.rows().len() > viewer.library.top.len(),
            "l opens the folder"
        );
        press(&mut viewer, "jjkk");
        assert_eq!(viewer.selected, first);
        press(&mut viewer, "\t");
        assert_eq!(viewer.pane, Pane::Page);
    }

    #[test]
    fn search_opens_a_page_found_by_its_text() {
        let mut viewer = viewer();
        press(&mut viewer, "/");
        assert_eq!(viewer.pane, Pane::Search);
        press(&mut viewer, "trusted publishing");
        assert!(!viewer.hits.is_empty());
        press(&mut viewer, "\n");
        assert_eq!(viewer.pane, Pane::Page);
        assert!(viewer.hits.is_empty());
        assert_ne!(page_name(&viewer), "README.md");
    }

    #[test]
    fn links_open_other_pages_and_back_returns() {
        let mut viewer = viewer();
        viewer.follow("docs/python.md#install");
        assert_eq!(page_name(&viewer), "docs/python.md");
        viewer.follow("../README.md");
        assert_eq!(page_name(&viewer), "README.md");
        viewer.follow("docs/missing.md");
        assert!(
            viewer
                .status
                .as_deref()
                .is_some_and(|status| status.starts_with("Check the link"))
        );
        press(&mut viewer, "\t\u{7f}");
        viewer.input(&InputEvent::Key(Key::Backspace, Mods::NONE));
        assert_eq!(page_name(&viewer), "docs/python.md");
        assert_eq!(normalize(Path::new("a/../../b")), None);
    }

    #[test]
    fn the_view_renders_this_demos_readme() {
        let mut viewer = viewer();
        let readme = viewer.library.find(Path::new("demos/md-viewer/README.md"));
        viewer.navigate(readme.expect("the demo has a README"));
        assert_eq!(page_name(&viewer), "demos/md-viewer/README.md");
        let cell = katui::ui::CellPixels {
            width: 9,
            height: 19,
        };
        let view = katui::widgets::Theme::dark(cell).scope(|_| viewer.view());
        let engine =
            katui::draw::TextEngine::new(katui::draw::bundled_fonts()).expect("the fonts parse");
        let mut ui: katui::ui::Ui<super::Message> =
            katui::ui::Ui::new(engine, katui::ui::UiOptions::default());
        let size = (9 * 170, 19 * 52);
        let surface = ui
            .render(&view, size, cell)
            .flatten(cell, size)
            .expect("whole pixels");
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../target/screenshots/md-viewer.png"
        );
        std::fs::create_dir_all(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../target/screenshots"
        ))
        .expect("the target folder is writable");
        image::save_buffer(
            path,
            surface.pixels(),
            surface.width(),
            surface.height(),
            image::ExtendedColorType::Rgba8,
        )
        .expect("the target folder is writable");
    }
}
