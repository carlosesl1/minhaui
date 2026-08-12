# ADR-0009: Native notification-area discovery and callback forwarding

- Status: Accepted
- Date: 2026-07-24
- Owner: platform-overlays
- Related: ADR-0002, ADR-0003, ADR-0004, ADR-0005, ADR-0008,
  `docs/superpowers/specs/2026-07-24-background-apps-native-menu-design.md`

## Context

ADR-0008 established an on-demand notification-area catalog backed by the
Explorer's registry data and live processes. That catalog is safe and useful,
but it cannot identify every live notification icon or open an application-owned
tray menu. The approved native-menu capability needs a bounded compatibility
path for Explorer's toolbar data and the owner-defined callback protocol.

ADR-0009 supersedes only ADR-0008's rejection of Explorer toolbar reads and
third-party notification callbacks. It preserves on-demand capture, one
serialized/coalesced worker, generation checks, bounded catalogs, privacy,
joined shutdown, failure isolation, and registry/process fallback. No other
decision or safety constraint in ADR-0008 is superseded.

## Decision

- `ExplorerTraySource` is a private, version-sensitive compatibility adapter in
  `shell-platform-windows`. It locates the notification-area and overflow
  toolbars, reads a fixed and bounded number of entries on the existing
  background worker, and decodes only explicitly validated x64 structures.
  Unsupported layouts, malformed records, access failures, and integrity
  boundaries produce an unavailable native source; they never expose partial
  remote memory or native handles to `shell-core` or `shell-renderer`.
- The native observation is merged with the existing
  `NotifyIconSettings`/live-process catalog. Native identities are preferred;
  registry/process entries remain the bounded fallback and are deduplicated by
  normalized executable identity. The final catalog remains capped at 32
  entries from at most 256 candidates.
- `BackgroundAppsWorker` remains the only asynchronous owner. It runs at most
  one capture at a time, coalesces a latest pending request, performs no
  periodic polling, rejects stale observation generations, and is joined before
  native window teardown. Privacy rules remain unchanged: raw pointers, paths,
  tooltips, window titles, process identifiers, and user names do not enter
  diagnostics.
- Native rows carry a private owner window, process, icon, callback message,
  negotiated icon version, optional GUID, and observation generation. Before
  activation, the adapter revalidates the generation, owner HWND, and owner
  PID, then permits foreground focus and arms the external-menu lifecycle.
- A native row forwards exactly one deterministic callback strategy per user
  action (version-aware, legacy, or combined). Delivery to an application-owned
  callback window is always non-blocking (`PostMessageW`); the UI thread never
  waits for another process, and `SendMessageW` is not used for callback
  forwarding. If no menu signal appears within the bounded probe window, the
  lifecycle hold is released and the shell-owned fallback is offered.
- Taskbar Jump Lists are a different mechanism and are not tray menus.
  Jump List reconstruction, private Jump List parsing, Explorer injection, and
  input simulation for another application's task list are prohibited.
- Native discovery and forwarding are one private capability. If the adapter is
  unsupported or unstable on a Windows build, it can be disabled while the
  balanced list and the registry/process activation fallback remain available.

## Consequences

### Positive

- Supported notification-area rows can open the original application's menu;
  application-owned submenu behavior and commands remain in that process.
- The UI responsiveness, bounded worker ownership, generation safety, privacy,
  and deterministic shutdown established by ADR-0008 remain in force.
- Unsupported or blocked applications degrade honestly to a shell-owned menu
  without pretending to reproduce application-specific tasks.

### Costs and risks

- Explorer toolbar structures and callback conventions are private and can
  change between Windows versions, requiring fixture coverage and native QA.
- Cross-process memory reads and callback delivery can fail because of access,
  integrity, owner-exit, HWND-reuse, or Explorer-restart conditions.
- The compatibility capability requires an explicit rollback switch and must
  never become a second polling loop or an unbounded worker.

## Alternatives considered

### Keep the ADR-0008 rejection permanently

Rejected because supported native rows would remain unable to open their
application-owned tray menus. The narrow amendment is acceptable only with the
private adapter, bounded worker, non-blocking delivery, and fallback controls
specified here.

### Reconstruct taskbar Jump Lists or inject into Explorer

Rejected. Jump Lists are not notification-area menus, and another application's
complete pinned, recent, custom, or task sections have no supported read
contract. Reconstruction and injection violate the stability and ownership
boundaries.

### Send blocking callbacks to the owner process

Rejected because a hung or slow application could block the shell. Callback
delivery must remain asynchronous and non-blocking.

## Verification

- `& .\scripts\Check-Architecture.ps1` passes the architecture gate.
- Automated pure and platform tests use injected process-memory, window,
  callback, clock, and menu-event interfaces. They cover bounded toolbar
  decoding, malformed and unsupported records, native/fallback deduplication,
  generation expiry, owner HWND/PID revalidation, non-blocking callback
  posting, fallback after a failed probe, Explorer restart, owner exit,
  external-menu hold release, and exact one-strategy callback sequences; they
  never read live Explorer memory or message real third-party applications.
- Live native QA on the target Windows machine exercises real Explorer
  notification-area and overflow discovery and real application-owned menus for
  supported tray applications, plus an elevated or blocked process, Explorer
  restart, multiple DPI/monitor configurations, keyboard context activation,
  Escape, and dismissal.

## Migration and rollback

No persisted schema migration is required. Rollback disables the private native
discovery and callback-forwarding capability, removes native identities from
the merged snapshot, and returns all rows to ADR-0008's registry/process
catalog and safe focus-or-open or shell-owned context-menu behavior. The
serialized worker, generation checks, bounded catalog, privacy, failure
isolation, and joined shutdown remain unchanged during rollback.
