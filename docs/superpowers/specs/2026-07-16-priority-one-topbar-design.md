# Priority One Topbar Functional Design

- Status: Approved
- Date: 2026-07-16
- Classification: L — public renderer contract, persistence, and new Windows FFI

## Goal

Deliver the first-priority topbar functions as a working Windows-native slice
while preserving the current visual language. Rich visual controls, spacing,
animation, and polish remain a later pass.

## Included functions

1. System menu with Settings, Task Manager, session, and power commands.
2. Active application identity in the left cluster.
3. Windows Search launcher.
4. Live network status and throughput with Windows network settings.
5. Live audio status, volume commands, sound settings, and global media keys.
6. Live battery/power status with power settings and session commands.
7. Control Center shortcuts for Windows Quick Settings, Wi-Fi, Bluetooth,
   display, projection modes, focus, and sound.
8. Current-month calendar with previous/next navigation and date/time settings.
9. Persisted visibility and order for the configurable status modules through
   the existing config transaction boundary.

## Functional-first interaction model

The existing row-based popover remains the first delivery surface. Each
interactive row emits a typed intent. Native actions execute only after they
cross a dedicated Windows action adapter.

- Topbar modules may open a popover, invoke Search directly, or be informational.
- Network, audio, power, and calendar rows are regenerated from the latest
  topbar snapshot whenever a popover opens.
- Volume is changed through explicit lower, mute/unmute, and raise actions.
- Media control uses the documented global media-key path. Track metadata and
  artwork are deferred.
- Control Center uses Windows Quick Settings and documented `ms-settings:`
  routes. Projection uses the documented Windows display topologies for PC
  screen only, duplicate, extend, and second screen only. Direct undocumented
  Wi-Fi/Bluetooth radio mutation is excluded.
- Destructive session actions retain the existing two-step confirmation.
- Calendar month navigation updates the open popover without closing it.

## State and ownership

### `shell-core`

- Add `AppIdentity` and `Search` topbar module kinds.
- Add a reorder event that moves a module before another module or to the end.
- Preserve the invariant that each module kind appears at most once.
- Visibility and reorder operations emit `PersistConfiguration`.

### `shell-config`

- Preserve the V1 persisted ordered list for configurable status modules.
- Add a builder used by Settings and runtime persistence.
- Runtime composition becomes:
  `SystemMenu, AppIdentity, Search, Network, Volume, Power, Notifications, Clock`.
- `AppIdentity` and `Search` remain fixed runtime modules and are not serialized
  into schema V1.

### Topbar controller and renderer

- Replace the popover-only visual intent with a typed topbar intent:
  `Popover`, `OpenSearch`, or no action.
- Keep `SystemMenu`, `AppIdentity`, and `Search` in the left layout cluster.
- Add the active app label to `TopbarSnapshot`.
- Keep a generic application glyph for this pass; per-app icon rendering is a
  visual follow-up.

### Popover provider

- Hold the latest privacy-safe topbar snapshot and a calendar month offset.
- Build dynamic rows for network, audio, power, and calendar.
- Keep the controller and renderer unaware of Win32 APIs.

### Windows action adapter

- Centralize Windows Search, `ms-settings:` routes, media keys, volume mutation,
  Task Manager, lock, sleep, sign out, restart, and shutdown.
- Keep destructive actions behind the controller confirmation.
- Treat launch failures as typed errors and keep the shell process alive.

## Architectural impact

- **Modules and crates:** `shell-core` owns pure module order and typed intents;
  `shell-config` owns schema validation and persistence; `shell-renderer`
  owns geometry/scenes; `shell-platform-windows` owns status and system-action
  Adapters. `NativeSurfaceRuntime` remains graphical only.
- **Interfaces:** the existing renderer topbar contract carries a pure
  `TopbarIntent`; no new public platform facade item is introduced. Geometry
  helper types are not reexported merely for platform convenience.
- **State owner:** ordered visibility remains in `ShellState`/`ShellConfigV1`;
  live status remains in `TopbarController`; calendar offset remains in the
  popover data owner. Native action Adapters hold no product source of truth.
- **Dependencies:** no new workspace edge or crate.
- **UI thread:** status reads and native commands must be bounded synchronous
  calls. No disk I/O, package discovery, process wait, WinRT async join, or
  unbounded work is allowed in input/redraw callbacks.
- **Diagnostics:** failures use the existing asynchronous, redacted platform
  diagnostics path. No per-frame status logging is added.
- **Tests:** pure reducers, schema validation, layout, calendar, and action
  classification are hermetic. Real Windows behavior remains in the separate
  native-validation gate and never performs destructive commands.
- **Migration:** none. Runtime composition injects the fixed leading modules
  after loading a valid V1 document.
- **Rollback:** remove the two runtime-only placements and typed actions; V1
  documents remain unchanged and readable.
- **ADR:** [ADR-0007](../../architecture/adr/0007-topbar-system-actions.md).

## Calendar model

A pure Gregorian calendar helper owns year/month shifting and six deterministic
week rows. The current local date is supplied by the status reader. Navigation
actions update a bounded month offset and reload the same popover.

## Customization

The existing Settings transaction model gains topbar edits:

- set one module visible/hidden;
- move one module before another or to the end.

The configurable status-module edits participate in preview/apply/cancel/reset
and persist through `ConfigStore`. `SystemMenu`, `AppIdentity`, and `Search`
cannot be hidden or reordered. A richer native settings form is deferred; the
state and transactional behavior are delivered now so the visual editor can
bind to a stable interface later.

## Non-goals

- Apple or MyDockFinder visual copying;
- rich sliders, toggle switches, media artwork, or a graphical calendar grid;
- direct undocumented radio toggles;
- notification account aggregation;
- per-application audio routing;
- online weather;
- visual redesign of the topbar, popovers, or settings window.

## Safety

- Session commands never execute before confirmation.
- Tests exercise action classification without shutting down or signing out the
  development machine.
- Only documented Windows input, shell execution, power, and settings routes are
  used.
- No network service or account is required.

## Success criteria

- Every listed topbar entry has a real behavior or live status.
- Search and global media commands reach Windows through the native adapter.
- Popover content reflects the latest status snapshot.
- Calendar navigation works in the open popover.
- Visibility/order edits validate, preview, apply, and survive config
  round-trips.
- Existing dock, renderer ownership, and overlay behavior remain green.
- Focused tests, Windows package check, formatting, and architecture gate pass.
