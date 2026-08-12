# Lightweight Liquid Glass Dock Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an opt-in `--liquid-glass` dock material prototype that reuses the
existing Direct2D/DirectComposition renderer and stays lightweight on hardware,
WARP, safe-mode, high-contrast, and reduced-motion paths.

**Architecture:** Carry one immutable private feature flag from `shell-app`
through the Windows Slot Runtime into `CompositionRenderer`. The renderer owns
a small cached dock overlay and specular bitmap created only when a dock surface
is created or resized. Per-frame work is limited to one or two `DrawBitmap`
calls driven by the existing hover position and material strength.

**Tech Stack:** Rust 2024, Win32, Direct2D, DirectComposition, DXGI flip-model
swap chains, Cargo tests, Clippy, rustfmt.

**Approved specification:**
[Lightweight Liquid Glass Dock Design](../specs/2026-07-16-lightweight-liquid-glass-dock-design.md)

**Repository note:** The worktree already contains uncommitted priority-one UI
work. Do not stage or commit during this plan unless the user explicitly
reviews the full inventory and authorizes it.

---

## File structure

- Modify `crates/shell-app/src/lib.rs`: parse and store the opt-in flag.
- Modify `crates/shell-app/src/main.rs`: map the flag into native launch
  configuration and suppress it in safe/high-contrast modes.
- Modify `crates/shell-app/tests/showcase_args.rs`: cover default-off and
  explicit opt-in argument behavior.
- Create `crates/shell-renderer/src/native_liquid_glass.rs`: own pure mode
  selection, deterministic raster generation, cached Direct2D bitmaps, and
  drawing policy.
- Modify `crates/shell-renderer/src/lib.rs`: register the renderer module.
- Modify `crates/shell-renderer/src/native.rs`: carry immutable renderer
  options, create/recreate dock Liquid Glass resources, and expose them to the
  showcase drawing path.
- Modify `crates/shell-renderer/src/native/resize.rs`: rebuild cached Liquid
  Glass bitmaps only after a dock resize or DPI change.
- Modify `crates/shell-renderer/src/native_showcase.rs`: pass hover position and
  material strength into the cached overlay draw.
- Modify `crates/shell-renderer/src/native_showcase_material.rs`: composite the
  cached overlay between the base material and icons without changing bounds.
- Modify `crates/shell-platform-windows/src/win32.rs`: add the native run flag
  and derive per-slot capability.
- Modify `crates/shell-platform-windows/src/win32_slots.rs`: carry the feature
  in `SlotFeatures`.
- Modify `crates/shell-platform-windows/src/win32_slot_lifecycle.rs`: pass the
  feature into runtime construction.
- Modify `crates/shell-platform-windows/src/win32_owner.rs`: add the feature and
  reduced-motion values to native surface options.
- Modify `crates/shell-platform-windows/src/win32_surface_runtime.rs`: extend
  private renderer construction options and recording tests.
- Modify `README.md` and `SUPPORT.md`: document the experimental switch,
  fallbacks, and performance diagnostics.

### Task 1: Parse and propagate the opt-in launch flag

**Files:**

- Modify: `crates/shell-app/src/lib.rs`
- Modify: `crates/shell-app/src/main.rs`
- Modify: `crates/shell-app/tests/showcase_args.rs`
- Modify: `crates/shell-platform-windows/src/win32.rs`
- Modify: `crates/shell-platform-windows/src/win32_slots.rs`
- Modify: `crates/shell-platform-windows/src/win32_slot_lifecycle.rs`
- Modify: `crates/shell-platform-windows/src/win32_owner.rs`
- Modify: `crates/shell-platform-windows/src/win32_surface_runtime.rs`

- [x] **Step 1: Write failing parser and launch-mapping tests**

Add to `crates/shell-app/tests/showcase_args.rs`:

```rust
#[test]
fn liquid_glass_is_disabled_by_default_and_requires_explicit_opt_in() {
    assert!(!parse_args(["shell-app.exe"]).liquid_glass);
    assert!(parse_args(["shell-app.exe", "--liquid-glass"]).liquid_glass);
}
```

