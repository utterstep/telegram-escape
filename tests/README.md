# Telegram Integration Tests

## Setup

1. Create a Telegram bot via [@BotFather](https://t.me/botfather)
2. Get your bot token from BotFather
3. Find your chat ID (you can use [@userinfobot](https://t.me/userinfobot) or similar)
4. Copy `.env.example` to `.env` and fill in your credentials:
   ```bash
   cp .env.example .env
   # Edit .env with your actual values
   ```

## Running the Tests

The integration tests are marked with `#[ignore]` so they won't run by default. To run them:

```bash
# Run all integration tests
cargo nextest run --test telegram_integration --run-ignored all

# Run specific test
cargo nextest run --test telegram_integration test_send_escaped_messages --run-ignored all
cargo nextest run --test telegram_integration test_edge_case_escaping --run-ignored all
```

## What the Tests Do

### `test_send_escaped_messages`
Sends various test messages with special characters that need escaping in Telegram's MarkdownV2 format:
- Simple text
- Markdown formatting characters
- Code blocks and inline code
- Special characters and symbols
- URLs and file paths
- Emojis and Unicode
- Multi-line messages

### `test_edge_case_escaping`
Tests edge cases:
- Empty strings
- Only whitespace
- Very long lines (4096 chars)
- Nested code blocks
- Unmatched brackets
- Unicode and zero-width characters

## Notes

- Messages are sent with a 500ms delay between them to avoid rate limiting
- Edge cases use a 300ms delay
- All messages are sent using MarkdownV2 parse mode
- The tests will panic if any message fails to send
