pub mod card;
pub mod recipient;
pub mod sender;

use mail_parser::{MessageParser, PartType};

pub struct ParsedEmail {
    pub from_addr: String,
    pub recipients: Vec<String>,
    pub subject: String,
    pub date_display: String,
    pub timestamp: i64,
    pub body: String,
}

/// Bare email address from a raw recipient value: handles
/// `Name <addr@host>` and bare `addr@host`.
fn bare_address(raw: &str) -> Option<String> {
    let s = raw.trim();
    if let (Some(lt), Some(gt)) = (s.rfind('<'), s.rfind('>')) {
        if lt < gt {
            let inner = s[lt + 1..gt].trim();
            if !inner.is_empty() {
                return Some(inner.to_string());
            }
        }
    }
    if s.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}

/// Index just past the closing `>` of the tag starting at `rest[0] == '<'`,
/// ignoring `>` inside quoted attribute values.
fn tag_end(rest: &str) -> Option<usize> {
    let mut quote: Option<char> = None;
    for (i, c) in rest.char_indices() {
        match (quote, c) {
            (Some(q), _) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '"') | (None, '\'') => quote = Some(c),
            (None, '>') => return Some(i + 1),
            _ => {}
        }
    }
    None
}

/// Tag name (lowercased) and whether it is a closing tag, from the tag's
/// inner content (between `<` and `>`).
fn tag_name(inner: &str) -> (String, bool) {
    let inner = inner.trim_start();
    let (inner, closing) = match inner.strip_prefix('/') {
        Some(rest) => (rest, true),
        None => (inner, false),
    };
    let name: String = inner
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect();
    (name, closing)
}

/// `href` attribute value from a tag's inner content, if any.
fn href_attr(inner: &str) -> Option<&str> {
    let lower = inner.to_ascii_lowercase();
    let mut from = 0;
    while let Some(pos) = lower[from..].find("href") {
        let at = from + pos;
        from = at + 4;
        // Must be a standalone attribute name (not e.g. `data-href`).
        if at > 0 && !lower.as_bytes()[at - 1].is_ascii_whitespace() {
            continue;
        }
        let after = inner[at + 4..].trim_start();
        let Some(after) = after.strip_prefix('=') else { continue };
        let after = after.trim_start();
        let mut chars = after.chars();
        return match chars.next() {
            Some(q @ ('"' | '\'')) => after[1..].find(q).map(|end| &after[1..1 + end]),
            Some(_) => Some(after.split_whitespace().next().unwrap_or(after)),
            None => None,
        };
    }
    None
}

