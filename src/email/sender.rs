use std::collections::HashSet;

use regex::Regex;

#[allow(dead_code)]
pub struct Sender {
    pub address: String,
    pub domain: String,
}

#[allow(dead_code)]
pub fn default_ddg_regex() -> &'static str {
    crate::config::DEFAULT_DDG
}

#[allow(dead_code)]
pub fn parse_sender(from_addr: &str, ddg: &Regex) -> Option<Sender> {
    let addr = from_addr.trim().to_lowercase();
    if let Some(caps) = ddg.captures(&addr) {
        let local = caps.get(1)?.as_str();
        let domain = caps.get(2)?.as_str().to_string();
        return Some(Sender { address: format!("{local}@{domain}"), domain });
    }
    let (_, domain) = addr.rsplit_once('@')?;
    if domain.is_empty() || !domain.contains('.') {
        return None;
    }
    Some(Sender { address: addr.clone(), domain: domain.to_string() })
}

#[allow(dead_code)]
pub fn domain_matches(domain: &str, allowed: &HashSet<String>) -> bool {
    let d = domain.to_lowercase();
    allowed.iter().any(|a| {
        d == *a
            || (d.len() > a.len()
                && d.ends_with(a.as_str())
                && d.as_bytes()[d.len() - a.len() - 1] == b'.')
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use regex::Regex;
    use std::collections::HashSet;

    fn re() -> Regex { Regex::new(default_ddg_regex()).unwrap() }

    #[test]
    fn ddg_address_decoded() {
        let s = parse_sender("noreply_at_openai.com_ab12cd@duck.com", &re()).unwrap();
        assert_eq!(s.domain, "openai.com");
        assert_eq!(s.address, "noreply@openai.com");
    }

    #[test]
    fn ddg_case_insensitive() {
        let s = parse_sender("Noreply_at_OpenAI.com_XX@duck.com", &re()).unwrap();
        assert_eq!(s.domain, "openai.com");
    }

    #[test]
    fn direct_address() {
        let s = parse_sender("billing@anthropic.com", &re()).unwrap();
        assert_eq!(s.domain, "anthropic.com");
        assert_eq!(s.address, "billing@anthropic.com");
    }

    #[test]
    fn garbage_returns_none() {
        assert!(parse_sender("not-an-email", &re()).is_none());
        assert!(parse_sender("", &re()).is_none());
    }

    #[test]
    fn domain_match_exact_and_subdomain() {
        let allowed: HashSet<String> = ["openai.com".to_string()].into_iter().collect();
        assert!(domain_matches("openai.com", &allowed));
        assert!(domain_matches("mail.openai.com", &allowed));
        assert!(!domain_matches("notopenai.com", &allowed));
        assert!(!domain_matches("openai.com.evil.com", &allowed));
    }
}
