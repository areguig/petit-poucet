use std::sync::LazyLock;

use regex::Regex;

static PATTERNS: LazyLock<Vec<(&str, Regex)>> = LazyLock::new(|| {
    [
        ("AWS access key", r"\b(AKIA|ASIA)[0-9A-Z]{16}\b"),
        (
            "GitHub token",
            r"\b(gh[pousr]_[A-Za-z0-9]{36,}|github_pat_[A-Za-z0-9_]{22,})",
        ),
        ("GitLab token", r"\bglpat-[A-Za-z0-9_-]{20,}"),
        ("Slack token", r"\bxox[abprs]-[A-Za-z0-9-]{10,}"),
        ("API key", r"\bsk-(ant-)?[A-Za-z0-9_-]{20,}"),
        ("Google API key", r"\bAIza[0-9A-Za-z_-]{35}"),
        ("private key", r"-----BEGIN [A-Z ]*PRIVATE KEY-----"),
        (
            "credential assignment",
            r#"(?i)\b(password|passwd|secret|token|api[_-]?key)\s*[:=]\s*["']?[^\s"'`]{8,}"#,
        ),
    ]
    .into_iter()
    .map(|(kind, pattern)| (kind, Regex::new(pattern).unwrap()))
    .collect()
});

// Returns only the kind, so the secret itself never reaches a log or a reply.
pub fn find(text: &str) -> Option<&'static str> {
    PATTERNS
        .iter()
        .find(|(_, re)| re.is_match(text))
        .map(|(kind, _)| *kind)
}

#[cfg(test)]
mod tests {
    use super::find;

    #[test]
    fn flags_secrets() {
        assert_eq!(
            find("key AKIAIOSFODNN7EXAMPLE here"),
            Some("AWS access key")
        );
        assert_eq!(find("-----BEGIN RSA PRIVATE KEY-----"), Some("private key"));
        assert_eq!(
            find("password: hunter22hunter"),
            Some("credential assignment")
        );
    }

    #[test]
    fn ignores_prose_about_secrets() {
        assert_eq!(find("use the git credential as PRIVATE-TOKEN"), None);
        assert_eq!(find("never print a token, pipe it"), None);
    }
}
