# Continuous Dock Magnification Motion Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement a continuous, spring-smoothed dock magnification interaction that pushes neighboring icons aside and idles at zero animation cost.

**Architecture:** Keep the spatial kernel and visual geometry in `shell-renderer`, deterministic spring state in the dock controller types, and the conditional frame clock in the Win32 runtime. Publish only animated pointer position and strength through `DockScene`; retain resting slots as hit targets.

**Tech Stack:** Rust 2024, windows-rs, Direct2D/DirectComposition, Win32 `WM_TIMER`, Cargo tests and Clippy.

---

### Task 1: Lock the spatial motion contract

**Files:**
- Modify: `crates/shell-renderer/tests/dock_scene.rs`
- Modify: `crates/shell-renderer/src/dock_scene.rs`
- Modify: `crates/shell-renderer/src/dock_layout.rs`

- [ ] Add failing tests that place the pointer between two icons and assert continuous symmetric scaling, outward neighbor displacement, non-overlap, and resting-slot hit testing.
- [ ] Run `cargo test -p shell-renderer --test dock_scene continuous` and confirm the new tests fail because `DockScene` has no horizontal hover position.
- [ ] Add `hover_position_x: Option<f32>` plus `with_hover_position_x` to `DockScene`.
- [ ] Replace discrete index sizes with the raised-cosine kernel and lay visual widths sequentially around the resting group center.
- [ ] Keep a separate resting `hit_bounds` per laid-out item and use it from `DockLayout::hit_test`.
- [ ] Run `cargo test -p shell-renderer --test dock_scene` and confirm all renderer geometry tests pass.

### Task 2: Implement deterministic spring state

**Files:**
- Modify: `crates/shell-platform-windows/src/dock_types.rs`
- Modify: `crates/shell-platform-windows/src/dock_controller.rs`
- Modify: `crates/shell-platform-windows/src/dock_controller_interaction.rs`
- Modify: `crates/shell-platform-windows/tests/dock_controller_interactions.rs`

- [ ] Add failing controller tests for first-entry pointer snapping, intermediate eased strength, convergence to target, retargeted velocity continuity, and exit convergence to rest.
- [ ] Run `cargo test -p shell-platform-windows --test dock_controller_interactions magnification` and confirm the motion APIs are absent.
- [ ] Replace the duration-only `DockAnimator` with two critically damped scalar springs using clamped deltas and fixed integration substeps.
- [ ] Retarget the animator on pointer move/exit, expose `advance_animation` and `snap_animation_to_target`, and increment visual generation only when the visible values change.
- [ ] Publish animated position/strength through `DockController::scene` while preserving the logical hovered item for preview and accessibility behavior.
- [ ] Run the interaction tests and confirm the spring contract passes.

### Task 3: Drive animation frames only while active

**Files:**
- Modify: `crates/shell-platform-windows/src/lib.rs`
- Modify: `crates/shell-platform-windows/src/runtime.rs`
- Modify: `crates/shell-platform-windows/src/win32.rs`
- Modify: `crates/shell-platform-windows/src/win32_timer.rs`
- Modify: `crates/shell-platform-windows/src/win32_windowing.rs`
- Modify: `crates/shell-platform-windows/src/win32_owner.rs`

- [ ] Add `PlatformEvent::DockAnimationFrame` and classify it as a local visual event.
- [ ] Add a dock animation timer ID and an eight-millisecond `TimerGuard` constructor without weakening the existing QA/sync timer bounds.
- [ ] Route dock `WM_TIMER` messages to the owning monitor slot.
- [ ] Store the optional timer and last `Instant` in `RuntimeSurfaces`; start them after pointer retargeting and drop them immediately after the animator settles.
- [ ] On each frame, clamp elapsed time, advance the controller, and use the existing dock redraw path without rebuilding other surfaces.
- [ ] In reduced-motion mode, snap once and keep the timer absent.
- [ ] Run `cargo test -p shell-platform-windows --all-features` and confirm event-routing and runtime tests pass.

### Task 4: Animate the dock material without moving its window

**Files:**
- Modify: `crates/shell-renderer/src/dock_scene.rs`
- Modify: `crates/shell-renderer/src/native_showcase.rs`
- Modify: `crates/shell-renderer/src/native_showcase_material.rs`
- Modify: `crates/shell-renderer/src/native_showcase_resources.rs`
- Modify: `crates/shell-platform-windows/src/dock_types.rs`

- [ ] Add a slower critically damped material-strength spring driven by the same pointer target.
- [ ] Keep the dock HWND and logical hit regions fixed while the internal material lifts two DIP and expands six DIP.
- [ ] Move a bounded reflected-light band toward the animated pointer.
- [ ] Scale the existing inset-shadow bitmap into the animated material bounds.
- [ ] Sanitize non-finite targets and ignore inactive animation-frame messages so a malformed event cannot keep the timer alive.
- [ ] Run the renderer material and platform interaction tests.

### Task 5: Verify native behavior

**Files:**
- Modify only when an observed defect requires it: `.omo/evidence/task-6-multimonitor/task-6-native-qa.ps1`

- [ ] Run `cargo fmt --all -- --check`.
- [ ] Run `cargo test -p shell-renderer --all-features` and `cargo test -p shell-platform-windows --all-features`.
- [ ] Run `cargo clippy --workspace --all-targets --all-features -- -D warnings`.
- [ ] Run `cargo build --release -p shell-app --all-features`.
- [ ] Launch the native shell, move across multiple dock icons, exit the dock, and observe continuous scaling, sideways displacement, smooth return, stable clicks, and no topbar flicker.
- [ ] Repeat in reduced-motion mode and observe immediate target states with no ongoing animation frames.

## Plan Self-Review

- Spec coverage: continuous distance response, pushed neighbors, temporal smoothing, stable hit testing, conditional scheduling, reduced motion, and native QA each map to a task.
- Placeholder scan: no deferred implementation markers or ambiguous error-handling steps remain.
- Type consistency: `DockScene`, `DockAnimator`, `DockController`, `PlatformEvent`, `TimerGuard`, and `RuntimeSurfaces` are the existing integration seams used throughout the plan.
