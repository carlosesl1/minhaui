# Background Apps Balanced List and Native Menu Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver the approved balanced Apps panel and open each supported application's real notification-area context menu from our row, with a truthful shell-owned fallback and no blocking or unbounded background work.

**Architecture:** Keep `BackgroundAppsWorker` as the single serialized, coalescing owner. Add a pure native-tray model and strategy coordinator inside `shell-platform-windows`, isolate Explorer memory reads and Win32 callback forwarding in narrowly allowed FFI adapters, and keep native handles out of `shell-core` and `shell-renderer`. Roll out the UI, discovery, and activation lifecycle as independently testable slices so the private native capability can be disabled without losing the balanced list or safe fallback.

**Tech Stack:** Rust 2024 workspace, `windows` 0.62.2 Win32 bindings, Direct2D/DirectWrite renderer, existing `LatestRequestWorker`, architecture PowerShell gate, Cargo unit/integration tests, native Windows QA.

---

## Working rules

- Work on the existing dedicated branch `codex/smooth-dock-wave`; do not create additional `target-*` directories.
- Use test-first RED/GREEN cycles for each behavior.
- Run Cargo commands serially with `-j 1`; never overlap release/LTO jobs.
- Before a build that may replace the executable, stop only a process whose resolved `ExecutablePath` equals `target\x86_64-pc-windows-msvc\release\shell-app.exe`.
- Do not log HWNDs, callback values, paths, tooltips, window titles, raw remote bytes, or user names.
- Every cross-process callback is asynchronous (`PostMessageW`); do not introduce `SendMessageW` to an application-owned callback window.
- Explorer toolbar reads stay on the existing background worker. The UI thread may only validate cached identities and post bounded callback sequences.

## Slice 0: Architecture governance

### Task 1: Amend the notification-area architecture decision

**Files:**

- Create: `docs/architecture/adr/0009-native-tray-menu-forwarding.md`
- Modify: `docs/architecture/adr/0008-background-apps-notification-catalog.md`
- Test: `scripts/Check-Architecture.ps1`

- [ ] **Step 1: Write ADR-0009**

Record:

```markdown
# ADR-0009: Native notification-area discovery and callback forwarding

Status: Accepted

ADR-0009 supersedes only ADR-0008's rejection of Explorer toolbar reads and
third-party notification callbacks. It preserves on-demand capture, one
serialized/coalesced worker, generation checks, bounded catalogs, privacy,
joined shutdown, failure isolation, and registry/process fallback.
```

The decision must explicitly classify toolbar decoding as a private,
version-sensitive compatibility adapter, prohibit Jump List reconstruction,
require non-blocking callback delivery, and define the capability rollback to
the registry/process fallback.

- [ ] **Step 2: Link ADR-0008 to its narrow successor**

Add a short `Superseded in part by ADR-0009` note beside the two rejected
options. Do not rewrite ADR-0008's original context or accepted safety
constraints.

- [ ] **Step 3: Run the architecture gate**

Run:

```powershell
& .\scripts\Check-Architecture.ps1
```

Expected: `Architecture checks passed.`

- [ ] **Step 4: Commit**

```powershell
git add docs/architecture/adr/0008-background-apps-notification-catalog.md docs/architecture/adr/0009-native-tray-menu-forwarding.md
git commit -m "docs(architecture): govern native tray forwarding"
```

## Slice 1: Balanced presentation and typed context intent

### Task 2: Add a dedicated balanced Apps layout

**Files:**

- Modify: `crates/shell-renderer/src/popover_scene.rs`
- Modify: `crates/shell-renderer/src/popover_layout.rs`
- Modify: `crates/shell-renderer/src/native_showcase.rs`
- Modify: `crates/shell-renderer/src/native_showcase_popover.rs`
- Modify: `crates/shell-renderer/tests/popover_scene.rs`

- [ ] **Step 1: Write failing renderer tests**

Add a `balanced_apps_scene()` fixture with twelve rows and assert:

```rust
assert_eq!(size, PopoverSurfaceSize::new(288.0, 388.0));
assert_eq!(scene.header_detail(), Some("12"));
assert_eq!(layout.rows().len(), 8);
assert!(layout.rows().iter().all(|row| row.bounds().height == 40.0));
assert_eq!(layout.rows()[0].bounds(), DipRect::new(16.0, 60.0, 256.0, 40.0));
assert_eq!(layout.icon_bounds(0), Some(DipRect::new(18.0, 66.0, 28.0, 28.0)));
```

