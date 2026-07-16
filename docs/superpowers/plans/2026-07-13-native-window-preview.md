# Native Window Preview Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a dedicated, stable native preview above the dock that shows every window for the hovered application, supports pointer and keyboard interaction, preserves dock auto-hide state, and matches the current dark liquid-glass visual language.

**Architecture:** Keep timing and interaction rules in a pure `PreviewController`; group discovered windows in `DockController`; compute all panel/card geometry in pure renderer layout code; host the result in one dedicated preview HWND per monitor; reuse the existing RAII DWM thumbnail wrapper through a bounded four-thumbnail collection. The dock remains independent: it supplies the anchor item and reveal hold, but never renders preview pixels inside its own 55-DIP surface.

**Tech Stack:** Rust workspace, Win32, Direct2D/DirectWrite, DWM thumbnails, existing `shell-core`, `shell-renderer`, and `shell-platform-windows` crates.

**Git policy:** The worktree already contains user changes and there is no authorization to stage or commit. Do not stage, commit, reset, move, or revert files. End each task with the listed focused verification and inspect only the scoped diff.

---

## Product constants

Use named constants instead of scattered literals:

```rust
pub const PREVIEW_DWELL_MS: u64 = 450;
pub const PREVIEW_BRIDGE_MS: u64 = 200;
pub const PREVIEW_MOTION_MS: u64 = 160;
pub const PREVIEW_PAGE_SIZE: usize = 4;
pub const PREVIEW_PANEL_RADIUS_DIP: f32 = 12.0;
pub const PREVIEW_WORK_MARGIN_DIP: f32 = 16.0;
pub const PREVIEW_DOCK_GAP_DIP: f32 = 10.0;
pub const PREVIEW_PANEL_PADDING_DIP: f32 = 12.0;
pub const PREVIEW_CARD_GAP_DIP: f32 = 10.0;
```

## Task 1: Preserve every discovered window and its metadata

**Files:**

- Modify: `crates/shell-platform-windows/src/dock_window_sync.rs`
- Modify: `crates/shell-platform-windows/src/win32_discovery.rs`
- Modify: `crates/shell-platform-windows/src/dock_controller.rs`
- Modify: `crates/shell-platform-windows/src/dock_controller_sync.rs`
- Modify: `crates/shell-platform-windows/tests/window_previews.rs`

- [ ] **Step 1: Add failing coverage for a multi-window application**

Extend `window_previews.rs` with one app owning three `ObservedWindow` values. Assert that:

- all three stable `WindowId` values are returned in the app's preview group;
- the focused window sorts first, followed by title and then `WindowId` for deterministic order;
- focus and close actions work for the second and third windows, not only the primary `RunningState` window;
- a stale window disappears after the next discovery sync.

Add an assertion shaped like:

```rust
let group = controller.preview_windows_for_item(item);
assert_eq!(
    group.iter().map(|window| window.window()).collect::<Vec<_>>(),
    vec![WindowId::new(703), WindowId::new(701), WindowId::new(702)],
);
assert!(matches!(
    controller.handle_preview_action(WindowId::new(702), PreviewAction::Close)?[..],
    [QueuedDockAction::Preview(PreviewQueuedAction::Close { .. })]
));
```

- [ ] **Step 2: Run the focused test and confirm the current single-window model fails**

Run:

```powershell
cargo test -p shell-platform-windows --test window_previews --all-features
```

Expected: the new test fails because `DockController` currently stores capture only and `app_for_window` checks only the primary `RunningState` window.

- [ ] **Step 3: Extend observed window metadata without exposing HWNDs**

Add a title field and builder to `ObservedWindow`:

```rust
pub struct ObservedWindow {
    window: WindowId,
    app: AppId,
    title: String,
    foreground: bool,
    minimized: bool,
    preview: Option<PreviewCapture>,
    fullscreen: bool,
}

pub fn with_title(mut self, title: impl Into<String>) -> Self {
    self.title = title.into();
    self
}

#[must_use]
pub fn title(&self) -> &str {
    &self.title
}
```

