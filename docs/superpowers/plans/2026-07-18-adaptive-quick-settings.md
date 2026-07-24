# Adaptive Quick Settings Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a native adaptive Quick Settings panel that opens from the top-bar controls module, exposes only real Windows capabilities, and lets the user configure compatible controls in Settings.

**Architecture:** Add stable quick-control identity and persistence below the platform boundary, a Windows capability/action adapter plus controller in `shell-platform-windows`, and a dedicated scene/layout/Direct2D renderer in `shell-renderer`. Keep the existing single popover HWND and route either `PopoverScene` or `QuickSettingsScene` into it; Settings receives a typed Quick Controls editor backed by the same persisted order and visibility.

**Tech Stack:** Rust 2024, Windows crate 0.62, Win32 WLAN/Bluetooth/Power/Display/Core Audio/WMI APIs, Direct2D/DirectWrite/DirectComposition, Serde, existing native shell runtime.

---

## File Structure

### Create

- `crates/shell-core/src/quick_settings.rs` — stable control identity and persisted placement value.
- `crates/shell-config/src/quick_settings.rs` — Quick Settings defaults, normalization, visibility, and ordering.
- `crates/shell-renderer/src/quick_settings_scene.rs` — platform-neutral rich scene model.
- `crates/shell-renderer/src/quick_settings_layout.rs` — adaptive grid, sections, scrolling, and hit testing.
- `crates/shell-renderer/src/native_showcase_quick_settings.rs` — Direct2D drawing for the approved panel.
- `crates/shell-renderer/tests/quick_settings_scene.rs` — scene and layout behavior.
- `crates/shell-platform-windows/src/quick_settings_types.rs` — capability snapshot and typed intents.
- `crates/shell-platform-windows/src/quick_settings_controller.rs` — filtering, focus, pointer/keyboard interaction, and scene production.
- `crates/shell-platform-windows/src/win32_quick_settings_capabilities.rs` — documented Windows capability readers.
- `crates/shell-platform-windows/src/win32_quick_settings_actions.rs` — direct actions and documented Settings fallbacks.
- `crates/shell-platform-windows/tests/quick_settings_controller.rs` — deterministic controller tests using synthetic snapshots.

### Modify

- `crates/shell-core/src/lib.rs` and `crates/shell-core/src/model.rs` — export quick-control types and add `Popover::QuickSettings`.
- `crates/shell-config/src/lib.rs`, `crates/shell-config/src/schema.rs`, and `crates/shell-config/tests/config.rs` — persist Quick Settings without breaking current V1 files.
- `crates/shell-renderer/src/lib.rs`, `crates/shell-renderer/src/integration_tests.rs`, `crates/shell-renderer/src/native.rs`, and `crates/shell-renderer/src/native_showcase.rs` — expose and render `QuickSettingsScene` on the existing popover surface.
- `crates/shell-renderer/src/settings_scene.rs` and `crates/shell-renderer/src/native_showcase_settings.rs` — add the Quick Controls editor state and drawing.
- `crates/shell-platform-windows/Cargo.toml` and `crates/shell-platform-windows/src/lib.rs` — enable documented API features and wire modules/tests.
- `crates/shell-platform-windows/src/topbar_controller.rs` and `crates/shell-platform-windows/tests/topbar_controller.rs` — turn the persisted Notifications slot into the visible Controls entry point.
- `crates/shell-platform-windows/src/settings_controller.rs` and `crates/shell-platform-windows/tests/settings_controller.rs` — configure only supported controls and persist order/visibility.
- `crates/shell-platform-windows/src/win32_owner.rs`, `crates/shell-platform-windows/src/win32_popover_render.rs`, `crates/shell-platform-windows/src/win32_windowing.rs`, `crates/shell-platform-windows/src/runtime.rs`, and `crates/shell-platform-windows/src/win32_config.rs` — runtime ownership, event routing, persistence, and refresh.
- `crates/shell-platform-windows/src/win32_system_actions.rs` — add only the new documented Settings URI variants used by route-only controls.

## Task 1: Stable Control Identity and Backward-Compatible Preferences

**Files:**
- Create: `crates/shell-core/src/quick_settings.rs`
- Create: `crates/shell-config/src/quick_settings.rs`
- Modify: `crates/shell-core/src/lib.rs`
- Modify: `crates/shell-core/src/model.rs`
- Modify: `crates/shell-config/src/lib.rs`
- Modify: `crates/shell-config/src/schema.rs`
- Modify: `crates/shell-config/tests/config.rs`

- [ ] **Step 1: Write the failing configuration tests**

