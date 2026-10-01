//! Redaction and minimisation helpers. Used (a) before anything leaves for a
//! model vendor, (b) in the log writer, and (c) in `Debug` for secret config.

use regex::Regex;
use std::{fmt, sync::LazyLock};

/// Wrapper whose `Debug`/`Display` never reveal the inner value.
#[derive(Clone)]
pub struct Secret<T>(T);

impl<T> Secret<T> {
    pub fn new(v: T) -> Self {
        Self(v)
    }
    pub fn expose(&self) -> &T {
        &self.0
    }
}
impl<T> fmt::Debug for Secret<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[REDACTED]")
    }
}
impl<T> fmt::Display for Secret<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[REDACTED]")
    }
}

static EMAIL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[A-Za-z0-9._%+\-]+@[A-Za-z0-9.\-]+\.[A-Za-z]{2,}").unwrap());
static LONG_DIGITS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\d(?:[ \-]?\d){8,}").unwrap());
static TOKEN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(?:sk|gsk|pk|jev|ghp|gho|xox[a-z])[_\-][A-Za-z0-9_\-]{8,}").unwrap()
});
static BEARER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)(bearer|basic)\s+[A-Za-z0-9._~+/=\-]{6,}").unwrap());
static DB_URL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)(postgres(?:ql)?://[^:/\s]+):([^@\s]+)@").unwrap());
static AUTH_HEADER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(authorization|cookie|set-cookie|x-api-key)\s*[:=]\s*[^\s,;]+").unwrap()
});

/// Mask credentials / tokens in free text (used by the log writer).
pub fn scrub_secrets(input: &str) -> String {
    let s = DB_URL.replace_all(input, "$1:***@");
    // Bearer first: otherwise the header rule would eat only the word "Bearer".
    let s = BEARER.replace_all(&s, "$1 [REDACTED]");
    let s = AUTH_HEADER.replace_all(&s, "$1: [REDACTED]");
    TOKEN.replace_all(&s, "[secret]").into_owned()
}

/// Sanitise user-provided text before it is sent to a model vendor or stored
/// as a safe summary: strip control chars, mask secrets, emails and long digit
/// runs (card / account numbers), and cap the length.
pub fn scrub_text(input: &str, max_chars: usize) -> String {
    let no_ctrl: String = input
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let s = scrub_secrets(&no_ctrl);
    let s = EMAIL.replace_all(&s, "[email]");
    let s = LONG_DIGITS.replace_all(&s, "[number]");
    let s: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if s.chars().count() > max_chars {
        let t: String = s.chars().take(max_chars).collect();
        format!("{t}…")
    } else {
        s
    }
}

/// `postgres://user:secret@host/db` -> `postgres://user:***@host/db`
pub fn redact_database_url(url: &str) -> String {
    DB_URL.replace_all(url, "$1:***@").into_owned()
}

/// Remove exact known secret values (API keys loaded at startup) from a line.
pub fn redact_known(line: &str, secrets: &[String]) -> String {
    let mut out = line.to_owned();
    for s in secrets.iter().filter(|s| s.len() >= 6) {
        out = out.replace(s.as_str(), "[REDACTED]");
    }
    scrub_secrets(&out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn database_url_password_never_survives() {
        let u = "postgres://finguard:hunter2@db.supabase.co:5432/postgres?sslmode=require";
        let r = redact_database_url(u);
        assert!(!r.contains("hunter2"));
        assert!(r.contains("finguard:***@"));
        assert!(!scrub_secrets(&format!("connecting to {u}")).contains("hunter2"));
    }

    #[test]
    fn tokens_and_headers() {
        let s = scrub_secrets(
            "Authorization: Bearer abcdef123456 and key gsk_ABCDEFGHIJKLMNOP and sk_live_12345678",
        );
        assert!(!s.contains("abcdef123456"));
        assert!(!s.contains("ABCDEFGHIJKLMNOP"));
        assert!(!s.contains("sk_live_12345678"));
    }

    #[test]
    fn scrub_text_masks_pii_and_truncates() {
        let s = scrub_text("pay a@b.co card 4111 1111 1111 1111 now\n\t please", 200);
        assert_eq!(s, "pay [email] card [number] now please");
        assert!(scrub_text(&"x".repeat(500), 10).chars().count() <= 11);
        // amounts are preserved
        assert_eq!(
            scrub_text("Send ₱75,000 to Alex", 100),
            "Send ₱75,000 to Alex"
        );
    }

    #[test]
    fn known_secret_values_are_removed() {
        let r = redact_known(
            "calling with key MYSECRETKEY123",
            &["MYSECRETKEY123".into()],
        );
        assert!(!r.contains("MYSECRETKEY123"));
    }

    #[test]
    fn secret_wrapper_does_not_leak_via_debug() {
        let s = Secret::new("topsecret".to_string());
        assert_eq!(format!("{s:?}"), "[REDACTED]");
        assert_eq!(s.expose(), "topsecret");
    }
}