In `win32_discovery.rs`, reuse the existing `GetWindowTextLengthW` flow to read the UTF-16 title once and attach it to `ObservedWindow`. Do not retain the raw HWND in renderer-facing data.

- [ ] **Step 4: Replace the capture-only map with complete per-window state**

Add an internal value type:

```rust
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreviewWindowState {
    window: WindowId,
    app: AppId,
    title: String,
    capture: PreviewCapture,
    foreground: bool,
    minimized: bool,
}
```

Store `HashMap<WindowId, PreviewWindowState>` in `DockController`. `sync_previews` replaces the whole snapshot each discovery pass. Resolve preview actions directly through this map so every secondary window retains its application identity.

Expose a crate-visible `preview_windows_for_item(item)` that filters by the dock item's `AppId` and returns a deterministic ordered vector.

- [ ] **Step 5: Verify grouping and secondary-window actions**

Run:

```powershell
cargo test -p shell-platform-windows --test window_previews --all-features
cargo test -p shell-platform-windows dock_controller --all-features
```

Expected: both pass; no behavior change for launch, pin, focus, or close of a single-window app.

## Task 2: Introduce renderer-ready preview scene and pure layout

**Files:**

- Create: `crates/shell-renderer/src/window_preview_scene.rs`
- Create: `crates/shell-renderer/src/window_preview_layout.rs`
- Modify: `crates/shell-renderer/src/lib.rs`
- Modify: `crates/shell-renderer/src/native.rs`
- Create: `crates/shell-renderer/tests/window_preview_scene.rs`
- Create: `crates/shell-renderer/tests/window_preview_layout.rs`

- [ ] **Step 1: Write failing scene pagination tests**

Cover zero, one, four, five, and nine cards. Assert page count, visible slice, and page correction after a close. The public model should be renderer-only and contain no `HWND`:

```rust
pub struct PreviewCardVisual {
    pub window: WindowId,
    pub title: String,
    pub capture: WindowPreviewCapture,
    pub focused: bool,
    pub minimized: bool,
}

pub struct WindowPreviewScene {
    pub item: DockItemId,
    pub app_label: String,
    pub icon: DockIcon,
    pub cards: Vec<PreviewCardVisual>,
    pub page: usize,
    pub hovered_card: Option<WindowId>,
    pub focused_card: Option<WindowId>,
    pub reduced_motion: bool,
}
```

- [ ] **Step 2: Write failing geometry tests**

Test:

- one window produces one approximately 320 x 200 DIP thumbnail region;
- two to four windows use two columns without overlap;
- five or more still lay out only four visible cards;
- title, thumbnail, close control, and pagination bounds remain inside the panel;
- physical placement is centered on the invoking icon, 10 DIP above the dock, and clamped to a 16 DIP work-area margin;
- negative monitor coordinates and 100%, 150%, 200%, and 250% DPI round without leaving the work area.

Use data types similar to:

```rust
pub struct PreviewCardLayout {
    pub window: WindowId,
    pub card: DipRect,
    pub thumbnail: DipRect,
    pub title: DipRect,
    pub close: DipRect,
}

pub struct WindowPreviewLayout {
    pub panel: DipRect,
    pub cards: Vec<PreviewCardLayout>,
    pub previous: Option<DipRect>,
    pub next: Option<DipRect>,
    pub page_indicator: Option<DipRect>,
}
```

- [ ] **Step 3: Confirm tests fail before the modules exist**

Run:

```powershell
cargo test -p shell-renderer --test window_preview_scene --all-features
cargo test -p shell-renderer --test window_preview_layout --all-features
```

Expected: compile failure for the missing public preview scene/layout API.

- [ ] **Step 4: Implement scene paging and layout as pure functions**

Implement:

```rust
impl WindowPreviewScene {
    pub fn page_count(&self) -> usize;
    pub fn corrected_page(&self) -> usize;
    pub fn visible_cards(&self) -> &[PreviewCardVisual];
}

pub fn preview_panel_size(card_count: usize, paginated: bool) -> PreviewPanelSize;
pub fn layout_window_preview(scene: &WindowPreviewScene, surface: DipRect)
    -> WindowPreviewLayout;
pub fn place_window_preview(
    anchor: PhysicalRect,
    dock: PhysicalRect,
    work: PhysicalRect,
    dpi: Dpi,
    size: PreviewPanelSize,
) -> PhysicalRect;
```