Add tests proving that old V1 JSON without `quick_settings` receives defaults, a customized order/visibility round-trips, and duplicate placements are rejected:

```rust
#[test]
fn old_v1_defaults_quick_settings_without_recovery() -> Result<(), Box<dyn std::error::Error>> {
    let encoded = encode_config(&ShellConfigV1::default())?;
    let mut value: serde_json::Value = serde_json::from_slice(&encoded)?;
    value.as_object_mut().unwrap().remove("quick_settings");
    let ConfigLoad::Current(decoded) = decode_config(&serde_json::to_vec(&value)?) else {
        return Err("old V1 config must remain current".into());
    };
    assert_eq!(decoded.quick_settings(), &QuickSettingsSettings::default());
    Ok(())
}

#[test]
fn quick_control_order_and_visibility_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let settings = QuickSettingsSettings::default()
        .with_visibility(QuickControlKind::Bluetooth, false)
        .reordered(QuickControlKind::Volume, Some(QuickControlKind::Wifi));
    let config = ShellConfigV1::default().with_quick_settings(settings.clone());
    let ConfigLoad::Current(decoded) = decode_config(&encode_config(&config)?) else {
        return Err("quick settings must remain current".into());
    };
    assert_eq!(decoded.quick_settings(), &settings);
    Ok(())
}
```

- [ ] **Step 2: Run the focused test and verify the missing API failure**

Run: `cargo test -p shell-config old_v1_defaults_quick_settings_without_recovery -- --exact`

Expected: FAIL because `QuickSettingsSettings`, `QuickControlKind`, and `ShellConfigV1::quick_settings` do not exist.

- [ ] **Step 3: Add the complete stable model**

Create `shell-core/src/quick_settings.rs` with the exhaustive initial identity and a placement value:

```rust
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuickControlKind {
    Wifi,
    Bluetooth,
    NearbySharing,
    Focus,
    Multitasking,
    Projection,
    Brightness,
    DarkMode,
    NightLight,
    Volume,
    Battery,
    EnergySaver,
}

impl QuickControlKind {
    pub const ALL: [Self; 12] = [
        Self::Wifi, Self::Bluetooth, Self::NearbySharing, Self::Focus,
        Self::Multitasking, Self::Projection, Self::Brightness, Self::DarkMode,
        Self::NightLight, Self::Volume, Self::Battery, Self::EnergySaver,
    ];
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct QuickControlPlacement {
    kind: QuickControlKind,
    visible: bool,
}

impl QuickControlPlacement {
    pub const fn new(kind: QuickControlKind, visible: bool) -> Self { Self { kind, visible } }
    pub const fn kind(self) -> QuickControlKind { self.kind }
    pub const fn visible(self) -> bool { self.visible }
}
```

Export both types from `shell-core/src/lib.rs`, add `QuickSettings` to `Popover`, and leave `Notifications` in place for future notification-center semantics.

- [ ] **Step 4: Implement normalized preferences and schema defaults**

Create `QuickSettingsSettings { controls: Vec<QuickControlPlacement> }`. `Default` maps `QuickControlKind::ALL` to visible placements. `normalized()` removes duplicate kinds, appends omitted known kinds in default order, and preserves visibility for retained entries. `with_visibility` and `reordered` always return normalized settings. Add this field to `ShellConfigV1`:

```rust
#[serde(default)]
quick_settings: QuickSettingsSettings,
```

Initialize it in `Default` and V0 migration, add getter/builder methods, and extend `validate()` with `quick_settings.validate()`.

- [ ] **Step 5: Run the focused crate tests**

Run: `cargo test -p shell-config quick_settings`

Expected: PASS for legacy defaulting, round-trip, normalization, and validation tests.

- [ ] **Step 6: Commit the model and persistence seam**

```powershell
git add crates/shell-core/src/quick_settings.rs crates/shell-core/src/lib.rs crates/shell-core/src/model.rs crates/shell-config/src/quick_settings.rs crates/shell-config/src/lib.rs crates/shell-config/src/schema.rs crates/shell-config/tests/config.rs
git commit -m "feat: persist adaptive quick controls"
```

## Task 2: Rich Scene, Adaptive Layout, and Hit Testing

**Files:**
- Create: `crates/shell-renderer/src/quick_settings_scene.rs`
- Create: `crates/shell-renderer/src/quick_settings_layout.rs`
- Create: `crates/shell-renderer/tests/quick_settings_scene.rs`
- Modify: `crates/shell-renderer/src/lib.rs`
- Modify: `crates/shell-renderer/src/integration_tests.rs`

