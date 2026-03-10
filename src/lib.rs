/// Telegram MarkdownV2 special characters that must be escaped in regular text.
///
/// Source of truth: <https://core.telegram.org/bots/api#markdownv2-style>
const TG_SPECIAL_CHARS: &[char] = &[
    '_', '*', '[', ']', '(', ')', '~', '`', '>', '#', '+', '-', '=', '|', '{', '}', '.', '!',
    '\\',
];

/// O(1) lookup table built at compile time from [`TG_SPECIAL_CHARS`].
const TG_SPECIAL: [bool; 128] = {
    let mut table = [false; 128];
    let mut i = 0;
    while i < TG_SPECIAL_CHARS.len() {
        table[TG_SPECIAL_CHARS[i] as usize] = true;
        i += 1;
    }
    table
};

/// Returns `true` if `c` is a Telegram MarkdownV2 special character.
fn is_tg_special(c: char) -> bool {
    let code = c as u32;
    code < 128 && TG_SPECIAL[code as usize]
}

/// Push a character to `out`, escaping it for code context (only `` ` `` and `\`).
fn push_code_escaped(out: &mut String, c: char) {
    if c == '`' || c == '\\' {
        out.push('\\');
    }
    out.push(c);
}

/// Find the position of a closing delimiter in `text` starting from byte offset `start`.
/// Returns the byte offset of the first character of the closing delimiter, or `None`.
///
/// Skips over:
/// - already-escaped characters (`\X`)
/// - inline code spans (`` `...` ``)
/// - code blocks (` ```...``` `)
fn find_closing(text: &str, start: usize, delim: &str) -> Option<usize> {
    let mut i = start;

    while i < text.len() {
        let ch = text[i..].chars().next().unwrap();

        // Skip already-escaped characters
        if ch == '\\'
            && let Some(next_ch) = text.get(i + 1..).and_then(|s| s.chars().next())
            && is_tg_special(next_ch)
        {
            i += 1 + next_ch.len_utf8();
            continue;
        }

        // Skip code blocks
        if text[i..].starts_with("```")
            && let Some(close) = find_code_block_end(text, i + 3)
        {
            i = close;
            continue;
        }

        // Skip inline code
        if ch == '`'
            && let Some(close) = find_inline_code_end(text, i + 1)
        {
            i = close + 1; // past the closing `
            continue;
        }

        // Check for closing delimiter
        if text[i..].starts_with(delim) {
            return Some(i);
        }

        i += ch.len_utf8();
    }

    None
}

/// Find the end of a code block starting after the opening ` ``` `.
/// Returns the byte position right after the closing ` ``` `.
fn find_code_block_end(text: &str, start: usize) -> Option<usize> {
    // Skip optional language tag (rest of the opening line)
    let after_lang = match text[start..].find('\n') {
        Some(p) => start + p,
        None => return None,
    };

    // Scan for \n``` followed by \n or end of string
    let mut search_from = after_lang;
    while search_from < text.len() {
        let pos = text[search_from..].find("\n```")?;
        let end = search_from + pos + 4; // \n + ```
        if end >= text.len() || text[end..].starts_with('\n') {
            return Some(end);
        }
        search_from += pos + 1;
    }

    None
}

/// Find the closing `` ` `` for inline code starting at byte offset `start`.
/// Returns the byte position of the closing backtick.
fn find_inline_code_end(text: &str, start: usize) -> Option<usize> {
    text[start..].find('`').map(|pos| start + pos)
}

/// Find closing `)` for a link URL. Does not skip over formatting —
/// URLs are opaque, just find the first unescaped `)`.
fn find_raw_closing_paren(text: &str, start: usize) -> Option<usize> {
    text[start..].find(')').map(|pos| start + pos)
}

// ---------------------------------------------------------------------------
// Inline formatting delimiter table
// ---------------------------------------------------------------------------