Keep hit testing as methods on `WindowPreviewLayout`: `card_at`, `close_at`, `previous_at`, and `next_at`. Layout must use one centralized rounding boundary when converting DIP rectangles to DWM destination rectangles.

- [ ] **Step 5: Add preview scene to the native renderer contract**

Add `ShowcaseRole::Preview` and `ShellScenes.preview: Option<WindowPreviewScene>`. Update all exhaustive matches and constructors without changing existing dock, topbar, popover, context-menu, or settings scenes.

- [ ] **Step 6: Verify pure renderer behavior**

Run:

```powershell
cargo test -p shell-renderer --test window_preview_scene --all-features
cargo test -p shell-renderer --test window_preview_layout --all-features
cargo test -p shell-renderer --all-features
```

Expected: all pass.

## Task 3: Implement the preview interaction state machine

**Files:**

- Create: `crates/shell-platform-windows/src/preview_controller.rs`
- Modify: `crates/shell-platform-windows/src/lib.rs`
- Create: `crates/shell-platform-windows/tests/preview_controller.rs`

- [ ] **Step 1: Write state-transition tests first**

Cover exact boundaries:

- initial target remains `Dwelling` at 449 ms and becomes `Visible` at 450 ms;
- moving from app A to app B while visible emits immediate replacement without a new dwell;
- leaving dock starts the bridge and entering preview before 200 ms cancels dismissal;
- leaving both remains visible at 199 ms and closes at 200 ms;
- `Space` opens immediately from keyboard focus;
- arrows wrap within the visible page, page controls change page, Enter focuses, close acts on only the selected card, Escape dismisses and requests focus restoration;
- window list changes clamp the current page and close when no cards remain;
- reduced motion skips `Closing` and dismisses immediately.

- [ ] **Step 2: Run the test and confirm the controller API is absent**

Run:

```powershell
cargo test -p shell-platform-windows --test preview_controller --all-features
```

Expected: compile failure for missing `PreviewController` and typed inputs/effects.

- [ ] **Step 3: Implement a pure controller**

Use typed events and effects, with no Win32 calls:

```rust
pub enum PreviewPhase {
    Closed,
    Dwelling { item: DockItemId, deadline_ms: u64 },
    Visible {
        item: DockItemId,
        page: usize,
        hovered: Option<WindowId>,
        focused: Option<WindowId>,
        bridge_deadline_ms: Option<u64>,
    },
    Closing { item: DockItemId, deadline_ms: u64 },
}

pub enum PreviewInput {
    DockTargetChanged { item: Option<DockItemId>, now_ms: u64 },
    OpenFromKeyboard { item: DockItemId, now_ms: u64 },
    PreviewEntered,
    PreviewLeft { now_ms: u64 },
    PointerMoved { point: DipPoint },
    PointerReleased { point: DipPoint },
    Key(PreviewKey),
    WindowListChanged { window_ids: Vec<WindowId> },
    FocusLost,
    OutsideInteraction,
    Tick { now_ms: u64 },
}

pub enum PreviewEffect {
    Show { item: DockItemId, page: usize },
    Update { item: DockItemId, page: usize },
    BeginClose,
    Hide,
    HoldDockReveal,
    ReleaseDockReveal,
    FocusWindow(WindowId),
    CloseWindow(WindowId),
    RestoreDockFocus(DockItemId),
}
```

The host passes the current layout into pointer hit testing; the controller stores stable IDs and page state, never rectangles or HWNDs.

- [ ] **Step 4: Verify timing and keyboard behavior**

Run:

```powershell
cargo test -p shell-platform-windows --test preview_controller --all-features
```

Expected: all transition cases pass with deterministic synthetic timestamps and no sleeps.

## Task 4: Create the native preview visual surface

**Files:**

