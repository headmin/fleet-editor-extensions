//! Position utilities for converting between byte offsets and LSP positions.
//!
//! LSP uses 0-indexed line numbers and UTF-16 code unit offsets for columns.

use tower_lsp::lsp_types::Position;

/// Find the line and column (1-indexed) of a YAML key in source text.
///
/// Returns the position of the first character of the key name.
pub fn find_yaml_key(source: &str, key: &str, occurrence: usize) -> Option<(usize, usize)> {
    let pattern = format!(r"(?m)^\s*{}:", regex::escape(key));
    let re = regex::Regex::new(&pattern).ok()?;

    let matches: Vec<_> = re.find_iter(source).collect();
    matches.get(occurrence).map(|m| {
        let line = source[..m.start()].matches('\n').count() + 1;
        let line_start = source[..m.start()].rfind('\n').map(|i| i + 1).unwrap_or(0);
        let col = m.start() - line_start + 1;
        (line, col)
    })
}

/// Convert 1-indexed line/column to LSP Position (0-indexed, UTF-16 code units).
pub fn to_lsp_position(line: usize, col: usize, source: &str) -> Position {
    let line_0 = line.saturating_sub(1) as u32;

    // Find the line content for UTF-16 conversion
    let line_content = source.lines().nth(line_0 as usize).unwrap_or("");
    let byte_col = col.saturating_sub(1);

    // Convert byte offset to UTF-16 code units
    let utf16_col = byte_offset_to_utf16(line_content, byte_col);

    Position {
        line: line_0,
        character: utf16_col,
    }
}

/// Convert a byte offset within a line to UTF-16 code units.
fn byte_offset_to_utf16(line: &str, byte_offset: usize) -> u32 {
    let mut utf16_offset = 0u32;
    let mut current_byte = 0usize;

    for c in line.chars() {
        if current_byte >= byte_offset {
            break;
        }
        utf16_offset += c.len_utf16() as u32;
        current_byte += c.len_utf8();
    }

    utf16_offset
}

/// Line index for efficient line number lookups.
///
/// Pre-computes line start byte offsets for O(log n) line number lookups.
pub struct LineIndex {
    /// Byte offset of the start of each line (0-indexed).
    line_starts: Vec<usize>,
}

impl LineIndex {
    /// Build a line index from source text.
    pub fn new(source: &str) -> Self {
        let mut line_starts = vec![0];
        for (i, c) in source.char_indices() {
            if c == '\n' {
                line_starts.push(i + 1);
            }
        }
        Self { line_starts }
    }

    /// Get the 0-indexed line number for a byte offset.
    pub fn line_of(&self, byte_offset: usize) -> usize {
        match self.line_starts.binary_search(&byte_offset) {
            Ok(line) => line,
            Err(line) => line.saturating_sub(1),
        }
    }

    /// Get the column (0-indexed byte offset from line start) for a byte offset.
    pub fn column_of(&self, byte_offset: usize) -> usize {
        let line = self.line_of(byte_offset);
        byte_offset - self.line_starts[line]
    }

    /// Convert a byte offset to an LSP Position.
    pub fn to_position(&self, byte_offset: usize, source: &str) -> Position {
        let line = self.line_of(byte_offset);
        let col_byte = self.column_of(byte_offset);

        // Get the line content for UTF-16 conversion
        let line_start = self.line_starts[line];
        let line_end = self
            .line_starts
            .get(line + 1)
            .map(|&s| s.saturating_sub(1))
            .unwrap_or(source.len());
        let line_content = &source[line_start..line_end];

        let utf16_col = byte_offset_to_utf16(line_content, col_byte);

        Position {
            line: line as u32,
            character: utf16_col,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_line_index_simple() {
        let source = "line1\nline2\nline3";
        let index = LineIndex::new(source);

        assert_eq!(index.line_of(0), 0); // 'l' in line1
        assert_eq!(index.line_of(5), 0); // '\n' after line1
        assert_eq!(index.line_of(6), 1); // 'l' in line2
        assert_eq!(index.line_of(12), 2); // 'l' in line3
    }

    #[test]
    fn test_find_yaml_key() {
        let source = "policies:\n  - name: test\n    query: SELECT 1";
        assert_eq!(find_yaml_key(source, "policies", 0), Some((1, 1)));
        assert_eq!(find_yaml_key(source, "name", 0), Some((2, 5)));
        assert_eq!(find_yaml_key(source, "query", 0), Some((3, 5)));
    }

    #[test]
    fn test_utf16_conversion() {
        // ASCII-only
        assert_eq!(byte_offset_to_utf16("hello", 3), 3);

        // Multi-byte UTF-8 char (emoji = 4 bytes UTF-8, 2 UTF-16 code units)
        let line = "hi 👋 there";
        // 'h'=0, 'i'=1, ' '=2, '👋'=3-6, ' '=7, 't'=8
        assert_eq!(byte_offset_to_utf16(line, 0), 0); // 'h'
        assert_eq!(byte_offset_to_utf16(line, 3), 3); // start of emoji
        assert_eq!(byte_offset_to_utf16(line, 7), 5); // space after emoji (3 + 2 for emoji)
    }
}
