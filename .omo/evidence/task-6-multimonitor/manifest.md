# Task 6 Evidence Manifest

Objective: multi-monitor placement, fullscreen suppression, and window previews for Windows native dock v1.

Environment: Windows native worktree, MSVC target, release binary at `target/x86_64-pc-windows-msvc/release/shell-app.exe`.

Scenarios:

1. TDD RED
   Invocation: `cargo test -p shell-platform-windows --test multimonitor_placement --test window_previews`
   Observable: compile failed for missing Task 6 placement and preview APIs before implementation.
   Artifact: `task-6-red.txt`

2. Mixed-DPI signed-coordinate topology simulation
   Invocation: `cargo test -p shell-platform-windows --test multimonitor_placement`
   Observable: tests passed for independent primary/negative-origin monitor placement, per-monitor DPI, taskbar work-area insets, and fullscreen suppression only on the covered monitor.
   Artifact: `task-6-green-placement.txt`

3. Window preview behavior and actions
   Invocation: `cargo test -p shell-platform-windows --test window_previews`
   Observable: tests passed for DWM thumbnail preview state, explicit restricted-capture degradation, and non-blocking focus/close queued actions with current app identity.
   Artifact: `task-6-green-previews.txt`

4. Final focused regression
   Invocation: `cargo test -p shell-platform-windows --test window_previews --test multimonitor_placement`
   Observable: 5 Task 6 tests passed after final runtime routing changes.
   Artifact: `task-6-fix-focused.txt`

5. Workspace formatting
   Invocation: `cargo fmt --all -- --check`
   Observable: exit code 0, no formatter diffs.
   Artifact: `task-6-fmt.txt`

6. Workspace clippy
   Invocation: `cargo clippy --workspace --all-targets --all-features -- -D warnings`
   Observable: exit code 0.
   Artifact: `task-6-clippy.txt`

7. Workspace tests
   Invocation: `cargo test --workspace --all-targets --all-features`
   Observable: exit code 0; full workspace tests passed, including Task 6 tests and existing lifecycle recovery tests for display change, DPI change, power resume, TaskbarCreated, and hover redraw without resource generation.
   Artifact: `task-6-tests.txt`

8. Release build
   Invocation: `cargo build --workspace --all-targets --all-features --release`
   Observable: exit code 0.
   Artifact: `task-6-release.txt`

9. Manual desktop QA, privacy-safe
   Invocation: `target/x86_64-pc-windows-msvc/release/shell-app.exe --window-smoke --force-warp --simulate-lifecycle-events --qa-exit-ms 1500` with `MINHA_UI_QA_TRACE=1`.
   Observable: release binary enumerated two physical monitors, created two topbar/dock pairs with signed coordinates, showed a fullscreen-suppressed primary dock strip (`height=8`) and normal secondary dock, processed simulated DisplayChanged/PowerResumed/TaskbarCreated lifecycle recovery, and exited automatically. External running-window entries were redacted.
   Artifact: `task-6-manual-window-smoke-redacted.txt`

10. Cleanup
    Invocation: `Get-Process shell-app -ErrorAction SilentlyContinue`
    Observable: no shell-app process remained after manual QA.
    Artifact: `task-6-cleanup.txt`

11. Size and slop checks
    Invocation: changed-Rust pure LOC scan and changed-Rust placeholder scan.
    Observable: all changed Rust modules are <=250 pure LOC; no slop scan matches in changed Rust files.
    Artifacts: `task-6-loc.txt`, `task-6-slop-scan-changed.txt`, `task-6-slop-scan.txt`

12. Evidence privacy and integrity checks
    Invocation: evidence privacy scan, empty-file scan, `git diff --check`, SHA-256 hash generation.
    Observable: no removed screenshot reference or sensitive external-window terms remained, no empty evidence files remained, whitespace check passed except benign CRLF warnings, and hashes were generated for every evidence file except the hash list itself.
    Artifacts: `task-6-privacy-scan.txt`, `task-6-empty-file-check.txt`, `task-6-diff-check.txt`, `SHA256SUMS.txt`

Review notes:
- `task-6-programming-remove-ai-slops-review.md` records the programming/remove-AI-slops review.
- `qa-review/` contains the earlier manual QA lane artifacts; the screenshot artifact was removed after inspection and the remaining QA text artifacts were redacted.

Hashes: `SHA256SUMS.txt`.