- Create: `crates/shell-renderer/src/native_showcase_preview.rs`
- Modify: `crates/shell-renderer/src/native_showcase.rs`
- Modify: `crates/shell-renderer/src/native_showcase_material.rs`
- Modify: `crates/shell-renderer/src/native_showcase_resources.rs`
- Modify: `crates/shell-renderer/src/lib.rs`
- Create: `crates/shell-renderer/tests/window_preview_render.rs`

- [ ] **Step 1: Add a render-command test**

For a four-card scene, assert the renderer emits:

- one rounded 12-DIP glass panel clipped to its rounded path;
- one subdued thumbnail backdrop per card;
- title text with ellipsis bounds;
- close control only for hovered/focused cards;
- pagination only when page count exceeds one;
- a safe icon/title/reason placeholder for restricted capture;
- no opaque full-window rectangle behind the rounded panel.

- [ ] **Step 2: Implement the preview renderer using existing tokens**

Render order:

1. transparent clear;
2. exterior shadow bounded to the rounded panel;
3. dark translucent glass fill;
4. half-pixel-equivalent rim and subtle inner top response;
5. card thumbnail backdrops/placeholders;
6. titles and controls;
7. focus/hover states and pagination.

Reuse `ShowcaseTokens` for surface, raised, hover, primary/secondary text, rim, and elevation. Do not introduce Apple assets, logos, traffic-light controls, or invented application icons.

The 160 ms entrance is opacity plus a 6 DIP upward translation. When `scene.reduced_motion` is true, render directly at the final state.

- [ ] **Step 3: Update role-specific resources**

Give `ShowcaseRole::Preview` its own DirectWrite format selection using Segoe UI Variable and the existing body/caption weights. Keep material fallback compatible with high contrast and solid-material mode.

- [ ] **Step 4: Verify render commands and existing surfaces**

Run:

```powershell
cargo test -p shell-renderer --test window_preview_render --all-features
cargo test -p shell-renderer --all-features
```

Expected: preview render test and all dock/topbar/menu renderer tests pass.

## Task 5: Add one dedicated preview HWND per monitor

**Files:**

- Modify: `crates/shell-platform-windows/src/win32.rs`
- Modify: `crates/shell-platform-windows/src/win32_window.rs`
- Modify: `crates/shell-platform-windows/src/win32_windowing.rs`
- Modify: `crates/shell-platform-windows/src/win32_timer.rs`
- Modify: `crates/shell-platform-windows/src/win32_backdrop.rs`
- Modify: `crates/shell-platform-windows/src/win32_hit_test.rs`
- Modify: `crates/shell-platform-windows/src/native_event_route.rs`
- Modify: `crates/shell-platform-windows/src/win32_slots.rs`
- Modify: `crates/shell-platform-windows/src/win32_slot_lifecycle.rs`
- Modify: `crates/shell-platform-windows/tests/native_event_routing.rs`

- [ ] **Step 1: Add a failing routing test for the new role**

Extend `NativeWindowSlot::new` with a preview window ID. Assert preview pointer, keyboard, focus-loss, timer, DPI, display, and close messages route to the correct monitor slot. Assert closing a preview dismisses it and does not emit the application-wide `CloseRequested` event.

- [ ] **Step 2: Register the preview role and HWND**

Add a preview entry to global role lookup and each `ShellSlot`. Create the HWND during slot lifecycle and add it to discovery's excluded HWND list so the shell never previews itself.

Use a tool-window style that can support keyboard activation, but show it without activation for pointer hover:

```rust
pub fn show_preview_pointer(&self) {
    unsafe { ShowWindow(self.hwnd, SW_SHOWNOACTIVATE) };
}

pub fn show_preview_keyboard(&self) {
    unsafe {
        ShowWindow(self.hwnd, SW_SHOW);
        SetForegroundWindow(self.hwnd);
        SetFocus(self.hwnd);
    }
}
```

Do not mark it `WS_EX_NOACTIVATE` permanently, because keyboard-opened previews must receive arrows, Enter, and Escape. Use `WS_EX_TOPMOST | WS_EX_TOOLWINDOW` and the correct show method per invocation.

- [ ] **Step 3: Add preview positioning and backdrop**

