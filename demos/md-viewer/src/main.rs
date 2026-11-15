//! A markdown reader for a repository's docs: a file tree and a search
//! beside the rendered page. Run `md-viewer` in a folder, or pass a folder
//! or a markdown file.

mod library;
mod viewer;

use std::path::PathBuf;

use library::Library;
use viewer::Viewer;

fn main() -> Result<(), katui::rt::RtError> {
    let target = std::env::args()
        .nth(1)
        .map_or_else(|| PathBuf::from("."), PathBuf::from);
    let (root, file) = if target.is_file() {
        let root = target
            .parent()
            .map_or_else(|| PathBuf::from("."), PathBuf::from);
        let root = if root.as_os_str().is_empty() {
            PathBuf::from(".")
        } else {
            root
        };
        (root, target.file_name().map(PathBuf::from))
    } else {
        (target, None)
    };
    let library = Library::scan(&root);
    let first = file.and_then(|file| library.find(&file));
    katui::app::run(Viewer::new(library, first))
}
