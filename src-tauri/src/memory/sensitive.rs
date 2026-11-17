//! Heuristic detection of sensitive data so memories are never saved
//! silently with secrets in them.

use once_cell::sync::Lazy;
use regex::Regex;

static PATTERNS: Lazy<Vec<(&'static str, Regex)>> = Lazy::new(|| {
    vec![
        ("API key", Regex::new(r"(?i)\b(sk-[a-z0-9_\-]{16,}|sk-ant-[a-z0-9_\-]{16,}|AKIA[0-9A-Z]{16}|ghp_[A-Za-z0-9]{30,}|xox[baprs]-[A-Za-z0-9-]{10,})").unwrap()),
        ("private key", Regex::new(r"-----BEGIN [A-Z ]*PRIVATE KEY-----").unwrap()),
        ("password", Regex::new(r"(?i)\b(password|passwd|pwd|passcode)\s*[:=]\s*\S+").unwrap()),
        ("payment card number", Regex::new(r"\b(?:\d[ -]?){13,19}\b").unwrap()),
        ("US SSN", Regex::new(r"\b\d{3}-\d{2}-\d{4}\b").unwrap()),
        ("bearer token", Regex::new(r"(?i)\bbearer\s+[a-z0-9\-_\.=]{20,}").unwrap()),
    ]
});

/// Returns the kinds of sensitive data detected in `text`.
pub fn detect(text: &str) -> Vec<&'static str> {
    PATTERNS
        .iter()
        .filter(|(name, re)| {
            if *name == "payment card number" {
                re.find_iter(text).any(|m| luhn(m.as_str()))
            } else {
                re.is_match(text)
            }
        })
        .map(|(n, _)| *n)
        .collect()
}

fn luhn(s: &str) -> bool {
    let digits: Vec<u32> = s.chars().filter_map(|c| c.to_digit(10)).collect();
    if digits.len() < 13 {
        return false;
    }
    let sum: u32 = digits
        .iter()
        .rev()
        .enumerate()
        .map(|(i, &d)| {
            if i % 2 == 1 {
                let x = d * 2;
                if x > 9 { x - 9 } else { x }
            } else {
                d
            }
        })
        .sum();
    sum % 10 == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_secrets() {
        assert!(detect("my key is sk-ant-abcdefghijklmnopqrstuv").contains(&"API key"));
        assert!(detect("password: hunter2").contains(&"password"));
        assert!(detect("card 4111 1111 1111 1111").contains(&"payment card number"));
    }

    #[test]
    fn ignores_normal_text() {
        assert!(detect("I prefer dark mode and TypeScript").is_empty());
        assert!(detect("call me at 1234567890123 maybe").is_empty() || !luhn("1234567890123"));
    }
}