Also assert that a long label receives the full label area through
`PopoverLayout::label_bounds(index)` and has no detail column.

Run:

```powershell
cargo test -p shell-renderer --release --locked -j 1 popover_scene
```

Expected RED: `BalancedApps`, `header_detail`, `icon_bounds`, and
`label_bounds` do not exist.

- [ ] **Step 2: Add presentation metadata and geometry**

Add:

```rust
pub enum PopoverLayoutStyle {
    Compact,
    SystemPanel,
    BalancedApps,
}

impl PopoverScene {
    pub fn with_header_detail(mut self, detail: &str) -> Self;
    pub fn header_detail(&self) -> Option<&str>;
}
```

Implement `BalancedApps` in `popover_layout.rs` with named constants:

```rust
const APPS_WIDTH: f32 = 288.0;
const APPS_BODY_TOP: f32 = 60.0;
const APPS_ROW_HEIGHT: f32 = 40.0;
const APPS_VISIBLE_ROWS: usize = 8;
const APPS_BOTTOM_PADDING: f32 = 8.0;
```

`popover_surface_size` must calculate height from
`rows.len().min(APPS_VISIBLE_ROWS)`. `layout_popover_scene` must emit no more
than eight rows and expose deterministic icon and label bounds derived from
the laid-out row. Keep compact and system-panel geometry byte-for-byte
unchanged.

- [ ] **Step 3: Draw the balanced header and rows**

Route `BalancedApps` to `draw_balanced_apps`. Draw:

- notch and header using the current system-panel brushes;
- title on the left and active count right-aligned without overlap;
- a 9-DIP continuous focus/hover band;
- native icon in the 28-DIP optical box, preserving aspect/alpha through
  `NativeIconCache`;
- raised rounded fallback only when native icon drawing fails;
- label in the layout-provided bounds and no trailing detail column.

Reuse existing DirectWrite trimming/ellipsis behavior; do not manually truncate
the label in Rust.

- [ ] **Step 4: Run the renderer tests**

```powershell
cargo test -p shell-renderer --release --locked -j 1 popover_scene
```

Expected GREEN: balanced geometry passes and existing compact/system-panel
tests remain unchanged.

- [ ] **Step 5: Commit**

```powershell
git add crates/shell-renderer/src/popover_scene.rs crates/shell-renderer/src/popover_layout.rs crates/shell-renderer/src/native_showcase.rs crates/shell-renderer/src/native_showcase_popover.rs crates/shell-renderer/tests/popover_scene.rs
git commit -m "feat(apps): add balanced panel presentation"
```

### Task 3: Separate primary and context activation

**Files:**

- Modify: `crates/shell-platform-windows/src/popover_types.rs`
- Modify: `crates/shell-platform-windows/src/popover_controller.rs`
- Modify: `crates/shell-platform-windows/src/popover_adapters.rs`
- Modify: `crates/shell-platform-windows/src/lib.rs`
- Modify: `crates/shell-platform-windows/src/runtime.rs`
- Modify: `crates/shell-platform-windows/src/win32_windowing.rs`
- Modify: `crates/shell-platform-windows/src/win32_owner.rs`
- Modify: `crates/shell-platform-windows/src/win32_system_actions.rs`
- Modify: `crates/shell-platform-windows/tests/popover_controller.rs`

- [ ] **Step 1: Write failing pointer and keyboard tests**

Introduce test expectations:

```rust
assert_eq!(
    controller.handle_pointer_context(point, surface),
    vec![QueuedPopoverAction::TypedIntent(
        PopoverAction::OpenBackgroundAppContextMenu(app_id)
    )]
);
assert_eq!(
    controller.handle_key(PopoverKey::ContextMenu),
    vec![QueuedPopoverAction::TypedIntent(
        PopoverAction::OpenBackgroundAppContextMenu(app_id)
    )]
);
```

Also assert that `Enter` and left-click still yield `OpenBackgroundApp`, that
right-clicking outside a row returns no action, and that context activation
keeps the row focused.

Run:

```powershell
cargo test -p shell-platform-windows --release --locked -j 1 popover_controller
```

Expected RED: context variants and handler are absent.