Add an `OwnedWindow::place_preview` method that consumes the already-tested physical rectangle from `place_window_preview`. Apply rounded hit testing/backdrop to `ShowcaseRole::Preview` with the same fallback hierarchy used by popovers, without reusing the popover HWND or focus scope.

- [ ] **Step 4: Route input and a non-blocking timer**

Add a unique timer ID after the current animation timer, for example `0x4D58`. The timer ticks at roughly one frame interval but all boundaries are determined by monotonic `now_ms`, not tick counts.

Route:

- `WM_MOUSEMOVE`, `WM_MOUSELEAVE`, `WM_LBUTTONUP`;
- `WM_KEYDOWN`, `WM_KILLFOCUS`, `WM_ACTIVATE`;
- `WM_TIMER`, `WM_DPICHANGED`, `WM_DISPLAYCHANGE`;
- preview-specific `WM_CLOSE` to dismissal only.

- [ ] **Step 5: Split Space from Enter on dock keyboard input**

Add `DockKey::Preview`. Map Space to `Preview`, keep Enter as `Activate`, and preserve existing Left/Right/Escape behavior. Add/update unit tests for the key mapping.

- [ ] **Step 6: Verify native role lifecycle and routing**

Run:

```powershell
cargo test -p shell-platform-windows --test native_event_routing --all-features
cargo test -p shell-platform-windows win32_windowing --all-features
```

Expected: all routes pass and preview close cannot terminate the shell.

## Task 6: Generalize DWM thumbnail hosting to a bounded collection

**Files:**

- Modify: `crates/shell-platform-windows/src/win32_preview.rs`
- Create: `crates/shell-platform-windows/src/win32_preview_host.rs`
- Modify: `crates/shell-platform-windows/src/win32_owner.rs`
- Modify: `crates/shell-platform-windows/src/win32_dock_render.rs`
- Modify: `crates/shell-platform-windows/src/win32_slot_lifecycle.rs`
- Create: `crates/shell-platform-windows/tests/preview_thumbnail_host.rs`

- [ ] **Step 1: Add failure-isolation and RAII tests**

Using the existing fake DWM API seam, assert:

- up to four visible card thumbnails register against the preview HWND;
- paging unregisters old thumbnails and registers only the new visible page;
- restricted cards never call `DwmRegisterThumbnail`;
- one registration/update failure marks only that card unavailable;
- partial construction failure unregisters every earlier successful registration;
- hide, device rebuild, slot removal, and drop leave zero registrations.

- [ ] **Step 2: Implement a bounded thumbnail host**

Build on `RegisteredThumbnail<DwmThumbnailApi>`:

```rust
pub struct DwmPreviewHost {
    thumbnails: Vec<(WindowId, DwmPreviewThumbnail)>,
}

impl DwmPreviewHost {
    pub fn sync(
        &mut self,
        host: HWND,
        cards: &[PreviewCardVisual],
        layout: &WindowPreviewLayout,
        dpi: Dpi,
    ) -> Vec<(WindowId, PreviewUnavailableReason)>;

    pub fn clear(&mut self);
}
```

Register only after final placement is known. Convert the thumbnail DIP rectangle to client physical pixels once. Preserve source aspect ratio inside the destination via letterboxing; use source-client-area-only and existing privacy behavior. Do not re-register unchanged window/page pairs on every animation tick.

- [ ] **Step 3: Remove the embedded dock preview path**

Delete the `MINHA_UI_EXPERIMENTAL_DOCK_PREVIEW` gate, `dock_preview_allowed`, the 55-DIP embedded destination calculation, and `RuntimeSurfaces.preview_thumbnail`. Replace them with the preview HWND host. Remove the tiny in-dock unavailable badge only if it is no longer referenced by another feature.

- [ ] **Step 4: Use the preview HWND for discovery probing**

Change slot lifecycle from `preview_host: Some(dock.hwnd)` to `Some(preview.hwnd)`. A failed probe remains metadata for a safe placeholder and must not abort discovery.

- [ ] **Step 5: Verify cleanup and no embedded-preview regression**

Run:

