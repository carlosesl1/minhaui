# Reliable Night Light Implementation Plan

> **Superseded (2026-08-12):** the stable product no longer reads or writes the
> undocumented CloudStore contract. Night Light now opens Microsoft's documented
> `ms-settings:nightlight` page. Keep this plan only as historical context; do not
> re-enable its registry strategy in a stable build.

> **Execution:** implement after the media player plan. Keep this delivery independent so failures in the undocumented Windows CloudStore seam cannot destabilize media or other Quick Settings controls.

**Goal:** Make rapid Night light clicks deterministic: one native write at a time, latest desired state wins, the UI never lies about confirmed state, and Windows registry records receive strictly increasing timestamps.

**Architecture:** A pure `NightLightCommandCoordinator` tracks confirmed, desired, in-flight, and error state. A one-shot background worker performs each native write and verification outside the UI thread. The existing Windows adapter writes the global and every per-device CloudStore record with a monotonic outer timestamp and re-reads all records before reporting success.

**Reference:** `docs/superpowers/specs/2026-07-19-quick-settings-media-and-night-light-design.md`

---

## Task 1: Implement the pure command coordinator

**Files**

- Create: `crates/shell-platform-windows/src/night_light_coordinator.rs`
- Modify: `crates/shell-platform-windows/src/lib.rs`

### 1.1 Write the coordinator tests first

Embed unit tests beside the coordinator for:

- one click starts one request targeting `!confirmed`;
- clicks during an in-flight request only replace `desired` and never start parallel work;
- three rapid clicks settle on the last desired state;
- a matching completion updates confirmed and clears pending;
- a completion followed by a different desired state immediately returns the next serialized request;
- stale request ids are ignored;
- failure preserves confirmed state and exposes `Could not change Night light`.

### 1.2 Implement the state machine