- [ ] **Step 2: Add typed context activation**

Add:

```rust
pub enum PopoverKey {
    Next,
    Previous,
    Activate,
    ContextMenu,
    Escape,
}

pub enum PopoverAction {
    // existing variants
    OpenBackgroundApp(BackgroundAppId),
    OpenBackgroundAppContextMenu(BackgroundAppId),
}
```

`PopoverItem` keeps its primary action. `PopoverController` derives the
context action only for `Popover::BackgroundApps`, so other popovers cannot
accidentally emit it.

Set the Apps scene to:

```rust
PopoverLayoutStyle::BalancedApps
```

and set `header_detail` to the total item count. Change the visible-row
calculation to a per-style helper so Apps scrolls in windows of eight while
the current compact behavior remains seventeen.

- [ ] **Step 3: Route Win32 input without changing left-click**

Add `PlatformEvent::PopoverContextRequested(DipPoint)`. Route:

- `WM_RBUTTONDOWN` on the popover to pointer-focus only;
- `WM_RBUTTONUP` on the popover to `PopoverContextRequested`;
- `VK_APPS`, or `VK_F10` while Shift is down, to
  `PopoverKey::ContextMenu`.

Do not broadcast `DismissTransientOverlays` for a right-click inside the Apps
popover.

- [ ] **Step 4: Run controller and workspace compile tests**

```powershell
cargo test -p shell-platform-windows --release --locked -j 1 popover_controller
cargo check -p shell-platform-windows --all-targets --all-features --release --locked -j 1
```

Expected GREEN.

- [ ] **Step 5: Commit**

```powershell
git add crates/shell-platform-windows/src/popover_types.rs crates/shell-platform-windows/src/popover_controller.rs crates/shell-platform-windows/src/popover_adapters.rs crates/shell-platform-windows/src/lib.rs crates/shell-platform-windows/src/runtime.rs crates/shell-platform-windows/src/win32_windowing.rs crates/shell-platform-windows/src/win32_owner.rs crates/shell-platform-windows/src/win32_system_actions.rs crates/shell-platform-windows/tests/popover_controller.rs
git commit -m "feat(apps): add typed context activation"
```

## Slice 2: Native tray discovery and honest fallback

### Task 4: Introduce the pure tray identity and catalog merge

**Files:**

- Create: `crates/shell-platform-windows/src/native_tray.rs`
- Modify: `crates/shell-platform-windows/src/background_apps.rs`
- Modify: `crates/shell-platform-windows/src/lib.rs`

- [ ] **Step 1: Write failing model tests**

Tests must cover:

- normalized executable dedup prefers a native entry over a fallback entry;
- two native entries from the same executable remain distinct when
  `(owner_window, icon_id/guid)` differs;
- result is sorted and bounded to `MAX_BACKGROUND_APPS`;
- stale generation is rejected by `is_current`;
- invalid zero owner/callback identities cannot be constructed.

Run:

```powershell
cargo test -p shell-platform-windows --release --locked -j 1 native_tray
```

Expected RED: module and types do not exist.

- [ ] **Step 2: Add platform-private value types**

Use integer value types rather than `HWND` so the pure model remains testable:

```rust
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct NativeTrayIdentity {
    owner_window: NativeWindowId,
    owner_process_id: u32,
    icon_id: u32,
    callback_message: u32,
    version: u32,
    guid: Option<[u8; 16]>,
    generation: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum BackgroundAppOrigin {
    Native(NativeTrayIdentity),
    RegistryFallback,
}
```

Extend `BackgroundAppEntry` with `origin`, and add constructors instead of
exposing fields. Keep native identity private to this crate.

- [ ] **Step 3: Implement deterministic merge**

Add:

```rust
pub(crate) fn merge_background_apps(
    native: Vec<BackgroundAppEntry>,
    fallback: Vec<BackgroundAppEntry>,
) -> Vec<BackgroundAppEntry>;
```

Normalize paths with the existing helper, deduplicate fallback rows against
native executable identity, preserve distinct native icons, sort by safe
label, then truncate to 32.

- [ ] **Step 4: Run tests and commit**

```powershell
cargo test -p shell-platform-windows --release --locked -j 1 native_tray
git add crates/shell-platform-windows/src/native_tray.rs crates/shell-platform-windows/src/background_apps.rs crates/shell-platform-windows/src/lib.rs
git commit -m "feat(apps): model native tray identities"
```

