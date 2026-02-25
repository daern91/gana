pub mod diff;
pub mod err;
pub mod list;
pub mod menu;
pub mod overlay;
pub mod preview;
pub mod tabbed_window;

pub use diff::DiffView;
pub use err::ErrorDisplay;
pub use list::ListPane;
pub use menu::MenuBar;
pub use preview::PreviewPane;
pub use tabbed_window::{Tab, TabbedWindow};

// UI constants matching the Go version
pub const MIN_WIDTH: u16 = 40;
pub const MIN_HEIGHT: u16 = 10;
