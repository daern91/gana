/// Statistics from a git diff.
#[derive(Debug, Clone, Default)]
pub struct DiffStats {
    pub content: String,
    pub added_lines: usize,
    pub removed_lines: usize,
}
