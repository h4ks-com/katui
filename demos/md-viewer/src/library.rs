//! The markdown files of a folder: their tree, their text and the search
//! over both.

use std::fs;
use std::path::{Path, PathBuf};

/// Folders that hold build output or dependencies, never a project's docs.
const SKIPPED: [&str; 6] = [
    "target",
    "node_modules",
    "vendor",
    "dist",
    "build",
    "__pycache__",
];
const DEEPEST: usize = 12;
const LARGEST_FILE: u64 = 4 * 1024 * 1024;
const MOST_HITS: usize = 200;

/// A file or a folder in the tree. Its id is its index in
/// [`Library::entries`].
#[derive(Debug)]
pub(crate) struct Entry {
    /// The path from the library's root.
    pub path: PathBuf,
    pub name: String,
    pub depth: usize,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    /// The file's text; folders have none.
    pub text: Option<String>,
}

impl Entry {
    pub(crate) fn is_folder(&self) -> bool {
        self.text.is_none()
    }
}

/// A search result: the entry, and the first line of its text that
/// matches when its path does not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Hit {
    pub entry: usize,
    pub line: Option<String>,
}

#[derive(Debug)]
pub(crate) struct Library {
    pub root: PathBuf,
    pub entries: Vec<Entry>,
    /// The entries at the top of the tree.
    pub top: Vec<usize>,
}

impl Library {
    /// Every markdown file under `root`, skipping hidden folders and the
    /// ones in [`SKIPPED`].
    pub(crate) fn scan(root: &Path) -> Self {
        let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        let mut library = Self {
            root,
            entries: Vec::new(),
            top: Vec::new(),
        };
        let root = library.root.clone();
        library.top = library.walk(&root, None, 0);
        library
    }

    fn walk(&mut self, folder: &Path, parent: Option<usize>, depth: usize) -> Vec<usize> {
        if depth > DEEPEST {
            return Vec::new();
        }
        let mut children = listing(folder);
        children
            .sort_by_cached_key(|(path, folder)| (!folder, path.to_string_lossy().to_lowercase()));
        let mut ids = Vec::new();
        for (path, is_folder) in children {
            let id = self.entries.len();
            let text = if is_folder { None } else { read(&path) };
            if !is_folder && text.is_none() {
                continue;
            }
            self.entries.push(Entry {
                path: path.strip_prefix(&self.root).unwrap_or(&path).to_path_buf(),
                name: name_of(&path),
                depth,
                parent,
                children: Vec::new(),
                text,
            });
            if is_folder {
                let inside = self.walk(&path, Some(id), depth + 1);
                if inside.is_empty() {
                    self.entries.truncate(id);
                    continue;
                }
                if let Some(entry) = self.entries.get_mut(id) {
                    entry.children = inside;
                }
            }
            ids.push(id);
        }
        ids
    }

    pub(crate) fn entry(&self, id: usize) -> Option<&Entry> {
        self.entries.get(id)
    }

    pub(crate) fn parent(&self, id: usize) -> Option<usize> {
        self.entry(id).and_then(|entry| entry.parent)
    }

    pub(crate) fn is_folder(&self, id: usize) -> bool {
        self.entry(id).is_some_and(Entry::is_folder)
    }

    /// The README at the top of the tree, or else the first file.
    pub(crate) fn front_page(&self) -> Option<usize> {
        let readme = self.top.iter().copied().find(|&id| {
            self.entry(id).is_some_and(|entry| {
                let name = entry.name.to_lowercase();
                name == "readme.md" || name == "readme.markdown"
            })
        });
        readme.or_else(|| self.entries.iter().position(|entry| !entry.is_folder()))
    }

    /// The file at `path` from the root.
    pub(crate) fn find(&self, path: &Path) -> Option<usize> {
        self.entries
            .iter()
            .position(|entry| entry.path == path && !entry.is_folder())
    }

    /// The files whose path holds every word of `query`, then the files
    /// whose text does, each with the first line that matches.
    pub(crate) fn search(&self, query: &str) -> Vec<Hit> {
        let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
        if words.is_empty() {
            return Vec::new();
        }
        let holds_all = |text: &str| {
            let text = text.to_lowercase();
            words.iter().all(|word| text.contains(word))
        };
        let files = self
            .entries
            .iter()
            .enumerate()
            .filter_map(|(id, entry)| entry.text.as_deref().map(|text| (id, entry, text)));
        let mut by_path = Vec::new();
        let mut by_text = Vec::new();
        for (id, entry, text) in files {
            if holds_all(&entry.path.to_string_lossy()) {
                by_path.push(Hit {
                    entry: id,
                    line: None,
                });
            } else if let Some(line) = text.lines().find(|line| holds_all(line)) {
                by_text.push(Hit {
                    entry: id,
                    line: Some(line.trim().to_owned()),
                });
            }
        }
        by_path.extend(by_text);
        by_path.truncate(MOST_HITS);
        by_path
    }
}

/// The folders and markdown files right inside `folder`.
fn listing(folder: &Path) -> Vec<(PathBuf, bool)> {
    let Ok(entries) = fs::read_dir(folder) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let kind = entry.file_type().ok()?;
            let shown = !name.starts_with('.') && !SKIPPED.contains(&name.as_str());
            let path = entry.path();
            match () {
                () if shown && kind.is_dir() => Some((path, true)),
                () if kind.is_file() && is_markdown(&path) => Some((path, false)),
                () => None,
            }
        })
        .collect()
}

pub(crate) fn is_markdown(path: &Path) -> bool {
    path.extension().is_some_and(|extension| {
        extension.eq_ignore_ascii_case("md") || extension.eq_ignore_ascii_case("markdown")
    })
}

fn read(path: &Path) -> Option<String> {
    let size = fs::metadata(path).ok()?.len();
    (size <= LARGEST_FILE)
        .then(|| fs::read_to_string(path).ok())
        .flatten()
}

fn name_of(path: &Path) -> String {
    path.file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::Library;

    fn repo() -> Library {
        Library::scan(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
    }

    #[test]
    fn the_tree_holds_the_docs_and_leaves_build_output_out() {
        let library = repo();
        let paths: Vec<_> = library
            .entries
            .iter()
            .map(|entry| entry.path.clone())
            .collect();
        assert!(paths.contains(&Path::new("docs/python.md").to_path_buf()));
        assert!(paths.iter().all(|path| !path.starts_with("target")));
        let front = library
            .front_page()
            .map(|id| library.entries[id].name.clone());
        assert_eq!(front.as_deref(), Some("README.md"));
        let docs = library.find(Path::new("docs")).is_none();
        assert!(docs, "a folder is not a page");
    }

    #[test]
    fn search_finds_paths_first_then_lines_of_text() {
        let library = repo();
        let hits = library.search("python");
        let first = &library.entries[hits[0].entry];
        assert!(first.path.to_string_lossy().contains("python"));
        assert!(hits[0].line.is_none());
        let text = library.search("pip install katui");
        assert!(text.iter().any(|hit| {
            hit.line
                .as_deref()
                .is_some_and(|line| line.contains("pip install katui"))
        }));
        assert!(library.search("   ").is_empty());
    }
}
