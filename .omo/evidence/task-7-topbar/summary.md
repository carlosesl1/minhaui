# Task 7 Topbar Evidence

## Scenarios

- Focused renderer layout: `cargo test -p shell-renderer --test topbar_scene` -> pass. Observable: compact narrow topbar keeps order, collapses overflow, and hit-tests overflow to `Popover::SystemMenu`.
- Focused platform controller: `cargo test -p shell-platform-windows --test topbar_controller` -> pass. Observable: topbar click emits `QueuedTopbarAction::OpenPopover(Popover::Network)` and polling budget defers adapter refresh.
- Format gate: `cargo fmt --all -- --check` -> pass.
- Clippy gate: `cargo clippy --workspace --all-targets --all-features -- -D warnings` -> pass.
- Workspace tests: `cargo test --workspace --all-targets --all-features` -> pass.
- Release build: `cargo build --workspace --release` -> pass.
- Manual smoke: `.\target\x86_64-pc-windows-msvc\release\shell-app.exe --showcase --qa-exit-ms 3000` -> exit 0.

## Artifacts

- Smoke stdout: `.omo/evidence/task-7-topbar/showcase-smoke-stdout.txt`
- Smoke stderr: `.omo/evidence/task-7-topbar/showcase-smoke-stderr.txt`

## Notes

- Rust LSP diagnostics could not run because the installed `1.97.0-x86_64-pc-windows-msvc` toolchain does not provide `rust-analyzer.exe`; Cargo compile, clippy, tests, release, and smoke were used as diagnostics.
