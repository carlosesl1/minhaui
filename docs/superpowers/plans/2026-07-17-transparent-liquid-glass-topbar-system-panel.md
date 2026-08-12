# Transparent Liquid Glass Top Bar and System Panel Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the native top bar visually transparent, give its open module a balanced violet liquid-glass state, and refine the existing seven-action `Minha UI` system panel with grouped hierarchy and the dock's shared material language.

**Architecture:** Extend the existing renderer-owned liquid-glass resource into explicit dock, panel, and active-module profiles. Keep controller actions and Windows adapters unchanged while enriching renderer scenes with presentation-only group boundaries, icons, active state, panel size, and anchor information. Use cached Direct2D resources, existing DirectComposition entrance animation, and deterministic solid/static fallbacks without desktop capture or a new timer.

**Tech Stack:** Rust 2024, Win32, Direct2D, DirectWrite, DirectComposition, DXGI flip-model swap chains, Cargo tests, Clippy, rustfmt, PowerShell architecture checks.

**Approved specification:** [Transparent Liquid Glass Top Bar and System Panel Design](../specs/2026-07-17-transparent-liquid-glass-topbar-system-panel-design.md)

**Repository safety:** The current worktree contains uncommitted prerequisite work in most files named below. Before execution, record `git status --short` and preserve those changes. Do not stage or commit a task when its files also contain unreviewed pre-existing edits. Use task checkpoints in that case and commit only after the user approves the combined inventory.

---

## File structure

- Modify `crates/shell-renderer/src/popover_scene.rs`: add system-panel layout style, semantic icon glyph, section boundaries, and local anchor position.
- Modify `crates/shell-renderer/src/popover_layout.rs`: own system-panel title/row/separator geometry, 288-DIP surface sizing, scrolling, placement, and anchor-notch position.
- Modify `crates/shell-renderer/src/lib.rs`: export the new scene and layout types.
- Modify `crates/shell-renderer/tests/popover_scene.rs`: verify grouped system layout, compact-panel regression, scaling, and hit testing.
- Modify `crates/shell-platform-windows/src/popover_types.rs`: carry presentation-only glyph and section-start metadata with existing actions.
- Modify `crates/shell-platform-windows/src/popover_adapters.rs`: preserve and group the seven existing system actions.
- Modify `crates/shell-platform-windows/src/popover_controller.rs`: map system item metadata into the renderer scene and use the `Minha UI` title.
- Modify `crates/shell-platform-windows/tests/popover_controller.rs`: verify action inventory, group boundaries, keyboard order, confirmations, and placement.
- Modify `crates/shell-platform-windows/src/win32_window.rs`: store scene-driven popover size and local anchor position.
- Modify `crates/shell-platform-windows/src/win32_popover_render.rs`: place from the complete scene, synchronize the active top-bar module, and pass the local anchor into rendering.
- Modify `crates/shell-platform-windows/src/win32_owner.rs`: use scene-driven placement after asynchronous popover loads.
- Modify `crates/shell-renderer/src/native_liquid_glass.rs`: generalize dock-only resources into cached surface profiles.
- Modify `crates/shell-renderer/src/native.rs`: own one profile-specific glass resource per applicable surface.
- Modify `crates/shell-renderer/src/native/resize.rs`: recreate the cached resource only on resize/DPI change.
- Modify `crates/shell-renderer/src/native_showcase.rs`: supply shared material resources and new brushes/formats.
- Modify `crates/shell-renderer/src/native_showcase_material.rs`: draw transparent top-bar, dock, and panel material plans.
- Modify `crates/shell-renderer/src/native_showcase_topbar.rs`: draw protected text/icons and pressed/active glass capsules.
- Modify `crates/shell-renderer/src/native_showcase_popover.rs`: draw the title, notch, dividers, icon wells, rows, shortcuts, and focus state.
- Modify `crates/shell-renderer/src/native_showcase_primitives.rs`: add a bounded triangle/notch primitive.
- Modify `crates/shell-renderer/src/native_showcase_resources.rs`: add the system-panel title text format.
- Modify `crates/shell-renderer/src/topbar_scene.rs`: distinguish keyboard focus, pointer press, and open-module state.
- Modify `crates/shell-renderer/src/topbar_layout.rs`: expose pressed and active state on laid-out items without changing hit bounds.
- Modify `crates/shell-renderer/tests/topbar_scene.rs`: test active-state geometry and existing overflow behavior.
- Modify `crates/shell-platform-windows/src/topbar_controller.rs`: emit immediate press redraws and expose the active module to the scene.
- Modify `crates/shell-platform-windows/tests/topbar_controller.rs`: verify press/open/dismiss visual state transitions.
- Modify `README.md`: describe the expanded `--liquid-glass` scope and fallbacks.
- Modify `SUPPORT.md`: document the idle/redraw acceptance checks for the top bar and system panel.

## Task 1: Model grouped system-panel layout without changing compact panels

**Files:**

- Modify: `crates/shell-renderer/src/popover_scene.rs`
- Modify: `crates/shell-renderer/src/popover_layout.rs`
- Modify: `crates/shell-renderer/src/lib.rs`
- Test: `crates/shell-renderer/tests/popover_scene.rs`

- [ ] **Step 1: Write failing scene and layout tests**

Append these tests to `crates/shell-renderer/tests/popover_scene.rs`:

