# Quick Settings Media Player Implementation Plan

> **Execution:** implement this plan in the current `topbar-system-liquid` worktree. Keep the existing liquid-glass material and spacing tokens; the media reference controls information hierarchy only.

**Goal:** Replace the two left grid controls (`Nearby sharing` and `Snap windows`) with a native, event-driven Windows media player that spans exactly those two slots, keeps the last paused session visible, and opens an inline session selector from the artwork.

**Architecture:** A long-lived Windows media worker owns GSMTC/WinRT objects and subscriptions. It publishes platform-neutral snapshots through the existing routed event queue. `QuickSettingsController` owns selection and pending UI state and converts snapshots to renderer scene data. The renderer lays out a two-row media card beside the unchanged Focus and Projection tiles and caches decoded artwork by generation.

**Reference:** `docs/superpowers/specs/2026-07-19-quick-settings-media-and-night-light-design.md`

**Platform API:** `Windows.Media.Control::GlobalSystemMediaTransportControlsSessionManager`, session change events, media/playback properties, and `Try*Async` transport commands. These APIs are available from Windows 10 version 1809; failure to acquire the manager is a supported “no media player” state.

---

## Task 1: Add a platform-neutral media scene contract

**Files**

- Modify: `crates/shell-renderer/src/quick_settings_scene.rs`
- Modify: `crates/shell-renderer/src/lib.rs`
- Modify: `crates/shell-renderer/tests/quick_settings_scene.rs`
- Modify: `crates/shell-renderer/src/integration_tests.rs`

### 1.1 Write the failing scene tests

Add tests asserting that:

- the scene can hold one current player and multiple session choices;
- title and artist are preserved independently;
- transport availability and pending state are represented without inventing `QuickControlKind` values;
- artwork and each transport action produce distinct focus/hit identities.

Use these concrete renderer contracts:

```rust
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct QuickSettingsMediaSessionId(u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QuickSettingsPlaybackState {
    Playing,
    Paused,
    Stopped,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QuickSettingsMediaAction {
    Previous,
    TogglePlayback,
    Next,
}

#[derive(Clone, Debug, PartialEq)]
pub struct QuickSettingsMediaArtwork {
    generation: u64,
    encoded: std::sync::Arc<[u8]>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct QuickSettingsMediaPlayer {
    session_id: QuickSettingsMediaSessionId,
    source: String,
    title: String,
    artist: String,
    artwork: Option<QuickSettingsMediaArtwork>,
    playback: QuickSettingsPlaybackState,
    can_previous: bool,
    can_toggle: bool,
    can_next: bool,
    pending: Option<QuickSettingsMediaAction>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct QuickSettingsMediaChoice {
    session_id: QuickSettingsMediaSessionId,
    source: String,
    title: String,
    artist: String,
    artwork: Option<QuickSettingsMediaArtwork>,
    playback: QuickSettingsPlaybackState,
    selected: bool,
}
```

Extend `QuickSettingsFocus` and `QuickSettingsHit` with `MediaArtwork`, `MediaAction(QuickSettingsMediaAction)`, and `MediaSession(QuickSettingsMediaSessionId)`. Add `media: Option<QuickSettingsMediaPlayer>` and `media_choices: Option<Vec<QuickSettingsMediaChoice>>` to `QuickSettingsScene`, with builders/accessors matching the existing scene style.

Run: `cargo test -p shell-renderer quick_settings_scene`

Expected: RED until the contracts exist, then GREEN.

### 1.2 Export the contract and register tests

Re-export all media types from `shell-renderer/src/lib.rs` and ensure `integration_tests.rs` includes the existing test file. Do not add a media `QuickControlKind`; the player is a capability-driven composite.

Run: `cargo test -p shell-renderer quick_settings_scene`

### 1.3 Commit

```powershell
git add crates/shell-renderer/src/quick_settings_scene.rs crates/shell-renderer/src/lib.rs crates/shell-renderer/tests/quick_settings_scene.rs crates/shell-renderer/src/integration_tests.rs
git commit -m "feat: model quick settings media player"
```

---

