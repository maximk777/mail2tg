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

pub fn build_card(
    from_display: &str,
    subject: &str,
    date: &str,
    body: &str,
    preview_chars: usize,
) -> String {
    let body_preview = truncate_chars(body, preview_chars);
    format!(
        "📧 <b>Новое письмо</b>\n<b>От:</b> {}\n<b>Тема:</b> {}\n<b>Дата:</b> {}\n\n{}",
        html_escape(from_display),
        html_escape(subject),
        html_escape(date),
        html_escape(&body_preview),
    )
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
}
