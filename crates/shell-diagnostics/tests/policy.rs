use shell_diagnostics::{RetentionPolicy, STANDARD_DIAGNOSTIC_POLICY};

#[test]
fn privacy_policy_redacts_sensitive_keys_and_profile_paths_case_insensitively() {
    let policy = STANDARD_DIAGNOSTIC_POLICY;

    assert_eq!(
        policy.sanitize_value("window_title", "Calculator"),
        "[redacted]"
    );
    assert_eq!(
        policy.sanitize_value("error", r"failed at c:\uSeRs\Carlos\private.txt"),
        "[redacted]"
    );
    assert_eq!(
        policy.sanitize_value("error", "/HOME/Carlos/private.txt"),
        "[redacted]"
    );
}

#[test]
fn privacy_policy_bounds_non_sensitive_values_and_field_count() {
    let policy = STANDARD_DIAGNOSTIC_POLICY;
    let oversized = "x".repeat(policy.max_value_chars() + 1);

    assert_eq!(policy.max_fields(), 8);
    assert_eq!(
        policy.sanitize_value("status", &oversized).chars().count(),
        1_024
    );
}

#[test]
fn retention_policy_keeps_newest_valid_segments_and_drives_writer_rotation() {
    let policy = RetentionPolicy::new(2, 100);

    assert_eq!(policy.retained_segments(&[40, 120, 60, 80]), vec![2, 3]);
    assert!(!policy.should_rotate(70, 30));
    assert!(policy.should_rotate(71, 30));
    assert!(!policy.accepts_record(101));
}
