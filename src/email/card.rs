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

/// Truncate the assembled card to Telegram's 4096-char limit without leaving a
/// dangling partial HTML entity at the tail (e.g. `&l` or `&`). All user content
/// is HTML-escaped before assembly, so every `&` in the card starts a complete
/// entity (`&lt;`/`&gt;`/`&amp;`). If the last `&` after truncation has no `;`
/// following it, the cut landed mid-entity — drop back to just before that `&`
/// so the HTML stays valid and Telegram does not reject it with a 400 (which,
/// since the daemon now breaks the batch on delivery failure, would stall the
/// whole mailbox).
fn truncate_card(card: String) -> String {
    if card.chars().count() <= TG_MAX_CHARS {
        return card;
    }
    let mut out: String = card.chars().take(TG_MAX_CHARS).collect();
    if let Some(amp) = out.rfind('&') {
        if !out[amp..].contains(';') {
            out.truncate(amp);
        }
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
    let card = format!(
        "📧 <b>Новое письмо</b>\n<b>От:</b> {}\n<b>Тема:</b> {}\n<b>Дата:</b> {}\n\n{}",
        html_escape(from_display),
        html_escape(subject),
        html_escape(date),
        html_escape(&body_preview),
    );
    truncate_card(card)
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

    #[test]
    fn card_truncation_is_entity_safe() {
        // ~2000 '<' chars each escape to "&lt;" (4 chars), pushing the card well
        // past 4096; the cut is highly likely to land inside a "&lt;" entity.
        let body = "<".repeat(2000);
        let card = build_card("a@b.com", "subject", "2026-01-01", &body, 2000);
        assert!(card.chars().count() <= 4096, "card was {} chars", card.chars().count());
        // No dangling partial entity at the tail.
        assert!(!card.ends_with('&'));
        assert!(!card.ends_with("&l"));
        assert!(!card.ends_with("&lt"));
        // Robust check: the last '&' in the card must be followed by a ';'.
        if let Some(amp) = card.rfind('&') {
            assert!(
                card[amp..].contains(';'),
                "trailing '&' at {amp} is not a complete entity: {:?}",
                &card[amp..]
            );
        }
    }
}