```powershell
cargo test -p shell-platform-windows --test preview_thumbnail_host --all-features
cargo test -p shell-platform-windows win32_preview --all-features
rg -n "MINHA_UI_EXPERIMENTAL_DOCK_PREVIEW|dock_preview_allowed|embedded_dock_preview" crates
```

Expected: tests pass and `rg` returns no product-code matches.

## Task 7: Integrate controller, dock reveal hold, rendering, and actions

**Files:**

- Modify: `crates/shell-platform-windows/src/lib.rs`
- Modify: `crates/shell-platform-windows/src/runtime.rs`
- Modify: `crates/shell-platform-windows/src/win32_owner.rs`
- Create: `crates/shell-platform-windows/src/win32_preview_render.rs`
- Modify: `crates/shell-platform-windows/src/win32_actions.rs`
- Modify: `crates/shell-platform-windows/src/dock_controller_interaction.rs`
- Modify: `crates/shell-platform-windows/tests/window_previews.rs`
- Create: `crates/shell-platform-windows/tests/preview_integration.rs`

- [ ] **Step 1: Add an integration test around runtime effects**

Drive synthetic platform events through the orchestrator and assert:

- 450 ms hover shows the preview for the current item;
- when visible, another eligible hovered item updates immediately;
- `HoldDockReveal` calls `DockController::hold_revealed_for_overlay` once;
- moving through the 200 ms bridge keeps the dock and preview visible;
- dismissal calls `release_overlay_hold` once;
- focus/close effects reuse the existing `PreviewQueuedAction` pipeline;
- window discovery updates the scene in place and hides only when the group becomes empty;
- preview interaction does not show, hide, resize, or repaint the topbar.

- [ ] **Step 2: Add typed platform events**

Extend `PlatformEvent` with preview-specific events rather than overloading popover events:

```rust
PreviewPointerMoved(DipPoint),
PreviewPointerExited,
PreviewPointerReleased(DipPoint),
PreviewKey(PreviewKey),
PreviewFocusLost,
PreviewTimer { now_ms: u64 },
PreviewOutsideInteraction,
```

Update `RuntimeOrchestrator::handle` exhaustively.

- [ ] **Step 3: Build a scene from the current app group**

Translate `PreviewWindowState` to `PreviewCardVisual` using the existing Windows-provided dock icon source. Never invent or substitute an app icon unless capture is unavailable and the same resolved app icon is used as a placeholder.

Clamp page before calling `visible_cards`. If any DWM sync fails, replace only that card's capture state and rerender once.

- [ ] **Step 4: Apply controller effects without recursive state changes**

Centralize effects in `win32_preview_render.rs`:

- `Show` / `Update`: build scene, compute placement, resize only if size changed, render, sync DWM, then show;
- `HoldDockReveal`: call `hold_revealed_for_overlay`;
- `BeginClose`: animate only the preview surface;
- `Hide`: hide host, clear DWM registrations, stop timer when idle;
- `ReleaseDockReveal`: call `release_overlay_hold` after host/DWM teardown;
- `FocusWindow` / `CloseWindow`: queue the existing native action;
- `RestoreDockFocus`: focus the originating item without activating an application.

Ensure `Show` and cross-item `Update` do not recreate Direct2D device resources. A content update may render and reposition the preview only; it must not touch the topbar window.

- [ ] **Step 5: Make dock and preview one reveal region**

When preview is visible, entering either the invoking dock item or preview cancels the bridge deadline. Leaving both starts it. The existing `overlay_reveal_hold` remains the single source of truth for auto-hide suppression.

- [ ] **Step 6: Verify runtime integration**

Run:

```powershell
cargo test -p shell-platform-windows --test preview_integration --all-features
cargo test -p shell-platform-windows --test window_previews --all-features
cargo test -p shell-platform-windows --all-features
```

Expected: all pass; no topbar or dock animation regression tests fail.

## Task 8: Accessibility, localization, and resilience

**Files:**

