/// Return the help text displayed in the help overlay.
pub fn help_text() -> String {
    format!(
        "\
☸ Gana — Orchestrate Your AI Agent Teams

Navigation:
  j/↓      Move down
  k/↑      Move up
  Enter    Attach to session
  Tab      Switch Preview/Diff

Session Management:
  n        New session
  N        New session with prompt
  d        Delete session
  D        Kill session (force)
  a        Attach to session

Preview:
  K        Scroll up
  J        Scroll down
  Esc      Reset scroll

General:
  ?        Toggle help
  q        Quit

Version: {}",
        env!("CARGO_PKG_VERSION")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_help_text_contains_version() {
        let text = help_text();
        assert!(text.contains("Version:"));
        assert!(text.contains(env!("CARGO_PKG_VERSION")));
    }

    #[test]
    fn test_help_text_contains_key_bindings() {
        let text = help_text();
        assert!(text.contains("j/↓"));
        assert!(text.contains("k/↑"));
        assert!(text.contains("New session"));
        assert!(text.contains("Kill session"));
        assert!(text.contains("Quit"));
    }
}
