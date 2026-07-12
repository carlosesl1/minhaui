use std::fs;
use std::io;
use std::path::Path;

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RotationPolicy {
    max_files: usize,
    max_file_bytes: u64,
}

impl RotationPolicy {
    #[must_use]
    pub const fn new(max_files: usize, max_file_bytes: u64) -> Self {
        Self {
            max_files,
            max_file_bytes,
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
pub fn retained_log_segments(policy: RotationPolicy, segment_bytes: &[u64]) -> Vec<usize> {
    let mut retained = Vec::new();
    let mut index = segment_bytes.len();
    while index > 0 && retained.len() < policy.max_files {
        index -= 1;
        if segment_bytes[index] <= policy.max_file_bytes {
            retained.push(index);
        }
    }
    retained.reverse();
    retained
}

impl DiagnosticEvent {
    fn to_json_line(&self) -> String {
        let mut line = String::from("{\"event\":\"");
        line.push_str(&escape_json(&redact("event", &self.event)));
        line.push('"');
        for field in &self.fields {
            line.push_str(",\"");
            line.push_str(&escape_json(&field.key));
            line.push_str("\":\"");
            line.push_str(&escape_json(&redact(&field.key, &field.value)));
            line.push('"');
        }
        line.push('}');
        line
    }
}

fn redact(key: &str, value: &str) -> String {
    let lower_key = key.to_ascii_lowercase();
    if lower_key.contains("title")
        || lower_key.contains("path")
        || lower_key.contains("token")
        || lower_key.contains("secret")
        || lower_key.contains("password")
        || value.contains("\\Users\\")
    {
        String::from("[redacted]")
    } else {
        value.to_owned()
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
