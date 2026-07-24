# Background Apps Balanced List and Native Menu Design

- Status: Design approved; written-spec review pending
- Date: 2026-07-24
- Classification: L — cross-process Windows tray discovery, native callback forwarding, and popover lifecycle changes
- Supersedes: the visual-polish and exact-callback non-goals in
  `2026-07-16-background-apps-topbar-design.md`
- Requires: a new ADR that amends ADR-0008 before implementation is complete

## Goal

Turn the `Apps` popover into a visually consistent notification-area list and
make right-click open the original application-owned tray menu beside the
selected row.

The interaction must preserve UI responsiveness and fail closed. It must not
pretend that a generic fallback is the application's native menu.

## Scope boundary

This feature targets Windows notification-area icons.

The Codex example supplied during design is a taskbar Jump List, which uses a
different Windows mechanism. Complete third-party Jump Lists are not available
through a supported public read API. This delivery does not scrape or inject
into Explorer to reproduce `Running`, pinned, custom, or application-task
sections from another application's Jump List.

When a row represents a real notification-area icon, the original application
menu is forwarded. Rows that only have the existing registry/process fallback
retain safe activation and receive a clearly identified generic context menu.

## Research basis

MyDockFinder is closed-source, so its internal implementation cannot be
verified directly. Its official changelog documents a per-application tray
activation setting that automatically tests the right-click menu and asks the
user to choose another activation method when the menu does not appear. This
is direct evidence that it forwards app-specific tray activation through more
than one compatibility strategy rather than reconstructing a universal menu:

