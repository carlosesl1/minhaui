# Task 6 Canonical Evidence Manifest

Objective: finish Task 6 for the Windows native dock: multi-monitor slot routing, hotplug reconciliation, DWM preview safety, native preview actions, capture-restricted fallback, fullscreen suppression, and clean gates.

Canonical pack only: old attempts, empty receipts, and stale QA screenshots were removed before commit.

## TDD Red

- Platform RED
  - Scenario: native routing / hotplug / preview APIs did not exist yet.
  - Invocation: `cargo test -p shell-platform-windows --test native_event_routing --test multimonitor_placement --test window_previews`
  - Binary observable: compile/test failure captured before implementation.
  - Artifact: `.omo/evidence/task-6-multimonitor/task-6-red-fix-gate.txt`

- Renderer RED
  - Scenario: capture-restricted preview fallback layout API did not exist yet.
  - Invocation: `cargo test -p shell-renderer --test dock_scene restricted_preview_has_visible_fallback_layout`
  - Binary observable: unresolved renderer fallback imports before implementation.
  - Artifact: `.omo/evidence/task-6-multimonitor/task-6-red-renderer-fallback.txt`

## Green Scenarios

- Preview and fallback behavior
  - Scenario: restricted preview state reaches the renderer fallback layout.
  - Invocation: `cargo test -p shell-platform-windows --test window_previews && cargo test -p shell-renderer --test dock_scene restricted_preview_has_visible_fallback_layout`
  - Binary observable: 4 platform preview tests and 1 renderer fallback test passed.
  - Artifact: `.omo/evidence/task-6-multimonitor/task-6-green-preview-focused-after-visual-fix.txt`

- Renderer fallback focused test
  - Scenario: restricted preview has a visible unavailable layout.
  - Invocation: `cargo test -p shell-renderer --test dock_scene restricted_preview_has_visible_fallback_layout`
  - Binary observable: `restricted_preview_has_visible_fallback_layout ... ok`.
  - Artifact: `.omo/evidence/task-6-multimonitor/task-6-green-renderer-fallback.txt`

- DWM thumbnail RAII
  - Scenario: failed `DwmUpdateThumbnailProperties` unregisters the already registered thumbnail.
  - Invocation: `cargo test -p shell-platform-windows failed_dwm_update_unregisters_registered_thumbnail`
  - Binary observable: fake API call sequence test passed.
  - Artifact: `.omo/evidence/task-6-multimonitor/task-6-green-dwm-raii.txt`

- Native preview actions
  - Scenario: focus and close preview actions use native HWND/app validation.
  - Invocation: `cargo test -p shell-platform-windows preview_focus_and_close_actions_use_native_window_input_path`
  - Binary observable: safe Win32 test window accepted focus path and was destroyed by close path.
  - Artifact: `.omo/evidence/task-6-multimonitor/task-6-green-native-preview-actions.txt`

- Strict Miri
  - Scenario: DWM RAII fake path under strict provenance/alignment checks.
  - Invocation: `MIRIFLAGS="-Zmiri-strict-provenance -Zmiri-symbolic-alignment-check" cargo +nightly miri test -p shell-platform-windows failed_dwm_update_unregisters_registered_thumbnail`
  - Binary observable: Miri test passed.
  - Artifact: `.omo/evidence/task-6-multimonitor/task-6-miri-dwm-raii-strict.txt`

## Final Gates

- Format
  - Invocation: `cargo fmt --all -- --check`
  - Binary observable: exit code 0.
  - Artifact: `.omo/evidence/task-6-multimonitor/task-6-fmt-final.txt`

- Clippy
  - Invocation: `cargo clippy --workspace --all-targets --all-features -- -D warnings`
  - Binary observable: exit code 0.
  - Artifact: `.omo/evidence/task-6-multimonitor/task-6-clippy-final.txt`

- Workspace tests
  - Invocation: `cargo test --workspace --all-targets --all-features`
  - Binary observable: exit code 0; includes multi-monitor, HWND routing, preview, DWM RAII, and renderer fallback tests.
  - Artifact: `.omo/evidence/task-6-multimonitor/task-6-tests-workspace-final.txt`

- Release build
  - Invocation: `cargo build --release --workspace --all-targets --all-features`
  - Binary observable: exit code 0.
  - Artifact: `.omo/evidence/task-6-multimonitor/task-6-release-final.txt`

- Size / scan gates
  - Invocation: changed Rust pure-LOC scan and changed Rust slop scan.
  - Binary observable: every changed Rust file is <=250 pure LOC; no scanned slop terms remain.
  - Artifacts: `.omo/evidence/task-6-multimonitor/task-6-loc-final.txt`, `.omo/evidence/task-6-multimonitor/task-6-slop-scan-final.txt`

- Empty evidence check
  - Invocation: `find .omo/evidence/task-6-multimonitor -type f -size 0`
  - Binary observable: no zero-byte canonical evidence files listed.
  - Artifact: `.omo/evidence/task-6-multimonitor/task-6-empty-file-check-final.txt`

## Native QA

- Physical monitor enumeration
  - Invocation: PowerShell monitor/WMI enumeration.
  - Binary observable: two active monitors recorded, including negative-origin secondary monitor and work areas.
  - Artifact: `.omo/evidence/task-6-multimonitor/task-6-qa-monitor-enumeration.txt`

- Fullscreen 8px visual
  - Scenario: a safe fullscreen WinForms window covers the primary monitor while the dock runs.
  - Invocation: `task-6-native-qa.ps1 -Scenario Suppressed`
  - Binary observable: primary dock rect `760,1420,1040,8`; `task-6-qa-fullscreen8-window-2.png` is a 1040x8 PNG; secondary dock remains 1040x180.
  - Artifacts: `.omo/evidence/task-6-multimonitor/task-6-qa-fullscreen8-run.txt`, `.omo/evidence/task-6-multimonitor/task-6-qa-fullscreen8-windows.txt`, `.omo/evidence/task-6-multimonitor/task-6-qa-fullscreen8-window-2.png`

- Preview fallback visual
  - Scenario: QA restricted-preview seed exercises the production fallback drawing path.
  - Invocation: `task-6-native-qa.ps1 -Scenario PreviewFallback`
  - Binary observable: 1040x180 dock screenshot visibly contains `Preview unavailable`; stdout shows the release binary ran with WARP and dock windows on both monitors.
  - Artifacts: `.omo/evidence/task-6-multimonitor/task-6-qa-preview-fallback-run.txt`, `.omo/evidence/task-6-multimonitor/task-6-qa-preview-fallback-window-0.png`, `.omo/evidence/task-6-multimonitor/task-6-qa-preview-fallback-stdout-redacted.txt`

- Lifecycle recovery
  - Scenario: release binary processes simulated DisplayChanged / power / TaskbarCreated path.
  - Invocation: `task-6-native-qa.ps1 -Scenario Lifecycle`
  - Binary observable: stdout shows initial fullscreen 8px primary dock, recovery to 1040x180, and `RESOURCE generation=2`.
  - Artifact: `.omo/evidence/task-6-multimonitor/task-6-qa-lifecycle-stdout-redacted.txt`

## Review

- Doneclaim / review artifact: `.omo/evidence/task-6-multimonitor/task-6-doneclaim-review.md`
- Stop-hook verification artifact: `.omo/evidence/task-6-multimonitor/task-6-stop-hook-verification.txt`
- Hashes: `.omo/evidence/task-6-multimonitor/SHA256SUMS.txt`