- [ ] **Step 1: Write failing layout tests**

Cover filtering-independent placement with three tiles, a brightness section, and a high-scale scroll clamp:

```rust
#[test]
fn odd_final_tile_spans_the_two_column_grid() {
    let scene = fixture_scene(vec![QuickControlKind::Wifi, QuickControlKind::Focus, QuickControlKind::Projection]);
    let layout = layout_quick_settings(&scene, DipRect::new(0.0, 0.0, 320.0, 560.0));
    assert_eq!(layout.tiles().len(), 3);
    assert_eq!(layout.tiles()[0].bounds().width, layout.tiles()[1].bounds().width);
    assert_eq!(layout.tiles()[2].bounds().width, layout.content_bounds().width);
}

#[test]
fn hit_test_distinguishes_tiles_sliders_and_edit_footer() {
    let scene = full_fixture_scene();
    let layout = layout_quick_settings(&scene, DipRect::new(0.0, 0.0, 320.0, 620.0));
    assert_eq!(layout.hit_test(layout.tiles()[0].bounds().center()), Some(QuickSettingsHit::Tile(QuickControlKind::Wifi)));
    assert!(matches!(layout.hit_test(layout.brightness().unwrap().track().center()), Some(QuickSettingsHit::Slider { kind: QuickControlKind::Brightness, .. })));
    assert_eq!(layout.hit_test(layout.edit_bounds().center()), Some(QuickSettingsHit::EditControls));
}
```

- [ ] **Step 2: Run the renderer test and verify it fails**

Run: `cargo test -p shell-renderer quick_settings_scene`

Expected: FAIL because the new scene and layout modules are absent.

- [ ] **Step 3: Implement scene values with no platform dependencies**

Define `QuickSettingsScene`, `QuickSettingsTile`, `QuickSettingsSection`, `QuickSettingsSlider`, `QuickSettingsFocus`, and `QuickSettingsHit`. Store values as bounded integers (`0..=100`) and strings; do not store HWND, COM, or Win32 values. Include `anchor_x`, `scroll_offset`, `status_text`, and optional Display/Sound/Energy sections.

Use these constructors consistently:

```rust
QuickSettingsTile::new(kind, label, detail, glyph, active, enabled)
QuickSettingsSlider::new(kind, label, value, enabled)
QuickSettingsScene::new(tiles)
    .with_display(display)
    .with_sound(sound)
    .with_energy(energy)
    .with_focus(focus)
```

- [ ] **Step 4: Implement deterministic layout constants and geometry**

Use `320.0` DIP width, `12.0` outer padding, `8.0` column gap, `64.0` tile height, `12.0` section gap, and `44.0` footer height. Filtered tiles are already compacted by the controller. Pair tiles by index; when `tiles.len() % 2 == 1`, make the last bounds equal the content width. Clamp requested height to the surface and track scroll offset in scene coordinates.

- [ ] **Step 5: Run the focused renderer tests**

Run: `cargo test -p shell-renderer quick_settings_scene`

Expected: PASS for two-column placement, odd spanning, conditional sections, focus order, slider value clamping, scrolling, and hit testing.

- [ ] **Step 6: Commit the renderer model**

```powershell
git add crates/shell-renderer/src/quick_settings_scene.rs crates/shell-renderer/src/quick_settings_layout.rs crates/shell-renderer/tests/quick_settings_scene.rs crates/shell-renderer/src/lib.rs crates/shell-renderer/src/integration_tests.rs
git commit -m "feat: add adaptive quick settings scene"
```

## Task 3: Capability Snapshot and Controller

**Files:**
- Create: `crates/shell-platform-windows/src/quick_settings_types.rs`
- Create: `crates/shell-platform-windows/src/quick_settings_controller.rs`
- Create: `crates/shell-platform-windows/tests/quick_settings_controller.rs`
- Modify: `crates/shell-platform-windows/src/lib.rs`
- Modify: `crates/shell-platform-windows/src/integration_tests.rs`

- [ ] **Step 1: Write failing controller tests using synthetic hardware**