- [MyDockFinder official changelog](https://steamcommunity.com/app/1787090/announcements/)
- [MyDockFinder Steam feature list](https://store.steampowered.com/app/1787090/MyDockFinder/)

ManagedShell and Cairo provide a public reference implementation:

- [Explorer tray discovery](https://github.com/cairoshell/ManagedShell/blob/8f5b97ecb4088792dbc770f1bacf28aeaee9950c/src/ManagedShell.WindowsTray/ExplorerTrayService.cs#L61-L169)
- [Notification icon model and deduplication](https://github.com/cairoshell/ManagedShell/blob/8f5b97ecb4088792dbc770f1bacf28aeaee9950c/src/ManagedShell.WindowsTray/NotificationArea.cs#L274-L369)
- [Original callback forwarding](https://github.com/cairoshell/ManagedShell/blob/8f5b97ecb4088792dbc770f1bacf28aeaee9950c/src/ManagedShell.WindowsTray/NotifyIcon.cs#L409-L509)
- [Cairo tray pointer wiring](https://github.com/cairoshell/cairoshell/blob/e1ce0b67caaf2e475b48c25f6b4105cad44c7c3d/Cairo%20Desktop/CairoDesktop.MenuBarExtensions/SystemTrayIcon.xaml.cs#L137-L155)

Microsoft documents the owner-defined `hWnd`, `uID`, `uCallbackMessage`,
`guidItem`, version behavior, and callback delivery for an application's own
notification icon. Microsoft does not document a public global enumeration
API for other applications' tray icons:

- [NOTIFYICONDATA](https://learn.microsoft.com/en-us/windows/win32/api/shellapi/ns-shellapi-notifyicondataw)
- [Shell_NotifyIcon](https://learn.microsoft.com/en-us/windows/win32/api/shellapi/nf-shellapi-shell_notifyiconw)
- [Shell_NotifyIconGetRect](https://learn.microsoft.com/en-us/windows/win32/api/shellapi/nf-shellapi-shell_notifyicongetrect)

The Explorer toolbar reader and third-party callback forwarder are therefore a
private compatibility adapter, not a supported Windows contract.

## Visual direction

The selected direction is **Balanced**.

### Visual thesis

Use a calm Windows system surface with one continuous reading rhythm. The icon,
friendly application name, focus state, and menu anchor must feel like one
row, not unrelated elements floating inside translucent glass.

No new hard-coded color palette is introduced. The surface reuses the current
dynamic renderer brushes for base, raised, hover, focus, primary, secondary,
rim, and error states so light, dark, high-contrast, and solid fallback modes
remain coherent.

### Geometry and content

- Width: 288 DIP, matching the established system-panel width.
- Header: `Apps`, with the active item count aligned on the right.
- Body begins below the existing notch and header rhythm.
- Row height: 40 DIP.
- Horizontal row inset: 16 DIP.
- Row corner radius: 9 DIP.
- Icon optical box: 28 by 28 DIP.
- Native icons preserve alpha and aspect ratio. A raised rounded fallback is
  drawn only when native extraction fails.
- Label begins on one shared column and consumes the remaining width.
- Long labels are ellipsized. There is no competing detail column.
- No permanent dot, chevron, or decorative trailing control is shown.
- At most eight rows are visible. Additional rows use the existing wheel and
  keyboard scrolling behavior.

The friendly label resolution order remains tooltip, file description, then
clean executable stem. A tooltip that is only the raw executable stem may
yield to a non-empty file description. Labels are never title-cased
heuristically because that would damage product names.

### Interaction states

- Rest: no per-row card fill.
- Hover: one continuous neutral rounded band.
- Keyboard focus: visible focus treatment using the current focus token.
- Pressed: the current pressed token without geometry movement.
- External menu open: the originating row remains highlighted.
- Loading, empty, Explorer-restarting, and unavailable states preserve panel
  width and header geometry.

There is no new animation family. Opening, scrolling, focus, and reduced-motion
behavior reuse existing popover behavior.

## Native tray model

The platform model adds a notification-icon identity separate from the current
registry/process catalog:

- owner window handle;
- owner process ID;
- icon ID;
- application-defined callback message;
- negotiated notification-icon version;
- optional icon GUID;
- tooltip;
- executable identity;
- icon source;
- observation generation.

Raw native identity stays private to `shell-platform-windows`. The renderer
receives only stable presentation data and typed row identities.

The final catalog prefers a live native notification icon. The current
`NotifyIconSettings` plus live-process catalog remains a fallback for entries
that cannot be read from the Explorer tray. Native and fallback entries
deduplicate by normalized executable identity, then by native
`hWnd + uID/GUID` identity where available.

## Discovery architecture

### `ExplorerTraySource`

A private Windows adapter:

1. Locates the Explorer notification-area host and overflow surfaces.
2. Reads a fixed, bounded number of toolbar entries.
3. Uses a fixed-size remote buffer and minimal required process rights.
4. Copies only the known toolbar and tray-data structures.
5. validates structure size, pointer range, owner HWND, owner PID, callback
   message, string length, and icon count before constructing an entry.
6. Releases every remote allocation, process handle, and duplicated icon on
   all success and failure paths.

Unsupported Explorer layouts return an explicit unsupported result. They never
fall through to partially decoded memory.

### Worker ownership

The existing `BackgroundAppsWorker` remains the only asynchronous owner.

- One capture may run at a time.
- Reopens coalesce to the latest pending generation.
- Discovery remains on demand; no periodic polling is added.
- A late result cannot replace a newer or dismissed popover.
- Worker shutdown is joined before native window teardown.

The worker combines the native source with the existing registry/process
fallback and returns one bounded pure snapshot.

## Activation architecture

### Left-click

A native entry receives its original application callback protocol. A fallback
entry retains the current focus-or-open behavior.

### Right-click

Pointer right-click or `Shift+F10` produces a typed
`OpenBackgroundAppContextMenu` intent containing the stable row ID and anchor
point.

Before forwarding:

1. Revalidate that the observation generation is current.
2. Validate `IsWindow(owner_hwnd)`.
3. Validate that `GetWindowThreadProcessId(owner_hwnd)` still matches the
   captured owner process.
4. Permit that owner process to take foreground focus.
5. Arm the external-menu lifecycle hold.

The adapter supports three compatibility strategies:

1. **Version-aware:** use the negotiated modern notification-icon semantics,
   including point and icon ID packing required by the icon version.
2. **Legacy:** send the owner callback with right-button down and up semantics.
3. **Combined:** use the compatibility sequence demonstrated by ManagedShell,
   adding `WM_CONTEXTMENU` for icons that require it.

Exactly one strategy runs for each user action. The initial strategy is chosen
deterministically from the negotiated notification-icon version. If no menu
signal or eligible foreground popup appears within one second, the coordinator
releases the hold and opens the shell-owned fallback with a `Try alternate
activation` compatibility action. A second protocol is sent only after that
explicit user choice or a later right-click; the same click never emits
multiple protocol sequences.

The first strategy that produces an observed menu is cached in memory by
executable identity for the lifetime of the shell process. Failed strategies
are also remembered for that session so the next right-click selects the next
eligible method. No strategy is persisted in V1 configuration and executable
paths are not logged.

All cross-process callback delivery uses non-blocking notification APIs. The UI
thread never waits for the target application to process a message.

## External menu lifecycle

The background-apps popover must remain visible behind the application-owned
menu.

An `ExternalMenuCoordinator`:

- marks the originating row as externally active before forwarding;
- observes standard menu popup start/end events for the owner process;
- uses foreground-window ownership as a fallback for custom menus;
- suppresses the one deactivation dismissal caused by opening the external
  menu;
- restores focus to the originating row when the menu closes without an
  action;
- releases on menu end, owner exit, Explorer restart, `Escape`, popover
  replacement, or shell shutdown;
- has a 30-second watchdog for custom menus that expose no completion event.

The watchdog only releases the hold; it does not send input to or close another
application's window.

## Generic fallback

When native tray identity is unavailable or blocked by integrity boundaries,
right-click opens a clearly shell-owned menu beside the row.

The fallback contains only actions that can be validated safely, such as
opening/focusing the application and opening its file location when the path is
still associated with the captured live process. It does not copy labels from
examples, invent app-specific tasks, terminate processes, or claim to be the
native application menu.

## Jump Lists

Taskbar Jump Lists are not treated as tray menus.

Microsoft's public APIs allow an application to create its own tasks and allow
limited retrieval of Recent/Frequent destinations, but do not provide a
supported way to read another application's complete task list, pinned items,
or custom categories:

- [Application User Model IDs](https://learn.microsoft.com/en-us/windows/win32/shell/appids)
- [IApplicationDocumentLists](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nn-shobjidl_core-iapplicationdocumentlists)
- [Jump Lists](https://learn.microsoft.com/en-us/windows/win32/shell/taskbar-extensions)

Deep Explorer injection, private Jump List file parsing, and taskbar input
simulation are non-goals because they conflict with the stability objective.

## Error handling

- Explorer restart invalidates the native catalog and requests one coalesced
  refresh if the popover remains open.
- Owner exit or HWND/PID mismatch cancels activation and refreshes the row.
- Access denied, cross-integrity restrictions, or unsupported Explorer layout
  degrade to the generic fallback.
- A malformed native entry is skipped. A wholly unsupported capture preserves
  fallback entries and exposes a redacted diagnostic code.
- A failed strategy probe cannot leave the external-menu hold armed.
- Failure of one app does not change the cached strategy of another app.
- Multiple monitors use physical cursor coordinates and the selected monitor's
  DPI/work area; negative desktop coordinates remain valid.
- Raw pointers, paths, tooltips, window titles, and user names never enter
  diagnostics.

## Accessibility

- Arrow keys move through enabled rows.
- `Enter` performs primary activation.
- `Shift+F10` and the keyboard context-menu key open the same context action as
  pointer right-click.
- Before forwarding, `Escape` cancels the shell interaction. After the
  application-owned menu opens, that application handles `Escape`; the
  coordinator only observes the resulting menu-end event and never injects an
  Escape key into another process.
- Focus remains visible before, during, and after the external menu.
- High-contrast and large-text behavior reuse existing renderer formats and
  brushes; labels remain contained.

## Performance limits

- No polling timer.
- Maximum 256 raw native or registry candidates.
- Maximum 32 final catalog entries.
- Maximum one active capture and one coalesced pending request per shell slot.
- Maximum one non-blocking native activation sequence per user action.
- Icon extraction remains cached by the renderer and never occurs in a redraw
  callback.
- Cross-process reads occur only during on-demand capture and use fixed-size
  buffers.

## Testing

Use test-first development for every behavior.

### Pure and renderer tests

- balanced panel is 288 DIP wide;
- header and active-count text do not overlap;
- rows are 40 DIP high with a 28 DIP icon box;
- no more than eight rows are visible;
- long labels remain contained and ellipsized;
- hover, focus, pressed, and externally active states preserve geometry;
- pointer hit testing distinguishes primary and context activation;
- keyboard context activation targets the focused row.

### Platform tests

- decode valid 64-bit Explorer toolbar/tray fixtures;
- reject truncated, oversized, null, and inconsistent native structures;
- deduplicate native and registry fallback entries;
- discard stale observation generations;
- exact modern, legacy, and combined callback sequences;
- never emit more than one strategy sequence for one user action;
- expose the alternate strategy only after an observed failure;
- reject owner HWND reuse when the PID differs;
- release external-menu hold for menu end, timeout, owner exit, Explorer
  restart, popover replacement, and shutdown;
- fall back on access denied or incomplete metadata;
- callback forwarding never waits on the target process.

Tests use injected process-memory, window, callback, clock, and menu-event
interfaces. They never read Explorer memory or message real third-party
applications.

### Native QA

- AMD Software application-owned submenu;
- Google Drive;
- Windows Security;
- Realtek Audio;
- Vivaldi;
- one elevated or otherwise blocked application exercising fallback;
- Explorer restart while the panel is open;
- owner exit between discovery and right-click;
- 100%, 125%, and 150% DPI;
- primary and negative-coordinate secondary monitors;
- pointer, `Shift+F10`, context-menu key, Escape, and outside dismissal.

Final gates are formatting, architecture policy, Clippy with warnings denied,
workspace tests, release build, single-target cleanup, native startup smoke,
and rendered inspection of the balanced list.

## Architecture governance

ADR-0008 explicitly rejected toolbar reading and callback forwarding. The
implementation must add an ADR that supersedes only those clauses while
preserving ADR-0008's on-demand capture, bounded concurrency, privacy,
generation, shutdown, and failure-isolation requirements.

The private Windows adapter must not leak native structures into `shell-core`
or `shell-renderer`.

## Rollout and rollback

Deliver in three internal slices behind one private platform capability:

1. balanced presentation and typed right-click intent;
2. native tray discovery with fallback catalog;
3. callback forwarding and external-menu lifecycle.

If native discovery or forwarding proves unstable on the target Windows build,
disable that private capability and retain the balanced list with the existing
safe activation/fallback behavior. No persisted schema migration is required.

## Success criteria

- The Apps list uses the approved balanced geometry and remains readable with
  long names.
- Right-click on a supported tray app opens the app-owned menu beside the
  selected row.
- Submenus and application-specific commands remain owned and executed by the
  original process.
- Unsupported apps degrade honestly to a safe shell-owned menu.
- Left-click, keyboard, scroll, Explorer restart, and dismissal remain stable.
- No new polling loop, unbounded worker, blocking cross-process call, raw-path
  diagnostic, or Jump List injection is introduced.
