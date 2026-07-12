# Task 9 Settings Evidence

DoneClaim: settings native surface, transactional preview controls, persistence, and local theme import/export are wired and verified.

## Scenarios

- Focused settings controller: `cargo test -p shell-platform-windows --test settings_controller` -> 4 passed, 0 failed. Covered Apply, Cancel, Reset, invalid draft isolation, restart persistence through `ConfigStore`, checksum export, corrupt import rejection, traversal import rejection, and oversized import rejection.
- System menu settings intent: `cargo test --workspace --all-targets --all-features` -> `system_menu_settings_activation_emits_open_settings_intent` passed; SystemMenu activation emits `QueuedPopoverAction::TypedIntent(PopoverAction::OpenSettings)`.
- Native settings routing: `cargo test --workspace --all-targets --all-features` -> `native_broadcast_events_are_not_misrouted_to_a_single_slot` passed with settings HWND `203` routed to monitor `20`.
- Workspace format: `cargo fmt --all` -> exit 0.
- Workspace lint: `cargo clippy --workspace --all-targets --all-features -- -D warnings` -> exit 0.
- Workspace tests: `cargo test --workspace --all-targets --all-features` -> 98 passed, 0 failed.
- Release build: `cargo build --workspace --release` -> exit 0, `Finished release profile` in 8.15s.
- Native 3s smoke: `target\x86_64-pc-windows-msvc\release\shell-app.exe --showcase --qa-exit-ms 3000` -> exit 0.

## Binary Observables

- Smoke stdout recorded real native settings HWNDs, including `WINDOW role=settings ... title="Minha UI Settings" ... visible=true renderer=hardware`.
- Smoke stdout also recorded per-monitor topbar, dock, popover, and settings HWND creation before the QA timer exited.
- Smoke stderr artifact contains `NO_STDERR`.

## Artifacts

- Summary: `.omo/evidence/task-9-settings/summary.md`
- Smoke stdout: `.omo/evidence/task-9-settings/showcase-smoke-stdout.txt` (18122 bytes)
- Smoke stderr: `.omo/evidence/task-9-settings/showcase-smoke-stderr.txt` (11 bytes)

## Notes

- LSP diagnostics could not run because Rust toolchain `1.97.0-x86_64-pc-windows-msvc` lacks `rust-analyzer.exe`; cargo fmt, clippy, tests, release build, and native smoke passed.