```rust
use crate::{
    PopoverLayoutStyle, PopoverSurfaceSize, popover_surface_size,
};

#[test]
fn system_panel_has_title_spacing_group_dividers_and_288_dip_width() {
    let rows = vec![
        PopoverRow::new("Settings", "Ctrl+,", true).with_icon_glyph("\u{E713}"),
        PopoverRow::new("Task Manager", "", true).with_icon_glyph("\u{E9D9}"),
        PopoverRow::new("Lock", "Win+L", true)
            .with_icon_glyph("\u{E72E}")
            .with_section_start(),
        PopoverRow::new("Sleep", "Requires confirmation", true)
            .with_icon_glyph("\u{E708}"),
        PopoverRow::new("Sign out", "Requires confirmation", true)
            .with_icon_glyph("\u{E8AC}"),
        PopoverRow::new("Restart", "Requires confirmation", true)
            .with_icon_glyph("\u{E777}")
            .with_section_start(),
        PopoverRow::new("Shut down", "Requires confirmation", true)
            .with_icon_glyph("\u{E7E8}"),
    ];
    let scene = PopoverScene::new(
        Popover::SystemMenu,
        "Minha UI",
        PopoverContentState::Ready,
        rows,
        Some(0),
    )
    .with_layout_style(PopoverLayoutStyle::SystemPanel);
    let size = popover_surface_size(&scene);
    let layout = layout_popover_scene(
        &scene,
        DipRect::new(0.0, 0.0, size.width(), size.height()),
    );

    assert_eq!(size, PopoverSurfaceSize::new(288.0, 372.0));
    assert_eq!(layout.title_bounds(), Some(DipRect::new(16.0, 20.0, 256.0, 28.0)));
    assert_eq!(layout.rows().len(), 7);
    assert!(layout.rows().iter().all(|row| row.bounds().height == 40.0));
    assert_eq!(layout.separators().len(), 2);
    assert!(layout.rows()[2].bounds().y > layout.rows()[1].bounds().y + 40.0);
    assert!(layout.rows()[5].bounds().y > layout.rows()[4].bounds().y + 40.0);
}

#[test]
fn compact_popover_keeps_existing_244_dip_width_and_24_dip_rows() {
    let scene = PopoverScene::new(
        Popover::Network,
        "Network",
        PopoverContentState::Ready,
        vec![PopoverRow::new("Online", "", true)],
        Some(0),
    );
    let size = popover_surface_size(&scene);
    let layout = layout_popover_scene(
        &scene,
        DipRect::new(0.0, 0.0, size.width(), size.height()),
    );

    assert_eq!(size.width(), 244.0);
    assert_eq!(layout.rows()[0].bounds().height, 24.0);
    assert_eq!(layout.title_bounds(), None);
}
```

- [ ] **Step 2: Run the focused test and verify RED**

Run:

```powershell
cargo test -p shell-renderer popover_scene
```

Expected: compilation fails because `PopoverLayoutStyle`, `PopoverSurfaceSize`, section metadata, icon glyphs, and the new layout accessors do not exist.

- [ ] **Step 3: Add the presentation-only scene API**

Add to `popover_scene.rs`:

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PopoverLayoutStyle {
    Compact,
    SystemPanel,
}

pub struct PopoverRow {
    label: String,
    detail: String,
    icon_source: Option<String>,
    icon_glyph: Option<String>,
    enabled: bool,
    section_start: bool,
}

impl PopoverRow {
    #[must_use]
    pub fn with_icon_glyph(mut self, glyph: &str) -> Self {
        self.icon_glyph = Some(glyph.to_owned());
        self
    }

    #[must_use]
    pub const fn with_section_start(mut self) -> Self {
        self.section_start = true;
        self
    }

    #[must_use]
    pub fn icon_glyph(&self) -> Option<&str> {
        self.icon_glyph.as_deref()
    }

    #[must_use]
    pub const fn section_start(&self) -> bool {
        self.section_start
    }
}
```

Initialize `icon_glyph` to `None` and `section_start` to `false` in `PopoverRow::new`. Add `layout_style: PopoverLayoutStyle` and `anchor_x: Option<f32>` to `PopoverScene`; initialize them to `Compact` and `None`, then add complete builders/accessors:

```rust
#[must_use]
pub const fn with_layout_style(mut self, style: PopoverLayoutStyle) -> Self {
    self.layout_style = style;
    self
}

#[must_use]
pub fn with_anchor_x(mut self, anchor_x: f32) -> Self {
    self.anchor_x = anchor_x.is_finite().then_some(anchor_x);
    self
}

#[must_use]
pub const fn layout_style(&self) -> PopoverLayoutStyle {
    self.layout_style
}

#[must_use]
pub const fn anchor_x(&self) -> Option<f32> {
    self.anchor_x
}
```

- [ ] **Step 4: Implement style-specific geometry**

Replace the single hard-coded layout rhythm in `popover_layout.rs` with these public value types and constants:

```rust
const COMPACT_WIDTH: f32 = 244.0;
const SYSTEM_WIDTH: f32 = 288.0;
const SYSTEM_TITLE_Y: f32 = 20.0;
const SYSTEM_TITLE_HEIGHT: f32 = 28.0;
const SYSTEM_ROW_HEIGHT: f32 = 40.0;
const SYSTEM_SECTION_GAP: f32 = 12.0;
const SYSTEM_BODY_TOP: f32 = 60.0;
const SYSTEM_BOTTOM_PADDING: f32 = 8.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PopoverSurfaceSize {
    width: f32,
    height: f32,
}