### Task 5: Decode bounded Explorer tray records behind an injected reader

**Files:**

- Create: `crates/shell-platform-windows/src/tray_record_decoder.rs`
- Modify: `crates/shell-platform-windows/src/lib.rs`

- [ ] **Step 1: Add fixed 64-bit fixtures and failing decoder tests**

Build byte fixtures in tests with explicit little-endian writers. Cover:

- a valid x64 `TBBUTTON` plus 32-byte tray-data record;
- null `dwData`;
- truncated button and tray records;
- owner zero, PID zero, callback below `WM_USER`, and tooltip over 160 chars;
- a remote pointer outside the validated process range;
- more than 256 candidate records.

Run:

```powershell
cargo test -p shell-platform-windows --release --locked -j 1 tray_record_decoder
```

Expected RED: decoder module is absent.

- [ ] **Step 2: Implement pure checked decoding**

Define explicit raw-layout constants and checked readers. Do not cast an
untrusted byte slice to a Rust struct:

```rust
pub(crate) const MAX_NATIVE_TRAY_CANDIDATES: usize = 256;
pub(crate) const MAX_TRAY_TOOLTIP_CHARS: usize = 160;

pub(crate) struct DecodedTrayRecord {
    pub owner_window: usize,
    pub icon_id: u32,
    pub callback_message: u32,
    pub icon_handle: usize,
    pub tooltip_pointer: usize,
}

pub(crate) fn decode_x64_tray_record(
    button: &[u8],
    tray_data: &[u8],
    valid_remote_range: Range<usize>,
) -> Result<DecodedTrayRecord, TrayDecodeError>;
```

Every offset read uses `get(range)` plus `from_le_bytes`. Unsupported pointer
width returns `TrayDecodeError::UnsupportedLayout`.

- [ ] **Step 3: Run tests and commit**

```powershell
cargo test -p shell-platform-windows --release --locked -j 1 tray_record_decoder
git add crates/shell-platform-windows/src/tray_record_decoder.rs crates/shell-platform-windows/src/lib.rs
git commit -m "feat(apps): decode bounded tray records"
```

### Task 6: Read Explorer notification toolbars on the existing worker

**Files:**

- Create: `crates/shell-platform-windows/src/win32_tray_source.rs`
- Modify: `crates/shell-platform-windows/src/win32_background_apps.rs`
- Modify: `crates/shell-platform-windows/src/background_apps_worker.rs`
- Modify: `crates/shell-platform-windows/src/lib.rs`
- Modify: `crates/shell-platform-windows/Cargo.toml`

- [ ] **Step 1: Add failing source and worker tests**

Inject a `TrayToolbarReader` into `ExplorerTraySource` and test:

- notification host and overflow host are both queried;
- a read failure on one host does not discard the other host;
- access denied returns `Unsupported` and preserves fallback entries;
- duplicate native/registry entries merge correctly;
- repeated worker requests still produce at most one active capture and one
  latest pending generation.

Run:

```powershell
cargo test -p shell-platform-windows --release --locked -j 1 background_apps
cargo test -p shell-platform-windows --release --locked -j 1 background_apps_worker
```

Expected RED: the native source seam and combined result are absent.

- [ ] **Step 2: Add the scoped Win32 feature**

Add `Win32_System_Memory` to the existing `windows` feature list. Do not add
process-debug privileges or unrestricted access features.

- [ ] **Step 3: Implement `ExplorerTraySource`**

The FFI adapter must:

1. Locate `Shell_TrayWnd -> TrayNotifyWnd -> SysPager -> ToolbarWindow32` and
   `NotifyIconOverflowWindow -> ToolbarWindow32`.
2. Get Explorer PID with `GetWindowThreadProcessId`.
3. Open Explorer only with
   `PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_OPERATION |
   PROCESS_VM_READ | PROCESS_VM_WRITE`.
4. Allocate one fixed remote buffer with `VirtualAllocEx`.
5. Bound `TB_BUTTONCOUNT` to 256.
6. Read each button into the fixed buffer with `TB_GETBUTTON`, then copy it
   locally with `ReadProcessMemory`.
7. Decode and validate the referenced tray data and bounded UTF-16 tooltip.
8. Resolve the owner PID and executable using existing process helpers.
9. Free remote memory and close handles through RAII guards on every path.

