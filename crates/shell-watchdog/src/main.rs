#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
#![forbid(unsafe_code)]

use std::ffi::OsString;
use std::io;
use std::path::PathBuf;

use shell_watchdog::{
    ChildEvent, DiagnosticEvent, DiagnosticField, ProcessSupervisorConfig, RestartDelay,
    SupervisorAction, SupervisorConfig, SupervisorState, TaskbarRestoreOutcome, export_diagnostics,
    recovery_journal_path, restore_recovery_journal, supervise_process,
};

const USAGE: &str = "\
Obsidian Glass watchdog

Usage:
  shell-watchdog [--child <path>] [-- <shell arguments>...]
  shell-watchdog --restore-only --hook <update|uninstall> [--check-only]
  shell-watchdog --simulate-heartbeat-miss [count]
  shell-watchdog --simulate-crash-loop [count]
  shell-watchdog --diagnostics-export <path>

Without a command, the watchdog supervises the sibling shell-app executable.";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args_os().collect::<Vec<_>>();
    match CliCommand::parse(&args)? {
        CliCommand::Supervise { child, child_args } => {
            let executable = child.map_or_else(sibling_shell_app, Ok)?;
            supervise_process(
                &executable,
                &child_args,
                ProcessSupervisorConfig::production(),
            )?;
        }
        CliCommand::HeartbeatMisses(count) => {
            let action = simulate_restarts(ChildEvent::HeartbeatMissed, count);
            println!("shell-watchdog: heartbeat simulation action={action}");
        }
        CliCommand::CrashLoop(count) => {
            let action = simulate_restarts(ChildEvent::ChildCrashed, count);
            println!("shell-watchdog: crash simulation action={action}");
        }
        CliCommand::RestoreOnly { hook, check_only } => {
            run_restore_only(hook, check_only)?;
        }
        CliCommand::DiagnosticsExport(path) => {
            let event =
                DiagnosticEvent::new("shell.safe_mode", [DiagnosticField::new("mode", "safe")]);
            export_diagnostics(&path, &[event])?;
            println!("shell-watchdog: diagnostics exported");
        }
        CliCommand::Help => println!("{USAGE}"),
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum CliCommand {
    Supervise {
        child: Option<PathBuf>,
        child_args: Vec<OsString>,
    },
    HeartbeatMisses(u32),
    CrashLoop(u32),
    RestoreOnly {
        hook: RestoreOnlyHook,
        check_only: bool,
    },
    DiagnosticsExport(PathBuf),
    Help,
}

impl CliCommand {
    fn parse(args: &[OsString]) -> io::Result<Self> {
        let command_args = args.get(1..).unwrap_or_default();
        let Some(first) = command_args.first() else {
            return Ok(Self::Supervise {
                child: None,
                child_args: Vec::new(),
            });
        };

        match first.to_str() {
            Some("--simulate-heartbeat-miss") => Ok(Self::HeartbeatMisses(parse_simulation_count(
                command_args,
                "--simulate-heartbeat-miss",
            )?)),
            Some("--simulate-crash-loop") => Ok(Self::CrashLoop(parse_simulation_count(
                command_args,
                "--simulate-crash-loop",
            )?)),
            Some("--diagnostics-export") => parse_diagnostics_export(command_args),
            Some("--restore-only") => parse_restore_only(command_args),
            Some("--help" | "-h") => {
                require_argument_count(command_args, 1, "--help accepts no other arguments")?;
                Ok(Self::Help)
            }
            _ => parse_supervision(command_args),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RestoreOnlyHook {
    Update,
    Uninstall,
}

impl RestoreOnlyHook {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Update => "update",
            Self::Uninstall => "uninstall",
        }
    }
}

fn parse_supervision(args: &[OsString]) -> io::Result<CliCommand> {
    let mut child = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].to_str() {
            Some("--") => {
                return Ok(CliCommand::Supervise {
                    child,
                    child_args: args[index + 1..].to_vec(),
                });
            }
            Some("--child") => {
                if child.is_some() {
                    return Err(invalid_input("--child may be specified only once"));
                }
                let path = args
                    .get(index + 1)
                    .ok_or_else(|| invalid_input("--child requires an executable path"))?;
                if path.is_empty() {
                    return Err(invalid_input(
                        "--child requires a non-empty executable path",
                    ));
                }
                child = Some(PathBuf::from(path));
                index += 2;
            }
            Some(flag) => {
                return Err(invalid_input(format!(
                    "unknown watchdog argument '{flag}'; pass shell arguments after --"
                )));
            }
            None => {
                return Err(invalid_input(
                    "watchdog arguments before -- must contain valid Unicode",
                ));
            }
        }
    }
    Ok(CliCommand::Supervise {
        child,
        child_args: Vec::new(),
    })
}

fn parse_simulation_count(args: &[OsString], flag: &str) -> io::Result<u32> {
    if args.len() > 2 {
        return Err(invalid_input(format!(
            "{flag} accepts at most one count argument"
        )));
    }
    let Some(raw) = args.get(1) else {
        return Ok(1);
    };
    let raw = raw
        .to_str()
        .ok_or_else(|| invalid_input(format!("{flag} count must be valid Unicode")))?;
    let count = raw
        .parse::<u32>()
        .map_err(|_| invalid_input(format!("{flag} count must be an unsigned integer")))?;
    if count == 0 {
        return Err(invalid_input(format!(
            "{flag} count must be greater than zero"
        )));
    }
    Ok(count)
}

fn parse_diagnostics_export(args: &[OsString]) -> io::Result<CliCommand> {
    require_argument_count(
        args,
        2,
        "--diagnostics-export requires exactly one output path",
    )?;
    Ok(CliCommand::DiagnosticsExport(PathBuf::from(&args[1])))
}