```rust
#[test]
fn unsupported_controls_are_removed_and_off_hardware_remains() {
    let snapshot = QuickSettingsCapabilities::new(vec![
        capability(QuickControlKind::Bluetooth, QuickControlAvailability::Unsupported),
        capability(QuickControlKind::Wifi, QuickControlAvailability::Available { active: false }),
        capability(QuickControlKind::Focus, QuickControlAvailability::RouteOnly),
    ]);
    let controller = QuickSettingsController::new(QuickSettingsSettings::default(), snapshot);
    let scene = controller.scene();
    assert_eq!(scene.tiles().iter().map(|tile| tile.kind()).collect::<Vec<_>>(), vec![QuickControlKind::Wifi, QuickControlKind::Focus]);
    assert!(!scene.tiles()[0].active());
}

#[test]
fn hidden_absent_hardware_keeps_its_saved_preference() {
    let preferences = QuickSettingsSettings::default().with_visibility(QuickControlKind::Bluetooth, false);
    let mut controller = QuickSettingsController::new(preferences, empty_capabilities());
    controller.replace_capabilities(bluetooth_present());
    assert!(!controller.scene().tiles().iter().any(|tile| tile.kind() == QuickControlKind::Bluetooth));
}
```

- [ ] **Step 2: Run the test and verify missing controller types**

Run: `cargo test -p shell-platform-windows quick_settings_controller`

Expected: FAIL because the capability/controller modules are absent.

- [ ] **Step 3: Implement typed capability and intent values**

Define:

```rust
pub enum QuickControlAvailability {
    Unsupported,
    Available { active: bool },
    RouteOnly,
    TemporarilyUnavailable,
}

pub enum QuickSettingsIntent {
    Activate(QuickControlKind),
    SetValue { kind: QuickControlKind, value: u8 },
    EditControls,
    Dismiss,
}

pub struct QuickControlCapability {
    pub kind: QuickControlKind,
    pub availability: QuickControlAvailability,
    pub label: String,
    pub detail: String,
    pub value: Option<u8>,
}
```

`QuickSettingsCapabilities` stores one entry per kind and returns `Unsupported` when absent.

- [ ] **Step 4: Implement controller filtering and interaction**

`QuickSettingsController` owns preferences, snapshot, focus, hover, pressed hit, scroll offset, and last error. `open()` replaces the snapshot and resets transient interaction; `scene()` applies persisted order, removes unsupported/hidden kinds, groups the remaining kinds, and emits the renderer scene. `handle_key`, `pointer_move`, `pointer_press`, `pointer_release`, and `scroll` return typed intents plus redraw requests. Slider drag maps pointer X through the layout track to `0..=100`.

- [ ] **Step 5: Run controller tests**

Run: `cargo test -p shell-platform-windows quick_settings_controller`

Expected: PASS for capability filtering, off state, saved preference preservation, odd tile input order, focus, hover, slider drag, and route intent production.

- [ ] **Step 6: Commit the platform-neutral controller layer**

```powershell
git add crates/shell-platform-windows/src/quick_settings_types.rs crates/shell-platform-windows/src/quick_settings_controller.rs crates/shell-platform-windows/tests/quick_settings_controller.rs crates/shell-platform-windows/src/lib.rs crates/shell-platform-windows/src/integration_tests.rs
git commit -m "feat: model adaptive quick settings capabilities"
```

## Task 4: Documented Windows Capability Readers

**Files:**
- Create: `crates/shell-platform-windows/src/win32_quick_settings_capabilities.rs`
- Modify: `crates/shell-platform-windows/Cargo.toml`
- Modify: `crates/shell-platform-windows/src/lib.rs`

- [ ] **Step 1: Add pure classification tests beside the adapter**

Extract raw API results into pure classifiers and test the critical distinctions:

```rust
#[test]
fn wifi_hardware_can_be_present_while_interface_is_disabled() {
    let rows = [NetworkInterfaceFacts { interface_type: IF_TYPE_IEEE80211, hardware: true, admin_up: false }];
    assert_eq!(classify_wifi(&rows, None), QuickControlAvailability::Available { active: false });
}

#[test]
fn ups_only_power_does_not_create_battery_section() {
    assert!(!has_real_system_battery(SystemBatteryFacts { present: true, short_term: true }));
}
```

- [ ] **Step 2: Enable only required Windows crate features**

Add `Win32_Devices_Bluetooth`, `Win32_NetworkManagement_WiFi`, `Win32_System_Wmi`, `Win32_System_Ole`, `Win32_Media_Audio_Endpoints`, and `Devices_Radios`. Keep existing Display, Power, COM, IpHelper, Ndis, Audio, Shell, and WindowsAndMessaging features.

- [ ] **Step 3: Implement one bounded `read_capabilities()` snapshot**

Use documented APIs with RAII cleanup:

