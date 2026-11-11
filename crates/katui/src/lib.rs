//! katui in one dependency: every layer of the engine under one name,
//! the optional ones behind features.
//!
//! The protocol codecs, the scene, the presenters and the runtime are
//! always here. `widgets` (with `ui` and `draw`) and `layout` come by
//! default; `app`, `game`, `content`, `html` and `ratatui` are opt-in.
//!
//! ```
//! use katui::scene::CellSize;
//!
//! let size = CellSize { columns: 80, rows: 24 };
//! assert_eq!(size.columns, 80);
//! ```

pub use katui_present as present;
pub use katui_proto as proto;
pub use katui_rt as rt;
pub use katui_scene as scene;

#[cfg(feature = "app")]
pub use katui_app as app;
#[cfg(feature = "content")]
pub use katui_content as content;
#[cfg(feature = "draw")]
pub use katui_draw as draw;
#[cfg(feature = "game")]
pub use katui_game as game;
#[cfg(feature = "layout")]
pub use katui_layout as layout;
#[cfg(feature = "ratatui")]
pub use katui_ratatui as ratatui;
#[cfg(feature = "ui")]
pub use katui_ui as ui;
#[cfg(feature = "widgets")]
pub use katui_widgets as widgets;