- Modify: `crates/shell-platform-windows/src/win32_preview_render.rs`
- Modify: `crates/shell-platform-windows/src/win32_windowing.rs`
- Modify: `crates/shell-renderer/src/window_preview_scene.rs`
- Modify: `crates/shell-renderer/src/native_showcase_preview.rs`
- Modify: localized strings/resources used by the existing shell, if present
- Modify: `crates/shell-platform-windows/tests/preview_integration.rs`

- [ ] **Step 1: Add keyboard and fallback integration cases**

Cover:

- focus order follows visual order across pages;
- close is a separate action and never the card default;
- Escape returns focus to the originating dock item;
- restricted, stale, minimized-without-frame, and failed-capture cards expose a textual reason;
- high-contrast/solid material retains readable titles and controls;
- reduced motion keeps behavior identical while snapping transitions.

- [ ] **Step 2: Add accessibility names at the native boundary**

Each card must expose application name, window title, focused/minimized state, and actions. Keep stable IDs during in-place discovery updates so keyboard focus does not jump unnecessarily.

- [ ] **Step 3: Localize user-visible fallback strings**

Use the project's current string/resource mechanism for short states such as capture unavailable, protected content, previous page, next page, focus window, and close window. Do not log or persist window titles beyond the in-memory discovery snapshot.

- [ ] **Step 4: Verify accessibility/resilience tests**

Run:

```powershell
cargo test -p shell-platform-windows --test preview_integration --all-features
cargo test -p shell-renderer window_preview --all-features
```

Expected: all pass.

## Task 9: Release-build native QA on both monitors

**Files:**

- Modify only if required by a reproduced defect discovered in this task.

- [ ] **Step 1: Run focused formatting, lint, tests, and release build**

Run:

```powershell
cargo fmt --all -- --check
cargo clippy -p shell-renderer -p shell-platform-windows --all-targets --all-features -- -D warnings
cargo test -p shell-renderer --all-features
cargo test -p shell-platform-windows --all-features
cargo build --release -p shell-app --all-features
```

Expected: every command exits 0. If an unrelated pre-existing failure appears, record the exact command and output separately; do not weaken or delete its test.

- [ ] **Step 2: Launch the release build and test real applications**

Use one single-window app, one app with two to four windows, and one app with five or more windows. Validate on both monitors and, where available, mixed DPI:

- initial preview appears after a deliberate 450 ms hover;
- moving to another app while open switches immediately;
- pointer can travel from icon to preview without dismissal;
- dock stays visible while preview/menu is open;
- cards focus/restore the correct windows;
- card close closes only that window;
- pagination remains stable after closing a card;
- protected/unavailable windows show a safe placeholder;
- keyboard Space/arrows/Enter/Escape work;
- outside interaction closes cleanly;
- no black rectangle, gray backing square, clipping, title overflow, dock resize leak, topbar blink, or dock auto-hide race occurs.

- [ ] **Step 3: Inspect visual quality at 100%, 150%, 200%, and 250% scale where feasible**

Capture screenshots with the preview near the left edge, center, right edge, negative-coordinate monitor, and above the auto-hidden dock. Confirm:

- 12-DIP rounded glass clipping is smooth;
- the rim follows the curve;
- the panel stays at least 16 DIP inside the work area;
- thumbnail aspect ratios are preserved;
- title and close control stay aligned;
- only four cards are visible per page;
- the preview remains anchored to the invoking icon without causing dock overflow.

- [ ] **Step 4: Inspect scoped diff and report verification**

Run:

```powershell
git status --short
git diff --check
git diff --stat
```

Expected: no whitespace errors; only preview-related files and unavoidable exhaustive-role updates are included. Do not stage or commit unless the user explicitly requests it.

## Completion criteria

The feature is complete only when the release build passes native QA on both monitors and all of the following are true:

- preview content lives in the dedicated preview HWND, never inside the dock;
- every current window for an app can be reached, focused, and individually closed;
- first-hover, immediate switching, bridge delay, and auto-hide reveal hold match the approved timings;
- pagination, work-area clamping, mixed DPI, reduced motion, privacy fallback, and DWM cleanup are covered by deterministic tests;
- the preview uses the approved dark translucent glass language with no rectangular backing artifact;
- preview interaction never blinks or reflows the topbar and never lets the dock hide underneath an active preview.
