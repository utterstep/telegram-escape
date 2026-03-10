// _*[]()~`>#+-=|{}.!\

/// Returns true if the character is a Telegram MarkdownV2 special character
/// that must be escaped with `\` in regular text.
fn is_tg_special(c: char) -> bool {
    matches!(
        c,
        '_' | '*'
            | '['
            | ']'
            | '('
            | ')'
            | '~'
            | '`'
            | '>'
            | '#'
            | '+'
            | '-'
            | '='
            | '|'
            | '{'
            | '}'
            | '.'
            | '!'
            | '\\'
    )
}

/// Push a character to `out`, escaping it for regular Telegram MarkdownV2 text.
fn push_escaped(out: &mut String, c: char) {
    if is_tg_special(c) {
        out.push('\\');
    }
    out.push(c);
}

/// Push a character to `out`, escaping it for code context (only `` ` `` and `\`).
fn push_code_escaped(out: &mut String, c: char) {
    if c == '`' || c == '\\' {
        out.push('\\');
    }
    out.push(c);
}

/// Find the position of a closing delimiter `delim` in `bytes` starting from `start`.
/// Returns the byte offset (relative to `bytes`) of the first character of the closing delimiter,
/// or `None` if not found.
///
/// Skips over:
/// - already-escaped characters (`\X`)
/// - inline code spans (`` `...` ``)
/// - code blocks (` ```...``` `)
fn find_closing(bytes: &[u8], start: usize, delim: &[u8]) -> Option<usize> {
    let len = bytes.len();
    let mut i = start;

    while i < len {
        // Skip already-escaped characters
        if bytes[i] == b'\\' && i + 1 < len && is_tg_special(bytes[i + 1] as char) {
            i += 2;
            continue;
        }

        // Skip code blocks
        if bytes[i] == b'`' && i + 2 < len && bytes[i + 1] == b'`' && bytes[i + 2] == b'`' {
            // Find closing ```
            if let Some(close) = find_code_block_end(bytes, i + 3) {
                i = close;
                continue;
            }
        }

        // Skip inline code
        if bytes[i] == b'`' {
            if let Some(close) = find_inline_code_end(bytes, i + 1) {
                i = close + 1; // past the closing `
                continue;
            }
        }

        // Check for closing delimiter
        if i + delim.len() <= len && &bytes[i..i + delim.len()] == delim {
            return Some(i);
        }

        i += 1;
    }

    None
}

/// Find the end of a code block starting after the opening ` ``` `.
/// Returns the byte position right after the closing ` ``` `.
fn find_code_block_end(bytes: &[u8], start: usize) -> Option<usize> {
    let len = bytes.len();
    let mut i = start;

    // Skip optional language tag (rest of the opening line)
    while i < len && bytes[i] != b'\n' {
        i += 1;
    }

    // Now scan for closing ``` at the start of a line
    while i < len {
        if bytes[i] == b'\n' && i + 3 < len {
            if bytes[i + 1] == b'`' && bytes[i + 2] == b'`' && bytes[i + 3] == b'`' {
                // Check that the rest of the line is empty (or end of string)
                let end = i + 4;
                if end >= len || bytes[end] == b'\n' {
                    return Some(end);
                }
            }
        }
        i += 1;
    }

    None
}

/// Find the closing `` ` `` for inline code starting at `start`.
/// Returns the byte position of the closing backtick.
fn find_inline_code_end(bytes: &[u8], start: usize) -> Option<usize> {
    let len = bytes.len();
    let mut i = start;

    while i < len {
        if bytes[i] == b'`' {
            return Some(i);
        }
        // No escape handling inside inline code — backtick is the only terminator
        i += 1;
    }

    None
}

