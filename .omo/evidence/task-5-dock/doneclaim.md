# Task 5 Dock DoneClaim

Status: full Task 5 completion for the functional native dock loop.

Production commits covered:
- `55880af chore(dock): record canonical task 5 evidence`
- `a103caf fix(dock): redraw hover without surface rebuild`

Implemented behavior covered by this claim:
- Win32 running-window discovery/sync adds external running apps as unpinned dock items and updates focused/minimized/running state.
- Native context menu Pin acts on the item at the cursor/client point and changes an unpinned running item to pinned.
- `WM_DROPFILES` accepts a path with spaces, pins the dropped script as a dock item, and launches that exact path via ShellExecute.
- Dock click focuses a running app and a second click minimizes the focused app.
- Drag reorder persists in dock state order.
- Autohide physically shrinks the dock HWND to an 8 px strip and reveal restores a 180 px HWND.
- Hover/magnification visual changes redraw the existing dock surface and do not rebuild D3D/DComp renderer resources or advance resource generation.
- Native popup menu is captured through the desktop surface.
- QA cleanup removes the shell, dropped command process, external charmap process, and new Notepad processes.

RED/GREEN render seam:
- RED invocation: `cargo test -p shell-platform-windows --test native_slice dock_hover_visual_change_requests_redraw_without_resource_generation -- --exact`.
- RED artifact: `.omo/evidence/task-5-dock/red-hover-redraw.txt`, exit code 101 from unresolved render-action seam before implementation.
- GREEN focused artifact: `.omo/evidence/task-5-dock/green-hover-redraw.txt`, exit code 0.
- GREEN native slice artifact: `.omo/evidence/task-5-dock/green-native-slice-render-actions.txt`, exit code 0.
- Binary observable: `dock_hover_visual_change_requests_redraw_without_resource_generation` proves hover visual change classifies as redraw, not rebuild/resource generation; `dock_visibility_change_still_requests_rebuild_for_resized_strip` preserves HWND strip/reveal rebuild behavior.

Canonical desktop QA:
- Invocation: `powershell -NoProfile -ExecutionPolicy Bypass -File .omo\evidence\task-5-dock\scripted-qa.ps1`.
- Console artifact: `.omo/evidence/task-5-dock/canonical-qa-run-console.txt`, exit code 0.
- Manifest artifact: `.omo/evidence/task-5-dock/canonical/manifest.json`, sha256 `57A77E0AA9B7B6E4EFAB891DF079DBBB01F054F30A0708B92A2F141BC6C5784F`.
- Manifest validation artifact: `.omo/evidence/task-5-dock/final-manifest-hash-validation.txt`, exit code 0; all manifest-listed artifact hashes and byte counts match after manifest write.
- Binary under test: `target\x86_64-pc-windows-msvc\release\shell-app.exe`.
- Privacy observable: manifest records a solid neutral topmost backdrop `#202428`; screenshots are captured against the controlled backdrop instead of legible desktop content.
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

Render/resource trace:
- Artifact: `.omo/evidence/task-5-dock/final-render-resource-trace.txt`, exit code 0.
- Binary observable from `.omo/evidence/task-5-dock/canonical/app-stdout.txt`: `RESOURCE_LINES=3`, `DOCK_STATE_LINES=21`, `HOVER_DOCK_STATE_LINES=12`, `RESOURCE_GENERATIONS=1`.
- Judgment: hover and interaction redraw traces are present, while resource generation never advances beyond the initial generation.

Final verification:
- `cargo fmt --all --check`: `.omo/evidence/task-5-dock/final-fmt-check.txt`, exit code 0.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: `.omo/evidence/task-5-dock/final-clippy-all-targets-all-features.txt`, exit code 0.
- `cargo test --workspace --all-targets --all-features`: `.omo/evidence/task-5-dock/final-test-all-targets-all-features.txt`, exit code 0.
- `cargo build --workspace --all-targets --all-features --release`: `.omo/evidence/task-5-dock/final-build-release-all-targets-all-features.txt`, exit code 0.
- Changed source file size review: `.omo/evidence/task-5-dock/final-changed-file-review.txt`, exit code 0; touched source files are below 250 pure LOC.
- Cleanup observable: `.omo/evidence/task-5-dock/final-cleanup-process-scan.txt` is `[]` with exit code 0.
- Evidence inventory: `.omo/evidence/task-5-dock/final-evidence-inventory.txt`.