`SendMessageW` is permitted only for Explorer's toolbar query in this worker;
use a bounded `SendMessageTimeoutW` with `SMTO_ABORTIFHUNG` and a 250-ms timeout
per toolbar message so an unresponsive Explorer cannot pin the worker.

- [ ] **Step 4: Combine native and fallback capture**

Change `capture_background_apps(generation)` to collect native entries first,
then the existing registry/process fallback, and call
`merge_background_apps`. Pass the worker request generation into the native
identities. A completely unsupported native source is a redacted diagnostic,
not an error that empties the panel.

- [ ] **Step 5: Run tests, architecture gate, and commit**

```powershell
cargo test -p shell-platform-windows --release --locked -j 1 background_apps
cargo test -p shell-platform-windows --release --locked -j 1 background_apps_worker
& .\scripts\Check-Architecture.ps1
git add crates/shell-platform-windows/src/win32_tray_source.rs crates/shell-platform-windows/src/win32_background_apps.rs crates/shell-platform-windows/src/background_apps_worker.rs crates/shell-platform-windows/src/lib.rs crates/shell-platform-windows/Cargo.toml Cargo.lock
git commit -m "feat(apps): discover live Explorer tray icons"
```

## Slice 3: Native callback forwarding and lifecycle

### Task 7: Select exactly one callback strategy per user action

**Files:**

- Create: `crates/shell-platform-windows/src/tray_activation.rs`
- Modify: `crates/shell-platform-windows/src/lib.rs`

- [ ] **Step 1: Write failing strategy tests**

Use a recording `TrayCallbackSink` and assert exact sequences:

```rust
assert_eq!(modern.messages(), [TrayMessage::modern_context(icon_id, point)]);
assert_eq!(
    legacy.messages(),
    [
        TrayMessage::legacy(icon_id, WM_RBUTTONDOWN),
        TrayMessage::legacy(icon_id, WM_RBUTTONUP),
    ]
);
assert_eq!(
    combined.messages(),
    [
        TrayMessage::legacy(icon_id, WM_RBUTTONDOWN),
        TrayMessage::legacy(icon_id, WM_RBUTTONUP),
        TrayMessage::legacy(icon_id, WM_CONTEXTMENU),
    ]
);
```

Also assert:

- one call invokes exactly one strategy;
- modern is initially selected for version 4 and legacy otherwise;
- a timed-out strategy is marked failed and the next explicit attempt selects
  the next eligible strategy;
- success is cached by normalized executable identity only in memory;
- one app's result does not alter another app's strategy.

Run:

```powershell
cargo test -p shell-platform-windows --release --locked -j 1 tray_activation
```

Expected RED: coordinator does not exist.

- [ ] **Step 2: Implement the pure strategy coordinator**

Add:

```rust
pub(crate) enum TrayActivationStrategy {
    VersionAware,
    Legacy,
    Combined,
}

pub(crate) trait TrayCallbackSink {
    fn post(&mut self, target: NativeWindowId, message: TrayMessage) -> bool;
}

pub(crate) struct TrayActivationCoordinator {
    compatibility: HashMap<String, AppCompatibility>,
    pending: Option<PendingActivation>,
}
```

`begin` returns a strategy and the exact bounded messages for one user action.
`mark_observed_success`, `mark_timeout`, and `cancel` own every state
transition. No method performs Win32 calls.

- [ ] **Step 3: Run tests and commit**

```powershell
cargo test -p shell-platform-windows --release --locked -j 1 tray_activation
git add crates/shell-platform-windows/src/tray_activation.rs crates/shell-platform-windows/src/lib.rs
git commit -m "feat(apps): coordinate tray activation strategies"
```

### Task 8: Coordinate the external application-owned menu hold

**Files:**

- Create: `crates/shell-platform-windows/src/external_menu_coordinator.rs`
- Modify: `crates/shell-platform-windows/src/popover_controller.rs`
- Modify: `crates/shell-platform-windows/src/lib.rs`

- [ ] **Step 1: Write failing lifecycle tests with a fake clock**

Test transitions for:

- arm keeps the originating row externally active;
- popup start confirms activation;
- popup end releases and restores row focus;
- one `WA_INACTIVE` dismissal is suppressed only while armed/observed;
- one-second unobserved timeout requests fallback;
- 30-second observed custom-menu watchdog releases without sending input;
- owner exit, Explorer restart, Escape, popover replacement, and shutdown
  release immediately;
