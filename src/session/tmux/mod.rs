pub mod pty;

/// Prefix for all league tmux session names.
pub const TMUX_PREFIX: &str = "league_";

/// Sanitize a session name for use as a tmux session name.
/// Replaces non-alphanumeric characters with underscores and adds prefix.
pub fn sanitize_name(name: &str) -> String {
    let sanitized: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' { c } else { '_' })
        .collect();
    // Collapse consecutive underscores
    let mut result = String::with_capacity(sanitized.len());
    let mut prev_underscore = false;
    for c in sanitized.chars() {
        if c == '_' {
            if !prev_underscore {
                result.push(c);
            }
            prev_underscore = true;
        } else {
            result.push(c);
            prev_underscore = false;
        }
    }
    // Trim trailing underscores
    let trimmed = result.trim_end_matches('_');
    format!("{}{}", TMUX_PREFIX, trimmed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_name_simple() {
        assert_eq!(sanitize_name("asdf"), format!("{}asdf", TMUX_PREFIX));
    }

    #[test]
    fn test_sanitize_name_special_chars() {
        assert_eq!(
            sanitize_name("a sd f . . asdf"),
            format!("{}a_sd_f_asdf", TMUX_PREFIX)
        );
    }
}
