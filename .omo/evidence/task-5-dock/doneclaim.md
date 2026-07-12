# Task 5 Dock DoneClaim

Status: full Task 5 completion for the functional native dock loop.

Production commits already present on this branch:
- `17e24f6 feat(dock): wire rejected native dock interactions`
- `c9f17b8 chore(dock): expose opt-in qa state trace`
- `def4eda test(dock): make native menu qa deterministic`
- `e8eda69 test(dock): route native menu command qa by cursor point`

Implemented behavior covered by this claim:
- Win32 running-window discovery/sync adds external running apps as unpinned dock items and updates focused/minimized/running state.
- Native context menu Pin acts on the item at the cursor/client point and changes an unpinned running item to pinned.
- `WM_DROPFILES` accepts a path with spaces, pins the dropped script as a dock item, and launches that exact path via ShellExecute.
- Dock click focuses a running app and a second click minimizes the focused app.
- Drag reorder persists in dock state order.
- Autohide physically shrinks the dock HWND to an 8 px strip and reveal restores a 180 px HWND.
- Native popup menu is captured through the desktop surface.
- QA cleanup removes the shell, dropped command process, external charmap process, and new Notepad processes.

Canonical manual QA:
- Invocation: `powershell -NoProfile -ExecutionPolicy Bypass -File .omo\evidence\task-5-dock\scripted-qa.ps1`.
- Console artifact: `.omo/evidence/task-5-dock/canonical-qa-run-console.txt`, exit code 0.
- Manifest artifact: `.omo/evidence/task-5-dock/canonical/manifest.json`, sha256 `78354DF4AC4AD544B814D3CF8C14F9384A712B5EAE73F4A668B64FF70149FCD3`.
- Binary under test: `target\x86_64-pc-windows-msvc\release\shell-app.exe`.
- Binary observables recorded in manifest:
  - `running unpinned appears`: `DOCK_STATE` contains `charmap.exe:unpinned:running`.
  - `native context Pin`: `DOCK_STATE` contains `charmap.exe:pinned:running`.
  - `drop path launch`: dropped script writes marker value `launched`.
  - `focus`: `GetForegroundWindow` equals charmap HWND and `DOCK_STATE` contains `focused=true:minimized=false`.
  - `minimize`: `IsIconic` is `True` and `DOCK_STATE` contains `minimized=true`.
  - `drag reorder`: `DOCK_STATE` first item becomes `calc.exe`.
  - `autohide`: `GetWindowRect` height is `8`.
  - `reveal`: `GetWindowRect` height is `180`.
- Screenshot artifacts:
  - `.omo/evidence/task-5-dock/canonical/dock-initial.png`
  - `.omo/evidence/task-5-dock/canonical/topbar-initial.png`
  - `.omo/evidence/task-5-dock/canonical/running-unpinned.png`
  - `.omo/evidence/task-5-dock/canonical/focus-minimized-indicators.png`
  - `.omo/evidence/task-5-dock/canonical/drag-insertion.png`
  - `.omo/evidence/task-5-dock/canonical/drag-reordered.png`
  - `.omo/evidence/task-5-dock/canonical/hidden-8px.png`
  - `.omo/evidence/task-5-dock/canonical/revealed-180px.png`
  - `.omo/evidence/task-5-dock/canonical/menu-open.png`
- Cleanup observable: manifest cleanup has `shellProcessAliveAfterExit=false`, `dropProcessAliveAfterCleanup=0`, `unpinnedProcessAliveAfterCleanup=0`, `newNotepadAliveAfterCleanup=0`; `.omo/evidence/task-5-dock/final-cleanup-process-scan.txt` is `[]` with `EXIT_CODE=0`.

Final verification:
- `cargo fmt --all --check`: `.omo/evidence/task-5-dock/final-fmt-check.txt`, `EXIT_CODE=0`.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: `.omo/evidence/task-5-dock/final-clippy-all-targets-all-features.txt`, `EXIT_CODE=0`.
- `cargo test --workspace --all-targets --all-features`: `.omo/evidence/task-5-dock/final-test-all-targets-all-features.txt`, `EXIT_CODE=0`.
- `cargo build --workspace --all-targets --all-features --release`: `.omo/evidence/task-5-dock/final-build-release-all-targets-all-features.txt`, `EXIT_CODE=0`.
- Changed source file size review: `.omo/evidence/task-5-dock/final-changed-file-review.txt`, all checked touched source files are below 250 pure LOC.
- Evidence inventory: `.omo/evidence/task-5-dock/final-evidence-inventory.txt`.
