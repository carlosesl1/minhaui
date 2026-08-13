#![cfg(windows)]

use std::process::Command;

#[test]
fn bootstrap_startup_is_hermetic() {
    let output = Command::new(env!("CARGO_BIN_EXE_shell-app"))
        .arg("--bootstrap")
        .output()
        .expect("the bootstrap binary should launch");

    assert!(
        output.status.success(),
        "bootstrap failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("bootstrap complete"),
        "bootstrap did not report completion"
    );
}

#[cfg(feature = "native-validation")]
#[test]
fn native_showcase_reaches_the_message_loop_before_qa_exit() {
    let output = Command::new(env!("CARGO_BIN_EXE_shell-app"))
        .args(["--showcase", "--qa-exit-ms", "250"])
        .output()
        .expect("the showcase binary should launch");

    assert!(
        output.status.success(),
        "showcase exited during startup\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[cfg(feature = "native-validation")]
#[test]
fn native_showcase_runs_the_synthetic_rebuild_path_under_warp() {
    let output = Command::new(env!("CARGO_BIN_EXE_shell-app"))
        .args([
            "--window-smoke",
            "--qa-exit-ms",
            "750",
            "--force-warp",
            "--safe-mode",
            "--reduced-motion",
            "--simulate-device-loss-once",
            "--simulate-lifecycle-events",
        ])
        .env("MINHA_UI_QA_TRACE", "1")
        .output()
        .expect("the lifecycle smoke binary should launch");

    assert!(
        output.status.success(),
        "showcase failed to run the synthetic rebuild lifecycle\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("ACCESSIBILITY safe_mode=true"),
        "safe-mode accessibility policy was not reached"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout
            .lines()
            .any(|line| line.starts_with("RESOURCE generation=") && line.contains("renderer=warp")),
        "the synthetic rebuild path did not report a WARP resource generation\nstdout:\n{stdout}"
    );
}
