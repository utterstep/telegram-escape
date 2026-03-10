use std::{borrow::Cow, sync::LazyLock};

use pulldown_cmark::{Event, Options as DeOptions, Parser, Tag, TagEnd};
use pulldown_cmark_to_cmark::Options as SerOptions;
use regex::Regex;

macro_rules! regex {
    ($re:literal $(,)?) => {
        LazyLock::new(|| regex::Regex::new($re).unwrap())
    };
}

static TG_MD_ESCAPE_REGEX: LazyLock<Regex> = regex!(r"[_*\[\]()~`>#+\-=|{}\.!\\]");
static TG_MD_CODE_ESCAPE_REGEX: LazyLock<Regex> = regex!(r"[`\\]");
static TG_MD_SERIALIZE_OPTIONS: LazyLock<SerOptions> = LazyLock::new(|| SerOptions {
    code_block_token_count: 3,
    ..Default::default()
});
// _*[]()~`>#+-=|{}.!\

/// Escapes given text, abiding Telegram flavoured Markdown
/// [rules](https://core.telegram.org/bots/api#formatting-options).
pub fn tg_escape(text: &str) -> String {
    let mut options = DeOptions::empty();
    options.insert(DeOptions::ENABLE_STRIKETHROUGH);

    let mut inside_code = false;

    let parser = Parser::new_ext(text, options).map(|event| {
        match &event {
            Event::Start(Tag::CodeBlock(_)) => {
                inside_code = true;

                event
            }
            Event::End(TagEnd::CodeBlock) => {
                inside_code = false;

                event
            }
            Event::Text(text) | Event::Code(text) => {
                let in_code = inside_code || matches!(&event, Event::Code(_));
                let re = if in_code {
                    &TG_MD_CODE_ESCAPE_REGEX
                } else {
                    &TG_MD_ESCAPE_REGEX
                };

                if in_code {
                    // Inside code blocks/spans, pulldown-cmark-to-cmark does not
                    // re-escape anything, so we escape all characters ourselves.
                    let replaced = re.replace_all(text, r"\$0");
                    return match replaced {
                        Cow::Borrowed(_) => event,
                        Cow::Owned(new_text) => match event {
                            Event::Text(_) => Event::Text(new_text.into()),
                            Event::Code(_) => Event::Code(new_text.into()),
                            _ => unreachable!(),
                        },
                    };
                }

                if text.len() <= 1 {
                    // pulldown-cmark-to-cmark escapes single characters on its own
                    return event;
                }

                // pulldown-cmark-to-cmark escapes the first character of each text event
                // if it's a Telegram special character. To avoid double-escaping, we skip
                // the first character here and only apply our regex to the remainder.
                let first_char_len = text.chars().next().unwrap().len_utf8();
                let rest = &text[first_char_len..];

                let replaced = re.replace_all(rest, r"\$0");
                match replaced {
                    Cow::Borrowed(_) => event,
                    Cow::Owned(escaped_rest) => {
                        let new_text =
                            format!("{}{}", &text[..first_char_len], escaped_rest);
                        match event {
                            Event::Text(_) => Event::Text(new_text.into()),
                            Event::Code(_) => Event::Code(new_text.into()),
                            _ => unreachable!(),
                        }
                    }
                }
            }
            _ => event,
        }
    });

    let mut res = String::with_capacity(text.len());

    pulldown_cmark_to_cmark::cmark_with_options(parser, &mut res, TG_MD_SERIALIZE_OPTIONS.clone())
        .expect("writing to string failed!");

    res
}

#[cfg(feature = "python")]
mod python {
    use pyo3::prelude::*;

    /// Escape text for Telegram's MarkdownV2 formatting.
    ///
    /// Applies context-aware escaping:
    /// - In regular text: escapes ``_*[]()~`>#+-=|{}.!\\`` characters
    /// - In code blocks and inline code: only escapes `` ` `` and ``\\`` characters
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
        // All MarkdownV2 special characters should be escaped outside code (avoid link syntax)
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
        let input = r#"```
a_*[]()~`>#+-=|{}.!\
```"#;
        let expected = r#"
```
a_*[]()~\`>#+-=|{}.!\\
```"#;

        assert_eq!(tg_escape(input), expected);
    }

    #[test]
    fn test_mixed_multiple_inline_code_segments() {
        let input = r#"pre_* `codeA_*` mid_* `codeB_\` post_*"#;
        let expected = r#"pre\_\* `codeA_*` mid\_\* `codeB_\\` post\_\*"#;

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
        // pulldown-cmark interprets CommonMark backslash escapes before we see them:
        // \\ → \, \* → *, \_ → _, etc. Each resulting char is then Telegram-escaped.
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
}
