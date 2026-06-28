use std::collections::HashSet;

use anyhow::{anyhow, Result};
use serde_json::Value;

pub struct Update {
    pub update_id: i64,
    pub chat_id: Option<i64>,
    pub text: Option<String>,
}

pub fn parse_updates(v: &Value) -> Vec<Update> {
    let mut out = Vec::new();
    if let Some(arr) = v.get("result").and_then(|r| r.as_array()) {
        for item in arr {
            let update_id = item.get("update_id").and_then(|x| x.as_i64()).unwrap_or(0);
            let msg = item.get("message");
            let chat_id = msg
                .and_then(|m| m.get("chat"))
                .and_then(|c| c.get("id"))
                .and_then(|x| x.as_i64());
            let text = msg
                .and_then(|m| m.get("text"))
                .and_then(|x| x.as_str())
                .map(|s| s.to_string());
            out.push(Update { update_id, chat_id, text });
        }
    }
    out
}

fn sorted_domains(domains: &HashSet<String>) -> String {
    let mut v: Vec<&str> = domains.iter().map(String::as_str).collect();
    v.sort_unstable();
    v.join(", ")
}

pub fn start_reply(in_whitelist: bool, chat_id: i64, sender_domains: &HashSet<String>) -> Option<String> {
    if in_whitelist {
        Some(format!(
            "✅ Вы зарегистрированы. Письма от {} будут приходить сюда.",
            sorted_domains(sender_domains)
        ))
    } else {
        Some(format!(
            "Вы не зарегистрированы в системе. Передайте ваш ID администратору: {chat_id}"
        ))
    }
}

/// Returns `Some(reply)` only when `text` is exactly `"/start"` (trimmed).
pub fn start_reply_for_text(
    text: &str,
    in_whitelist: bool,
    chat_id: i64,
    sender_domains: &HashSet<String>,
) -> Option<String> {
    if text.trim() == "/start" {
        start_reply(in_whitelist, chat_id, sender_domains)
    } else {
        None
    }
}

pub trait TelegramApi: Send + Sync {
    fn send_message(&self, chat_id: i64, html: &str) -> Result<()>;
    fn get_updates(&self, offset: i64, timeout_secs: u32) -> Result<Vec<Update>>;
}

pub struct TgClient {
    token: String,
}

impl TgClient {
    pub fn new(token: &str) -> TgClient {
        TgClient { token: token.to_string() }
    }

    fn url(&self, method: &str) -> String {
        format!("https://api.telegram.org/bot{}/{}", self.token, method)
    }
}

/// Map a `ureq::Error` to an `anyhow::Error` that never includes the request
/// URL (which contains `/bot<TOKEN>/`). Only the HTTP status code or the
/// transport error *kind* (an enum variant name) is logged.
fn tg_error(method: &str, e: ureq::Error) -> anyhow::Error {
    match e {
        ureq::Error::Status(code, _resp) => {
            anyhow::anyhow!("telegram {method} failed: HTTP {code}")
        }
        ureq::Error::Transport(t) => {
            anyhow::anyhow!("telegram {method} transport error: {:?}", t.kind())
        }
    }
}

impl TelegramApi for TgClient {
    fn send_message(&self, chat_id: i64, html: &str) -> Result<()> {
        let resp: Value = ureq::post(&self.url("sendMessage"))
            .send_json(ureq::json!({
                "chat_id": chat_id,
                "text": html,
                "parse_mode": "HTML",
                "disable_web_page_preview": true
            }))
            .map_err(|e| tg_error("sendMessage", e))?
            .into_json()?;
        if resp.get("ok").and_then(|x| x.as_bool()) == Some(true) {
            Ok(())
        } else {
            Err(anyhow!("sendMessage failed: {resp}"))
        }
    }

    fn get_updates(&self, offset: i64, timeout_secs: u32) -> Result<Vec<Update>> {
        let resp: Value = ureq::get(&self.url("getUpdates"))
            .query("offset", &offset.to_string())
            .query("timeout", &timeout_secs.to_string())
            .timeout(std::time::Duration::from_secs(u64::from(timeout_secs) + 10))
            .call()
            .map_err(|e| tg_error("getUpdates", e))?
            .into_json()?;
        Ok(parse_updates(&resp))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn parses_updates_json() {
        let v: serde_json::Value = serde_json::from_str(r#"
        {"ok":true,"result":[
          {"update_id":10,"message":{"text":"/start","chat":{"id":777}}},
          {"update_id":11,"message":{"chat":{"id":888}}}
        ]}"#).unwrap();
        let ups = parse_updates(&v);
        assert_eq!(ups.len(), 2);
        assert_eq!(ups[0].update_id, 10);
        assert_eq!(ups[0].chat_id, Some(777));
        assert_eq!(ups[0].text.as_deref(), Some("/start"));
        assert_eq!(ups[1].text, None);
    }

    #[test]
    fn start_reply_registered_vs_not() {
        let domains: HashSet<String> = ["openai.com".to_string()].into_iter().collect();
        let yes = start_reply(true, 5, &domains).unwrap();
        assert!(yes.contains("зарегистрированы"));
        let no = start_reply(false, 4242, &domains).unwrap();
        assert!(no.contains("4242"));
        assert!(no.contains("не зарегистрированы"));
    }

    #[test]
    fn non_start_text_no_reply() {
        let domains: HashSet<String> = HashSet::new();
        assert!(start_reply_for_text("hello", true, 1, &domains).is_none());
        assert!(start_reply_for_text("/start", false, 1, &domains).is_some());
    }
}
