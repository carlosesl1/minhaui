# Task 6 Doneclaim / Review

Status: complete for the Task 6 fix gate.

Implemented:
- HWND-targeted native event routing so hover/context/drop input mutates only the owning monitor slot; broadcast events stay broadcast.
- Hotplug reconciliation that creates, reuses, removes, and reorders monitor slots on display/taskbar lifecycle events.
- Runtime sync now preserves preview capture metadata through `sync_running_windows_with_previews`.
- Native preview focus/close context commands validate HWND app identity before acting.
- DWM thumbnail registration now has an immediate RAII guard; update failure unregisters the thumbnail.
- Renderer fallback draws a visible `Preview unavailable` surface for capture-restricted previews.
- QA-only restricted preview seed is isolated behind `MINHA_UI_QA_RESTRICTED_PREVIEW` and used only by the native visual script.

Review findings:
- No hover resource rebuild / second Present path was introduced; hover remains a redraw path and existing `native_slice` tests stayed green in the final workspace run.
- All changed Rust files are at or below 250 pure LOC. The largest changed files are `win32_owner.rs` at 250 and `native_showcase_dock.rs` at 249.
- Unsafe code is limited to existing Win32/DWM boundaries and new unsafe blocks have `SAFETY:` comments. The DWM RAII fake path passed strict Miri.
- No raw external-window stdout is retained in the canonical pack; QA stdout is redacted to `DOCK_STATE [redacted]`.
- Old failed attempts, stale screenshots, and empty receipts were pruned from `.omo/evidence/task-6-multimonitor`.

Residual note:
- Physical hotplug add/remove was verified through the production reconcile path and deterministic tests, not by physically attaching/removing a monitor during this run.
