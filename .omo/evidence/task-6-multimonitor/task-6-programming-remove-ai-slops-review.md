# Task 6 Programming / Remove-AI-Slops Review

Scope: branch diff for Task 6 product, tests, and evidence. Plan and ledger files were intentionally not edited.

Behavior lock:
- RED captured in `task-6-red.txt` before implementation.
- Focused GREEN captured in `task-6-green-placement.txt`, `task-6-green-previews.txt`, and final `task-6-fix-focused.txt`.
- Workspace regression captured in `task-6-tests.txt`.
- Manual desktop QA captured in `task-6-manual-window-smoke-redacted.txt`.

Cleanup plan:
- `dock_placement.rs`: added typed monitor/fullscreen placement seam; no deletion candidate because existing autohide placement remains load-bearing.
- `window_preview.rs` and `dock_window_sync.rs`: added minimal preview state/action types; no speculative bitmap/cache layer.
- Win32 modules: use documented APIs only (`EnumDisplayMonitors`, `GetDpiForMonitor`, `GetMonitorInfoW`, `DwmRegisterThumbnail`, `DwmUpdateThumbnailProperties`, `PostMessageW`) behind `cfg(windows)` unsafe boundaries.
- Runtime wiring: creates one topbar/dock/runtime slot per monitor, registers every dock HWND for input routing, and suppresses only the dock on the monitor covered by a fullscreen observation.
- Renderer scene: carries preview metadata and a retained DWM thumbnail handle; hover redraw remains a redraw path with no surface rebuild and no second Present path.

Slop scan:
- Changed Rust files: no matches in `task-6-slop-scan-changed.txt`.
- Wide scan found pre-existing older-test matches only in `task-6-slop-scan.txt`; left untouched as unrelated work.

LOC / size:
- All changed Rust modules are <=250 pure LOC (`task-6-loc.txt`).
- Highest touched files: `win32_owner.rs` 248, `win32_windowing.rs` 241, `dock_scene.rs` 221.

Programming self-review:
1. Single responsibility: PASS. New concepts are placement planning, preview state/actions, Win32 enumeration/probing/rendering boundaries, and runtime slot ownership.
2. Boundary purity: PASS. Win32 data is parsed into typed `MonitorPlacementInput`, `ObservedWindow`, and `PreviewCapture` before controller/renderer use.
3. Variant discrimination: PASS. New enums use exhaustive matches.
4. Escape hatches: PASS. No unwrap/expect/panic in changed Rust files; unsafe remains isolated in Win32 modules with `SAFETY:` comments.
5. Defensive layer: PASS. Preview degradation is an explicit API outcome for DWM/capture restriction, not a fake fallback bitmap.
6. Helpers for one-off: PASS. New helpers are public seams used by tests/runtime or Win32 adapters.
7. Tests: PASS. New behavior is locked by failing-first tests plus final focused/full regression gates.
8. Parameter bloat: REVIEWED. `MonitorPlacementInput::new` takes four semantic fields matching existing constructor style in the repo; the fields are a typed boundary object and not an options bag.
9. Stale native handles: PASS. Preview focus/close actions include the expected `AppId`; Win32 action dispatch revalidates HWND-to-process identity before focus/close. Every dock HWND is registered/unregistered for routing.
10. Logging/privacy: PASS. No persistent logger was added; QA stdout was redacted and scanned.

Quality gates:
- fmt: PASS (`task-6-fmt.txt`)
- focused Task 6 tests: PASS (`task-6-fix-focused.txt`)
- clippy: PASS (`task-6-clippy.txt`)
- tests: PASS (`task-6-tests.txt`)
- release: PASS (`task-6-release.txt`)
- manual QA: PASS (`task-6-manual-window-smoke-redacted.txt`)
- cleanup/privacy/hash checks: PASS (`task-6-cleanup.txt`, `task-6-privacy-scan.txt`, `SHA256SUMS.txt`)

Final status: CLEAN
