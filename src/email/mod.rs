pub mod card;
pub mod recipient;
pub mod sender;

use mail_parser::MessageParser;

#[allow(dead_code)]
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

fn strip_html(html: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    let mut result = String::with_capacity(out.len());
    for word in out.split_whitespace() {
        if !result.is_empty() {
            result.push(' ');
        }
        result.push_str(word);
    }
    result
}

#[allow(dead_code)]
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

    let body = match msg.body_text(0) {
        Some(t) => t.to_string(),
        None => msg
            .body_html(0)
            .map(|h| strip_html(&h))
            .unwrap_or_default(),
    };

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
    fn html_only_body_is_stripped() {
        let raw = b"From: a@b.com\r\nTo: x@duck.com\r\nSubject: s\r\nContent-Type: text/html\r\n\r\n<p>Hi <b>there</b></p>\r\n";
        let e = parse_email(raw).unwrap();
        assert!(e.body.contains("Hi"));
        assert!(!e.body.contains("<p>"));
    }
}
