# Background Apps Topbar Dropdown Design

- Status: Approved
- Date: 2026-07-16
- Classification: M — new runtime-only topbar module and read-only Windows Adapter

## Goal

Add a compact dropdown to the topbar that lists applications currently running
through Windows notification-area icons, such as AMD Software, Windows
Security, Discord, Bing Wallpaper, Google Drive, ChatGPT, and Steam.

The first delivery is functional. It follows the existing topbar and popover
visual language; detailed visual polish remains a later pass.

## User experience

- Add an icon-only `BackgroundApps` module to the right status cluster, before
  the clock.
- Its accessible name and tooltip are `Aplicativos em segundo plano`.
- Activating it opens the existing exclusive popover surface.
- The popover header is `Aplicativos em segundo plano`.
- Each row shows the executable icon and a privacy-safe application label.
- Long labels are ellipsized and never overlap the popover edge.
- The list has a bounded height and scrolls when required.
- Pointer, keyboard activation, Escape, outside dismissal, and focus behavior
  reuse the existing popover controller.
- An empty snapshot shows `Nenhum aplicativo em segundo plano`.
- A failed observation shows `Aplicativos em segundo plano indisponíveis` and
  never closes the shell.

There is no new animation family. Opening, dismissal, focus, and reduced-motion
behavior remain identical to the current popovers.

## Source of truth

Windows exposes `Shell_NotifyIcon` operations for an application to add,
modify, and remove its own notification-area icon, but does not expose a
public global enumeration operation. On the target Windows 11 build, closed
overflow icons are also absent from the UI Automation tree until the native
flyout opens.

The selected read-only Adapter therefore combines:

1. Entries under `HKCU\Control Panel\NotifyIconSettings` maintained by
   Explorer.
2. A current same-session process snapshot.

An entry is visible only while a matching executable is running. Exact
normalized paths are preferred; executable-name matching is a fallback for
versioned packaged paths and known-folder encoded paths. Duplicate registry
entries for the same live executable collapse to one application row.

This Explorer registry contract is not a documented public API. It is isolated
behind a private Windows Adapter and must fail closed. No registry value is
written or persisted by Minha UI.

## Label and icon resolution

The display label uses the first available value in this order:

1. a non-empty first line of `InitialTooltip`;
2. the executable file description;
3. a cleaned executable stem.

The executable icon reuses the existing platform icon resolution and renderer
cache. A generic application glyph is used when icon extraction fails. Raw
paths and tooltip contents never enter diagnostics.

## Activation behavior

Selecting a row performs a conservative open action:

1. Focus a matching eligible top-level window already present in the
   centralized window observation snapshot.
2. If no eligible window exists, launch the already-running executable once.
   Single-instance applications can interpret this as an instruction to show
   their primary interface.
3. If neither action succeeds, record a redacted failure and keep the shell
   alive.

The implementation does not access another process's notification callback,
inject input into Explorer, or simulate undocumented notification-icon
messages. Exact tray left-click or context-menu behavior is outside this
increment.

## Architecture and ownership

### `shell-core`

- Add pure `TopbarModuleKind::BackgroundApps` and
  `Popover::BackgroundApps` variants.
- Treat the module as runtime-only and visible by default.
- Do not add it to the V1 persisted topbar list or make it configurable in this
  increment.

### `shell-renderer`

- Continue to own topbar and popover geometry, clipping, ellipsis, hit testing,
  and row presentation.
- Accept an optional platform icon source on a popover row without learning
  about the registry, processes, or Windows actions.

### `shell-platform-windows`

- A private notification-area observation Adapter reads registry entries,
  normalizes paths, matches the current process snapshot, resolves labels, and
  returns a pure bounded list.
- Observation runs on demand outside the UI thread. It does not add a periodic
  timer or continuous polling.
- Results return through the existing native event queue. A generation token
  prevents a late result from replacing a newer or dismissed popover.
- `TopbarController` opens the typed popover; `PopoverController` owns rows and
  selection; the Windows action boundary focuses or opens the selected app.
- `NativeSurfaceRuntime` remains graphical only.

### `shell-config` and `shell-app`

- No schema migration, new persisted field, or command-line flag.
- Runtime composition inserts the fixed module before the clock after loading
  V1 configuration.

No new crate, workspace dependency edge, network request, or background service
is introduced.

## Performance limits

- No continuous refresh timer.
- Read the notification-area snapshot only when the dropdown opens or when the
  user explicitly requests a refresh in a later increment.
- Bound the raw registry entries and final rows to prevent unbounded work.
- Reuse the current process observation where available instead of starting a
  second continuous process discovery path.
- Cache extracted icons through the existing renderer resource lifetime.
- Never read registry values, version metadata, or executable icons from a
  redraw callback.

## Error handling and compatibility

- Missing registry key: empty state.
- Registry access failure or malformed entry: skip the entry; if the complete
  observation fails, show the unavailable state.
- Explorer restart: the next open performs a fresh observation.
- Process exits during observation: omit it on the next open; activation failure
  is harmless.
- Unsupported future Windows layout: the Adapter returns unavailable without
  crashing or blocking the shell.
- Safe mode continues to allow the text-only list but may use generic icons if
  native icon extraction is unavailable.

## Testing

Use test-first development for each new behavior.

- Pure tests for path normalization, live-process matching, stale-entry
  filtering, deduplication, label fallback, ordering, and row bounds.
- Controller tests for topbar intent, popover exclusivity, keyboard activation,
  empty/unavailable states, and selected application action.
- Renderer tests for icon fallback, ellipsis, bounded list geometry, and
  non-overlap at narrow widths and large text scale.
- Platform tests use injected registry/process fixtures and never inspect or
  launch real user applications.
- Native smoke test confirms the local list includes representative active
  notification-area applications and that the shell survives a failed open.
- Final gates: formatting, architecture policy, Clippy with warnings denied,
  workspace tests, release build, and rendered inspection.

## Privacy and safety

- Do not log executable paths, user names, registry values, tooltips, or window
  titles.
- Do not persist the observed list.
- Do not terminate, suspend, or mutate background applications.
- Do not write to Explorer notification-area settings.
- Keep every native operation behind a private safe Interface with documented
  `unsafe` invariants where required.

## Non-goals

- Replacing the native Windows notification-area flyout.
- Exact tray-icon click, double-click, hover, or context-menu callbacks.
- Controlling whether Windows promotes or hides an icon.
- Closing or suspending background applications.
- Persisting row order or visibility.
- Redesigning the topbar or popover visual system.

## Success criteria

- The topbar exposes one compact background-apps button before the clock.
- Opening it lists only currently running applications associated with
  notification-area registrations.
- The local representative set includes AMD Software, Windows Security,
  Discord, Bing Wallpaper, Google Drive, ChatGPT, and Steam when those
  executables are running.
- Rows remain contained, accessible, keyboard-operable, and bounded.
- Selecting a row focuses an eligible window or safely requests the application
  to open.
- No continuous timer, schema migration, or renderer ownership regression is
  introduced.
- Failures never terminate the shell.

## Rollback

Remove the runtime-only module, popover variant, private Windows Adapter, and
optional row icon source. Existing V1 configuration files remain unchanged and
readable.
