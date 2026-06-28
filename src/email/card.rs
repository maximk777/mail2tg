pub fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
    out
}

pub fn truncate_chars(s: &str, max: usize) -> String {
    let mut out: String = s.chars().take(max).collect();
    if s.chars().count() > max {
        out.push('…');
    }
    out
}

const TG_MAX_CHARS: usize = 4096;

pub fn build_card(
    from_display: &str,
    subject: &str,
    date: &str,
    body: &str,
    preview_chars: usize,
) -> String {
    let body_preview = truncate_chars(body, preview_chars);
    let mut card = format!(
        "📧 <b>Новое письмо</b>\n<b>От:</b> {}\n<b>Тема:</b> {}\n<b>Дата:</b> {}\n\n{}",
        html_escape(from_display),
        html_escape(subject),
        html_escape(date),
        html_escape(&body_preview),
    );
    // Guarantee the card never exceeds Telegram's 4096-character limit.
    // A pathological subject/from/body can still push past the limit after
    // HTML-escaping; hard-truncate rather than letting the API return a 400.
    if card.chars().count() > TG_MAX_CHARS {
        card = card.chars().take(TG_MAX_CHARS).collect();
    }
    card
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_html() {
        assert_eq!(html_escape("a<b>&c"), "a&lt;b&gt;&amp;c");
    }

    #[test]
    fn truncates_with_ellipsis() {
        assert_eq!(truncate_chars("hello", 3), "hel…");
        assert_eq!(truncate_chars("hi", 5), "hi");
    }

    #[test]
    fn card_has_fields_and_escapes() {
        let card = build_card("noreply@openai.com", "Re: <urgent> & stuff", "2026-06-28 14:30", "body <x>", 1000);
        assert!(card.contains("noreply@openai.com"));
        assert!(card.contains("Re: &lt;urgent&gt; &amp; stuff"));
        assert!(card.contains("2026-06-28 14:30"));
        assert!(card.contains("body &lt;x&gt;"));
        assert!(card.contains("📧"));
    }

    #[test]
    fn card_truncates_body() {
        let body = "x".repeat(50);
        let card = build_card("a@b.com", "s", "d", &body, 10);
        assert!(card.contains(&format!("{}…", "x".repeat(10))));
    }

    #[test]
    fn card_never_exceeds_tg_limit() {
        let body = "y".repeat(10_000);
        let card = build_card("a@b.com", "subject", "2026-01-01", &body, 10_000);
        assert!(
            card.chars().count() <= 4096,
            "card was {} chars, expected <= 4096",
            card.chars().count()
        );
    }
}