Add to the `main.rs` unit tests:

```rust
#[test]
fn safe_and_high_contrast_modes_suppress_liquid_glass() {
    let safe = showcase_run_config(&parse_args([
        "shell-app.exe",
        "--liquid-glass",
        "--safe-mode",
    ]))
    .expect("safe config should be valid");
    let contrast = showcase_run_config(&parse_args([
        "shell-app.exe",
        "--liquid-glass",
        "--high-contrast",
    ]))
    .expect("contrast config should be valid");

    assert!(!safe.liquid_glass);
    assert!(!contrast.liquid_glass);
}
```

- [x] **Step 2: Run tests and verify RED**

Run:

```powershell
cargo test -p shell-app liquid_glass
```

Expected: compilation fails because `AppConfig` and `ShowcaseRunConfig` do not
yet contain `liquid_glass`.

- [x] **Step 3: Add the immutable private flag**

Add `pub liquid_glass: bool` to `AppConfig`, initialize it to `false`, parse
`--liquid-glass` in both argument parsers, and return it in both constructed
configs.

Add `pub liquid_glass: bool` to `ShowcaseRunConfig` and map it in
`showcase_run_config`:

```rust
liquid_glass: config.liquid_glass && !config.safe_mode && !config.high_contrast,
```

Carry the field through:

```rust
SlotFeatures {
    liquid_glass: config.liquid_glass,
    // existing fields
}
```

```rust
RuntimeOptions {
    liquid_glass: features.liquid_glass,
    // existing fields
}
```

```rust
NativeSurfaceOptions {
    liquid_glass: options.liquid_glass,
    reduced_motion: options.reduced_motion,
    // existing fields
}
```

- [x] **Step 4: Run focused tests and checks**

Run:

```powershell
cargo test -p shell-app liquid_glass
cargo check -p shell-platform-windows --all-targets
```

Expected: parser tests pass and platform construction compiles with the new
private fields.

- [x] **Step 5: Checkpoint without commit**

Run `git diff --check`. Do not stage or commit because the worktree inventory
has not been approved for a combined commit.

### Task 2: Define deterministic runtime degradation policy

**Files:**

- Create: `crates/shell-renderer/src/native_liquid_glass.rs`
- Modify: `crates/shell-renderer/src/lib.rs`

- [x] **Step 1: Write failing mode-selection tests**

Create the module with tests that reference the wished-for API:

```rust
#[cfg(test)]
mod tests {
    use crate::native::DeviceKind;

    use super::{LiquidGlassMode, liquid_glass_mode};

    #[test]
    fn liquid_glass_selects_dynamic_static_and_disabled_modes() {
        assert_eq!(
            liquid_glass_mode(false, false, DeviceKind::Hardware, false),
            LiquidGlassMode::Disabled
        );
        assert_eq!(
            liquid_glass_mode(true, false, DeviceKind::Hardware, false),
            LiquidGlassMode::Dynamic
        );
        assert_eq!(
            liquid_glass_mode(true, false, DeviceKind::Hardware, true),
            LiquidGlassMode::Static
        );
        assert_eq!(
            liquid_glass_mode(true, false, DeviceKind::Warp, false),
            LiquidGlassMode::Static
        );
        assert_eq!(
            liquid_glass_mode(true, true, DeviceKind::Hardware, false),
            LiquidGlassMode::Disabled
        );
    }
}
```

- [x] **Step 2: Run the test and verify RED**

Run:

```powershell
cargo test -p shell-renderer liquid_glass_selects_dynamic_static_and_disabled_modes --lib
```

Expected: failure because `LiquidGlassMode` and `liquid_glass_mode` are not
defined.

- [x] **Step 3: Implement the minimal pure policy**

Add:

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LiquidGlassMode {
    Disabled,
    Static,
    Dynamic,
}

