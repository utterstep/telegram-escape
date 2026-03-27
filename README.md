# telegram-escape

A Rust library for escaping text according to Telegram's MarkdownV2 formatting rules, with Python bindings.

## Overview

This library provides the `tg_escape` function that properly escapes special characters in text for use with Telegram Bot API's MarkdownV2 parse mode. It intelligently handles different escaping rules for regular text versus code blocks/inline code.

Available as both a Rust crate and a Python package (via [PyO3](https://pyo3.rs/)).

## Features

- Escapes special characters according to [Telegram MarkdownV2 rules](https://core.telegram.org/bots/api#formatting-options)
- Smart context-aware escaping:
  - In regular text: escapes `_*[]()~`>#+-=|{}.!\` characters
  - In code blocks and inline code: only escapes `` ` `` and `\` characters
- Preserves markdown structure while ensuring proper escaping
- Hand-crafted parser tailored to Telegram's Markdown flavor (no third-party parsing dependencies)

## Python Usage

Requires Python 3.13+.

### Installation

```bash
# With uv
uv add telegram-escape

# With pip
pip install telegram-escape
```

To install from source (requires Rust toolchain):

```bash
# With uv
uv pip install .

# With pip
pip install .
```

### Example

```python
from telegram_escape import tg_escape

# Basic escaping
escaped = tg_escape("Check /get_stat command :)")
# Result: "Check /get\\_stat command :\\)"

# Code blocks have different escaping rules
escaped = tg_escape("Before `a_*~>#+-=|{}.!\\` after")
# Inside backticks, only ` and \ are escaped
```

### Type Checking

The package ships with PEP 561 type stubs, so `tg_escape` is fully typed out of the box.

## Rust Usage

```rust
use telegram_escape::tg_escape;

let text = "Soon you'll get a stats for today, and the overall status can be viewed by the /get_stat command :)";
let escaped = tg_escape(text);
```

### Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
telegram-escape = "0.4.0"
```

## Testing

```bash
# Rust tests
cargo test

# Python (after installing in a venv)
python -c "from telegram_escape import tg_escape; print(tg_escape('hello_world'))"
```

## Dependencies

### Rust
- No runtime dependencies (only optional `pyo3` for Python bindings)

### Python build
- `maturin` - Build backend (PEP 517 compliant, works with pip/uv/any standard tool)
- `pyo3` - Rust ↔ Python bindings

## License

MIT — see [LICENSE](LICENSE) for details.

## Author

Vlad Stepanov <utterstep@hey.com>