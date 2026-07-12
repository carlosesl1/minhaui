# Task 5 Dock Evidence

Status: full Task 5 completion for the functional native dock loop.

Implemented in this pass:
- Running-window discovery and sync use documented Win32 APIs: `EnumWindows`, visibility/title/owner/tool-window filtering, `GetWindowThreadProcessId`, `OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION)`, `QueryFullProcessImageNameW`, `IsIconic`, and `GetForegroundWindow`.
- Pinned apps update running/focused/minimized state from discovered windows; stale windows close; unpinned external running apps are added with stable app-derived dock identities.
- Autohide physically repositions the dock HWND to the configured reveal strip and restores the normal dock rectangle when the pointer enters the strip.
- Right-click uses a native popup menu via `CreatePopupMenu`, `AppendMenuW`, `TrackPopupMenu(TPM_RETURNCMD)`, and dispatches the selected command.
- Scripted desktop QA drives mouse launch, focus/minimize clicks, native context menu selection, drag reorder, hide/reveal, privacy-safe cropped screenshots, and process/HWND cleanup.

Verification:
- RED core evidence: `.omo/evidence/task-5-dock/red-core-window-sync.txt` shows missing `WindowChanged` and `DockItem::running_unpinned`.
- RED platform evidence: `.omo/evidence/task-5-dock/red-platform-dock-blockers.txt` shows missing observed-window sync, placement, and native menu seams.
- GREEN core reducer tests: `.omo/evidence/task-5-dock/green-core-window-sync.txt`, 10 passed.
- GREEN dock controller tests: `.omo/evidence/task-5-dock/green-platform-dock-blockers.txt`, 8 passed.
- `cargo fmt --all -- --check`: `.omo/evidence/task-5-dock/fmt-check-final.txt`, exit code 0.
- `cargo clippy --workspace --all-targets -- -D warnings`: `.omo/evidence/task-5-dock/clippy-final.txt`, exit code 0.
- `cargo test --workspace`: `.omo/evidence/task-5-dock/workspace-tests.txt`, exit code 0.
- `cargo build --workspace --release`: `.omo/evidence/task-5-dock/release-build.txt`, exit code 0.

Manual QA:
- Invocation: `powershell -NoProfile -ExecutionPolicy Bypass -File .omo\evidence\task-5-dock\scripted-qa.ps1`.
- Binary under test: `target\x86_64-pc-windows-msvc\release\shell-app.exe --showcase --force-warp --qa-exit-ms 12000`.
- Binary observables: `.omo/evidence/task-5-dock/scripted-qa-metadata.json` records new Notepad PID `31392`, accepted follow-up focus/minimize clicks, native menu Enter selection, drag completion, hidden dock HWND height `8`, revealed dock HWND height `180`, `shellProcessAliveAfterExit=false`, `dockHwndAliveAfterExit=false`, `topbarHwndAliveAfterExit=false`, and `newNotepadAliveAfterCleanup=0`.
- Captured artifacts: `.omo/evidence/task-5-dock/scripted-dock-initial.png`, `.omo/evidence/task-5-dock/scripted-topbar-initial.png`, `.omo/evidence/task-5-dock/scripted-dock-hidden-strip.png`, `.omo/evidence/task-5-dock/scripted-dock-revealed.png`.
- Final process cleanup: `.omo/evidence/task-5-dock/process-clean-check-final.txt` records `no matching processes`.
- Final evidence inventory: `.omo/evidence/task-5-dock/evidence-inventory-final.txt`.