- Wi-Fi presence: `GetIfTable2`, `IF_TYPE_IEEE80211`, `InterfaceAndOperStatusFlags.HardwareInterface`; free with `FreeMibTable`. Query current WLAN radio/interface state without scanning networks.
- Bluetooth presence: `BluetoothFindFirstRadio`; close both radio and find handles. Mark route-only in the portable build.
- Battery: `GetPwrCapabilities` plus `GetSystemPowerStatus`; require `SystemBatteriesPresent && !BatteriesAreShortTerm`.
- Sound: `IMMDeviceEnumerator::GetDefaultAudioEndpoint` and endpoint property name. COM initialization failure suppresses only Sound.
- Projection: `GetDisplayConfigBufferSizes` plus `QueryDisplayConfig`; expose Projection when more than one active/available display path can form a supported topology.
- Brightness: query `WmiMonitorBrightnessMethods` for internal panels. Probe external DDC/CI with `GetMonitorCapabilities`; expose a slider only after a successful verified capability result.
- Route-only OS controls: add Nearby sharing, Focus, Multitasking, Dark mode, and Night light when their documented Settings URI is supported by the current Windows version.

Return partial success: each reader contributes zero or more capabilities and records a diagnostic on failure without aborting the full snapshot.

- [ ] **Step 4: Run the adapter's pure tests and compile-check Windows bindings**

Run: `cargo test -p shell-platform-windows win32_quick_settings_capabilities`

Expected: PASS for raw-fact classification and successful compilation of every documented API binding.

- [ ] **Step 5: Commit capability detection**

```powershell
git add crates/shell-platform-windows/Cargo.toml crates/shell-platform-windows/src/lib.rs crates/shell-platform-windows/src/win32_quick_settings_capabilities.rs Cargo.lock
git commit -m "feat: detect Windows quick settings capabilities"
```

## Task 5: Direct Actions and Settings Fallbacks

**Files:**
- Create: `crates/shell-platform-windows/src/win32_quick_settings_actions.rs`
- Modify: `crates/shell-platform-windows/src/win32_system_actions.rs`
- Modify: `crates/shell-platform-windows/src/popover_types.rs`
- Modify: `crates/shell-platform-windows/src/lib.rs`

- [ ] **Step 1: Write routing and confirmation-state tests**

Test that route-only capabilities never call a direct adapter, failed direct actions request a state refresh without changing the previous active/value state, and the portable Bluetooth capability routes to `ms-settings:bluetooth`.

- [ ] **Step 2: Implement the action adapter contract**

```rust
pub enum QuickSettingsActionResult {
    Applied,
    OpenedSettings,
    Failed(windows::core::Error),
}

pub fn apply_quick_settings_intent(
    intent: &QuickSettingsIntent,
    capability: &QuickControlCapability,
) -> QuickSettingsActionResult;
```

Map actions as follows:

- Wi-Fi direct radio state through documented WLAN set/query calls; open Wi-Fi Settings if Windows denies it.
- Bluetooth route-only for the portable executable; retain a package-identity branch for `Windows.Devices.Radios` only when access is granted.
- Volume and mute through `IAudioEndpointVolume`, replacing the legacy waveOut path only for Quick Settings.
- Brightness through the target adapter chosen during capability detection.
- Projection through the existing documented `SetDisplayConfig` mapping.
- Nearby sharing, Focus, Multitasking, Dark mode, Night light, output-device selection, and Energy saver through explicit `SystemRoute` variants and documented `ms-settings:` URIs.
- Edit controls through `PopoverAction::OpenSettingsSection(SettingsSection::QuickControls)`.

- [ ] **Step 3: Preserve confirmed state on failure**

After `Applied`, re-read the affected capability and replace it in the snapshot. After `OpenedSettings`, dismiss the panel. After `Failed`, keep the prior capability and set `last_error` to a short localized message; do not optimistically flip active/value state.

- [ ] **Step 4: Run focused action tests**

Run: `cargo test -p shell-platform-windows quick_settings_actions`

Expected: PASS for direct-versus-route decisions, failure preservation, and documented URI mapping.

- [ ] **Step 5: Commit the action layer**

```powershell
git add crates/shell-platform-windows/src/win32_quick_settings_actions.rs crates/shell-platform-windows/src/win32_system_actions.rs crates/shell-platform-windows/src/popover_types.rs crates/shell-platform-windows/src/lib.rs
git commit -m "feat: route native quick settings actions"
```

## Task 6: Native Liquid-Glass Rendering

**Files:**
- Create: `crates/shell-renderer/src/native_showcase_quick_settings.rs`
- Modify: `crates/shell-renderer/src/native_showcase.rs`
- Modify: `crates/shell-renderer/src/native.rs`
- Modify: `crates/shell-renderer/src/lib.rs`
- Modify: `crates/shell-renderer/tests/native_slice.rs`