/// Decode the few HTML entities that actually occur in email bodies.
fn decode_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        rest = &rest[amp..];
        let entity_end = rest[..rest.len().min(10)].find(';');
        let decoded = entity_end.and_then(|semi| {
            let (name, len) = (&rest[1..semi], semi + 1);
            let ch = match name {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                "nbsp" => Some(' '),
                _ => name.strip_prefix('#').and_then(|num| {
                    let cp = match num.strip_prefix(['x', 'X']) {
                        Some(hex) => u32::from_str_radix(hex, 16).ok(),
                        None => num.parse().ok(),
                    };
                    cp.and_then(char::from_u32)
                }),
            };
            ch.map(|c| (c, len))
        });
        match decoded {
            Some((c, len)) => {
                out.push(c);
                rest = &rest[len..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Tags whose text content is not human-readable body text.
const INVISIBLE: &[&str] = &["style", "script", "title", "head"];
/// Tags that visually separate text; emit a space so adjacent text nodes
/// from different blocks don't get glued together.
const BLOCK: &[&str] = &[
    "br", "p", "div", "td", "tr", "table", "li", "ul", "ol", "h1", "h2", "h3", "h4", "h5", "h6",
    "blockquote", "hr",
];

/// Convert an HTML email body to plain text: drop tags, comments and
/// style/script content, decode entities, and keep hyperlink targets —
/// `<a href="U">text</a>` becomes `text (U)` — since for many emails
/// (login links, confirmations) the URL is the entire point.
fn strip_html(html: &str) -> String {
    let mut out = String::new();
    let mut rest = html;
    // Set while inside <style>/<script>/etc.; text is dropped until the
    // matching closing tag.
    let mut skip_until: Option<String> = None;
    // Set while inside <a href=...>; (href, out.len() at the anchor start).
    let mut anchor: Option<(String, usize)> = None;

    while let Some(lt) = rest.find('<') {
        if skip_until.is_none() {
            out.push_str(&rest[..lt]);
        }
        rest = &rest[lt..];

        if rest.starts_with("<!--") {
            match rest.find("-->") {
                Some(end) => {
                    rest = &rest[end + 3..];
                    continue;
                }
                None => break,
            }
        }

        let Some(end) = tag_end(rest) else { break };
        let inner = &rest[1..end - 1];
        rest = &rest[end..];

        let (name, closing) = tag_name(inner);
        if let Some(skip) = &skip_until {
            if closing && name == *skip {
                skip_until = None;
            }
            continue;
        }
        if !closing && INVISIBLE.contains(&name.as_str()) {
            skip_until = Some(name);
            continue;
        }

        if name == "a" {
            if closing {
                if let Some((href, start)) = anchor.take() {
                    if !out[start..].contains(&href) {
                        out.push_str(&format!(" ({href})"));
                    }
                }
            } else if let Some(href) = href_attr(inner) {
                let href = decode_entities(href);
                let keep = !href.is_empty()
                    && !href.starts_with('#')
                    && !href.to_ascii_lowercase().starts_with("javascript:");
                anchor = keep.then_some((href, out.len()));
            }
        } else if BLOCK.contains(&name.as_str()) {
            out.push(' ');
        }
    }
    if skip_until.is_none() {
        out.push_str(rest);
    }

    let out = decode_entities(&out);
    let mut result = String::with_capacity(out.len());
    for word in out.split_whitespace() {
        if !result.is_empty() {
            result.push(' ');
        }
        result.push_str(word);
    }
    result
}

pub fn parse_email(raw: &[u8]) -> Option<ParsedEmail> {
    let msg = MessageParser::default().parse(raw)?;

    let from_addr = msg
        .from()
        .and_then(|a| a.first())
        .and_then(|a| a.address())
        .map(|s| s.to_string())
        .unwrap_or_default();

    // Collect recipients from To, Cc, Delivered-To, X-Forwarded-To.
    // Pre-size to avoid excessive reallocations; most messages have a handful.
    let mut recipients: Vec<String> = Vec::with_capacity(4);

    if let Some(to) = msg.to() {
        for a in to.iter() {
            if let Some(e) = a.address() {
                recipients.push(e.to_string());
            }
        }
    }
    if let Some(cc) = msg.cc() {
        for a in cc.iter() {
            if let Some(e) = a.address() {
                recipients.push(e.to_string());
            }
        }
    }
    // These headers carry recipients as raw text, possibly bracketed
    // (`Name <addr>`) and/or comma-separated; extract bare addresses.
    for name in ["Delivered-To", "X-Forwarded-To"] {
        if let Some(raw_val) = msg.header_raw(name) {
            for part in raw_val.split(',') {
                if let Some(addr) = bare_address(part) {
                    recipients.push(addr);
                }
            }
        }
    }

    let subject = msg.subject().unwrap_or_default().to_string();

    let (date_display, timestamp) = match msg.date() {
        Some(d) => (
            format!(
                "{:04}-{:02}-{:02} {:02}:{:02}",
                d.year, d.month, d.day, d.hour, d.minute
            ),
            d.to_timestamp(),
        ),
        None => (String::new(), 0),
    };

    // Only trust body_text() when the message has a genuine text/plain part:
    // for HTML-only emails mail_parser silently converts HTML to text and
    // drops hyperlink targets, which for login/confirmation emails are the
    // entire point. In that case strip the HTML ourselves, keeping hrefs.
    let plain = msg.text_part(0).and_then(|p| match &p.body {
        PartType::Text(t) => Some(t.to_string()),
        _ => None,
    });
    let body = plain
        .or_else(|| msg.body_html(0).map(|h| strip_html(&h)))
        .or_else(|| msg.body_text(0).map(|t| t.to_string()))
        .unwrap_or_default();

    Some(ParsedEmail {
        from_addr,
        recipients,
        subject,
        date_display,
        timestamp,
        body,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const RAW: &[u8] = b"From: \"noreply\" <noreply_at_openai.com_abc@duck.com>\r\n\
To: scpccomz@duck.com\r\n\
Subject: Hello there\r\n\
Date: Sun, 28 Jun 2026 14:30:00 +0000\r\n\
Content-Type: text/plain; charset=utf-8\r\n\
\r\n\
This is the body.\r\n";

    #[test]
    fn parses_core_fields() {
        let e = parse_email(RAW).unwrap();
        assert_eq!(e.from_addr, "noreply_at_openai.com_abc@duck.com");
        assert!(e.recipients.iter().any(|r| r == "scpccomz@duck.com"));
        assert_eq!(e.subject, "Hello there");
        assert!(e.body.contains("This is the body."));
        assert!(e.date_display.starts_with("2026-06-28"));
        assert!(e.timestamp > 0);
    }

    #[test]
    fn raw_header_recipients_are_bare() {
        let raw = b"From: a@b.com\r\n\
To: someone@duck.com\r\n\
Delivered-To: Someone <scpccomz@duck.com>\r\n\
X-Forwarded-To: forwarded@duck.com\r\n\
Subject: s\r\n\
\r\n\
body\r\n";
        let e = parse_email(raw).unwrap();
        assert!(e.recipients.iter().any(|r| r == "scpccomz@duck.com"));
        assert!(e.recipients.iter().any(|r| r == "forwarded@duck.com"));
    }

    #[test]
    fn anchor_href_is_preserved() {
        let html = r#"<p>Sign in with the secure link below</p><a href="https://claude.ai/magic-link#abc:XYZ=" style="color: white;">Sign in to Claude.ai</a>"#;
        let text = strip_html(html);
        assert!(text.contains("Sign in to Claude.ai"), "anchor text lost: {text}");
        assert!(
            text.contains("https://claude.ai/magic-link#abc:XYZ="),
            "href lost: {text}"
        );
    }

    #[test]
    fn style_script_head_and_comments_are_dropped() {
        let html = "<html><head><title>t</title><style type=\"text/css\">#outlook a { padding:0; }\nbody { margin:0; }</style></head>\
<body><!--[if mso | IE]><table role=\"presentation\"><![endif]-->Hello<script>var x = 1;</script> world</body></html>";
        let text = strip_html(html);
        assert_eq!(text, "Hello world");
    }

    #[test]
    fn block_tags_separate_words() {
        let html = "<div>Let's get you signed in</div><div>Sign in with the secure link below</div>";
        assert_eq!(
            strip_html(html),
            "Let's get you signed in Sign in with the secure link below"
        );
    }

    #[test]
    fn entities_are_decoded() {
        assert_eq!(strip_html("Tom &amp; Jerry&nbsp;&lt;3"), "Tom & Jerry <3");
    }

    #[test]
    fn anchor_matching_text_not_duplicated() {
        let html = r#"<a href="https://example.com/x">https://example.com/x</a>"#;
        assert_eq!(strip_html(html), "https://example.com/x");
    }

    #[test]
    fn real_anthropic_magic_link_email() {
        let raw = include_bytes!("testdata/anthropic_magic_link.eml");
        let e = parse_email(raw).unwrap();
        assert_eq!(e.from_addr, "no-reply-TESTTESTTESTTESTTESTTT@mail.anthropic.com");
        assert!(
            e.body.contains(
                "https://claude.ai/magic-link#00000000000000000000000000000000:dGVzdEBleGFtcGxlLmNvbQ=="
            ),
            "magic link lost: {}",
            e.body
        );
        assert!(e.body.contains("Let's get you signed in Sign in with the secure link below"));
        // Style-block CSS must not pollute the preview.
        assert!(!e.body.contains("padding:0"), "CSS leaked into body: {}", e.body);
        assert!(!e.body.contains("mj-column-per-100"), "CSS leaked into body: {}", e.body);
        // The magic link must survive into the default-size Telegram card.
        let card = crate::email::card::build_card("Anthropic", &e.subject, &e.date_display, &e.body, 1000);
        assert!(card.contains("claude.ai/magic-link"), "link missing from card: {card}");
    }

    #[test]
    fn quoted_printable_html_keeps_link() {
        // href split across QP soft line breaks, as SendGrid/Anthropic emails do.
        let raw = b"From: no-reply@mail.anthropic.com\r\n\
To: x@duck.com\r\n\
Subject: Secure link\r\n\
Content-Transfer-Encoding: quoted-printable\r\n\
Content-Type: text/html; charset=us-ascii\r\n\
\r\n\
<body><a clicktracking=3D\"off\" href=3D\"https://=\r\n\
claude.ai/magic-link#abc123:dGVzdA=3D\" style=3D\"color: white;\">Sign in t=\r\n\
o Claude.ai</a></body>\r\n";
        let e = parse_email(raw).unwrap();
        assert!(
            e.body.contains("https://claude.ai/magic-link#abc123:dGVzdA="),
            "magic link lost: {}",
            e.body
        );
        assert!(e.body.contains("Sign in to Claude.ai"), "anchor text lost: {}", e.body);
    }

    #[test]
    fn html_only_body_is_stripped() {
        let raw = b"From: a@b.com\r\nTo: x@duck.com\r\nSubject: s\r\nContent-Type: text/html\r\n\r\n<p>Hi <b>there</b></p>\r\n";
        let e = parse_email(raw).unwrap();
        assert!(e.body.contains("Hi"));
        assert!(!e.body.contains("<p>"));
    }
}
