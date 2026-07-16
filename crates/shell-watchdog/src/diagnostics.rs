use std::fs;
use std::io;
use std::path::Path;

use shell_diagnostics::{RetentionPolicy, STANDARD_DIAGNOSTIC_POLICY};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiagnosticField {
    key: String,
    value: String,
}

impl DiagnosticField {
    #[must_use]
    pub fn new(key: &str, value: &str) -> Self {
        Self {
            key: key.to_owned(),
            value: value.to_owned(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiagnosticEvent {
    event: String,
    fields: Vec<DiagnosticField>,
}

impl DiagnosticEvent {
    #[must_use]
    pub fn new<const N: usize>(event: &str, fields: [DiagnosticField; N]) -> Self {
        Self {
            event: event.to_owned(),
            fields: fields.into(),
        }
    }
}

pub fn export_diagnostics(path: &Path, events: &[DiagnosticEvent]) -> io::Result<()> {
    let mut body = String::new();
    for event in events {
        body.push_str(&event.to_json_line());
        body.push('\n');
    }
    fs::write(path, body)
}

#[must_use]
pub fn retained_log_segments(policy: RetentionPolicy, segment_bytes: &[u64]) -> Vec<usize> {
    policy.retained_segments(segment_bytes)
}

impl DiagnosticEvent {
    fn to_json_line(&self) -> String {
        let policy = STANDARD_DIAGNOSTIC_POLICY;
        let mut line = String::from("{\"event\":\"");
        line.push_str(&escape_json(&policy.sanitize_value("event", &self.event)));
        line.push('"');
        for field in self.fields.iter().take(policy.max_fields()) {
            line.push_str(",\"");
            line.push_str(&escape_json(&field.key));
            line.push_str("\":\"");
            line.push_str(&escape_json(
                &policy.sanitize_value(&field.key, &field.value),
            ));
            line.push('"');
        }
        line.push('}');
        line
    }
}

fn escape_json(value: &str) -> String {
    let mut escaped = String::new();
    for character in value.chars() {
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            other => escaped.push(other),
        }
    }
    escaped
}
