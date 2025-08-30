use serde::Deserialize;
use telegram_escape::tg_escape;
use teloxide::prelude::*;

#[derive(Debug, Deserialize)]
struct Config {
    telegram_bot_token: String,
    telegram_chat_id: String,
}

#[tokio::test]
#[ignore]
async fn test_send_escaped_messages() {
    dotenvy::dotenv().ok();

    let config: Config =
        envy::from_env().expect("Missing TELEGRAM_BOT_TOKEN or TELEGRAM_CHAT_ID in .env");

    let bot = Bot::new(&config.telegram_bot_token);
    let chat_id = ChatId(
        config
            .telegram_chat_id
            .parse::<i64>()
            .expect("TELEGRAM_CHAT_ID must be a valid i64"),
    );

    let test_messages = vec![
        // 1
        "Simple text without special characters",
        // 2
        "Text with _underscores_ and *asterisks*",
        // 3
        "Text with [brackets] and (parentheses)",
        // 4
        "Text with ~tilde~ and `backticks`",
        // 5
        "Text with special chars: > # + - = | { } . ! \\",
        // 6
        "Mixed formatting: *bold* _italic_ ~strikethrough~ `code`",
        // 7
        "Code block test:\n```rust\nfn main() {\n    println!(\"Hello, world!\");\n}\n```",
        // 8
        "Inline code: `let x = 5;` and more text",
        // 9
        "Escaped characters: \\* \\_ \\[ \\] \\( \\) \\~",
        // 10
        "URL-like text: https://example.com/path?query=value&param=123",
        // 11
        "Mathematical expressions: 2 + 2 = 4, x > y, a <= b",
        // 12
        "File paths: /usr/local/bin/cargo or C:\\Windows\\System32",
        // 13
        "JSON-like structure: {\"key\": \"value\", \"number\": 42}",
        // 14
        "Markdown links: [Click here](https://example.com)",
        // 15
        "Complex nested: *bold with `inline code` inside* and _italic_",
        // 16
        "Special telegram commands: /start /help /get_stats",
        // 17
        "Emojis and unicode: 🚀 Hello 世界 مرحبا мир",
        // 18
        "Edge cases: ``` triple backticks ``` in text",
        // 19
        "Multiple lines with\n*formatting*\nacross\n_different_\nlines",
        // 20
        "All special chars together: _*[]()~`>#+\\-=|{}.!\\\\",
    ];

    println!("Starting Telegram integration test...");
    println!(
        "Sending {} test messages to chat ID: {}",
        test_messages.len(),
        config.telegram_chat_id
    );

    for (i, original_msg) in test_messages.iter().enumerate() {
        let escaped_msg = tg_escape(original_msg);

        println!("\n[Message {}]", i + 1);
        println!("Original: {}", original_msg);
        println!("Escaped:  {}", escaped_msg);

        let result = bot
            .send_message(chat_id, &escaped_msg)
            .parse_mode(teloxide::types::ParseMode::MarkdownV2)
            .await;

        match result {
            Ok(msg) => {
                println!("✓ Successfully sent message {}", i + 1);
                assert!(msg.id.0 > 0, "Message ID should be positive");
            }
            Err(e) => {
                eprintln!("✗ Failed to send message {}: {:?}", i + 1, e);
                panic!("Failed to send message: {:?}", e);
            }
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    }

    println!(
        "\n✅ All {} messages sent successfully!",
        test_messages.len()
    );
}

#[tokio::test]
#[ignore]
async fn test_edge_case_escaping() {
    dotenvy::dotenv().ok();

    let config: Config =
        envy::from_env().expect("Missing TELEGRAM_BOT_TOKEN or TELEGRAM_CHAT_ID in .env");

    let bot = Bot::new(&config.telegram_bot_token);
    let chat_id = ChatId(
        config
            .telegram_chat_id
            .parse::<i64>()
            .expect("TELEGRAM_CHAT_ID must be a valid i64"),
    );

    let long_line = "x".repeat(4096);
    let edge_cases = vec![
        ("Empty string", ""),
        ("Only spaces", "   "),
        ("Only newlines", "\n\n\n"),
        ("Only special chars", "_*[]()~`>#+\\-=|{}.!\\\\"),
        ("Repeated escapes", "\\\\\\\\\\\\"),
        ("Unicode escapes", "\\u{1F600} \\u{2764}"),
        ("Zero-width chars", "Hello\u{200B}World"),
        ("Very long line", long_line.as_str()),
        ("Nested code blocks", "```\n```nested```\n```"),
        ("Unmatched brackets", "[[[[ )))) {{{{"),
    ];

    println!("\n=== Testing edge cases ===");

    for (description, test_case) in edge_cases.iter() {
        let escaped = tg_escape(test_case);

        println!("\nEdge case: {}", description);
        println!("Original: {:?}", test_case);
        println!("Escaped:  {:?}", escaped);

        let result = bot
            .send_message(
                chat_id,
                if escaped.is_empty() {
                    "〈empty〉"
                } else {
                    &escaped
                },
            )
            .parse_mode(teloxide::types::ParseMode::MarkdownV2)
            .await;

        match result {
            Ok(_) => println!("✓ Edge case handled successfully"),
            Err(e) => {
                eprintln!("✗ Edge case failed: {:?}", e);
                panic!("Edge case '{}' failed: {:?}", description, e);
            }
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;
    }

    println!("\n✅ All edge cases handled successfully!");
}
