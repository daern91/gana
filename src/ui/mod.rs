#[allow(unused_imports)]
pub mod consts;
#[allow(unused_imports)]
pub mod diff;
pub mod err;
pub mod list;
pub mod menu;
pub mod overlay;
pub mod preview;
pub mod tabbed_window;

#[allow(unused_imports)]
pub use diff::DiffView;
#[allow(unused_imports)]
pub use err::ErrorDisplay;
#[allow(unused_imports)]
pub use list::ListPane;
#[allow(unused_imports)]
pub use menu::MenuBar;
#[allow(unused_imports)]
pub use preview::PreviewPane;
#[allow(unused_imports)]
pub use tabbed_window::{Tab, TabbedWindow};

// UI constants matching the Go version
#[allow(dead_code)]
pub const MIN_WIDTH: u16 = 40;
#[allow(dead_code)]
pub const MIN_HEIGHT: u16 = 10;
