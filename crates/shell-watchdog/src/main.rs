#![forbid(unsafe_code)]

use std::path::PathBuf;

use shell_core::TaskbarPolicy;
use shell_watchdog::{
    ChildEvent, Consent, DiagnosticEvent, DiagnosticField, ExplorerTaskbarState, RecoveryHook,
    RestartDelay, SafeModeProfile, SupervisorAction, SupervisorConfig, SupervisorState,
    TaskbarMutation, TaskbarRequest, begin_taskbar_transaction, export_diagnostics,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().collect::<Vec<_>>();
    let command = CliCommand::parse(&args);
    match command {
        CliCommand::Bootstrap => {
            println!("shell-watchdog: bootstrap complete; exiting cleanly");
        }
        CliCommand::HeartbeatMisses(count) => {
            let action = simulate_restarts(ChildEvent::HeartbeatMissed, count);
            println!("shell-watchdog: heartbeat simulation action={action}");
        }
        CliCommand::CrashLoop(count) => {
            let action = simulate_restarts(ChildEvent::ChildCrashed, count);
            println!("shell-watchdog: crash simulation action={action}");
        }
        CliCommand::TaskbarRestore { mutation } => {
            let request = TaskbarRequest::new(
                TaskbarPolicy::Hide,
                Consent::Granted,
                mutation,
                SafeModeProfile::safe(),
            );
            let transaction = begin_taskbar_transaction(ExplorerTaskbarState::AutoHide, request);
            let action = transaction.restore_action(RecoveryHook::CrashDetected);
            println!(
                "shell-watchdog: taskbar simulation startup={:?} restore={:?}",
                transaction.startup_action(),
                action
            );
        }
        CliCommand::DiagnosticsExport(path) => {
            let event =
                DiagnosticEvent::new("shell.safe_mode", [DiagnosticField::new("mode", "safe")]);
            export_diagnostics(&path, &[event])?;
            println!("shell-watchdog: diagnostics exported");
        }
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum CliCommand {
    Bootstrap,
    HeartbeatMisses(u32),
    CrashLoop(u32),
    TaskbarRestore { mutation: TaskbarMutation },
    DiagnosticsExport(PathBuf),
}

impl CliCommand {
    fn parse(args: &[String]) -> Self {
        let mutation = taskbar_mutation(args);
        let mut index = 1;
        while index < args.len() {
            match args[index].as_str() {
                "--simulate-heartbeat-miss" => {
                    return Self::HeartbeatMisses(parse_count(args.get(index + 1)));
                }
                "--simulate-crash-loop" => {
                    return Self::CrashLoop(parse_count(args.get(index + 1)));
                }
                "--simulate-taskbar-restore" => {
                    return Self::TaskbarRestore { mutation };
                }
                "--taskbar-mutation" => index += 1,
                "--diagnostics-export" => {
                    if let Some(path) = args.get(index + 1) {
                        return Self::DiagnosticsExport(PathBuf::from(path));
                    }
                }
                _ => {}
            }
            index += 1;
        }
        Self::Bootstrap
    }
}

fn taskbar_mutation(args: &[String]) -> TaskbarMutation {
    let mut index = 1;
    while index < args.len() {
        if args[index] == "--taskbar-mutation"
            && matches!(args.get(index + 1).map(String::as_str), Some("disabled"))
        {
            return TaskbarMutation::DisabledForTest;
        }
        index += 1;
    }
    TaskbarMutation::Enabled
}

fn parse_count(raw: Option<&String>) -> u32 {
    raw.and_then(|value| value.parse::<u32>().ok()).unwrap_or(1)
}

fn simulate_restarts(event: ChildEvent, count: u32) -> &'static str {
    let config = SupervisorConfig::new(
        RestartDelay::from_millis(100),
        RestartDelay::from_millis(1_000),
        3,
    );
    let mut state = SupervisorState::default();
    let mut action = SupervisorAction::Continue;
    let mut index = 0;
    while index < count {
        action = state.observe(event, config);
        index += 1;
    }
    match action {
        SupervisorAction::Continue => "continue",
        SupervisorAction::Restart { .. } => "restart",
        SupervisorAction::EnterSafeMode => "safe_mode",
    }
}