Use this complete public seam:

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct NightLightRequest {
    pub(crate) id: u64,
    pub(crate) active: bool,
    pub(crate) minimum_timestamp: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct NightLightCommandCoordinator {
    confirmed: bool,
    desired: bool,
    in_flight: Option<NightLightRequest>,
    next_id: u64,
    last_timestamp: u64,
    error: String,
}

impl NightLightCommandCoordinator {
    pub(crate) fn new(confirmed: bool, last_timestamp: u64) -> Self;
    pub(crate) fn request_toggle(&mut self) -> Option<NightLightRequest>;
    pub(crate) fn complete(
        &mut self,
        id: u64,
        result: Result<NightLightApplyResult, NightLightApplyError>,
    ) -> Option<NightLightRequest>;
    pub(crate) fn confirmed(&self) -> bool;
    pub(crate) fn detail(&self) -> &str;
}
```

`request_toggle` must calculate `desired = !desired`. If idle, it creates a request; if busy, it returns `None`. `complete` accepts only the matching id, updates `last_timestamp`, then starts the next request only when `desired != confirmed`.

Run: `cargo test -p shell-platform-windows night_light_coordinator`

### 1.3 Commit

```powershell
git add crates/shell-platform-windows/src/night_light_coordinator.rs crates/shell-platform-windows/src/lib.rs
git commit -m "feat: serialize night light intent"
```

---

## Task 2: Make CloudStore timestamps strictly monotonic

**Files**

- Modify: `crates/shell-platform-windows/src/win32_quick_settings_system.rs`

### 2.1 Add pure timestamp tests

Add a helper and tests:

```rust
fn next_night_light_timestamp(now: u64, observed: u64, last_written: u64) -> u64 {
    now.max(observed.saturating_add(1))
        .max(last_written.saturating_add(1))
}
```

Cover equal-second writes, a clock moving backwards, and `u64::MAX` saturation. The important invariant is `next > observed` whenever `observed < u64::MAX`.

### 2.2 Return a verified native result

Replace the current `set_night_light(active) -> Result<()>` seam with:

```rust
pub(crate) struct NightLightApplyResult {
    pub(crate) active: bool,
    pub(crate) timestamp: u64,
}

pub(crate) fn apply_night_light(
    active: bool,
    minimum_timestamp: u64,
) -> Result<NightLightApplyResult>;
```

Implementation order:

1. read the global state and all per-device states;
2. compute the maximum observed outer timestamp;
3. mutate only `is_enabled` through the library’s `enable`/`disable` methods so schedule/config bytes remain intact;
4. override each outgoing state timestamp with one shared value from `next_night_light_timestamp(current_filetime_seconds, observed, minimum_timestamp)`;
5. write global then every per-device record;
6. read all records again;
7. succeed only if every record agrees with `active` and has the requested timestamp or newer.

Keep `read_active(NightLight)` unchanged except that it should require agreement across records; disagreement is an error, not “any enabled”. This prevents the tile from presenting a mixed state as confirmed.

Run: `cargo test -p shell-platform-windows win32_quick_settings_system::tests`

### 2.3 Commit

```powershell
git add crates/shell-platform-windows/src/win32_quick_settings_system.rs
git commit -m "fix: write monotonic night light state"
```

---

## Task 3: Move Night light writes off the UI thread

**Files**

- Create: `crates/shell-platform-windows/src/night_light_worker.rs`
- Modify: `crates/shell-platform-windows/src/lib.rs`
- Modify: `crates/shell-platform-windows/src/runtime.rs`
- Modify: `crates/shell-platform-windows/src/win32_event_queue.rs`

### 3.1 Add a one-shot worker matching the existing worker pattern

Use `background_apps_worker.rs` as the pattern. Each request spawns one thread, calls `apply_night_light`, queues a window-targeted completion, and wakes the owner. The coordinator guarantees that only one thread exists at a time.

```rust
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NightLightWorkerResult {
    pub(crate) request: u64,
    pub(crate) result: Result<NightLightApplyResult, NightLightApplyError>,
}

pub(super) fn request_night_light(
    request: NightLightRequest,
    wake_window: NativeWindowId,
);
```

Add `PlatformEvent::NightLightCompleted(NightLightWorkerResult)` and a dedicated wake message. No repeating timer, no polling, and no registry call on paint/pointer handlers.

Run: `cargo test -p shell-platform-windows night_light_worker`

### 3.2 Commit

```powershell
git add crates/shell-platform-windows/src/night_light_worker.rs crates/shell-platform-windows/src/lib.rs crates/shell-platform-windows/src/runtime.rs crates/shell-platform-windows/src/win32_event_queue.rs
git commit -m "feat: apply night light asynchronously"
```

---

## Task 4: Integrate confirmed/pending state into Quick Settings

**Files**

- Modify: `crates/shell-platform-windows/src/quick_settings_controller.rs`
- Modify: `crates/shell-platform-windows/src/quick_settings_types.rs`
- Modify: `crates/shell-platform-windows/src/win32_quick_settings_actions.rs`
- Modify: `crates/shell-platform-windows/src/win32_owner.rs`
- Modify: `crates/shell-platform-windows/src/win32_popover_render.rs`
- Modify: `crates/shell-platform-windows/tests/quick_settings_controller.rs`

### 4.1 Write interaction tests

Add focused tests showing that:

- clicking Night light emits `StartNightLight` instead of synchronous `Activate(NightLight)`;
- while pending, the tile active fill follows `confirmed`, not `desired`;
- detail reads `Turning on...` or `Turning off...`;
- repeated clicks update the desired direction without additional worker actions;
- completion redraws once and starts at most one queued follow-up;
- failure shows `Could not change Night light` and retains the last confirmed fill.

### 4.2 Route Night light around the generic toggle adapter

Add `StartNightLight(NightLightRequest)` to `QueuedQuickSettingsAction`. Intercept Night light in pointer and keyboard activation before the generic `QuickSettingsIntent::Activate` path. Remove Night light from `win32_quick_settings_actions::toggle_native`; leaving a direct synchronous fallback would reintroduce races.

When `PlatformEvent::NightLightCompleted` arrives, pass it to the coordinator, update only the Night light capability on success, queue a follow-up request if returned, and request a redraw. Do not close/reopen or reflow the popover.

Run: `cargo test -p shell-platform-windows quick_settings_controller night_light_coordinator`

### 4.3 Commit

```powershell
git add crates/shell-platform-windows/src/quick_settings_controller.rs crates/shell-platform-windows/src/quick_settings_types.rs crates/shell-platform-windows/src/win32_quick_settings_actions.rs crates/shell-platform-windows/src/win32_owner.rs crates/shell-platform-windows/src/win32_popover_render.rs crates/shell-platform-windows/tests/quick_settings_controller.rs
git commit -m "fix: coordinate rapid night light changes"
```

---

## Task 5: Proportional verification and executable

### 5.1 Focused checks

```powershell
cargo fmt --all -- --check
cargo test -p shell-platform-windows night_light_coordinator
cargo test -p shell-platform-windows win32_quick_settings_system::tests
cargo test -p shell-platform-windows quick_settings_controller
cargo check -p shell-app
```

### 5.2 Manual scenario

Open the panel and click Night light 10 times rapidly, ending both on an even and odd count. Verify:

- the panel never flashes/closes;
- detail tracks the latest desired direction;
- only the last desired state is eventually confirmed;
- Settings/Windows and the panel agree;
- a native failure leaves the prior state visible and shows the concise error.

### 5.3 Build an unlocked combined preview

After both plans are complete:

```powershell
$env:CARGO_TARGET_DIR = Join-Path $PWD 'target-media-nightlight-preview'
cargo build -p shell-app --target x86_64-pc-windows-msvc
Copy-Item -LiteralPath "$env:CARGO_TARGET_DIR\x86_64-pc-windows-msvc\debug\shell-app.exe" -Destination "target\x86_64-pc-windows-msvc\debug\shell-app-media-nightlight-preview.exe" -Force
```

Report the absolute path and the exact manual behaviors verified. Do not overwrite a running binary.