pub(crate) const fn liquid_glass_mode(
    requested: bool,
    solid_material: bool,
    device_kind: DeviceKind,
    reduced_motion: bool,
) -> LiquidGlassMode {
    if !requested || solid_material {
        LiquidGlassMode::Disabled
    } else if reduced_motion || matches!(device_kind, DeviceKind::Warp) {
        LiquidGlassMode::Static
    } else {
        LiquidGlassMode::Dynamic
    }
}
```

- [x] **Step 4: Run the focused test and verify GREEN**

Run:

```powershell
cargo test -p shell-renderer liquid_glass_selects_dynamic_static_and_disabled_modes --lib
```

Expected: one passing test.

### Task 3: Build cached lightweight glass resources

**Files:**

- Modify: `crates/shell-renderer/src/native_liquid_glass.rs`
- Modify: `crates/shell-renderer/src/native.rs`
- Modify: `crates/shell-renderer/src/native/resize.rs`

- [x] **Step 1: Write failing raster and geometry tests**

Add tests for a pure raster API:

```rust
#[test]
fn overlay_raster_is_bounded_premultiplied_and_nonempty() {
    let raster = LiquidGlassRaster::new(513.0, 53.0, 19.0);

    assert_eq!(raster.stride(), raster.width() * 4);
    assert_eq!(
        raster.pixels().len(),
        (raster.width() * raster.height() * 4) as usize
    );
    assert!(raster.pixels().chunks_exact(4).any(|pixel| pixel[3] > 0));
    assert!(raster.pixels().chunks_exact(4).all(|pixel| {
        pixel[0] <= pixel[3] && pixel[1] <= pixel[3] && pixel[2] <= pixel[3]
    }));
}

#[test]
fn dynamic_specular_position_clamps_inside_material_bounds() {
    let bounds = DipRect::new(10.0, 2.0, 500.0, 53.0);

    assert_eq!(specular_left(bounds, Some(-100.0), 96.0), 10.0);
    assert_eq!(specular_left(bounds, Some(900.0), 96.0), 414.0);
    assert_eq!(specular_left(bounds, None, 96.0), 212.0);
}
```

- [x] **Step 2: Run tests and verify RED**

Run:

```powershell
cargo test -p shell-renderer liquid_glass --lib
```

Expected: missing raster/resource APIs.

- [x] **Step 3: Implement deterministic premultiplied rasters**

Implement:

```rust
struct LiquidGlassRaster {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}
```

Generate a transparent BGRA overlay containing only:

- a white inner rim;
- low-alpha cool pixels on the left edge;
- low-alpha warm pixels on the right edge;
- a narrow upper-left highlight.

Use a rounded-rectangle signed-distance calculation and premultiply every color
channel by alpha before storing it. Clamp dimensions to at least one pixel and
sanitize non-finite inputs.

Generate a second narrow specular raster of fixed logical width `96 DIP`.

- [x] **Step 4: Wrap both rasters in cached Direct2D bitmaps**

Add:

```rust
pub(crate) struct DockLiquidGlassResources {
    overlay: ID2D1Bitmap1,
    specular: ID2D1Bitmap1,
    specular_width: f32,
    mode: LiquidGlassMode,
}
```

Create the resources through `ID2D1DeviceContext::CreateBitmap` with
premultiplied `DXGI_FORMAT_B8G8R8A8_UNORM`. Provide:

```rust
pub(crate) fn create(
    context: &ID2D1DeviceContext,
    width: f32,
    height: f32,
    radius: f32,
    mode: LiquidGlassMode,
) -> Result<Option<Self>>
```

Return `None` for `Disabled`.

- [x] **Step 5: Store and rebuild resources only with the dock surface**

Add `dock_liquid_glass: Option<DockLiquidGlassResources>` to `WindowSurface`.
Create it in `CompositionRenderer::create_surface` after mode selection and
recreate it in `resize_surface` only when the role is `Dock`.

Do not allocate these resources in `redraw_surface` or `draw_showcase`.

- [x] **Step 6: Run renderer tests and verify GREEN**

Run:

```powershell
cargo test -p shell-renderer liquid_glass --lib
cargo test -p shell-renderer native::tests --lib
```

Expected: pure raster tests and existing native resource tests pass.

### Task 4: Composite the cached overlay during dock redraw

**Files:**

- Modify: `crates/shell-renderer/src/native_liquid_glass.rs`
- Modify: `crates/shell-renderer/src/native.rs`
- Modify: `crates/shell-renderer/src/native_showcase.rs`
- Modify: `crates/shell-renderer/src/native_showcase_material.rs`

- [x] **Step 1: Write failing drawing-policy tests**

Add a pure presentation calculation:

```rust
#[derive(Clone, Copy, Debug, PartialEq)]
struct LiquidGlassPresentation {
    overlay_opacity: f32,
    specular_opacity: f32,
    specular_left: f32,
}
```

Test:

```rust
#[test]
fn static_mode_ignores_hover_and_dynamic_mode_tracks_it() {
    let bounds = DipRect::new(0.0, 2.0, 500.0, 53.0);
    let static_frame =
        presentation(LiquidGlassMode::Static, bounds, Some(420.0), 1.0, 96.0);
    let dynamic_frame =
        presentation(LiquidGlassMode::Dynamic, bounds, Some(420.0), 1.0, 96.0);

    assert_eq!(static_frame.specular_opacity, 0.0);
    assert!(dynamic_frame.specular_opacity > 0.0);
    assert!(dynamic_frame.specular_left > bounds.x + bounds.width / 2.0);
}

