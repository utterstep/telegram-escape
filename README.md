# telegram-escape

A Rust library for escaping text according to Telegram's MarkdownV2 formatting rules.

## Overview

This library provides the `tg_escape` function that properly escapes special characters in text for use with Telegram Bot API's MarkdownV2 parse mode. It intelligently handles different escaping rules for regular text versus code blocks/inline code.

## Features

- Escapes special characters according to [Telegram MarkdownV2 rules](https://core.telegram.org/bots/api#formatting-options)
- Smart context-aware escaping:
  - In regular text: escapes `_*[]()~`>#+-=|{}.!\` characters
  - In code blocks and inline code: only escapes `` ` `` and `\` characters
- Preserves markdown structure while ensuring proper escaping
- Uses `pulldown-cmark` for robust markdown parsing

## Usage

```rust
use telegram_escape::tg_escape;

// Basic escaping
let text = "Soon you'll get a stats for today, and the overall status can be viewed by the /get_stat command :)";
let escaped = tg_escape(text);
// Result: "Soon you'll get a stats for today, and the overall status can be viewed by the /get\_stat command :\)"

// Code blocks have different escaping rules
let text_with_code = "Before `a_*~>#+-=|{}.!\` after";
let escaped = tg_escape(text_with_code);
// Inside backticks, only ` and \ are escaped
```

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
telegram-escape = "0.1.0"
```

## Testing

Run tests with:
```bash
cargo test
```

**Note:** Some tests are currently marked as `#[ignore]` due to failing edge cases:
- `test_escaped_characters` - handling of already-escaped characters
- `test_math_expressions` - mathematical operators escaping

## Dependencies

- `pulldown-cmark` - Markdown parsing
- `pulldown-cmark-to-cmark` - Markdown serialization (using custom fork)
- `regex` - Pattern matching for escape characters

## License

See LICENSE file for details.

## Author

Vlad Stepanov <utterstep@hey.com>