/// Escapes given text, abiding Telegram flavoured Markdown
/// [rules](https://core.telegram.org/bots/api#formatting-options).
///
/// Preserves Telegram MarkdownV2 formatting constructs:
/// - `*bold*`, `_italic_`, `__underline__`, `~strikethrough~`, `||spoiler||`
/// - `` `inline code` ``, ` ```code block``` `
/// - `[link text](url)`
/// - Already-escaped characters like `\*` are passed through
///
/// All other special characters in regular text are escaped with `\`.
/// Inside code spans/blocks, only `` ` `` and `\` are escaped.
pub fn tg_escape(text: &str) -> String {
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut out = String::with_capacity(len);
    let mut i = 0;

    while i < len {
        let b = bytes[i];

        // 1. Already-escaped characters: \X → pass through verbatim
        if b == b'\\' && i + 1 < len && is_tg_special(bytes[i + 1] as char) {
            out.push('\\');
            out.push(bytes[i + 1] as char);
            i += 2;
            continue;
        }

        // 2. Code block: ```
        if b == b'`' && i + 2 < len && bytes[i + 1] == b'`' && bytes[i + 2] == b'`' {
            if let Some(end) = find_code_block_end(bytes, i + 3) {
                // Emit opening ```
                out.push_str("```");
                // Emit content with code escaping
                // Content runs from after opening ``` to just before closing ```.
                let content_end = end - 3;
                let content_bytes = &bytes[i + 3..content_end];
                for &cb in content_bytes {
                    push_code_escaped(&mut out, cb as char);
                }
                out.push_str("```");
                i = end;
                continue;
            }
        }

        // 3. Inline code: `
        if b == b'`' {
            if let Some(close) = find_inline_code_end(bytes, i + 1) {
                out.push('`');
                for &cb in &bytes[i + 1..close] {
                    push_code_escaped(&mut out, cb as char);
                }
                out.push('`');
                i = close + 1;
                continue;
            }
        }

        // 4. Link: [text](url)
        if b == b'[' {
            if let Some(bracket_close) = find_closing(bytes, i + 1, b"]") {
                if bracket_close + 1 < len && bytes[bracket_close + 1] == b'(' {
                    if let Some(paren_close) = find_raw_closing_paren(bytes, bracket_close + 2) {
                        out.push('[');
                        // Escape the link text
                        let saved_i = i;
                        let link_text = &text[i + 1..bracket_close];
                        out.push_str(&tg_escape(link_text));
                        out.push_str("](");
                        // URL is emitted verbatim (no escaping)
                        let url = &text[bracket_close + 2..paren_close];
                        out.push_str(url);
                        out.push(')');
                        i = paren_close + 1;
                        let _ = saved_i;
                        continue;
                    }
                }
            }
        }

        // 5. Spoiler: ||
        if b == b'|' && i + 1 < len && bytes[i + 1] == b'|' {
            if let Some(close) = find_closing(bytes, i + 2, b"||") {
                out.push_str("||");
                let inner = &text[i + 2..close];
                out.push_str(&tg_escape(inner));
                out.push_str("||");
                i = close + 2;
                continue;
            }
        }

        // 6. Underline: __ (must check before single _)
        if b == b'_' && i + 1 < len && bytes[i + 1] == b'_' {
            // Make sure this isn't the start of a triple or more
            if !(i + 2 < len && bytes[i + 2] == b'_') {
                if let Some(close) = find_closing(bytes, i + 2, b"__") {
                    out.push_str("__");
                    let inner = &text[i + 2..close];
                    out.push_str(&tg_escape(inner));
                    out.push_str("__");
                    i = close + 2;
                    continue;
                }
            }
        }

        // 7. Bold: *
        if b == b'*' {
            if let Some(close) = find_closing(bytes, i + 1, b"*") {
                out.push('*');
                let inner = &text[i + 1..close];
                out.push_str(&tg_escape(inner));
                out.push('*');
                i = close + 1;
                continue;
            }
        }

        // 8. Italic: _ (single, not part of __)
        if b == b'_' {
            if let Some(close) = find_closing(bytes, i + 1, b"_") {
                // Make sure the closing _ is not part of __
                let close_is_double =
                    (close + 1 < len && bytes[close + 1] == b'_')
                    || (close > 0 && bytes[close - 1] == b'_' && close - 1 > i);
                if !close_is_double {
                    out.push('_');
                    let inner = &text[i + 1..close];
                    out.push_str(&tg_escape(inner));
                    out.push('_');
                    i = close + 1;
                    continue;
                }
            }
        }

        // 9. Strikethrough: ~
        if b == b'~' {
            if let Some(close) = find_closing(bytes, i + 1, b"~") {
                out.push('~');
                let inner = &text[i + 1..close];
                out.push_str(&tg_escape(inner));
                out.push('~');
                i = close + 1;
                continue;
            }
        }

        // 10. Plain text: escape if special
        push_escaped(&mut out, b as char);
        i += 1;
    }

    out
}

/// Find closing `)` for a link URL. Does not skip over formatting —
/// URLs are opaque, just find the first unescaped `)`.
fn find_raw_closing_paren(bytes: &[u8], start: usize) -> Option<usize> {
    let len = bytes.len();
    let mut i = start;

    while i < len {
        if bytes[i] == b')' {
            return Some(i);
        }
        i += 1;
    }

    None
}

#[cfg(feature = "python")]
mod python {
    use pyo3::prelude::*;

    /// Escape text for Telegram's MarkdownV2 formatting.
    ///
    /// Applies context-aware escaping:
    /// - In regular text: escapes ``_*[]()~`>#+-=|{}.!\\`` characters
    /// - In code blocks and inline code: only escapes `` ` `` and ``\\`` characters
    /// - Preserves formatting: ``*bold*``, ``_italic_``, ``~strike~``, etc.
    #[pyfunction]
    fn tg_escape(text: &str) -> String {
        super::tg_escape(text)
    }

