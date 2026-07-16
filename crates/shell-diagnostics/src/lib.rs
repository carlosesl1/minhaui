#![forbid(unsafe_code)]

const REDACTED: &str = "[redacted]";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetentionPolicy {
    max_files: usize,
    max_file_bytes: u64,
}

impl RetentionPolicy {
    #[must_use]
    pub const fn new(max_files: usize, max_file_bytes: u64) -> Self {
        Self {
            max_files,
            max_file_bytes,
        }
    }

    #[must_use]
    pub const fn max_file_bytes(self) -> u64 {
        self.max_file_bytes
    }

    #[must_use]
    pub const fn accepts_record(self, record_bytes: u64) -> bool {
        record_bytes <= self.max_file_bytes
    }

    #[must_use]
    pub const fn should_rotate(self, current_bytes: u64, incoming_bytes: u64) -> bool {
        current_bytes > 0 && current_bytes.saturating_add(incoming_bytes) > self.max_file_bytes
    }

    #[must_use]
    pub fn retained_segments(self, segment_bytes: &[u64]) -> Vec<usize> {
        let mut retained = Vec::new();
        let mut index = segment_bytes.len();
        while index > 0 && retained.len() < self.max_files {
            index -= 1;
            if self.accepts_record(segment_bytes[index]) {
                retained.push(index);
            }
        }
        retained.reverse();
        retained
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DiagnosticPolicy {
    max_fields: usize,
    max_value_chars: usize,
    retention: RetentionPolicy,
}

impl DiagnosticPolicy {
    #[must_use]
    pub const fn new(
        max_fields: usize,
        max_value_chars: usize,
        retention: RetentionPolicy,
    ) -> Self {
        Self {
            max_fields,
            max_value_chars,
            retention,
        }
    }

    #[must_use]
    pub const fn max_fields(self) -> usize {
        self.max_fields
    }

    #[must_use]
    pub const fn max_value_chars(self) -> usize {
        self.max_value_chars
    }

    #[must_use]
    pub const fn retention(self) -> RetentionPolicy {
        self.retention
    }

    #[must_use]
    pub fn sanitize_value(self, key: &str, value: &str) -> String {
        let bounded = value.chars().take(self.max_value_chars).collect::<String>();
        if is_sensitive_key(key)
            || contains_ascii_case_insensitive(&bounded, r"\Users\")
            || contains_ascii_case_insensitive(&bounded, "/Users/")
            || contains_ascii_case_insensitive(&bounded, "/home/")
        {
            REDACTED.to_owned()
        } else {
            bounded
        }
    }
}

pub const STANDARD_DIAGNOSTIC_POLICY: DiagnosticPolicy =
    DiagnosticPolicy::new(8, 1_024, RetentionPolicy::new(1, 4 * 1_024 * 1_024));

fn is_sensitive_key(key: &str) -> bool {
    let lower_key = key.to_ascii_lowercase();
    ["title", "path", "token", "secret", "password"]
        .iter()
        .any(|marker| lower_key.contains(marker))
}

fn contains_ascii_case_insensitive(value: &str, pattern: &str) -> bool {
    value
        .as_bytes()
        .windows(pattern.len())
        .any(|window| window.eq_ignore_ascii_case(pattern.as_bytes()))
}