/// Edge-case guard for a formatting delimiter.
#[derive(Clone, Copy, PartialEq, Eq)]
enum DelimiterGuard {
    /// No special handling.
    None,
    /// Reject if the opening is immediately followed by an extra copy of the
    /// delimiter's first character.  Prevents `__` from greedily matching `___`
    /// (underline eating into italic).
    RejectTripled,
    /// Reject if the closing delimiter is adjacent to another copy of the same
    /// character.  Prevents single `_` (italic) from matching a `_` that is
    /// part of `__` (underline).
    RejectDoubledClose,
}

struct InlineDelimiter {
    delim: &'static str,
    guard: DelimiterGuard,
}

/// Inline formatting delimiters, checked **in order**.
///
/// Multi-character delimiters must precede their single-character subsets
/// (e.g. `||` before `|`, `__` before `_`).
const INLINE_DELIMITERS: &[InlineDelimiter] = &[
    InlineDelimiter {
        delim: "||",
        guard: DelimiterGuard::None,
    }, // spoiler
    InlineDelimiter {
        delim: "__",
        guard: DelimiterGuard::RejectTripled,
    }, // underline
    InlineDelimiter {
        delim: "*",
        guard: DelimiterGuard::None,
    }, // bold
    InlineDelimiter {
        delim: "_",
        guard: DelimiterGuard::RejectDoubledClose,
    }, // italic
    InlineDelimiter {
        delim: "~",
        guard: DelimiterGuard::None,
    }, // strikethrough
];

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
    let mut out = String::with_capacity(text.len());
    let mut i = 0;

    'outer: while i < text.len() {
        let ch = text[i..].chars().next().unwrap();

        // 1. Already-escaped characters: \X → pass through verbatim
        if ch == '\\'
            && let Some(next_ch) = text.get(i + 1..).and_then(|s| s.chars().next())
            && is_tg_special(next_ch)
        {
            out.push('\\');
            out.push(next_ch);
            i += 1 + next_ch.len_utf8();
            continue;
        }

        // 2. Code block: ```
        if text[i..].starts_with("```")
            && let Some(end) = find_code_block_end(text, i + 3)
        {
            out.push_str("```");
            let content = &text[i + 3..end - 3];
            for c in content.chars() {
                push_code_escaped(&mut out, c);
            }
            out.push_str("```");
            i = end;
            continue;
        }

        // 3. Inline code: `
        if ch == '`'
            && let Some(close) = find_inline_code_end(text, i + 1)
        {
            out.push('`');
            for c in text[i + 1..close].chars() {
                push_code_escaped(&mut out, c);
            }
            out.push('`');
            i = close + 1;
            continue;
        }

        // 4. Link: [text](url)
        if ch == '['
            && let Some(bracket_close) = find_closing(text, i + 1, "]")
            && text.get(bracket_close + 1..bracket_close + 2) == Some("(")
            && let Some(paren_close) = find_raw_closing_paren(text, bracket_close + 2)
        {
            out.push('[');
            out.push_str(&tg_escape(&text[i + 1..bracket_close]));
            out.push_str("](");
            out.push_str(&text[bracket_close + 2..paren_close]);
            out.push(')');
            i = paren_close + 1;
            continue;
        }

        // 5–9. Inline formatting delimiters (table-driven)
        for d in INLINE_DELIMITERS {
            let rest = &text[i..];
            if !rest.starts_with(d.delim) {
                continue;
            }

            let len = d.delim.len();

            // Open guard: e.g. reject "___" when matching "__"
            if d.guard == DelimiterGuard::RejectTripled
                && rest
                    .get(len..)
                    .is_some_and(|s| s.starts_with(&d.delim[..1]))
            {
                continue;
            }

            let Some(close) = find_closing(text, i + len, d.delim) else {
                continue;
            };

            // Close guard: e.g. reject closing "_" that is part of "__"
            if d.guard == DelimiterGuard::RejectDoubledClose {
                let dc = d.delim.as_bytes()[0];
                let close_is_double = text.as_bytes().get(close + len) == Some(&dc)
                    || (close > i + len && text.as_bytes().get(close - 1) == Some(&dc));
                if close_is_double {
                    continue;
                }
            }

            out.push_str(d.delim);
            out.push_str(&tg_escape(&text[i + len..close]));
            out.push_str(d.delim);
            i = close + len;
            continue 'outer;
        }

        // 10. Plain text: escape special chars, pass through everything else
        if is_tg_special(ch) {
            out.push('\\');
        }
        out.push(ch);
        i += ch.len_utf8();
    }

    out
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
            tg_escape(
                "Soon you'll get a stats for today, and the overall status can be viewed by the /get_stat command :)"
            ),
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

    // --- Formatting preservation ---

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
        assert_eq!(tg_escape("*bold _italic_ bold*"), "*bold _italic_ bold*");
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

    // --- Edge cases ---

    #[test]
    fn test_empty_string() {
        assert_eq!(tg_escape(""), "");
    }

    #[test]
    fn test_no_special_chars() {
        assert_eq!(tg_escape("hello world"), "hello world");
    }

    #[test]
    fn test_unmatched_bold() {
        assert_eq!(tg_escape("price is 5*3"), r"price is 5\*3");
    }

    #[test]
    fn test_unmatched_italic() {
        assert_eq!(tg_escape("file_name"), r"file\_name");
    }

    #[test]
    fn test_unmatched_backtick() {
        assert_eq!(tg_escape("it's a `test"), r"it's a \`test");
    }

    #[test]
    fn test_adjacent_formatting() {
        assert_eq!(tg_escape("*bold*_italic_"), "*bold*_italic_");
    }

    #[test]
    fn test_formatting_with_special_inside() {
        assert_eq!(tg_escape("*2+2=4*"), r"*2\+2\=4*");
    }

    #[test]
    fn test_multiple_newlines() {
        assert_eq!(tg_escape("a\n\nb"), "a\n\nb");
    }

    #[test]
    fn test_non_special_chars_pass_through() {
        // < @ / : ; are NOT Telegram MarkdownV2 special chars
        assert_eq!(tg_escape("a < b @ c / d : e ; f"), "a < b @ c / d : e ; f");
    }

    #[test]
    fn test_code_block_with_backticks_inside() {
        let input = "```\nsome `code` here\n```";
        let expected = "```\nsome \\`code\\` here\n```";
        assert_eq!(tg_escape(input), expected);
    }

    #[test]
    fn test_link_with_formatted_text() {
        assert_eq!(
            tg_escape("[*bold link*](https://example.com)"),
            "[*bold link*](https://example.com)"
        );
    }

    #[test]
    fn test_unmatched_bracket_not_link() {
        assert_eq!(tg_escape("[not a link"), r"\[not a link");
    }

    #[test]
    fn test_bracket_without_paren() {
        assert_eq!(tg_escape("[text] no url"), r"\[text\] no url");
    }

    #[test]
    fn test_spoiler_with_special_inside() {
        assert_eq!(tg_escape("||secret!||"), r"||secret\!||");
    }

    #[test]
    fn test_underline_vs_italic() {
        // __ is underline, not double italic
        assert_eq!(tg_escape("__underline__"), "__underline__");
        // single _ around __ content
        assert_eq!(tg_escape("_italic_"), "_italic_");
    }

    #[test]
    fn test_escaped_delimiter_not_matched() {
        // \* should not be treated as a bold delimiter
        assert_eq!(tg_escape(r"\*not bold\*"), r"\*not bold\*");
    }

    #[test]
    fn test_backslash_before_non_special() {
        // \ before a non-special char: the \ itself is special and gets escaped
        assert_eq!(tg_escape(r"\n"), r"\\n");
    }

    #[test]
    fn test_consecutive_specials() {
        assert_eq!(tg_escape("()[]{}"), r"\(\)\[\]\{\}");
    }

    #[test]
    fn test_cyrillic_text() {
        assert_eq!(tg_escape("НОВЫЙ"), "НОВЫЙ");
        assert_eq!(tg_escape("Привет мир"), "Привет мир");
        assert_eq!(tg_escape("Привет *мир*!"), r"Привет *мир*\!");
    }

    #[test]
    fn test_multibyte_in_code() {
        assert_eq!(tg_escape("`код`"), "`код`");
        assert_eq!(tg_escape("```\nкод\n```"), "```\nкод\n```");
    }
}