## Task 2: Reserve the exact two-slot media geometry

**Files**

- Modify: `crates/shell-renderer/src/quick_settings_layout.rs`
- Modify: `crates/shell-renderer/tests/quick_settings_scene.rs`

### 2.1 Write the failing geometry tests

For a scene with media + Focus + Projection, assert:

- media bounds are `TILE_HEIGHT * 2 + GAP` high (120 DIP);
- media occupies the left column;
- Focus and Projection occupy the right column at 56 DIP each with an 8 DIP gap;
- the next section begins 8 DIP below the 120 DIP grid block;
- the artwork and three transport buttons have separate hit rectangles;
- without media, the remaining grid compacts normally and does not reserve a hole.

### 2.2 Implement named media layout objects

Add:

```rust
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct QuickSettingsLaidOutMedia {
    bounds: DipRect,
    artwork: DipRect,
    previous: DipRect,
    toggle: DipRect,
    next: DipRect,
}
```

When `scene.media().is_some()`, pull Focus and Projection from the ordinary tile list and place them in the right column. Place all other grid tiles after the composite block using the existing two-column algorithm. Hit-test media before ordinary tiles so overlapping edge pixels cannot trigger the wrong action.

Use 12 DIP card padding, 48 DIP artwork, an 8 DIP content gap, 30 DIP transport hit targets, and centered 16 DIP gaps between actions. These values fit inside 120 DIP without changing panel width.

Run: `cargo test -p shell-renderer quick_settings_scene`

### 2.3 Commit

```powershell
git add crates/shell-renderer/src/quick_settings_layout.rs crates/shell-renderer/tests/quick_settings_scene.rs
git commit -m "feat: lay out two-slot media control"
```

---

## Task 3: Build the deterministic media controller

**Files**

- Create: `crates/shell-platform-windows/src/media_session_types.rs`
- Create: `crates/shell-platform-windows/src/media_session_controller.rs`
- Modify: `crates/shell-platform-windows/src/lib.rs`
- Modify: `crates/shell-platform-windows/src/integration_tests.rs`
- Create: `crates/shell-platform-windows/tests/media_session_controller.rs`

### 3.1 Define the snapshot and command seam

Use a platform-neutral snapshot so controller tests need no WinRT:

```rust
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MediaSessionSnapshot {
    pub(crate) generation: u64,
    pub(crate) sessions: Vec<MediaSessionEntry>,
    pub(crate) current: Option<MediaSessionId>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MediaSessionEntry {
    pub(crate) id: MediaSessionId,
    pub(crate) source: String,
    pub(crate) title: String,
    pub(crate) artist: String,
    pub(crate) artwork: Option<MediaArtworkBytes>,
    pub(crate) playback: MediaPlaybackState,
    pub(crate) commands: MediaCommandAvailability,
    pub(crate) activity_sequence: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MediaWorkerCommand {
    Refresh,
    Transport { request: u64, session: MediaSessionId, action: MediaTransportAction },
}
```

### 3.2 Write controller tests first

Cover only the behaviors requested:

- manager current session is selected initially;
- manual selection stays fixed while other sessions change;
- selected paused media stays visible;
- closing the selected session falls back to the greatest `activity_sequence`;
- an empty snapshot removes the player and its selector;
- only one transport command per session/action is pending;
- a stale completion cannot clear a newer request;
- artwork activation opens the media session page; Back restores main.

Run: `cargo test -p shell-platform-windows media_session_controller`

### 3.3 Implement selection and pending state

`MediaSessionController` owns:

```rust
pub(crate) struct MediaSessionController {
    snapshot: MediaSessionSnapshot,
    selected: Option<MediaSessionId>,
    manually_selected: bool,
    pending: Option<PendingMediaCommand>,
    next_request: u64,
    last_error: String,
}
```

Selection rule order is fixed: preserve a still-live manual selection; otherwise use `snapshot.current`; otherwise use the live session with greatest `activity_sequence`; otherwise none. Never clear a paused session merely because playback stopped.

Run: `cargo test -p shell-platform-windows media_session_controller`

### 3.4 Commit