- [ ] **Step 1: Add a native source-structure test**

Extend the existing native slice test to require `draw_quick_settings`, `QuickSettingsScene`, `layout_quick_settings`, and shared `LiquidGlassProfile::Panel`, while rejecting a second DWM backdrop or a timer/capture loop in the new file.

- [ ] **Step 2: Add the scene to `ShellScenes`**

Add:

```rust
pub quick_settings: Option<&'a QuickSettingsScene>,
```

Update every `ShellScenes` literal with `quick_settings: None`, then route the active scene in `draw_showcase`. `wants_desktop_blur` must accept Quick Settings as a panel scene on `ShowcaseRole::Popover` so the existing rounded capture/blur resource is reused.

- [ ] **Step 3: Draw the approved hierarchy**

Implement `draw_quick_settings` using `layout_quick_settings` and existing primitives/resources:

- one continuous rounded panel body and anchor notch;
- compact two-column tiles with icon, label, and detail;
- full-width Display, Sound, and Energy group backgrounds with a lower local contrast than the outer panel;
- slider tracks, values, mute/active state, and endpoint/battery detail;
- dark panel-aware foreground when the selected light glass profile is active, protected high-contrast tokens otherwise;
- hover as a soft tonal lift, pressed as a smaller local darkening, active as the strongest state, and focus as a distinct 1-DIP accessible ring;
- the existing static liquid-glass highlight and rounded desktop blur, with no extra outer border and no rectangular backdrop.

- [ ] **Step 4: Run renderer tests and compile the native path**

Run: `cargo test -p shell-renderer quick_settings`

Expected: PASS for layout/source structure and successful compilation of the Direct2D renderer.

- [ ] **Step 5: Commit rendering**

```powershell
git add crates/shell-renderer/src/native_showcase_quick_settings.rs crates/shell-renderer/src/native_showcase.rs crates/shell-renderer/src/native.rs crates/shell-renderer/src/lib.rs crates/shell-renderer/tests/native_slice.rs
git commit -m "feat: render liquid glass quick settings panel"
```

## Task 7: Top-Bar Entry Point and Existing Popover Runtime

**Files:**
- Modify: `crates/shell-platform-windows/src/topbar_controller.rs`
- Modify: `crates/shell-platform-windows/tests/topbar_controller.rs`
- Modify: `crates/shell-platform-windows/src/win32_owner.rs`
- Modify: `crates/shell-platform-windows/src/win32_popover_render.rs`
- Modify: `crates/shell-platform-windows/src/win32_windowing.rs`
- Modify: `crates/shell-platform-windows/src/runtime.rs`
- Modify: `crates/shell-renderer/src/popover_layout.rs`

- [ ] **Step 1: Write the failing entry-point test**

```rust
#[test]
fn persisted_notifications_slot_is_presented_as_controls() {
    let controller = TopbarController::new(ShellState::default(), TopbarSnapshot::default());
    let scene = controller.scene();
    let controls = scene.modules().iter().find(|module| module.kind() == TopbarModuleKind::Notifications).unwrap();
    assert_eq!(controls.label(), "Controls");
    assert_eq!(controls.intent(), Some(TopbarIntent::Popover(Popover::QuickSettings)));
}
```

- [ ] **Step 2: Change only presentation and intent metadata**

For `TopbarModuleKind::Notifications`, use the sliders glyph `\u{E713}`, label `Controls`, neutral status, and `Popover::QuickSettings`. Do not rename the persisted enum variant.

- [ ] **Step 3: Own and route `QuickSettingsController`**

Add it to `RuntimeSurfaces`. When `Popover::QuickSettings` is invoked, dismiss the generic `PopoverController`, read a fresh capability snapshot, open the quick controller, place the existing popover using `quick_settings_surface_size`, and set the same active top-bar anchor. Generic popovers dismiss the quick controller. Context menus remain mutually exclusive with both.

Route popover keyboard, pointer move/down/up, wheel, Escape, DPI, and placement through the quick controller when active. Slider capture begins on pointer down and ends on pointer up or `WM_CAPTURECHANGED`.

- [ ] **Step 4: Render the exclusive scene on the existing HWND**

In `redraw_popover`, pass either `popover: scene.as_ref(), quick_settings: None` or `popover: None, quick_settings: quick_scene.as_ref()`. In `place_active_overlay`, choose `quick_settings_surface_size` when the quick controller is active. Do not create a new window class, HWND, swap chain, or system backdrop.

