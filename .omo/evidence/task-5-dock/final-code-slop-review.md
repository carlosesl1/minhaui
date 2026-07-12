# Task 5 Final Code/Slop Review

Scope: Task 5 diff from `55880af chore(dock): record canonical task 5 evidence` through the current hygiene worktree, covering:
- `a103caf fix(dock): redraw hover without surface rebuild`
- `4955cee chore(dock): record redraw evidence pack`
- pending hygiene split/review commit

Review criteria: `omo:programming` Rust rules plus `omo:remove-ai-slops` categories.

Findings and priorities:
- P0 BLOCK: none.
- P1 FIXED: `crates/shell-platform-windows/tests/dock_controller.rs` was 344 pure LOC and exceeded the 250 LOC ceiling. It is split by responsibility into lifecycle, interaction, and config/placement test targets while preserving all test names and assertions.
- P2 WATCH: existing Win32/Direct2D unsafe blocks remain isolated in already allow-listed platform/native modules. The Task 5 redraw change adds reuse of the existing `WindowSurface` bitmap target and preserves device-loss fallback to rebuild.

Tests anti-tautology review:
- `native_slice::dock_hover_visual_change_requests_redraw_without_resource_generation` asserts the public render-action classifier returns `RedrawDock` for hover visual changes and does not increment resource generation.
- `native_slice::dock_visibility_change_still_requests_rebuild_for_resized_strip` asserts visibility/HWND strip changes still classify as `RebuildSurfaces`.
- `dock_controller` lifecycle tests assert queued launch/focus/minimize, pin persistence for discovered apps, and running-window sync state transitions.
- `dock_controller_interactions` tests assert observable actions and state mutations for drag reorder, context menu/drop persistence, hover magnification, launch path preservation, and noninteractive separators.
- `dock_controller_config` tests assert autohide reveal state, physical hidden-strip placement, and native menu id mapping.
- Judgment: tests are behavior-based and not tautological; they assert public controller actions/state, layout bounds, placement rectangles, and command mapping rather than private implementation calls.

Rust LOC scan:
- `crates/shell-renderer/src/native.rs`: 184 pure LOC.
- `crates/shell-platform-windows/src/lib.rs`: 130 pure LOC.
- `crates/shell-platform-windows/src/runtime.rs`: 77 pure LOC.
- `crates/shell-platform-windows/src/win32_owner.rs`: 192 pure LOC.
- `crates/shell-platform-windows/src/win32_dock_render.rs`: 89 pure LOC.
- `crates/shell-platform-windows/tests/native_slice.rs`: 115 pure LOC.
- `crates/shell-platform-windows/tests/dock_controller.rs`: 126 pure LOC.
- `crates/shell-platform-windows/tests/dock_controller_interactions.rs`: 195 pure LOC.
- `crates/shell-platform-windows/tests/dock_controller_config.rs`: 71 pure LOC.

Unsafe/resource review:
- `win32_dock_render.rs` is `#![deny(unsafe_code)]`; it classifies render changes, redraws the existing dock surface, and rebuilds only for visibility/resource-loss paths.
- `CompositionRenderer::redraw_surface` reuses `WindowSurface.bitmap` and presents the existing swap chain. It does not create a renderer, D3D device, DComp visual, or swap chain on hover.
- Device-loss recovery is preserved: `PresentOutcome::DeviceLost` and recoverable draw errors route back to `RuntimeSurfaces::rebuild`.
- Canonical trace evidence recorded `RESOURCE_GENERATIONS=1` with 12 hover dock-state redraw traces, so hover redraw did not advance resource generation during QA.

Slop category review:
- Obvious comments: kept only BDD `Given/When/Then` markers in tests and SAFETY comments in unsafe native code.
- Over-defensive code: no new redundant null checks, duplicate validation, broad catch, or fallback shims found.
- Excessive complexity: oversized test target fixed; no touched function exceeds the requested file ceiling review threshold.
- Needless abstraction: `DockRenderAction`/`DockRenderChange` are public seams used by runtime and tests; `win32_dock_render.rs` owns the rendering decision path and removes code from `win32_owner.rs`.
- Boundary violations: no wrong-layer imports found in the Task 5 diff.
- Dead code/debug leftovers: no `dbg!`, `todo!`, or stale helper found in changed Rust files. QA trace `println!` calls are existing opt-in QA instrumentation.
- Duplication: duplicated test fixtures are intentionally local to integration test targets to avoid a shared helper module that would add indirection for small tests.
- Performance equivalence: hover now redraws existing resources instead of rebuilding renderer/surfaces; behavior is covered by classifier tests and canonical trace.
- Missing tests: no blocker; render classifier, controller lifecycle, interactions, config, and canonical QA evidence cover the changed behavior.

Verification receipts recorded for this review:
- `.omo/evidence/task-5-dock/hygiene-fmt-check.txt`
- `.omo/evidence/task-5-dock/hygiene-clippy-all-targets-all-features.txt`
- `.omo/evidence/task-5-dock/hygiene-test-all-targets-all-features.txt`
- `.omo/evidence/task-5-dock/hygiene-build-release-all-targets-all-features.txt`
- `.omo/evidence/task-5-dock/hygiene-diff-checks.txt`

Conclusion: CLEAR.