- stale generations cannot arm a hold.

Run:

```powershell
cargo test -p shell-platform-windows --release --locked -j 1 external_menu_coordinator
```

Expected RED: coordinator is absent.

- [ ] **Step 2: Implement a pure lifecycle state machine**

Add:

```rust
pub(crate) enum ExternalMenuEffect {
    Redraw,
    SuppressDismissOnce,
    ShowFallback { app: BackgroundAppId, allow_alternate: bool },
    RestoreFocus(BackgroundAppId),
    Release,
}

pub(crate) struct ExternalMenuCoordinator {
    state: ExternalMenuState,
}
```

The coordinator accepts timestamps supplied by the caller. It does not create
threads or timers; owner-window timer events drive `tick`.

Expose an `externally_active` row ID to `PopoverController::scene`, and add a
row visual-state flag so the renderer keeps the same geometry while drawing
the active band.

- [ ] **Step 3: Run tests and commit**

```powershell
cargo test -p shell-platform-windows --release --locked -j 1 external_menu_coordinator
git add crates/shell-platform-windows/src/external_menu_coordinator.rs crates/shell-platform-windows/src/popover_controller.rs crates/shell-platform-windows/src/lib.rs
git commit -m "feat(apps): hold panel for external menus"
```

### Task 9: Add the Win32 callback and menu-event adapters

**Files:**

- Create: `crates/shell-platform-windows/src/win32_tray_activation.rs`
- Create: `crates/shell-platform-windows/src/win32_external_menu_events.rs`
- Modify: `crates/shell-platform-windows/src/lib.rs`
- Modify: `crates/shell-platform-windows/src/win32_windowing.rs`
- Modify: `crates/shell-platform-windows/Cargo.toml`

- [ ] **Step 1: Write validation tests around injected Win32 operations**

Use a fake `NativeTrayOps` to assert:

- `IsWindow` failure rejects activation;
- reused HWND with a different PID rejects activation;
- generation mismatch rejects activation;
- `AllowSetForegroundWindow` is called before any callback;
- post failure cancels the pending hold;
- all callback deliveries use the non-blocking `post_message` operation.

Run:

```powershell
cargo test -p shell-platform-windows --release --locked -j 1 win32_tray_activation
```

Expected RED: adapter is absent.

- [ ] **Step 2: Implement validated callback forwarding**

Define a testable operation boundary:

```rust
pub(crate) trait NativeTrayOps {
    fn is_window(&self, window: NativeWindowId) -> bool;
    fn owner_process_id(&self, window: NativeWindowId) -> Option<u32>;
    fn allow_foreground(&self, process_id: u32) -> bool;
    fn post_message(&self, window: NativeWindowId, message: TrayMessage) -> bool;
}
```

The real adapter uses `IsWindow`, `GetWindowThreadProcessId`,
`AllowSetForegroundWindow`, and `PostMessageW`. It revalidates generation,
window, and PID before arming the lifecycle hold and posting the one selected
strategy.

- [ ] **Step 3: Observe app-owned menu lifecycle**

Add `Win32_UI_Accessibility` to the `windows` feature list. Install one
process-owned out-of-context WinEvent hook for
`EVENT_SYSTEM_MENUPOPUPSTART`/`EVENT_SYSTEM_MENUPOPUPEND`, filter events by the
pending owner PID, and post a private `WM_APP` wake event to the owner window.
Use foreground-window ownership as the bounded fallback for custom menus.

Store the hook in an RAII guard and unhook it before native window teardown.
The callback only copies scalar event data into the existing routed event
queue; it never mutates controllers.

- [ ] **Step 4: Run tests and architecture gate**

```powershell
cargo test -p shell-platform-windows --release --locked -j 1 win32_tray_activation
& .\scripts\Check-Architecture.ps1
```

Expected GREEN.

- [ ] **Step 5: Commit**

```powershell
git add crates/shell-platform-windows/src/win32_tray_activation.rs crates/shell-platform-windows/src/win32_external_menu_events.rs crates/shell-platform-windows/src/lib.rs crates/shell-platform-windows/src/win32_windowing.rs crates/shell-platform-windows/Cargo.toml Cargo.lock
git commit -m "feat(apps): forward native tray context callbacks"
```