#[test]
fn liquid_glass_never_changes_material_bounds() {
    let bounds = DipRect::new(3.0, 2.0, 507.0, 53.0);
    let frame = presentation(
        LiquidGlassMode::Dynamic,
        bounds,
        Some(250.0),
        1.0,
        96.0,
    );

    assert_eq!(frame.overlay_bounds, bounds);
}
```

- [x] **Step 2: Run tests and verify RED**

Run:

```powershell
cargo test -p shell-renderer static_mode_ignores_hover_and_dynamic_mode_tracks_it --lib
```

Expected: missing presentation policy.

- [x] **Step 3: Implement bounded drawing policy**

Clamp material strength to `0..=1`. Use:

```rust
overlay_opacity = 0.72 + 0.18 * strength;
specular_opacity = if mode == LiquidGlassMode::Dynamic {
    0.12 + 0.28 * strength
} else {
    0.0
};
```

For dynamic mode, position the narrow specular bitmap around
`DockScene::hover_position_x()`, clamped inside the dock material bounds. For
static mode, draw only the full cached overlay.

- [x] **Step 4: Expose resources through `ShowcaseStyle`**

Extend `ShowcaseStyle` with:

```rust
dock_liquid_glass: Option<&DockLiquidGlassResources>,
```

Pass `surface.dock_liquid_glass.as_ref()` from create, redraw, and resize paths.

In `draw_showcase`, read:

```rust
let hover_position_x = scenes.dock.and_then(DockScene::hover_position_x);
let motion_strength = scenes
    .dock
    .map_or(0.0, DockScene::material_motion_strength);
```

Pass both to `MaterialSurface`.

- [x] **Step 5: Composite before existing inset depth**

In `draw_dock_material`, keep the current bounds and base layers. Draw the
cached Liquid Glass overlay after `luminance`, `veil`, and `reflection`, but
before `dock_inset.draw_at`. Icons remain drawn later by the unchanged dock
scene path.

- [x] **Step 6: Run renderer regression tests**

Run:

```powershell
cargo test -p shell-renderer dock_material --lib
cargo test -p shell-renderer dock_scene
cargo check -p shell-renderer --all-targets
```

Expected: existing material bounds and dock scene tests remain green, plus the
new presentation tests pass.

### Task 5: Document, verify, measure, and build the experiment

**Files:**

- Modify: `README.md`
- Modify: `SUPPORT.md`
- Modify: `docs/superpowers/plans/2026-07-16-lightweight-liquid-glass-dock.md`

- [x] **Step 1: Document the experimental flag**

Add a concise README example:

```powershell
.\shell-app.exe --liquid-glass
```

State that the flag is disabled by default, affects only the dock, and is
suppressed in safe mode and high contrast. In `SUPPORT.md`, document
`MINHA_UI_LOG_MODULES=dock.performance` for the A/B comparison.

- [x] **Step 2: Run formatting, architecture, Clippy, and tests**

Run:

```powershell
cargo fmt --all --check
powershell -NoProfile -File scripts/Check-Architecture.ps1
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