    #[pymodule]
    #[pyo3(name = "_core")]
    fn telegram_escape_core(m: &Bound<'_, PyModule>) -> PyResult<()> {
        m.add_function(wrap_pyfunction!(tg_escape, m)?)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_md_escape() {
        assert_eq!(
            tg_escape("Soon you'll get a stats for today, and the overall status can be viewed by the /get_stat command :)"),
            r#"Soon you'll get a stats for today, and the overall status can be viewed by the /get\_stat command :\)"#
        )
    }

    #[test]
    fn test_escape_outside_code_all_specials() {
        // All special chars are unmatched formatting, so all get escaped
        let input = r#"a_*~`>#+-=|{}.!\x"#;
        let expected = r"a\_\*\~\`\>\#\+\-\=\|\{\}\.\!\\x";

        assert_eq!(tg_escape(input), expected);
    }

    #[test]
    fn test_inline_code_escapes_only_backtick_and_backslash() {
        // Inside inline code, only ` and \\ are escaped
        let input = r#"Before `a_*~>#+-=|{}.!\` after"#;
        let expected = r#"Before `a_*~>#+-=|{}.!\\` after"#;

        assert_eq!(tg_escape(input), expected);
    }

    #[test]
    fn test_code_block_escapes_only_backtick_and_backslash() {
        // Inside code blocks, only ` and \\ are escaped
        let input = "```\na_*[]()~`>#+-=|{}.!\\\n```";
        let expected = "```\na_*[]()~\\`>#+-=|{}.!\\\\\n```";

        assert_eq!(tg_escape(input), expected);
    }

    #[test]
    fn test_mixed_multiple_inline_code_segments() {
        // The new parser finds matched _..._ (italic) and *...* (bold) pairs
        // spanning across code spans — this is valid Telegram MarkdownV2.
        let input = r#"pre_* `codeA_*` mid_* `codeB_\` post_*"#;
        let expected = r#"pre_\* `codeA_*` mid_* `codeB_\\` post\_*"#;

        assert_eq!(tg_escape(input), expected);
    }

    #[test]
    fn test_emphasis_around_text_with_inline_code() {
        let input = r#"*start* `inside_*` end_*"#;
        let expected = r#"*start* `inside_*` end\_\*"#;

        assert_eq!(tg_escape(input), expected);
    }

    #[test]
    fn test_escaped_characters() {
        // Already-escaped characters are preserved verbatim
        let input = r"Escaped characters: \\ \* \_ \[ \] \( \) \~";
        let expected = r"Escaped characters: \\ \* \_ \[ \] \( \) \~";

        assert_eq!(tg_escape(input), expected);
    }

    #[test]
    fn test_math_expressions() {
        // '<' is not a Telegram MarkdownV2 reserved character, so it is not escaped.
        let input = r"Mathematical expressions: 2 + 2 = 4, x > y, a <= b";
        let expected = r"Mathematical expressions: 2 \+ 2 \= 4, x \> y, a <\= b";

        assert_eq!(tg_escape(input), expected);
    }

    // --- New tests for Telegram-native formatting preservation ---

    #[test]
    fn test_bold_preserved() {
        assert_eq!(tg_escape("*bold*"), "*bold*");
    }

    #[test]
    fn test_italic_preserved() {
        assert_eq!(tg_escape("_italic_"), "_italic_");
    }

    #[test]
    fn test_underline_preserved() {
        assert_eq!(tg_escape("__underline__"), "__underline__");
    }

    #[test]
    fn test_strikethrough_preserved() {
        assert_eq!(tg_escape("~strikethrough~"), "~strikethrough~");
    }

    #[test]
    fn test_spoiler_preserved() {
        assert_eq!(tg_escape("||spoiler||"), "||spoiler||");
    }

    #[test]
    fn test_link_preserved() {
        assert_eq!(
            tg_escape("[Click here](https://example.com)"),
            "[Click here](https://example.com)"
        );
    }

    #[test]
    fn test_link_text_escaped() {
        assert_eq!(
            tg_escape("[click + go](https://example.com)"),
            r"[click \+ go](https://example.com)"
        );
    }

    #[test]
    fn test_nested_formatting() {
        assert_eq!(
            tg_escape("*bold _italic_ bold*"),
            "*bold _italic_ bold*"
        );
    }

    #[test]
    fn test_bold_with_special_chars() {
        assert_eq!(tg_escape("hello *world*!"), r"hello *world*\!");
    }

    #[test]
    fn test_mixed_formatting_and_plain() {
        assert_eq!(
            tg_escape("hello *world* and _stuff_!"),
            r"hello *world* and _stuff_\!"
        );
    }

    #[test]
    fn test_code_block_with_language() {
        let input = "```rust\nfn main() {}\n```";
        let expected = "```rust\nfn main() {}\n```";
        assert_eq!(tg_escape(input), expected);
    }
}