### Task 10: Integrate context activation, fallback, and cleanup

**Files:**

- Create: `crates/shell-platform-windows/src/background_app_menu.rs`
- Modify: `crates/shell-renderer/src/native.rs`
- Modify: `crates/shell-renderer/src/native_showcase.rs`
- Modify: `crates/shell-platform-windows/src/win32_surface_runtime.rs`
- Modify: `crates/shell-platform-windows/src/win32_window.rs`
- Modify: `crates/shell-platform-windows/src/win32_windowing.rs`
- Modify: `crates/shell-platform-windows/src/win32_slots.rs`
- Modify: `crates/shell-platform-windows/src/win32_owner.rs`
- Modify: `crates/shell-platform-windows/src/win32_popover_render.rs`
- Modify: `crates/shell-platform-windows/src/win32_background_apps.rs`
- Modify: `crates/shell-platform-windows/src/win32_slot_lifecycle.rs`
- Modify: `crates/shell-platform-windows/src/lib.rs`
- Modify: `crates/shell-platform-windows/tests/popover_controller.rs`

- [ ] **Step 1: Write failing fallback and integration tests**

Test that:

- native context intent validates and forwards without dismissing the Apps
  popover;
- a fallback-only row opens a menu labelled as shell-owned;
- fallback contains only `Open or focus` and `Open file location` when the live
  process path is revalidated;
- `Try alternate activation` appears only after an observed native-strategy
  timeout;
- no fallback command terminates a process;
- a successful primary activation still dismisses the popover;
- Explorer restart invalidates native identities and issues one coalesced
  refresh when Apps remains open.

Run:

```powershell
cargo test -p shell-platform-windows --release --locked -j 1 background_app_menu
cargo test -p shell-platform-windows --release --locked -j 1 popover_controller
```

Expected RED: fallback controller and integration routes are absent.

- [ ] **Step 2: Add a shell-owned fallback controller**

`BackgroundAppMenuController` owns typed commands:

```rust
pub(crate) enum BackgroundAppMenuCommand {
    OpenOrFocus(BackgroundAppId),
    OpenFileLocation(BackgroundAppId),
    TryAlternateActivation(BackgroundAppId),
}
```

Render it through the existing `ContextMenuScene` vocabulary and keep the
Apps row highlighted.

Add `ShowcaseRole::AppMenu` and one hidden, owned tool window for the fallback
surface. `Win32NativeSurfaceRuntime` owns its render surface beside the
existing topbar/dock/popover/preview/settings surfaces. Register, create,
route, hide, unregister, and destroy this window through the same
`OwnedWindow`/slot lifecycle as the other surfaces. `native_showcase.rs` draws
only `ContextMenuScene` for this role.

Place the dedicated surface adjacent to the selected row using the monitor
work area, DPI, and physical coordinates. If there is insufficient right-side
space, place it on the left. Clamp negative desktop coordinates only against
the selected monitor's work area. Pointer and keyboard events from this window
route only to `BackgroundAppMenuController`; they must not activate or dismiss
a dock menu.

Do not use `TrackPopupMenu`, because its modal loop would block our owner
thread. The existing dock menu controller remains unchanged.

- [ ] **Step 3: Wire runtime ownership**

Add both coordinators and the WinEvent hook guard to `RuntimeSurfaces`. Route:

- context intent -> validate -> arm -> post one native strategy;
- unobserved timeout -> shell-owned fallback;
- native popup start/end -> lifecycle state machine;
- fallback command -> current safe activation or revalidated Explorer
  `select` route;
- Explorer restart/owner exit/dismiss/replacement/shutdown -> release.

Revalidate the live process path immediately before `Open file location`; use
`ShellExecuteW` with Explorer's `/select,` argument only after that equality
check.

- [ ] **Step 4: Prove shutdown and stale-result behavior**

Add tests that drop the worker, hook guard, and coordinators in owner order and
assert no queued native event can revive a dismissed generation.

- [ ] **Step 5: Run focused tests and commit**

