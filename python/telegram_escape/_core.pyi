def tg_escape(text: str) -> str:
    """Escape text for Telegram's MarkdownV2 formatting.

    Applies context-aware escaping:
    - In regular text: escapes ``_*[]()~`>#+-=|{}.!\\`` characters
    - In code blocks and inline code: only escapes `` ` `` and ``\\`` characters
    """
    ...