- [ ] **Step 5: Run focused top-bar and controller tests**

Run: `cargo test -p shell-platform-windows topbar_controller quick_settings_controller`

Expected: PASS with Controls opening the typed Quick Settings route and all other popovers retaining their previous routes.

- [ ] **Step 6: Commit runtime integration**

```powershell
git add crates/shell-platform-windows/src/topbar_controller.rs crates/shell-platform-windows/tests/topbar_controller.rs crates/shell-platform-windows/src/win32_owner.rs crates/shell-platform-windows/src/win32_popover_render.rs crates/shell-platform-windows/src/win32_windowing.rs crates/shell-platform-windows/src/runtime.rs crates/shell-renderer/src/popover_layout.rs
git commit -m "feat: open adaptive controls from topbar"
```

## Task 8: Quick Controls Editor in Settings

**Files:**
- Modify: `crates/shell-renderer/src/settings_scene.rs`
- Modify: `crates/shell-renderer/src/native_showcase_settings.rs`
- Modify: `crates/shell-platform-windows/src/settings_controller.rs`
- Modify: `crates/shell-platform-windows/tests/settings_controller.rs`
- Modify: `crates/shell-platform-windows/src/win32_owner.rs`
- Modify: `crates/shell-platform-windows/src/win32_popover_render.rs`
- Modify: `crates/shell-platform-windows/src/win32_windowing.rs`
- Modify: `crates/shell-platform-windows/src/win32_config.rs`

- [ ] **Step 1: Write failing Settings behavior tests**

```rust
#[test]
fn quick_controls_editor_lists_only_supported_controls() {
    let mut controller = SettingsController::new(ShellConfigV1::default());
    controller.set_supported_quick_controls([QuickControlKind::Wifi, QuickControlKind::Focus]);
    controller.open_section(SettingsSection::QuickControls);
    let scene = controller.scene();
    assert_eq!(scene.quick_controls().unwrap().iter().map(|row| row.kind()).collect::<Vec<_>>(), vec![QuickControlKind::Wifi, QuickControlKind::Focus]);
}

#[test]
fn hiding_and_reordering_supported_control_preserves_absent_preferences() -> Result<(), SettingsError> {
    let mut controller = SettingsController::new(ShellConfigV1::default());
    controller.set_supported_quick_controls([QuickControlKind::Wifi, QuickControlKind::Focus]);
    controller.edit(SettingsEdit::QuickControlVisibility { control: QuickControlKind::Focus, visible: false })?;
    controller.edit(SettingsEdit::QuickControlReorder { control: QuickControlKind::Focus, before: Some(QuickControlKind::Wifi) })?;
    assert!(controller.preview().quick_settings().controls().iter().any(|item| item.kind() == QuickControlKind::Bluetooth));
    Ok(())
}
```

- [ ] **Step 2: Add the Quick Controls section and typed rows**

Insert `SettingsSection::QuickControls` after Modules and add overview copy `Show, hide, and reorder compatible controls`. Extend `SettingsScene` with `SettingsContent::Overview(Vec<SettingsRow>)` and `SettingsContent::QuickControls(Vec<QuickControlSettingsRow>)`. Each Quick Controls row contains kind, label, visible, can-move-up, can-move-down, and focused action.

- [ ] **Step 3: Implement pointer and keyboard editing**

Add `SettingsPointer(DipPoint)` and Settings hit testing. A row exposes visibility, move-up, and move-down targets. Enter/Space activates the focused target; Escape returns to overview on the first press and dismisses Settings from overview. Every edit updates the draft/preview and produces `QueuedSettingsAction::ApplyQuickSettings`.

- [ ] **Step 4: Persist edits and refresh the open panel**

Handle `ApplyQuickSettings` by calling `settings_controller.apply()`, persisting through a narrow `win32_config::persist_config(&ShellConfigV1)` wrapper, updating `QuickSettingsController` preferences, and rebuilding its scene if open. Persist only after validation succeeds; on failure retain the previous committed config and display a Settings status error.

- [ ] **Step 5: Draw the editor**

In `native_showcase_settings.rs`, draw a Quick Controls heading, concise explanation, and one continuous ordered list. Use the existing settings material. Each row has a visibility checkbox and explicit up/down icon buttons; unsupported controls are not present. When the supported list is empty, draw `No compatible controls are currently available.`

- [ ] **Step 6: Run focused Settings tests**

Run: `cargo test -p shell-platform-windows settings_controller`

Expected: PASS for supported-only filtering, hide/show, reorder, absent-preference preservation, apply/persist action, and Escape behavior.

