pub mod confirmation;
pub mod restart;
pub mod text_input;
pub mod text_overlay;

#[allow(unused_imports)]
pub use confirmation::ConfirmationOverlay;
#[allow(unused_imports)]
pub use text_input::TextInputOverlay;
#[allow(unused_imports)]
pub use restart::RestartOverlay;
#[allow(unused_imports)]
pub use text_overlay::TextOverlay;

use ratatui::prelude::*;
use ratatui::widgets::Clear;

/// Calculate a centered rect of given percentage within `area`.
#[allow(dead_code)]
pub fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let popup_layout = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
    .split(area);
    Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .split(popup_layout[1])[1]
}

/// Render an overlay: clears the area then renders the widget centered.
#[allow(dead_code)]
pub fn render_overlay(
    frame: &mut Frame,
    area: Rect,
    widget: impl Widget,
    percent_x: u16,
    percent_y: u16,
) {
    let overlay_area = centered_rect(percent_x, percent_y, area);
    frame.render_widget(Clear, overlay_area);
    frame.render_widget(widget, overlay_area);
}