```powershell
cargo test -p shell-platform-windows --release --locked -j 1 background_app_menu
cargo test -p shell-platform-windows --release --locked -j 1 popover_controller
cargo test -p shell-platform-windows --release --locked -j 1 background_apps_worker
git add crates/shell-renderer/src/native.rs crates/shell-renderer/src/native_showcase.rs crates/shell-platform-windows/src/background_app_menu.rs crates/shell-platform-windows/src/win32_surface_runtime.rs crates/shell-platform-windows/src/win32_window.rs crates/shell-platform-windows/src/win32_windowing.rs crates/shell-platform-windows/src/win32_slots.rs crates/shell-platform-windows/src/win32_owner.rs crates/shell-platform-windows/src/win32_popover_render.rs crates/shell-platform-windows/src/win32_background_apps.rs crates/shell-platform-windows/src/win32_slot_lifecycle.rs crates/shell-platform-windows/src/lib.rs crates/shell-platform-windows/tests/popover_controller.rs
git commit -m "feat(apps): integrate native and fallback menus"
```

## Verification and release hygiene

### Task 11: Run automated quality gates

**Files:**

- Modify only if a gate exposes a defect in the files owned by Tasks 1-10.

- [ ] **Step 1: Stop only the workspace release executable if it is running**

Resolve the expected executable and compare every candidate's
`ExecutablePath` before calling `Stop-Process`. Do not stop by process name
alone.

- [ ] **Step 2: Format and verify architecture**

```powershell
cargo fmt --all -- --check
& .\scripts\Check-Architecture.ps1
```

Expected: both pass.

- [ ] **Step 3: Run Clippy**

```powershell
cargo clippy --workspace --all-targets --all-features --release --locked -j 1 -- -D warnings
```

Expected: no warnings.

- [ ] **Step 4: Run the full workspace suite**

```powershell
cargo test --workspace --release --locked -j 1
```

Expected: every test passes; record the exact count in the delivery notes.

- [ ] **Step 5: Build one release target**

```powershell
cargo build --workspace --release --locked -j 1
```

Expected: `target\x86_64-pc-windows-msvc\release\shell-app.exe` is current.
Verify with `Get-ChildItem -Directory -Filter 'target*'` that only `target`
exists.

### Task 12: Native and visual QA

**Files:**

- Create: `docs/qa/2026-07-24-background-apps-native-menu.md`

- [ ] **Step 1: Inspect the balanced list at three DPIs**

At 100%, 125%, and 150%, capture evidence that:

- width/header/count remain aligned;
- eight rows fit without clipping;
- 28-DIP icons preserve alpha/aspect;
- long labels ellipsize;
- hover, focus, pressed, and externally active states do not move geometry.

- [ ] **Step 2: Exercise real application menus**

Record results for AMD Software, Google Drive, Windows Security, Realtek Audio,
and Vivaldi. Verify submenus and commands are owned by the target application
and open beside our panel.

- [ ] **Step 3: Exercise accessibility and failure paths**

Verify pointer right-click, `Shift+F10`, context-menu key, Enter, Escape,
outside dismissal, one blocked/elevated application fallback, owner exit
between discovery and click, Explorer restart, and a negative-coordinate
secondary monitor.

- [ ] **Step 4: Record performance/stability evidence**

Keep Apps closed and confirm no tray polling occurs. Reopen it rapidly and
confirm one active scan plus one coalesced pending request. Confirm one native
strategy sequence per click and no owner-thread wait on the target app.

- [ ] **Step 5: Commit QA evidence and final fixes**

```powershell
git add docs/qa/2026-07-24-background-apps-native-menu.md
git commit -m "test(apps): document native menu validation"
git status --short
```

Expected: only intentionally ignored local visual-companion artifacts may
remain outside Git.

## Final acceptance checklist

- [ ] Balanced Apps panel is 288 DIP wide with 40-DIP rows, 28-DIP icons, and
  at most eight visible rows.
- [ ] Left-click and Enter keep the existing safe primary activation.
- [ ] Right-click, `Shift+F10`, and the context-menu key target the same
  focused row.
- [ ] Supported notification icons open the original app-owned menu beside the
  panel.
- [ ] Unsupported/blocked icons show an honest shell-owned fallback.
- [ ] A user action emits exactly one non-blocking callback strategy.
- [ ] External-menu state releases for every documented end/failure path.
- [ ] No Jump List scraping, Explorer injection, periodic polling, unbounded
  worker, raw-path diagnostic, or blocking application callback exists.
- [ ] Architecture, formatting, Clippy, workspace tests, release build, target
  cleanup, startup smoke, and native visual QA all pass.
