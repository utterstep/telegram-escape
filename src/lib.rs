/// Telegram MarkdownV2 special characters that must be escaped in regular text.
///
/// Source of truth: <https://core.telegram.org/bots/api#markdownv2-style>
const TG_SPECIAL_CHARS: &[char] = &[
    '_', '*', '[', ']', '(', ')', '~', '`', '>', '#', '+', '-', '=', '|', '{', '}', '.', '!', '\\',
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

// ---------------------------------------------------------------------------
// Finding helpers (work with slices, return relative offsets)
// ---------------------------------------------------------------------------

/// Find the end of a code block. `after_opening` starts right after the opening `` ``` ``.
/// Returns the byte length consumed (including the closing `` ``` ``), or `None`.
fn find_code_block_end(after_opening: &str) -> Option<usize> {
    let newline_pos = after_opening.find('\n')?;
    let mut search_from = newline_pos;
    while search_from < after_opening.len() {
        let pos = after_opening[search_from..].find("\n```")?;
        let end = search_from + pos + 4; // \n + ```
        if end >= after_opening.len() || after_opening[end..].starts_with('\n') {
            return Some(end);
        }
        search_from += pos + 1;
    }
    None
}

/// Find the position of a closing delimiter in `content`.
/// Returns the byte offset relative to `content`, or `None`.
///
/// Skips over:
/// - already-escaped characters (`\X`)
/// - inline code spans (`` `...` ``)
/// - code blocks (`` ```...``` ``)
fn find_closing(content: &str, delim: &str) -> Option<usize> {
    let mut i = 0;

    while i < content.len() {
        let ch = content[i..].chars().next().unwrap();

        // Skip already-escaped characters
        if ch == '\\'
            && let Some(next_ch) = content.get(i + 1..).and_then(|s| s.chars().next())
            && is_tg_special(next_ch)
        {
            i += 1 + next_ch.len_utf8();
            continue;
        }

        // Skip code blocks
        if content[i..].starts_with("```")
            && let Some(end) = find_code_block_end(&content[i + 3..])
        {
            i += 3 + end;
            continue;
        }

        // Skip inline code
        if ch == '`'
            && let Some(pos) = content[i + 1..].find('`')
        {
            i += pos + 2; // past both backticks
            continue;
        }

        // Check for closing delimiter
        if content[i..].starts_with(delim) {
            return Some(i);
        }

        i += ch.len_utf8();
    }

    None
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

impl InlineDelimiter {
    /// Returns `true` if the opening context rejects this match.
    fn open_rejected(&self, after_open: &str) -> bool {
        match self.guard {
            DelimiterGuard::RejectTripled => after_open.starts_with(&self.delim[..1]),
            _ => false,
        }
    }

    /// Returns `true` if the closing position should be rejected.
    fn close_rejected(&self, after_open: &str, close_pos: usize) -> bool {
        match self.guard {
            DelimiterGuard::RejectDoubledClose => {
                let dc = self.delim.as_bytes()[0];
                let len = self.delim.len();
                after_open.as_bytes().get(close_pos + len) == Some(&dc)
                    || (close_pos > 0 && after_open.as_bytes().get(close_pos - 1) == Some(&dc))
            }
            _ => false,
        }
    }
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

// ---------------------------------------------------------------------------
// Fragment: parsed piece of the input
// ---------------------------------------------------------------------------

/// A parsed fragment of the input text.
enum Fragment<'a> {
    /// Already-escaped character (e.g., `\*`), pass through verbatim.
    Escaped(char),
    /// Code block content (between `` ``` `` markers).
    CodeBlock(&'a str),
    /// Inline code content (between `` ` `` markers).
    InlineCode(&'a str),
    /// Link with text and URL.
    Link { text: &'a str, url: &'a str },
    /// Formatted text with delimiter (e.g., `*bold*`).
    Formatted {
        delim: &'static str,
        content: &'a str,
    },
    /// Plain character (escape if special).
    Plain(char),
}

impl Fragment<'_> {
    fn render(&self, out: &mut String) {
        match self {
            Self::Escaped(c) => {
                out.push('\\');
                out.push(*c);
            }
            Self::CodeBlock(content) => {
                out.push_str("```");
                for c in content.chars() {
                    push_code_escaped(out, c);
                }
                out.push_str("```");
            }
            Self::InlineCode(content) => {
                out.push('`');
                for c in content.chars() {
                    push_code_escaped(out, c);
                }
                out.push('`');
            }
            Self::Link { text, url } => {
                out.push('[');
                out.push_str(&tg_escape(text));
                out.push_str("](");
                out.push_str(url);
                out.push(')');
            }
            Self::Formatted { delim, content } => {
                out.push_str(delim);
                out.push_str(&tg_escape(content));
                out.push_str(delim);
            }
            Self::Plain(c) => {
                if is_tg_special(*c) {
                    out.push('\\');
                }
                out.push(*c);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Fragment parsers — each returns `Some` and advances `input` on success,
// or returns `None` leaving `input` unchanged.
// ---------------------------------------------------------------------------

fn try_escaped_char<'a>(input: &mut &'a str) -> Option<Fragment<'a>> {
    let rest = *input;
    let mut chars = rest.chars();
    if chars.next()? != '\\' {
        return None;
    }
    let next = chars.next().filter(|c| is_tg_special(*c))?;
    *input = &rest[1 + next.len_utf8()..];
    Some(Fragment::Escaped(next))
}

fn try_code_block<'a>(input: &mut &'a str) -> Option<Fragment<'a>> {
    let rest = *input;
    let after_opening = rest.strip_prefix("```")?;
    let end = find_code_block_end(after_opening)?;
    let content = &after_opening[..end - 3]; // everything before closing ```
    *input = &after_opening[end..];
    Some(Fragment::CodeBlock(content))
}

fn try_inline_code<'a>(input: &mut &'a str) -> Option<Fragment<'a>> {
    let rest = *input;
    let after_backtick = rest.strip_prefix('`')?;
    let close = after_backtick.find('`')?;
    let content = &after_backtick[..close];
    *input = &after_backtick[close + 1..];
    Some(Fragment::InlineCode(content))
}

fn try_link<'a>(input: &mut &'a str) -> Option<Fragment<'a>> {
    let rest = *input;
    let after_bracket = rest.strip_prefix('[')?;

    let bracket_close = find_closing(after_bracket, "]")?;
    let after_text = after_bracket[bracket_close + 1..].strip_prefix('(')?;
    let paren_close = after_text.find(')')?;

    let text = &after_bracket[..bracket_close];
    let url = &after_text[..paren_close];
    *input = &after_text[paren_close + 1..];
    Some(Fragment::Link { text, url })
}

