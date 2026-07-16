# Responsive Liquid Glass Shell Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the approved Figma-derived compact Liquid Glass dock and matching topbar in the native Rust/Direct2D shell.

**Architecture:** Keep geometry and material tokens in `shell-renderer`, keep Windows placement and capability behavior in `shell-platform-windows`, and leave application identity/state in `shell-core`. Render the effect as bounded translucent layers on the existing DirectComposition surface, with the current solid/high-contrast paths remaining valid.

**Tech Stack:** Rust 2024, windows-rs, Direct2D, DirectWrite, DirectComposition, WIC, Cargo tests and Clippy.

---

### Task 1: Lock the Figma geometry in tests

**Files:**
- Modify: `crates/shell-renderer/tests/dock_scene.rs`
- Modify: `crates/shell-renderer/tests/native_slice.rs`

- [ ] Add a default-layout test asserting 36 DIP app slots, 9 DIP gaps, 8 DIP padding, and intrinsic width.
- [ ] Update physical-placement assertions to require a 55 DIP revealed dock and 4 DIP work-area offset.
- [ ] Run `cargo test -p shell-renderer --tests --all-features` and confirm the old 52/8/14 and 88/16 defaults fail the new assertions.

### Task 2: Implement compact responsive geometry

**Files:**
- Modify: `crates/shell-renderer/src/dock_scene.rs`
- Modify: `crates/shell-renderer/src/dock_layout.rs`
- Modify: `crates/shell-renderer/src/geometry.rs`
- Modify: `crates/shell-renderer/src/native_showcase_dock.rs`

- [ ] Change `DockLayoutConfig::new` defaults to item size `36.0`, spacing `9.0`, padding `8.0`, and magnified size `43.92`.
- [ ] Anchor resting icon slots at the eight-DIP top inset while allowing magnified neighbors to grow without changing the host width.
- [ ] Change `ShellMetrics` dock height to `55.0` and bottom margin to `4.0`.
- [ ] Draw runtime icons across the full 36 DIP slot; keep Windows-declared transparent padding intact.
- [ ] Place the running indicator in the three-DIP gap below the icon and keep the attention badge inside the shell rim.
- [ ] Run the renderer tests and confirm all layout and hit-testing assertions pass.

### Task 3: Encode the layered glass material

**Files:**
- Modify: `crates/shell-renderer/src/showcase_model.rs`
- Modify: `crates/shell-renderer/src/native_showcase.rs`
- Modify: `crates/shell-renderer/src/native_showcase_resources.rs`
- Modify: `crates/shell-renderer/tests/native_slice.rs`

- [ ] Add semantic dock/topbar material tokens for luminance tint, dark veil, reflected light, lower depth, outer rim, and inner highlight.
- [ ] Add failing token assertions for the approved 15 DIP dock radius and the new semantic layers.
- [ ] Render the dock in ordered layers: transparent clear, outer separation, luminance tint, dark veil, reflected-light wash, top inner rim, lower depth seam.
- [ ] Render the topbar from the same material family with lower reflection and only a bottom separator.
- [ ] Keep popover/settings rendering on their current raised material so this iteration remains scoped.
- [ ] Run `cargo test -p shell-renderer --all-features` and confirm the material contract passes.

### Task 4: Align topbar geometry and visual states

**Files:**
- Modify: `crates/shell-renderer/src/topbar_layout.rs`
- Modify: `crates/shell-renderer/src/native_showcase_topbar.rs`
- Modify: `crates/shell-renderer/tests/topbar_scene.rs`

- [ ] Assert 10 DIP leading and 20 DIP trailing padding on wide surfaces, a 24 DIP content band, and single-line overflow behavior.
- [ ] Keep 13 DIP Segoe UI Variable Semibold with shared vertical centering for text and platform glyphs.
- [ ] Replace the oversized focus fill with a four-DIP-radius, low-opacity active surface consistent with the Figma menu-item treatment.
- [ ] Keep network throughput compact and prevent status values from wrapping.
- [ ] Run topbar renderer and controller tests.

### Task 5: Verify integration and runtime fallbacks

**Files:**
- Modify only if required by observed runtime behavior: `crates/shell-platform-windows/src/win32_window.rs`
- Modify only if required by observed runtime behavior: `crates/shell-platform-windows/src/win32_owner.rs`
- Modify: `.omo/evidence/task-6-multimonitor/task-6-native-qa.ps1`

- [ ] Build with `cargo build --release -p shell-app --all-features`.
- [ ] Run the native QA harness with WARP and capture revealed dock, hidden reveal strip, and both topbars.
- [ ] Confirm the app starts with real Windows icons, intrinsic dock width, 55 DIP revealed height, 8-pixel hidden strip, and topbars at monitor `y=0`.
- [ ] Inspect full and safe-mode/solid rendering; correct any unreadable alpha or black-surface regression.
- [ ] Run `cargo fmt --all -- --check`, workspace Clippy with `-D warnings`, and `cargo test --workspace --all-features`.
- [ ] Dispatch two independent read-only visual reviews against fresh captures and repeat capture/fix until no blocking findings remain.

## Plan Self-Review

- Spec coverage: dock geometry, intrinsic width, layered material, topbar, native icons, auto-hide, DPI, multi-monitor, fallback, and manual QA are each covered.
- Placeholder scan: no deferred implementation markers or ambiguous code steps remain.
- Type consistency: existing `DockLayoutConfig`, `ShellMetrics`, `ShowcaseTokens`, and native scene APIs remain the integration seams; no new domain state is introduced.