fn parse_restore_only(args: &[OsString]) -> io::Result<CliCommand> {
    let mut hook = None;
    let mut check_only = false;
    let mut index = 1;
    while index < args.len() {
        match args[index].to_str() {
            Some("--hook") => {
                if hook.is_some() {
                    return Err(invalid_input("--hook may be specified only once"));
                }
                hook = Some(match args.get(index + 1).and_then(|value| value.to_str()) {
                    Some("update") => RestoreOnlyHook::Update,
                    Some("uninstall") => RestoreOnlyHook::Uninstall,
                    Some(_) => {
                        return Err(invalid_input("--hook must be 'update' or 'uninstall'"));
                    }
                    None => return Err(invalid_input("--hook requires update or uninstall")),
                });
                index += 2;
            }
            Some("--check-only") if !check_only => {
                check_only = true;
                index += 1;
            }
            Some("--check-only") => {
                return Err(invalid_input("--check-only may be specified only once"));
            }
            Some(flag) => {
                return Err(invalid_input(format!("unknown restore argument '{flag}'")));
            }
            None => {
                return Err(invalid_input(
                    "restore arguments must contain valid Unicode",
                ));
            }
        }
    }
    Ok(CliCommand::RestoreOnly {
        hook: hook.ok_or_else(|| invalid_input("--restore-only requires --hook"))?,
        check_only,
    })
}

fn require_argument_count(args: &[OsString], expected: usize, message: &str) -> io::Result<()> {
    if args.len() == expected {
        Ok(())
    } else {
        Err(invalid_input(message))
    }
}

fn sibling_shell_app() -> io::Result<PathBuf> {
    let current = std::env::current_exe()?;
    Ok(current.with_file_name(shell_app_filename()))
}

const fn shell_app_filename() -> &'static str {
    if cfg!(windows) {
        "shell-app.exe"
    } else {
        "shell-app"
    }
}

fn run_restore_only(hook: RestoreOnlyHook, check_only: bool) -> io::Result<()> {
    let journal = pending_recovery_journal_path()?;
    let outcome = restore_recovery_journal(&journal, check_only).map_err(io::Error::other)?;
    let message = match outcome {
        TaskbarRestoreOutcome::NoJournal => "no pending taskbar recovery journal",
        TaskbarRestoreOutcome::Checked => "pending taskbar recovery journal validated",
        TaskbarRestoreOutcome::Restored => "taskbar recovery restored and journal removed",
    };
    println!("shell-watchdog: {message} for {}", hook.as_str());
    Ok(())
}

fn pending_recovery_journal_path() -> io::Result<PathBuf> {
    let local_app_data = std::env::var_os("LOCALAPPDATA")
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "LOCALAPPDATA is unavailable"))?;
    let state_directory = PathBuf::from(local_app_data)
        .join("Minha UI")
        .join("watchdog");
    Ok(recovery_journal_path(&state_directory))
}

fn invalid_input(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
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

#[cfg(test)]
mod tests {
    use super::{CliCommand, RestoreOnlyHook};
    use shell_watchdog::recovery_journal_path;
    use std::ffi::OsString;
    use std::path::PathBuf;

    fn args(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    #[test]
    fn release_windows_binary_declares_the_gui_subsystem() {
        assert!(include_str!("main.rs").contains(
            "#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = \"windows\")]"
        ));
    }

    #[test]
    fn empty_arguments_supervise_the_sibling_shell() {
        let parsed = CliCommand::parse(&args(&["shell-watchdog"]));

        assert_eq!(
            parsed.ok(),
            Some(CliCommand::Supervise {
                child: None,
                child_args: Vec::new(),
            })
        );
    }

    #[test]
    fn child_path_and_arguments_are_kept_separate() {
        let parsed = CliCommand::parse(&args(&[
            "shell-watchdog",
            "--child",
            "C:\\Program Files\\Obsidian Glass\\shell-app.exe",
            "--",
            "--safe-mode",
            "not-a-watchdog-flag",
        ]));

        assert_eq!(
            parsed.ok(),
            Some(CliCommand::Supervise {
                child: Some(PathBuf::from(
                    "C:\\Program Files\\Obsidian Glass\\shell-app.exe"
                )),
                child_args: args(&["--safe-mode", "not-a-watchdog-flag"]),
            })
        );
    }

    #[test]
    fn unknown_watchdog_argument_is_rejected() {
        let parsed = CliCommand::parse(&args(&["shell-watchdog", "--safe-mode"]));

        assert!(parsed.is_err());
    }

    #[test]
    fn restore_only_requires_a_known_hook() {
        assert_eq!(
            CliCommand::parse(&args(&[
                "shell-watchdog",
                "--restore-only",
                "--hook",
                "uninstall",
                "--check-only",
            ]))
            .ok(),
            Some(CliCommand::RestoreOnly {
                hook: RestoreOnlyHook::Uninstall,
                check_only: true,
            })
        );
        assert!(
            CliCommand::parse(&args(&[
                "shell-watchdog",
                "--restore-only",
                "--hook",
                "restart",
            ]))
            .is_err()
        );
    }

    #[test]
    fn journal_path_is_per_user_and_versioned() {
        assert_eq!(
            recovery_journal_path(
                &PathBuf::from("C:\\Users\\Test\\AppData\\Local")
                    .join("Minha UI")
                    .join("watchdog")
            ),
            PathBuf::from("C:\\Users\\Test\\AppData\\Local")
                .join("Minha UI")
                .join("watchdog")
                .join("taskbar-recovery-v1.json")
        );
    }
}