Expected: all commands exit successfully with zero warnings or test failures.

- [x] **Step 3: Build the release executable**

Run:

```powershell
cargo build --workspace --release
```

Expected executable:

```text
target/x86_64-pc-windows-msvc/release/shell-app.exe
```

- [x] **Step 4: Run controlled native smoke tests**

Run each with a bounded exit:

```powershell
.\target\x86_64-pc-windows-msvc\release\shell-app.exe --qa-exit-ms 3000
.\target\x86_64-pc-windows-msvc\release\shell-app.exe --qa-exit-ms 3000 --liquid-glass
.\target\x86_64-pc-windows-msvc\release\shell-app.exe --qa-exit-ms 3000 --liquid-glass --force-warp
.\target\x86_64-pc-windows-msvc\release\shell-app.exe --qa-exit-ms 3000 --liquid-glass --safe-mode
```

Expected:

- default: current material;
- opt-in hardware: Liquid Glass dynamic mode;
- WARP: Liquid Glass static mode;
- safe mode: solid material with Liquid Glass disabled.

- [x] **Step 5: Measure idle A/B**

For each path, run the release app long enough to collect ten one-second
samples of:

- working set;
- private memory;
- CPU percentage;
- thread count;
- handle count.

Use the same two-monitor layout and no user interaction. Accept idle CPU only
when it remains equivalent to the disabled path and working-set growth is at
most 3 MB.

- [x] **Step 6: Measure redraw A/B**

Set:

```powershell
$env:MINHA_UI_LOG_MODULES = "dock.performance"
```

Run the disabled and enabled paths with the same hover sequence. Compare
`dock.redraw.slow` values for `scene_us`, `present_us`, and total duration.
Reject or simplify the effect if it introduces sustained redraws over 16 ms.

- [x] **Step 7: Inspect the rendered dock**

Check both monitors at rest and hover for:

- visible but restrained convex depth;
- no tint over icons;
- unchanged bounds and hit targets;
- no clipping during autohide;
- static behavior in WARP/reduced motion;
- solid fallback in safe mode.

- [x] **Step 8: Record evidence and provide the executable**

Mark this plan’s completed checkboxes, report exact A/B values, calculate the
release SHA-256, and provide the executable path. Do not claim the visual or
performance criteria passed without fresh command output and rendered
inspection.

## Completion evidence

- Final gates: `cargo fmt --all --check`, architecture policy, workspace Clippy
  with warnings denied, and `cargo test --workspace` all passed.
- Native smoke tests passed for disabled, hardware opt-in, WARP, and safe-mode
  suppression paths.
- Two static idle A/B pairs averaged 83.18 MB working set without the effect
  and 83.95 MB with it, a 0.78 MB increase. Private memory increased by
  0.81 MB. Combined sampled CPU time differed by 0.0469 seconds.
- The first 18-sample pair showed no additional steady-state threads or
  handles in the enabled path.
- An identical hardware hover sequence produced zero diagnostic redraws above
  16 ms in both the disabled and enabled paths.
- Rendered inspection confirmed unchanged 448 x 55 dock bounds, restrained
  edge depth, readable untinted icons, and the expected static reduced-motion
  presentation.
- Release executable:
  `target-liquid-glass/x86_64-pc-windows-msvc/release/shell-app.exe`
- SHA-256:
  `FEED677461D0939D9312D55397A5139C7985E95D53B5ECF466DF179FCFA98B74`
