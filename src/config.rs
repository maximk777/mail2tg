use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{anyhow, Result};

pub const DEFAULT_DDG: &str = r"(?i)^(.+?)_at_([a-z0-9.-]+)_[^@]+@duck\.com$";

pub struct Settings {
    pub tg_bot_token: String,
    pub sender_domains: HashSet<String>,
    pub poll_interval: Duration,
    pub body_preview_chars: usize,
    pub state_dir: PathBuf,
    pub config_path: PathBuf,
    pub credentials_path: PathBuf,
    pub pid_path: PathBuf,
    pub ddg_regex: String,
}

pub fn from_env() -> Result<Settings> {
    from_lookup(|k| std::env::var(k).ok())
}

pub fn from_lookup<F: Fn(&str) -> Option<String>>(get: F) -> Result<Settings> {
    let tg_bot_token = get("TG_BOT_TOKEN")
        .filter(|v| !v.trim().is_empty())
        .ok_or_else(|| anyhow!("TG_BOT_TOKEN is required"))?;

    let raw_domains = get("SENDER_DOMAINS")
        .filter(|v| !v.trim().is_empty())
        .ok_or_else(|| anyhow!("SENDER_DOMAINS is required"))?;
    let sender_domains: HashSet<String> = raw_domains
        .split(',')
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .collect();
    if sender_domains.is_empty() {
        return Err(anyhow!("SENDER_DOMAINS is empty after parsing"));
    }

    let poll_secs: u64 = get("POLL_INTERVAL_SECS")
        .map(|v| v.parse())
        .transpose()
        .map_err(|_| anyhow!("POLL_INTERVAL_SECS must be an integer"))?
        .unwrap_or(15);
    let body_preview_chars: usize = get("BODY_PREVIEW_CHARS")
        .map(|v| v.parse())
        .transpose()
        .map_err(|_| anyhow!("BODY_PREVIEW_CHARS must be an integer"))?
        .unwrap_or(1000);

    let path = |key: &str, default: &str| -> PathBuf {
        get(key).filter(|v| !v.is_empty()).unwrap_or_else(|| default.to_string()).into()
    };

    Ok(Settings {
        tg_bot_token,
        sender_domains,
        poll_interval: Duration::from_secs(poll_secs),
        body_preview_chars,
        state_dir: path("STATE_DIR", "./state"),
        config_path: path("MAIL2TG_CONFIG", "mail2tg.json"),
        credentials_path: path("MAIL2TG_CREDENTIALS", "mail2tg.credentials"),
        pid_path: path("MAIL2TG_PIDFILE", "mail2tg.pid"),
        ddg_regex: get("DDG_FROM_REGEX").filter(|v| !v.is_empty()).unwrap_or_else(|| DEFAULT_DDG.to_string()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn lookup(map: HashMap<&'static str, &'static str>) -> impl Fn(&str) -> Option<String> {
        move |k| map.get(k).map(|s| s.to_string())
    }

    fn minimal() -> HashMap<&'static str, &'static str> {
        HashMap::from([
            ("TG_BOT_TOKEN", "tok"),
            ("SENDER_DOMAINS", "OpenAI.com, anthropic.com"),
        ])
    }

    #[test]
    fn parses_minimal_with_defaults() {
        let s = from_lookup(lookup(minimal())).unwrap();
        assert_eq!(s.tg_bot_token, "tok");
        assert!(s.sender_domains.contains("openai.com"));
        assert!(s.sender_domains.contains("anthropic.com"));
        assert_eq!(s.poll_interval.as_secs(), 15);
        assert_eq!(s.body_preview_chars, 1000);
        assert_eq!(s.state_dir.to_str().unwrap(), "./state");
        assert_eq!(s.config_path.to_str().unwrap(), "mail2tg.json");
        assert_eq!(s.credentials_path.to_str().unwrap(), "mail2tg.credentials");
        assert_eq!(s.pid_path.to_str().unwrap(), "mail2tg.pid");
    }

    #[test]
    fn missing_token_errors() {
        let mut m = minimal();
        m.remove("TG_BOT_TOKEN");
        assert!(from_lookup(lookup(m)).is_err());
    }

    #[test]
    fn missing_sender_domains_errors() {
        let mut m = minimal();
        m.remove("SENDER_DOMAINS");
        assert!(from_lookup(lookup(m)).is_err());
    }

    #[test]
    fn overrides_applied() {
        let mut m = minimal();
        m.insert("POLL_INTERVAL_SECS", "30");
        m.insert("BODY_PREVIEW_CHARS", "200");
        m.insert("STATE_DIR", "/var/state");
        let s = from_lookup(lookup(m)).unwrap();
        assert_eq!(s.poll_interval.as_secs(), 30);
        assert_eq!(s.body_preview_chars, 200);
        assert_eq!(s.state_dir.to_str().unwrap(), "/var/state");
    }
}