- [ ] **Step 7: Commit Settings integration**

```powershell
git add crates/shell-renderer/src/settings_scene.rs crates/shell-renderer/src/native_showcase_settings.rs crates/shell-platform-windows/src/settings_controller.rs crates/shell-platform-windows/tests/settings_controller.rs crates/shell-platform-windows/src/win32_owner.rs crates/shell-platform-windows/src/win32_popover_render.rs crates/shell-platform-windows/src/win32_windowing.rs crates/shell-platform-windows/src/win32_config.rs
git commit -m "feat: configure compatible quick controls"
```

## Task 9: Event-Driven Refresh and Focused Native Verification

**Files:**
- Modify: `crates/shell-platform-windows/src/lib.rs`
- Modify: `crates/shell-platform-windows/src/runtime.rs`
- Modify: `crates/shell-platform-windows/src/win32_windowing.rs`
- Modify: `crates/shell-platform-windows/src/win32_owner.rs`
- Modify: `crates/shell-platform-windows/tests/quick_settings_controller.rs`

- [ ] **Step 1: Add refresh classification tests**

Test a pure `quick_settings_refresh_scope(event)` mapping:

```rust
assert_eq!(quick_settings_refresh_scope(&PlatformEvent::DisplayChanged), Some(RefreshScope::Display));
assert_eq!(quick_settings_refresh_scope(&PlatformEvent::PowerResumed), Some(RefreshScope::All));
assert_eq!(quick_settings_refresh_scope(&PlatformEvent::TopbarPointer(sample)), None);
```

- [ ] **Step 2: Route device and subsystem notifications**

Handle `WM_DEVICECHANGE` as `PlatformEvent::QuickSettingsRefresh(RefreshScope::Devices)`. Reuse `DisplayChanged` and `PowerResumed`; add audio-endpoint and WLAN notifications only if their documented callbacks can enqueue a lightweight event. Coalesce repeated notifications into one pending refresh before querying adapters. Do not add a timer.

- [ ] **Step 3: Refresh only relevant open/configuration surfaces**

When Quick Settings is open, refresh the requested scope and redraw/reposition if content height changes. When Settings Quick Controls is open, update its supported list. When neither surface is open, invalidate the cached snapshot and defer the actual query until the next open.

- [ ] **Step 4: Run the freshest practical verification**

Run:

```powershell
cargo fmt --all -- --check
cargo test -p shell-core quick_settings
cargo test -p shell-config quick_settings
cargo test -p shell-renderer quick_settings
cargo test -p shell-platform-windows quick_settings
cargo build -p shell-app --target x86_64-pc-windows-msvc
```

Expected: formatting clean; focused tests pass; native debug executable builds at `target\x86_64-pc-windows-msvc\debug\shell-app.exe`.

- [ ] **Step 5: Perform one concise manual native pass**

Open the executable and verify on the available PC:

- Controls opens the new panel directly.
- Missing Wi-Fi/Bluetooth/battery controls are absent on unsupported hardware.
- Hover and keyboard focus are visible.
- Wallpaper detail is blurred only inside the rounded panel.
- The grid has no empty holes.
- Volume changes and Settings routes behave correctly.
- Edit controls opens the supported-only Settings section and persists one reorder.

Use synthetic controller fixtures for notebook-only combinations instead of requiring additional physical machines.

- [ ] **Step 6: Commit refresh wiring and any verification-only fixes**

```powershell
git add crates/shell-platform-windows/src/lib.rs crates/shell-platform-windows/src/runtime.rs crates/shell-platform-windows/src/win32_windowing.rs crates/shell-platform-windows/src/win32_owner.rs crates/shell-platform-windows/tests/quick_settings_controller.rs
git commit -m "feat: refresh adaptive controls from system events"
```

## Implementation Notes

- Preserve all unrelated dirty worktree changes and stage only the files named in each task.
- Do not rename `TopbarModuleKind::Notifications` in persisted configuration during this delivery.
- Do not revive the current `control_center_rows()` UI for `Popover::QuickSettings`; the generic row popover remains for other surfaces.
- Do not use undocumented registry writes for Dark mode, Night light, Focus, Nearby sharing, Bluetooth, or Energy saver.
- Do not scan Wi-Fi networks or nearby Bluetooth devices to decide whether a control exists.
- Do not add continuous timers, capture loops, or per-frame capability calls.
- Keep diagnostics free of SSIDs, device names beyond the displayed audio endpoint, desktop pixels, and raw adapter identifiers.
