//! Reactive element trees over katui layers.
//!
//! An application rebuilds an [`Element`] tree from its state on every
//! change. [`Ui::render`] lays the tree out, rasterizes only the layers whose
//! content changed and returns placements plus the images they show.
//! Scrolling a list of layers changes placements and uploads no pixels.

mod element;
mod layout;
mod resolve;
mod ui;

pub use element::{Align, Element, Length, Scroll};
pub use katui_draw::TextSpan;
pub use katui_proto::probe::CellPixels;
pub use resolve::Hit;
pub use ui::{Hover, LayerImage, RenderOutput, Ui, UiOptions};