```powershell
git add crates/shell-platform-windows/src/media_session_types.rs crates/shell-platform-windows/src/media_session_controller.rs crates/shell-platform-windows/src/lib.rs crates/shell-platform-windows/src/integration_tests.rs crates/shell-platform-windows/tests/media_session_controller.rs
git commit -m "feat: control media session selection"
```

---

## Task 4: Add the event-driven Windows GSMTC worker

**Files**

- Create: `crates/shell-platform-windows/src/win32_media_sessions.rs`
- Create: `crates/shell-platform-windows/src/media_session_worker.rs`
- Modify: `crates/shell-platform-windows/src/lib.rs`
- Modify: `crates/shell-platform-windows/src/runtime.rs`
- Modify: `crates/shell-platform-windows/Cargo.toml`

### 4.1 Enable only the required WinRT namespaces

Add `Media_Control` and `Storage_Streams` to the existing `windows` feature list. Do not add a polling timer.

### 4.2 Implement one worker that owns all WinRT lifetimes

The worker thread:

1. calls `GlobalSystemMediaTransportControlsSessionManager::RequestAsync()?.join()`;
2. subscribes to `SessionsChanged` and `CurrentSessionChanged`;
3. subscribes each live session to `MediaPropertiesChanged`, `PlaybackInfoChanged`, and `TimelinePropertiesChanged`;
4. turns every callback into a coalesced `Refresh` command on its own channel;
5. emits `PlatformEvent::MediaSessionsChanged` only after it has built a complete snapshot;
6. runs transport commands through the matching `Try*Async()?.join()` and emits `MediaTransportCompleted` with the request id and returned boolean;
7. unregisters event tokens when sessions disappear and when the worker exits.

Read artwork bytes from `GlobalSystemMediaTransportControlsSessionMediaProperties::Thumbnail`, `OpenReadAsync`, and `DataReader`; cap accepted encoded artwork at 4 MiB. A missing/invalid thumbnail becomes `None`, not a failed snapshot.

Use a monotonic worker-owned `activity_sequence`; increment it when a session becomes current or reports playback/media changes. This supplies deterministic fallback without polling.

### 4.3 Route worker results through the existing queue

Add:

```rust
MediaSessionsChanged(Result<MediaSessionSnapshot, MediaSessionError>),
MediaTransportCompleted(MediaTransportResult),
```

to `PlatformEvent`, classify them as runtime-internal events, queue them with `RoutedPlatformEvent::window_id`, and wake the owner with a dedicated `WM_APP` value adjacent to `BACKGROUND_APPS_WAKE_MESSAGE`.

Run: `cargo check -p shell-platform-windows`

### 4.4 Commit

```powershell
git add crates/shell-platform-windows/Cargo.toml Cargo.lock crates/shell-platform-windows/src/win32_media_sessions.rs crates/shell-platform-windows/src/media_session_worker.rs crates/shell-platform-windows/src/lib.rs crates/shell-platform-windows/src/runtime.rs
git commit -m "feat: observe Windows media sessions"
```

---

## Task 5: Integrate media interactions with Quick Settings

**Files**

- Modify: `crates/shell-platform-windows/src/quick_settings_controller.rs`
- Modify: `crates/shell-platform-windows/src/quick_settings_types.rs`
- Modify: `crates/shell-platform-windows/src/win32_owner.rs`
- Modify: `crates/shell-platform-windows/src/win32_popover_render.rs`
- Modify: `crates/shell-platform-windows/tests/quick_settings_controller.rs`

### 5.1 Add the media page and UI actions

Extend `QuickSettingsPage` with `MediaSessions`. Add queued actions:

```rust
StartMediaWorker,
SendMediaCommand(MediaWorkerCommand),
Redraw,
Reflow,
```

On panel open, start/refresh the worker. The main scene receives the selected player. Clicking artwork opens `MediaSessions`; clicking Previous/Play/Next marks the action pending and sends one worker command. Clicking a session row changes the manual selection, returns to Main, and reflows once.

### 5.2 Handle asynchronous results without flashing the surface

