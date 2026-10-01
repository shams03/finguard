//! Log redaction layer. Even if some code path logged a secret by mistake, it
//! is scrubbed before it reaches stdout.

use finguard_domain::redact::redact_known;
use std::io::{self, Write};
use tracing_subscriber::fmt::MakeWriter;

#[derive(Clone)]
pub struct RedactingMakeWriter {
    secrets: std::sync::Arc<Vec<String>>,
}

impl RedactingMakeWriter {
    pub fn new(secrets: Vec<String>) -> Self {
        Self {
            secrets: std::sync::Arc::new(secrets),
        }
    }
}

pub struct RedactingWriter {
    secrets: std::sync::Arc<Vec<String>>,
    out: io::Stdout,
}

impl Write for RedactingWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let s = String::from_utf8_lossy(buf);
        self.out
            .write_all(redact_known(&s, &self.secrets).as_bytes())?;
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        self.out.flush()
    }
}

impl<'a> MakeWriter<'a> for RedactingMakeWriter {
    type Writer = RedactingWriter;
    fn make_writer(&'a self) -> Self::Writer {
        RedactingWriter {
            secrets: self.secrets.clone(),
            out: io::stdout(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writer_scrubs_known_and_pattern_secrets() {
        let m = RedactingMakeWriter::new(vec!["SUPERSECRETVALUE".into()]);
        let line = redact_known(
            "key=SUPERSECRETVALUE url=postgres://u:pw123@h/db auth: Bearer abcdefghij",
            &m.secrets,
        );
        assert!(
            !line.contains("SUPERSECRETVALUE")
                && !line.contains("pw123")
                && !line.contains("abcdefghij")
        );
    }
}
