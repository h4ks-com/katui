//! Every katui crate re-exported under one name, with the optional ones
//! behind the features listed in `Cargo.toml`. The guides are in
//! <https://github.com/matheusfillipe/katui/tree/main/docs>.

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

/// The guides in `docs/`, compiled as doctests so their programs keep
/// building against the current API.
#[cfg(all(
    doctest,
    feature = "app",
    feature = "game",
    feature = "html",
    feature = "ratatui"
))]
mod guides {
    #[doc = include_str!("../../../docs/getting-started.md")]
    struct GettingStarted;
    #[doc = include_str!("../../../docs/ui-apps.md")]
    struct UiApps;
    #[doc = include_str!("../../../docs/games.md")]
    struct Games;
    #[doc = include_str!("../../../docs/video.md")]
    struct Video;
    #[doc = include_str!("../../../docs/html-and-markdown.md")]
    struct HtmlAndMarkdown;
    #[doc = include_str!("../../../docs/ratatui.md")]
    struct Ratatui;
}
