pub fn recipient_matches(targets: &[String], candidates: &[String]) -> bool {
    targets.iter().any(|target| {
        let target = target.trim();
        candidates
            .iter()
            .any(|candidate| target.eq_ignore_ascii_case(candidate.trim()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_case_insensitive() {
        let targets = vec!["scpccomz@duck.com".to_string()];
        assert!(recipient_matches(&targets, &["SCPCCOMZ@Duck.com".to_string()]));
    }

    #[test]
    fn matches_any_candidate() {
        let targets = vec!["a@duck.com".to_string(), "b@nikitos.uk".to_string()];
        assert!(recipient_matches(&targets, &["x@y.com".to_string(), "b@nikitos.uk".to_string()]));
    }

    #[test]
    fn no_match() {
        let targets = vec!["a@duck.com".to_string()];
        assert!(!recipient_matches(&targets, &["z@duck.com".to_string()]));
        assert!(!recipient_matches(&targets, &[]));
    }
}