impl PopoverSurfaceSize {
    #[must_use]
    pub const fn new(width: f32, height: f32) -> Self { Self { width, height } }
    #[must_use]
    pub const fn width(self) -> f32 { self.width }
    #[must_use]
    pub const fn height(self) -> f32 { self.height }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PopoverSeparator { bounds: DipRect }

impl PopoverSeparator {
    #[must_use]
    pub const fn bounds(self) -> DipRect { self.bounds }
}
```

Extend `PopoverLayout` with `title_bounds: Option<DipRect>` and `separators: Vec<PopoverSeparator>`. For `SystemPanel`, begin rows at `52 DIP`, add `12 DIP` before every `section_start` row except index zero, place a `1 DIP` separator centered in that gap, and use `40 DIP` rows. Preserve the current `5/12/24 DIP` compact algorithm unchanged.

Add:

```rust
#[must_use]
pub fn popover_surface_size(scene: &PopoverScene) -> PopoverSurfaceSize {
    match scene.layout_style() {
        PopoverLayoutStyle::Compact => PopoverSurfaceSize::new(
            COMPACT_WIDTH,
            popover_height_for_rows(scene.rows().len()),
        ),
        PopoverLayoutStyle::SystemPanel => {
            let section_count = scene
                .rows()
                .iter()
                .enumerate()
                .filter(|(index, row)| *index > 0 && row.section_start())
                .count() as f32;
            PopoverSurfaceSize::new(
                SYSTEM_WIDTH,
                SYSTEM_BODY_TOP
                    + scene.rows().len() as f32 * SYSTEM_ROW_HEIGHT
                    + section_count * SYSTEM_SECTION_GAP
                    + SYSTEM_BOTTOM_PADDING,
            )
        }
    }
}
```

Export the new types/functions from `lib.rs`.

- [ ] **Step 5: Run renderer tests and checkpoint**

Run:

```powershell
cargo test -p shell-renderer popover_scene
cargo test -p shell-renderer popover --lib
git diff --check
```

Expected: grouped and compact layout tests pass with no whitespace errors. Record a checkpoint; do not stage overlapping pre-existing changes.

## Task 2: Preserve, group, and decorate the seven existing system actions

**Files:**

- Modify: `crates/shell-platform-windows/src/popover_types.rs`
- Modify: `crates/shell-platform-windows/src/popover_adapters.rs`
- Modify: `crates/shell-platform-windows/src/popover_controller.rs`
- Test: `crates/shell-platform-windows/tests/popover_controller.rs`

- [ ] **Step 1: Write failing inventory and grouping tests**

Append to `crates/shell-platform-windows/tests/popover_controller.rs`:

```rust
#[test]
fn system_menu_preserves_existing_actions_and_marks_three_visual_groups()
-> Result<(), Box<dyn std::error::Error>> {
    let mut controller = PopoverController::new();
    controller.open(Popover::SystemMenu, &DefaultPopoverDataProvider::offline())?;
    let scene = controller.scene().ok_or("missing system scene")?;

    assert_eq!(scene.title(), "Minha UI");
    assert_eq!(scene.layout_style(), PopoverLayoutStyle::SystemPanel);
    assert_eq!(
        scene.rows().iter().map(PopoverRow::label).collect::<Vec<_>>(),
        vec![
            "Settings", "Task Manager", "Lock", "Sleep", "Sign out", "Restart",
            "Shut down",
        ]
    );
    assert_eq!(
        scene.rows().iter().map(PopoverRow::section_start).collect::<Vec<_>>(),
        vec![false, false, true, false, false, true, false]
    );
    assert!(scene.rows().iter().all(|row| row.icon_glyph().is_some()));
    Ok(())
}
```

- [ ] **Step 2: Run the focused test and verify RED**

Run:

```powershell
cargo test -p shell-platform-windows system_menu_preserves
```

Expected: compilation fails because platform rows do not carry glyph/group metadata and the scene remains compact with title `System`.

- [ ] **Step 3: Add presentation metadata without touching actions**

Add `icon_glyph: Option<String>` and `section_start: bool` to `PopoverItem`, initialize them in `new`, and add:

```rust
#[must_use]
pub fn with_icon_glyph(mut self, glyph: &str) -> Self {
    self.icon_glyph = Some(glyph.to_owned());
    self
}

#[must_use]
pub const fn with_section_start(mut self) -> Self {
    self.section_start = true;
    self
}

#[must_use]
pub fn icon_glyph(&self) -> Option<&str> { self.icon_glyph.as_deref() }

#[must_use]
pub const fn section_start(&self) -> bool { self.section_start }
```

Keep the existing `action: Option<PopoverAction>` field and every action constructor unchanged.

- [ ] **Step 4: Add semantic glyphs and boundaries to `system_rows()`**

Retain the seven existing rows and actions. Apply these builders:

```rust
PopoverItem::new("Settings", "Ctrl+,", true, Some(PopoverAction::OpenSettings))
    .with_icon_glyph("\u{E713}"),
PopoverItem::new("Task Manager", "", true, Some(PopoverAction::OpenTaskManager))
    .with_icon_glyph("\u{E9D9}"),
PopoverItem::new(
    "Lock", "Win+L", true,
    Some(PopoverAction::ConfirmSession(SessionAction::Lock)),
)
.with_icon_glyph("\u{E72E}")
.with_section_start(),
PopoverItem::new(
    "Sleep", "Requires confirmation", true,
    Some(PopoverAction::ConfirmSession(SessionAction::Sleep)),
)
.with_icon_glyph("\u{E708}"),
PopoverItem::new(
    "Sign out", "Requires confirmation", true,
    Some(PopoverAction::ConfirmSession(SessionAction::SignOut)),
)
.with_icon_glyph("\u{E8AC}"),
PopoverItem::new(
    "Restart", "Requires confirmation", true,
    Some(PopoverAction::ConfirmSession(SessionAction::Restart)),
)
.with_icon_glyph("\u{E777}")
.with_section_start(),
PopoverItem::new(
    "Shut down", "Requires confirmation", true,
    Some(PopoverAction::ConfirmSession(SessionAction::ShutDown)),
)
.with_icon_glyph("\u{E7E8}"),
```

- [ ] **Step 5: Map metadata into the renderer scene**

In `ActivePopover::scene`, build each row as follows:

```rust
let mut row = PopoverRow::new(item.label(), detail, item.enabled())
    .with_icon_source(item.icon_source().map(str::to_owned));
if let Some(glyph) = item.icon_glyph() {
    row = row.with_icon_glyph(glyph);
}
if item.section_start() {
    row = row.with_section_start();
}
row
```

After `PopoverScene::new`, apply `PopoverLayoutStyle::SystemPanel` only for `Popover::SystemMenu`. Change `title(Popover::SystemMenu)` from `System` to `Minha UI`; leave other titles unchanged.

- [ ] **Step 6: Run controller regressions and checkpoint**

Run:

```powershell
cargo test -p shell-platform-windows popover_controller
cargo test -p shell-platform-windows system_menu --lib
git diff --check
```

Expected: the inventory/group test, current keyboard activation tests, and two-step confirmation tests all pass.

## Task 3: Size and anchor the native system panel from its scene

**Files:**

- Modify: `crates/shell-renderer/src/popover_layout.rs`
- Modify: `crates/shell-renderer/src/lib.rs`
- Modify: `crates/shell-platform-windows/src/win32_window.rs`
- Modify: `crates/shell-platform-windows/src/win32_popover_render.rs`
- Modify: `crates/shell-platform-windows/src/win32_owner.rs`
- Test: `crates/shell-platform-windows/tests/popover_controller.rs`

- [ ] **Step 1: Write failing placement tests**

Add to `crates/shell-platform-windows/tests/popover_controller.rs`:

```rust
#[test]
fn system_panel_uses_288_dip_width_and_keeps_notch_over_its_anchor() {
    let dpi = Dpi::from_raw(96);
    let work = PhysicalRect::new(0, 0, 1920, 1080);
    let anchor = PhysicalRect::new(16, 0, 92, 32);
    let placement = popover_placement(
        anchor,
        work,
        dpi,
        PopoverSurfaceSize::new(288.0, 372.0),
    );

    assert_eq!(placement.rect().width, 288);
    assert_eq!(placement.rect().y, 40);
    assert!((16.0..=272.0).contains(&placement.anchor_x_dip()));
    assert_eq!(placement.anchor_x_dip(), 62.0);
}
```

- [ ] **Step 2: Run the placement test and verify RED**

Run:

```powershell
cargo test -p shell-platform-windows system_panel_uses_288
```

Expected: compilation fails because `PopoverPlacement` and `popover_placement` do not exist.

- [ ] **Step 3: Add a pure placement result**

In `popover_layout.rs`, add:

```rust
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PopoverPlacement {
    rect: PhysicalRect,
    anchor_x_dip: f32,
}

impl PopoverPlacement {
    #[must_use]
    pub const fn rect(self) -> PhysicalRect { self.rect }
    #[must_use]
    pub const fn anchor_x_dip(self) -> f32 { self.anchor_x_dip }
}

#[must_use]
pub fn popover_placement(
    anchor: PhysicalRect,
    work: PhysicalRect,
    dpi: Dpi,
    size: PopoverSurfaceSize,
) -> PopoverPlacement {
    let width = physical_from_dip(size.width().clamp(244.0, 440.0), dpi);
    let height = physical_from_dip(size.height().clamp(58.0, 420.0), dpi);
    let margin = physical_from_dip(8.0, dpi);
    let x = (anchor.x + anchor.width / 2 - width / 2)
        .clamp(work.x, work.x + work.width - width);
    let y = (anchor.y + anchor.height + margin)
        .clamp(work.y, work.y + work.height - height);
    let anchor_center = anchor.x + anchor.width / 2;
    let anchor_x_dip = ((anchor_center - x) as f32 * 96.0 / dpi.raw() as f32)
        .clamp(16.0, size.width() - 16.0);
    PopoverPlacement {
        rect: PhysicalRect::new(x, y, width, height),
        anchor_x_dip,
    }
}
```

Keep `popover_anchor_rect_with_height` as a compatibility wrapper for compact/context callers until every caller is migrated.

- [ ] **Step 4: Store placement metadata in `OwnedWindow`**

Add `popover_width_dip: f32` and `popover_anchor_x_dip: f32` beside `popover_height_dip`. Change `place_popover` to accept `PopoverSurfaceSize`, call `popover_placement`, store both size values and `anchor_x_dip`, and set the returned rect. Add:

```rust
pub(super) const fn popover_anchor_x_dip(&self) -> f32 {
    self.popover_anchor_x_dip
}
```

Update all three placement paths in `win32_popover_render.rs` and the asynchronous background-app path in `win32_owner.rs` to obtain a scene, call `popover_surface_size(&scene)`, and pass the complete size.

- [ ] **Step 5: Attach the local anchor only at the rendering boundary**

In `redraw_popover`, avoid putting window coordinates in the controller. Clone the scene and add the local position:

```rust
let scene = self
    .popover_controller
    .scene()
    .map(|scene| scene.with_anchor_x(popover.popover_anchor_x_dip()));
```

Pass `scene.as_ref()` to `ShellScenes`. Controller tests remain platform-coordinate free.

- [ ] **Step 6: Run placement, multi-monitor, and controller tests**

Run:

```powershell
cargo test -p shell-platform-windows popover_controller
cargo test -p shell-platform-windows multimonitor_placement
cargo check -p shell-platform-windows --all-targets
git diff --check
```

Expected: 288-DIP system placement passes, compact popovers still clamp inside work areas, and multi-monitor tests remain green.

## Task 4: Generalize cached liquid-glass resources into surface profiles

**Files:**

- Modify: `crates/shell-renderer/src/native_liquid_glass.rs`
- Modify: `crates/shell-renderer/src/native.rs`
- Modify: `crates/shell-renderer/src/native/resize.rs`
- Modify: `crates/shell-renderer/src/native_showcase.rs`
- Modify: `crates/shell-renderer/src/native_showcase_material.rs`

- [ ] **Step 1: Write failing profile-policy tests**

Add to `native_liquid_glass.rs` tests:

```rust
#[test]
fn surface_profiles_keep_dock_dynamic_but_panel_and_active_module_static() {
    let dock = presentation(
        LiquidGlassProfile::Dock,
        LiquidGlassMode::Dynamic,
        DipRect::new(0.0, 0.0, 500.0, 53.0),
        Some(420.0),
        1.0,
        96.0,
    );
    let panel = presentation(
        LiquidGlassProfile::Panel,
        LiquidGlassMode::Dynamic,
        DipRect::new(0.0, 8.0, 288.0, 356.0),
        Some(240.0),
        1.0,
        96.0,
    );
    let active = presentation(
        LiquidGlassProfile::ActiveModule,
        LiquidGlassMode::Dynamic,
        DipRect::new(10.0, 4.0, 92.0, 24.0),
        Some(60.0),
        1.0,
        96.0,
    );

    assert!(dock.specular_opacity > 0.0);
    assert_eq!(panel.specular_opacity, 0.0);
    assert_eq!(active.specular_opacity, 0.0);
    assert!(active.overlay_opacity > panel.overlay_opacity);
}

#[test]
fn only_supported_native_roles_allocate_glass_profiles() {
    assert_eq!(profile_for_role(ShowcaseRole::Dock), Some(LiquidGlassProfile::Dock));
    assert_eq!(profile_for_role(ShowcaseRole::Popover), Some(LiquidGlassProfile::Panel));
    assert_eq!(
        profile_for_role(ShowcaseRole::Topbar),
        Some(LiquidGlassProfile::ActiveModule)
    );
    assert_eq!(profile_for_role(ShowcaseRole::Preview), None);
    assert_eq!(profile_for_role(ShowcaseRole::Settings), None);
}
```

- [ ] **Step 2: Run the focused tests and verify RED**

Run:

```powershell
cargo test -p shell-renderer surface_profiles --lib
cargo test -p shell-renderer supported_native_roles --lib
```

Expected: compilation fails because the resource is still named `DockLiquidGlassResources` and has no profile.

- [ ] **Step 3: Define profiles and keep existing degradation policy**

Add:

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LiquidGlassProfile {
    Dock,
    Panel,
    ActiveModule,
}

pub(crate) const fn profile_for_role(role: ShowcaseRole) -> Option<LiquidGlassProfile> {
    match role {
        ShowcaseRole::Dock => Some(LiquidGlassProfile::Dock),
        ShowcaseRole::Popover => Some(LiquidGlassProfile::Panel),
        ShowcaseRole::Topbar => Some(LiquidGlassProfile::ActiveModule),
        ShowcaseRole::Preview | ShowcaseRole::Settings => None,
    }
}
```

Rename `DockLiquidGlassResources` to `LiquidGlassResources`, store `profile`, and add `profile` to `LiquidGlassRaster::new`, `LiquidGlassResources::create`, and `presentation`. Preserve `liquid_glass_mode`: disabled for solid/high-contrast/safe mode, static for WARP/reduced motion, dynamic only on normal hardware.

Use profile constants:

```rust
let (overlay_opacity, specular_opacity) = match profile {
    LiquidGlassProfile::Dock => (
        0.72 + 0.18 * strength,
        if mode == LiquidGlassMode::Dynamic { 0.12 + 0.28 * strength } else { 0.0 },
    ),
    LiquidGlassProfile::Panel => (0.64, 0.0),
    LiquidGlassProfile::ActiveModule => (0.86, 0.0),
};
```

For `ActiveModule`, bias the cached raster toward cool violet-blue channels and remove the warm right-edge tint. For `Panel`, retain a neutral/cool rim with lower reflection alpha. Keep premultiplied BGRA validation unchanged.

- [ ] **Step 4: Own one profile-specific resource per surface**

Rename `WindowSurface::dock_liquid_glass` to:

```rust
liquid_glass: Option<LiquidGlassResources>,
```

In both `create_surface` and `resize_surface`, use `profile_for_role(role)` and create the resource only when a profile exists. Use the full logical dock/panel size; for `ActiveModule`, create a reusable `160 x topbar-height` raster and scale it into the active module bounds. Do not allocate in `redraw_surface`.

Rename `ShowcaseStyle::dock_liquid_glass` and `MaterialBrushes::liquid_glass` to the generic resource type, then pass `surface.liquid_glass.as_ref()` through create, resize, and redraw.

- [ ] **Step 5: Run pure resource and native lifecycle tests**

Run:

```powershell
cargo test -p shell-renderer liquid_glass --lib
cargo test -p shell-renderer native::tests --lib
cargo check -p shell-renderer --all-targets
git diff --check
```

Expected: original dock raster/bounds tests and new profile tests pass; resources are still created only on surface create/resize.

## Task 5: Make the top bar transparent and synchronize pressed/open module state

**Files:**

- Modify: `crates/shell-renderer/src/topbar_scene.rs`
- Modify: `crates/shell-renderer/src/topbar_layout.rs`
- Modify: `crates/shell-renderer/tests/topbar_scene.rs`
- Modify: `crates/shell-platform-windows/src/topbar_controller.rs`
- Modify: `crates/shell-platform-windows/src/win32_window.rs`
- Modify: `crates/shell-platform-windows/src/win32_popover_render.rs`
- Modify: `crates/shell-platform-windows/tests/topbar_controller.rs`
- Modify: `crates/shell-renderer/src/native_showcase_material.rs`
- Modify: `crates/shell-renderer/src/native_showcase_topbar.rs`
- Modify: `crates/shell-renderer/src/native_showcase.rs`

- [ ] **Step 1: Write failing state-separation tests**

Add to `crates/shell-renderer/tests/topbar_scene.rs`:

```rust
#[test]
fn focused_pressed_and_active_topbar_states_are_independent_and_keep_bounds() {
    let module = TopbarModuleVisual::new(
        TopbarModuleKind::SystemMenu,
        "Menu",
        "Minha UI",
        TopbarModuleStatus::Neutral,
        Some(TopbarIntent::Popover(Popover::SystemMenu)),
    );
    let base = TopbarScene::new(TopbarDensity::Compact, vec![module]);
    let normal = layout_topbar_scene(&base, DipRect::new(0.0, 0.0, 800.0, 32.0));
    let active = layout_topbar_scene(
        &base
            .clone()
            .with_focused_module(Some(TopbarModuleKind::SystemMenu))
            .with_pressed_module(Some(TopbarModuleKind::SystemMenu))
            .with_active_module(Some(TopbarModuleKind::SystemMenu)),
        DipRect::new(0.0, 0.0, 800.0, 32.0),
    );

    assert_eq!(normal.visible_items()[0].bounds(), active.visible_items()[0].bounds());
    assert!(active.visible_items()[0].focused());
    assert!(active.visible_items()[0].pressed());
    assert!(active.visible_items()[0].active());
}
```

Add controller tests that press the System module and expect `RedrawTopbar`, then open it and expect `scene.active_module() == Some(SystemMenu)`, then clear it and expect `None`.

- [ ] **Step 2: Run focused tests and verify RED**

Run:

```powershell
cargo test -p shell-renderer focused_pressed_and_active
cargo test -p shell-platform-windows pressed
```

Expected: missing pressed/active scene fields and controller synchronization API.

- [ ] **Step 3: Add explicit visual states**

Add `pressed_module` and `active_module` to `TopbarScene`, initialize both to `None`, and add builders/accessors matching the existing focused API. Add `pressed` and `active` booleans to `TopbarLaidOutItem`; set them from the scene without changing bounds, widths, order, overflow, or hit testing.

In `TopbarController`, add `active_module: Option<TopbarModuleKind>`, then expose:

```rust
pub(crate) fn set_active_module(&mut self, module: Option<TopbarModuleKind>) {
    if self.active_module != module {
        self.active_module = module;
        self.visual_generation = self.visual_generation.wrapping_add(1);
    }
}
```

Build the scene with both states. Derive `pressed_module` from `pressed_target` only when the anchor is `TopbarOverlayAnchor::Module(kind)`.

On pointer press, store the target and return `QueuedTopbarAction::RedrawTopbar`. On release or cancelled release, clear the pressed target and ensure the returned actions include one redraw.

- [ ] **Step 4: Synchronize open/dismiss state in the overlay owner**

In `apply_topbar_actions`, derive the active module from the anchor before redrawing the top bar:

```rust
let active_module = match anchor {
    TopbarOverlayAnchor::Module(kind) => Some(*kind),
    TopbarOverlayAnchor::Overflow => None,
};
self.topbar_controller.set_active_module(active_module);
```

If the same popover toggles closed, clear the active module. Centralize dismissal in:

```rust
fn clear_active_topbar_module(&mut self) {
    self.active_topbar_anchor = None;
    self.topbar_controller.set_active_module(None);
}
```

Call the helper from outside-click dismissal, `Escape`, settings transition, search transition, and every popover action that hides the popover. Redraw the top bar only when the active value changes.

Add this focused native-window helper next to `show_activating`:

```rust
pub(super) fn restore_focus(&self) {
    // SAFETY: Category 8 (FFI boundary). The top-bar HWND is owned by this UI
    // thread and remains live while a child popup is dismissed.
    let _ = unsafe { SetForegroundWindow(self.hwnd) };
    // SAFETY: Category 8 (FFI boundary). Focus returns to the invoker after the
    // popup releases activation.
    let _ = unsafe { SetFocus(Some(self.hwnd)) };
}
```

Call `topbar.restore_focus()` only for user-driven `Escape` and outside-click dismissal. Do not steal focus after actions that intentionally open Settings, Search, Task Manager, or another application.

- [ ] **Step 5: Remove the full-width top-bar fill and draw protected content**

Change `draw_topbar_material` so transparent mode performs no full-surface fill:

```rust
fn draw_topbar_material(
    context: &ID2D1DeviceContext,
    surface: MaterialSurface,
    brushes: MaterialBrushes<'_>,
) {
    if surface.solid {
        fill_round(
            context,
            rect(0.0, 0.0, surface.width, surface.height, 0.0),
            brushes.topbar_tint,
        );
    }
}
```

Add a pure `topbar_material_plan(solid: bool)` test proving `draw_full_surface == false` in transparent mode and `true` in solid mode.

In `draw_functional_topbar`, draw icon/text once at `y + 1 DIP` with a dark `contrast_shadow` brush, then at the original position with the semantic brush. For active or pressed items, draw the `ActiveModule` resource into the unchanged item bounds before text. Keep keyboard focus as a separate 1-DIP inner ring or underline so it remains visible without relying on violet.

- [ ] **Step 6: Run top-bar and controller regressions**

Run:

```powershell
cargo test -p shell-renderer topbar_scene
cargo test -p shell-renderer topbar_material --lib
cargo test -p shell-platform-windows topbar_controller
cargo check -p shell-platform-windows --all-targets
git diff --check
```

Expected: active/pressed tests pass and existing ordering, overflow, text-scale, and hit-test tests remain green.

## Task 6: Render the balanced liquid-glass System panel

**Files:**

- Modify: `crates/shell-renderer/src/native_showcase.rs`
- Modify: `crates/shell-renderer/src/native_showcase_material.rs`
- Modify: `crates/shell-renderer/src/native_showcase_popover.rs`
- Modify: `crates/shell-renderer/src/native_showcase_primitives.rs`
- Modify: `crates/shell-renderer/src/native_showcase_resources.rs`
- Test: `crates/shell-renderer/tests/popover_scene.rs`

- [ ] **Step 1: Write failing geometry-containment tests**

Add to `crates/shell-renderer/tests/popover_scene.rs`:

```rust
#[test]
fn system_panel_notch_and_rows_stay_inside_the_surface() {
    let scene = system_panel_scene().with_anchor_x(62.0);
    let size = popover_surface_size(&scene);
    let surface = DipRect::new(0.0, 0.0, size.width(), size.height());
    let layout = layout_popover_scene(&scene, surface);

    let notch = layout.notch().expect("system panel needs an anchor notch");
    assert_eq!(notch.tip_x(), 62.0);
    assert!(notch.bounds().x >= surface.x);
    assert!(notch.bounds().x + notch.bounds().width <= surface.width);
    assert!(layout.rows().iter().all(|row| {
        let bounds = row.bounds();
        bounds.x >= 0.0
            && bounds.y >= 0.0
            && bounds.x + bounds.width <= size.width()
            && bounds.y + bounds.height <= size.height()
    }));
}
```

Move the repeated seven-row fixture into a local `system_panel_scene()` test helper with the exact rows from Task 1.

- [ ] **Step 2: Run the focused test and verify RED**

Run:

```powershell
cargo test -p shell-renderer system_panel_notch
```

Expected: `PopoverNotch` and `layout.notch()` are missing.

- [ ] **Step 3: Add bounded notch geometry**

Add to `popover_layout.rs`:

```rust
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PopoverNotch {
    bounds: DipRect,
    tip_x: f32,
}

impl PopoverNotch {
    #[must_use]
    pub const fn bounds(self) -> DipRect { self.bounds }
    #[must_use]
    pub const fn tip_x(self) -> f32 { self.tip_x }
}
```

The system layout from Task 1 already reserves the first `8 DIP` above the panel body. Use that space for a `16 x 8 DIP` triangular notch, clamping its center to `16..surface.width - 16`; do not move the title, rows, or final `372 DIP` surface size.

Add one `fill_triangle` primitive that creates a three-point Direct2D path, closes it, fills it synchronously, and releases the geometry before returning:

```rust
pub(crate) fn fill_triangle(
    context: &ID2D1DeviceContext,
    points: [D2D_POINT_2F; 3],
    brush: &ID2D1Brush,
) -> windows::core::Result<()> {
    let mut factory = None;
    // SAFETY: Category 8 (FFI boundary). The live device context retains its
    // factory and writes one COM interface into the out parameter.
    unsafe { context.GetFactory(&mut factory) };
    let factory = factory.ok_or_else(|| windows::core::Error::from_hresult(E_FAIL))?;
    // SAFETY: Category 8 (FFI boundary). The path and sink remain live until
    // the synchronous fill completes.
    let geometry = unsafe { factory.CreatePathGeometry()? };
    let sink = unsafe { geometry.Open()? };
    unsafe {
        sink.BeginFigure(points[0], D2D1_FIGURE_BEGIN_FILLED);
        sink.AddLine(points[1]);
        sink.AddLine(points[2]);
        sink.EndFigure(D2D1_FIGURE_END_CLOSED);
        sink.Close()?;
        context.FillGeometry(&geometry, brush, None);
    }
    Ok(())
}
```

Import `D2D_POINT_2F`, `D2D1_FIGURE_BEGIN_FILLED`, `D2D1_FIGURE_END_CLOSED`, `ID2D1Brush`, and `E_FAIL`. The path is created only when the already-open popover redraws; it does not introduce a timer or bitmap generation.

- [ ] **Step 4: Draw one continuous panel material**

Extend `MaterialSurface` with `content_bounds` for `Popover`. For system style, pass `DipRect::new(0.0, 8.0, width, height - 8.0)`; for compact panels keep the full surface.

Implement `draw_panel_material`:

```rust
fn draw_panel_material(
    context: &ID2D1DeviceContext,
    surface: MaterialSurface,
    brushes: MaterialBrushes<'_>,
) {
    let bounds = surface
        .content_bounds
        .unwrap_or_else(|| DipRect::new(0.0, 0.0, surface.width, surface.height));
    let radius = surface.radius;
    fill_round(context, rounded_bounds(bounds, radius), brushes.rim_outer);
    let body = DipRect::new(
        bounds.x + 1.0,
        bounds.y + 1.0,
        (bounds.width - 2.0).max(1.0),
        (bounds.height - 2.0).max(1.0),
    );
    fill_round(context, rounded_bounds(body, (radius - 1.0).max(0.0)), brushes.luminance);
    fill_round(context, rounded_bounds(body, (radius - 1.0).max(0.0)), brushes.veil);
    fill_round(context, rounded_bounds(body, (radius - 1.0).max(0.0)), brushes.reflection);
    if let Some(liquid_glass) = brushes.liquid_glass {
        liquid_glass.draw(context, bounds, None, 0.0);
    }
}
```

Route `ShowcaseRole::Popover` to this material only when `scenes.context_menu.is_none()`. Preserve the existing warm context-menu material path unchanged. In solid mode, draw only the solid base/rim and omit transparent overlays.

- [ ] **Step 5: Draw title, separators, icon wells, rows, and shortcuts**

Add a `16 DIP`, semibold system-panel title format in `native_showcase_resources.rs`. Extend `PopoverFormats` with `title` and `PopoverBrushes` with `raised`, `divider`, and `focus` brushes.

In `draw_functional_popover`, branch on `scene.layout_style()`:

- draw `scene.title()` inside `layout.title_bounds()`;
- draw the notch using the same base/rim brushes as the panel;
- draw every `layout.separators()` as a 1-DIP low-alpha line;
- for each row with `icon_glyph`, draw a `28 x 28 DIP` rounded glass well and center the glyph inside it;
- retain `icon_source` only for runtime image icons such as background applications;
- use the middle column for the label and the right column for detail/shortcut;
- draw focused rows as one continuous low-alpha rounded highlight, not a card;
- keep disabled text on the secondary brush and preserve the confirmation detail string.

Use the existing compact renderer branch unchanged for non-system panels.

- [ ] **Step 6: Run renderer, controller, and context-menu regressions**

Run:

```powershell
cargo test -p shell-renderer popover_scene
cargo test -p shell-renderer popover --lib
cargo test -p shell-platform-windows popover_controller
cargo test -p shell-renderer context_menu --lib
cargo check -p shell-renderer --all-targets
git diff --check
```

Expected: grouped rendering geometry passes, system actions still activate, and the shared popover HWND does not regress context menus.

## Task 7: Document, verify, inspect, and measure the complete surface

**Files:**

- Modify: `README.md`
- Modify: `SUPPORT.md`
- Modify: `docs/superpowers/plans/2026-07-17-transparent-liquid-glass-topbar-system-panel.md`

- [ ] **Step 1: Update user and support documentation**

Replace README text claiming `--liquid-glass` affects only the dock with:

```markdown
The experimental `--liquid-glass` switch applies the shared lightweight
material to the dock, transparent top-bar interaction states, and owned
top-bar panels. The top bar remains visually transparent at rest. WARP and
reduced-motion runs use static glass; safe mode and high contrast use the solid
Obsidian Glass fallback.
```

In `SUPPORT.md`, add an idle acceptance note: no top-bar or popover redraws while both are idle/closed, no wallpaper capture, and no new background polling or timer.

- [ ] **Step 2: Run formatting and architecture gates**

Run:

```powershell
cargo fmt --all --check
powershell -NoProfile -File scripts/Check-Architecture.ps1
```

Expected: both commands exit `0`; the architecture check reports no dependency-direction violation.

- [ ] **Step 3: Run Clippy and the full test workspace**

Run:

```powershell
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

Expected: zero warnings and zero failing tests.

- [ ] **Step 4: Build the release executable**

Run:

```powershell
cargo build --workspace --release
```

Expected executable: `target/x86_64-pc-windows-msvc/release/shell-app.exe`.

- [ ] **Step 5: Run bounded native smoke paths**

Run each configuration:

```powershell
.\target\x86_64-pc-windows-msvc\release\shell-app.exe --qa-exit-ms 5000
.\target\x86_64-pc-windows-msvc\release\shell-app.exe --qa-exit-ms 5000 --liquid-glass
.\target\x86_64-pc-windows-msvc\release\shell-app.exe --qa-exit-ms 5000 --liquid-glass --force-warp
.\target\x86_64-pc-windows-msvc\release\shell-app.exe --qa-exit-ms 5000 --liquid-glass --reduced-motion
.\target\x86_64-pc-windows-msvc\release\shell-app.exe --qa-exit-ms 5000 --liquid-glass --safe-mode
.\target\x86_64-pc-windows-msvc\release\shell-app.exe --qa-exit-ms 5000 --liquid-glass --high-contrast
```

Expected: normal hardware uses dynamic dock plus static panel/module profiles; WARP and reduced motion use static profiles; safe/high contrast use solid material; every process exits cleanly.

- [ ] **Step 6: Perform rendered desktop inspection**

Run the hardware path with `--qa-exit-ms 60000`, open `Minha UI`, and capture evidence in `artifacts/topbar-system-liquid/` for:

- dark wallpaper: resting bar, pressed module, open panel, row focus;
- light wallpaper: resting bar and open panel;
- 100%, 150%, and 200% scaling;
- keyboard focus, `Escape`, outside-click dismissal, and focus restoration;
- reduced motion and solid fallback;
- dock and system panel visible together.

Accept only when the bar has no visible continuous plate, content remains readable, the active violet capsule stays within module bounds, the notch points to `Minha UI`, seven actions are present in the original order, panel groups are clear without visible section labels, and text/shortcuts do not clip.

- [ ] **Step 7: Measure idle and redraw behavior**

Use the same ten one-second sample method recorded in the existing liquid-glass dock plan. Compare release runs with and without `--liquid-glass`. Accept only when:

- no new steady-state timer/thread is present;
- idle CPU remains equivalent within sampling noise;
- working-set increase remains at or below `3 MB`;
- opening and focusing the panel produces no sustained redraw above `16 ms`;
- closing the panel returns the top bar and popover to idle with no redraw stream.

- [ ] **Step 8: Record final evidence and create reviewed commits**

Update the checkboxes and append exact command results, screenshot paths, A/B measurements, release path, and SHA-256 to this plan. Run `git status --short` and compare against the initial inventory. Stage only reviewed files. If pre-existing edits overlap, show the combined diff to the user before creating commits.

Suggested commit split after approval:

```powershell
git commit -m "feat(popover): add grouped system panel layout"
git commit -m "feat(renderer): share liquid glass across shell surfaces"
git commit -m "feat(topbar): add transparent active module treatment"
git commit -m "docs(topbar): record liquid glass verification"
```