fn try_formatting<'a>(input: &mut &'a str) -> Option<Fragment<'a>> {
    let rest = *input;

    for d in INLINE_DELIMITERS {
        if !rest.starts_with(d.delim) {
            continue;
        }

        let len = d.delim.len();
        let after_open = &rest[len..];

        if d.open_rejected(after_open) {
            continue;
        }

        let Some(close) = find_closing(after_open, d.delim) else {
            continue;
        };

        if d.close_rejected(after_open, close) {
            continue;
        }

        let content = &after_open[..close];
        *input = &after_open[close + len..];
        return Some(Fragment::Formatted {
            delim: d.delim,
            content,
        });
    }

    None
}

/// Parse the next fragment from `input`, advancing past it.
fn next_fragment<'a>(input: &mut &'a str) -> Fragment<'a> {
    if let Some(f) = try_escaped_char(input) {
        return f;
    }
    if let Some(f) = try_code_block(input) {
        return f;
    }
    if let Some(f) = try_inline_code(input) {
        return f;
    }
    if let Some(f) = try_link(input) {
        return f;
    }
    if let Some(f) = try_formatting(input) {
        return f;
    }

    let ch = input.chars().next().unwrap();
    *input = &input[ch.len_utf8()..];
    Fragment::Plain(ch)
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
    let mut out = String::with_capacity(text.len());
    let mut input = text;

    while !input.is_empty() {
        next_fragment(&mut input).render(&mut out);
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

    #[test]
    fn test_delimiter_ordering_invariant() {
        // A shorter delimiter must not precede a longer one it is a prefix of,
        // otherwise the shorter one would greedily consume the longer one's opening.
        for (i, a) in INLINE_DELIMITERS.iter().enumerate() {
            for b in &INLINE_DELIMITERS[i + 1..] {
                assert!(
                    !b.delim.starts_with(a.delim),
                    "'{0}' is a prefix of '{1}' but comes before it — \
                     multi-char delimiters must precede their subsets",
                    a.delim,
                    b.delim,
                );
            }
        }
    }
}