`win32_owner` applies media snapshots and completions to the existing controller, then requests `Redraw`; use `Reflow` only when player presence or page changes. Never close/reopen the popover and never recapture capabilities for a transport action.

### 5.3 Remove the replaced controls from the main presentation

Filter `NearbySharing` and `Multitasking` out of `visible_capabilities()` for the main panel while retaining their deserialization/configuration data for backward compatibility. Focus and Projection remain the right column. The media player is shown only when GSMTC has at least one session.

Run: `cargo test -p shell-platform-windows quick_settings_controller media_session_controller`

### 5.4 Commit

```powershell
git add crates/shell-platform-windows/src/quick_settings_controller.rs crates/shell-platform-windows/src/quick_settings_types.rs crates/shell-platform-windows/src/win32_owner.rs crates/shell-platform-windows/src/win32_popover_render.rs crates/shell-platform-windows/tests/quick_settings_controller.rs
git commit -m "feat: integrate media quick control"
```

---

## Task 6: Render artwork and refined transport controls

**Files**

- Modify: `crates/shell-renderer/src/native_icons.rs`
- Modify: `crates/shell-renderer/src/native_showcase_resources.rs`
- Modify: `crates/shell-renderer/src/native_showcase_quick_settings.rs`
- Modify: `crates/shell-renderer/src/native_showcase.rs`

### 6.1 Add a generation-keyed artwork cache

Extend the existing WIC/D2D icon resources with `bitmap_from_encoded(generation, &[u8])`. Create an in-memory COM stream with `SHCreateMemStream(Some(bytes))`, decode with `IWICImagingFactory::CreateDecoderFromStream`, convert to `GUID_WICPixelFormat32bppPBGRA`, and create the D2D bitmap. Cache by generation and evict generations absent from the current scene.

Keep the encoded buffer alive through the synchronous decode call; never decode on every paint.

### 6.2 Draw the media card in the current material language

Within its 120 DIP bounds:

- 48 DIP artwork at top-left, radius 10;
- title and artist on separate single-line DirectWrite layouts with character ellipsis;
- 30 DIP Previous, Play/Pause, Next hit targets along the lower row;
- disabled commands at existing secondary-text opacity;
- pending command at reduced opacity, without spinner or layout movement;
- missing artwork as the app glyph/media glyph in the same rounded artwork well;
- hover/pressed fills use existing quick-settings brushes; no new border/glow system.

For the selector page, reuse the projection list metrics and draw thumbnail, title/source, playback detail, and selected checkmark.

Run: `cargo test -p shell-renderer quick_settings`

### 6.3 Commit

```powershell
git add crates/shell-renderer/src/native_icons.rs crates/shell-renderer/src/native_showcase_resources.rs crates/shell-renderer/src/native_showcase_quick_settings.rs crates/shell-renderer/src/native_showcase.rs
git commit -m "feat: render polished media quick control"
```

---

## Task 7: Proportional verification and executable

### 7.1 Run the focused checks

```powershell
cargo fmt --all -- --check
cargo test -p shell-renderer quick_settings
cargo test -p shell-platform-windows media_session_controller
cargo test -p shell-platform-windows quick_settings_controller
cargo check -p shell-app
```

### 7.2 Manual scenarios

- Open panel with no media session: no blank media hole; remaining tiles compact.
- Start two media apps: selected session is current; artwork opens both choices.
- Manually choose a paused session: it remains selected while the other plays.
- Previous/Play/Next: each command executes once, button shows pending without panel flashing.
- Close selected app: next most recently active session is chosen; close all: card disappears.
- Verify text ellipsis and hit targets at 100%, 125%, and 150% DPI.

### 7.3 Build an unlocked preview executable

```powershell
$env:CARGO_TARGET_DIR = Join-Path $PWD 'target-media-preview'
cargo build -p shell-app --target x86_64-pc-windows-msvc
Copy-Item -LiteralPath "$env:CARGO_TARGET_DIR\x86_64-pc-windows-msvc\debug\shell-app.exe" -Destination "target\x86_64-pc-windows-msvc\debug\shell-app-media-preview.exe" -Force
```

Report the absolute preview path. Do not overwrite a running executable.

