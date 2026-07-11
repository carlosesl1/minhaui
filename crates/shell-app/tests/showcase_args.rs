use shell_app::{AppMode, QaExit, parse_args};

#[test]
fn parses_showcase_with_bounded_qa_exit() {
    let showcase_args = ["shell-app.exe", "--showcase", "--qa-exit-ms", "20000"];
    let config = parse_args(showcase_args);
    assert_eq!(config.mode, AppMode::Showcase);
    assert_eq!(config.qa_exit, Some(QaExit::from_millis(20_000)));
    assert!(!config.force_warp);
    assert!(!config.simulate_device_loss_once);
}

#[test]
fn parses_forced_warp_showcase() {
    let warp_args = [
        "shell-app.exe",
        "--showcase",
        "--force-warp",
        "--qa-exit-ms",
        "1000",
    ];
    let config = parse_args(warp_args);
    assert_eq!(config.mode, AppMode::Showcase);
    assert_eq!(config.qa_exit, Some(QaExit::from_millis(1_000)));
    assert!(config.force_warp);
}

#[test]
fn parses_one_shot_device_loss_simulation() {
    let args = [
        "shell-app.exe",
        "--window-smoke",
        "--simulate-device-loss-once",
        "--qa-exit-ms",
        "1000",
    ];
    let config = parse_args(args);
    assert!(config.simulate_device_loss_once);
}

#[test]
fn parses_lifecycle_event_simulation() {
    let args = [
        "shell-app.exe",
        "--window-smoke",
        "--simulate-lifecycle-events",
    ];
    assert!(parse_args(args).simulate_lifecycle_events);
}
