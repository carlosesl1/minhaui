use std::fs;

use shell_watchdog::{
    DiagnosticEvent, DiagnosticField, InstanceDecision, InstanceProbe, RotationPolicy,
    classify_instance, export_diagnostics, retained_log_segments,
};

#[test]
fn single_instance_signals_existing_process_when_mutex_is_owned() {
    let given_probe = InstanceProbe::OwnedByLiveProcess;

    let when_decision = classify_instance(given_probe);

    assert_eq!(when_decision, InstanceDecision::SignalExisting);
}

#[test]
fn diagnostics_redact_titles_paths_and_secrets_before_export()
-> Result<(), Box<dyn std::error::Error>> {
    let given_event = DiagnosticEvent::new(
        "shell.safe_mode",
        [
            DiagnosticField::new("window_title", "Budget Carlos.xlsx"),
            DiagnosticField::new("path", "C:\\Users\\Carlos\\Documents\\secret.txt"),
            DiagnosticField::new("token", "abc123"),
            DiagnosticField::new("mode", "safe"),
        ],
    );
    let given_path = std::env::temp_dir().join(format!(
        "shell-watchdog-diagnostics-{}.jsonl",
        std::process::id()
    ));

    export_diagnostics(&given_path, &[given_event])?;
    let when_exported = fs::read_to_string(&given_path)?;
    fs::remove_file(&given_path)?;

    assert!(when_exported.contains("\"mode\":\"safe\""));
    assert!(!when_exported.contains("Carlos"));
    assert!(!when_exported.contains("Budget"));
    assert!(!when_exported.contains("abc123"));
    assert!(when_exported.contains("[redacted]"));
    Ok(())
}

#[test]
fn diagnostics_redact_posix_paths_and_profile_names() -> Result<(), Box<dyn std::error::Error>> {
    let given_event = DiagnosticEvent::new(
        "shell.config_recovery",
        [
            DiagnosticField::new("config_path", "/Users/Carlos/.minha-ui/settings.json"),
            DiagnosticField::new("detail", "failed under C:/Users/Carlos/Documents"),
            DiagnosticField::new("result", "recovered"),
        ],
    );
    let given_path = std::env::temp_dir().join(format!(
        "shell-watchdog-posix-diagnostics-{}.jsonl",
        std::process::id()
    ));

    export_diagnostics(&given_path, &[given_event])?;
    let when_exported = fs::read_to_string(&given_path)?;
    fs::remove_file(&given_path)?;

    assert!(when_exported.contains("\"result\":\"recovered\""));
    assert!(!when_exported.contains("Carlos"));
    assert!(!when_exported.contains("/Users/"));
    assert!(!when_exported.contains("C:/Users/"));
    assert!(when_exported.contains("[redacted]"));
    Ok(())
}

#[test]
fn log_rotation_keeps_newest_segments_within_bounds() {
    let given_policy = RotationPolicy::new(2, 100);
    let given_segments = [40_u64, 90, 20];

    let when_retained = retained_log_segments(given_policy, &given_segments);

    assert_eq!(when_retained, vec![1, 2]);
}
