# Task 10 Recovery Evidence

## Scenarios

- Red test check: `cargo test -p shell-watchdog` failed before implementation with unresolved Task 10 APIs in `recovery_policy.rs`, `watchdog_supervisor.rs`, and `instance_diagnostics.rs`.
- Recovery policy: `cargo test -p shell-watchdog` passed `taskbar_policy_stays_off_when_consent_is_missing`, `restore_transaction_restores_original_state_for_recovery_hooks`, and `safe_mode_blocks_taskbar_mutation_and_disables_optional_features`.
- Watchdog: `cargo test -p shell-watchdog` passed bounded heartbeat backoff and crash-loop safe-mode tests.
- Single instance and diagnostics: `cargo test -p shell-watchdog` passed existing-instance activation classification, redaction export, and log retention tests.
- Task 10 lint/build: `cargo fmt --all -- --check`, `cargo clippy -p shell-watchdog --all-targets --all-features -- -D warnings`, and `cargo build -p shell-watchdog --release` passed.
- Safe smoke: `.\target\x86_64-pc-windows-msvc\release\shell-watchdog.exe --simulate-taskbar-restore --taskbar-mutation disabled` printed `startup=LeaveUntouched restore=Restore(AutoHide)`, so the smoke did not mutate the real taskbar.
- CLI seams: heartbeat and crash simulations printed `action=restart` and `action=safe_mode`.
- Diagnostics artifact: `.\target\x86_64-pc-windows-msvc\release\shell-watchdog.exe --diagnostics-export .omo\evidence\task-10-recovery\diagnostics.jsonl` wrote `.omo/evidence/task-10-recovery/diagnostics.jsonl` with structured safe-mode data.

## Blocked Workspace Gates

- `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --workspace`, and `cargo build --workspace --release` are blocked by pre-existing dirty `crates/shell-platform-windows/src/win32_dock_render.rs` errors: unused `error` variables at lines 99 and 187, plus clippy `too_many_arguments` at lines 33 and 175.
- That file was already modified before Task 10 started and was not edited for this task.

## Debugging Audit

- Hypothesis: heartbeat misses might restart without backoff. Runtime evidence: `.\target\x86_64-pc-windows-msvc\release\shell-watchdog.exe --simulate-heartbeat-miss 2` printed `shell-watchdog: heartbeat simulation action=restart`, and `watchdog_uses_bounded_exponential_backoff_for_missed_heartbeats` asserted `250ms -> 500ms -> 1000ms`.
- Hypothesis: child crashes might restart forever. Runtime evidence: `.\target\x86_64-pc-windows-msvc\release\shell-watchdog.exe --simulate-crash-loop 3` printed `shell-watchdog: crash simulation action=safe_mode`.
- Hypothesis: automated taskbar smoke might mutate the real taskbar. Runtime evidence: `.\target\x86_64-pc-windows-msvc\release\shell-watchdog.exe --simulate-taskbar-restore --taskbar-mutation disabled` printed `startup=LeaveUntouched restore=Restore(AutoHide)`.

## Review Gate

- Local verification passed for the Task 10 package.
- Background review lanes were launched for goal, code, security, QA, and context review. They produced no mailbox result after two bounded waits and were interrupted, so those lanes are inconclusive rather than counted as approval.
