# Task 5 Dock Evidence

Status: partial, not full Task 5 completion.

Commit: `ab37683 feat(dock): wire functional native dock loop`

Implemented:
- Renderer dock scene/layout for app items, separators, running indicators, alignment, spacing, item size, and hover magnification.
- Pure `DockController` for click activation, drag reorder, context command, file drop pinning, autohide/reveal state, and detached `DockAnimator`.
- Native Windows message bridge for dock mouse move/down/up/drag/right-click and `WM_DROPFILES`.
- Win32 action dispatch for `ShellExecuteW`, `ShowWindow(SW_RESTORE)`, `SetForegroundWindow`, and `ShowWindow(SW_MINIMIZE)`.
- Native Direct2D dock rendering from runtime `DockScene`, not pasted screenshots.

Verified:
- RED: `cargo test -p shell-renderer --test dock_scene` and `cargo test -p shell-platform-windows --test dock_controller` initially failed on unresolved APIs.
- `cargo fmt --all -- --check`: pass.
- `cargo clippy --workspace --all-targets -- -D warnings`: pass.
- `cargo test --workspace`: pass.
- `cargo build --workspace --release`: pass.
- Manual QA invocation: `target\x86_64-pc-windows-msvc\release\shell-app.exe --showcase --force-warp --qa-exit-ms 2500`.
- Manual QA artifacts: `dock-functional.png`, `topbar-functional.png`, `manual-qa-stdout.txt`, `manual-qa-stderr.txt`, `manual-qa-metadata.json`.
- Cleanup: metadata reports `exitCode: 0`, `processAliveAfterExit: false`, and dock/topbar `isWindowAfterExit: false`.
- Visual QA pass A: PASS, no blockers.
- Visual QA pass B: PASS, no blockers.

Known limitations blocking full Task 5 completion:
- Running-window discovery/synchronization is not implemented; dock running indicators do not yet track external process/window lifecycle.
- Autohide currently updates reducer/controller state but does not physically shrink/reposition/hide the HWND into a reveal-zone-only surface.
- Context menu path is a right-click command bridge, not a native popup menu with selectable commands.
- Manual QA covered deterministic launch/render/cleanup screenshots, not scripted real mouse/drag/drop interaction playback.
