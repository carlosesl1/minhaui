# Windows shell stability and Settings audit — 2026-08-12

## Outcome

This review converted the highest-risk findings into a first stabilization
slice. It does **not** declare the shell production-ready: native Windows QA,
UI Automation event notifications, journal arming before taskbar mutation,
single-instance/native recovery validation, and sustained performance testing
remain release gates.

The audit covered the Rust configuration/core/controller boundaries, the Win32
message and AppBar paths, DirectComposition rendering, multi-monitor ownership,
the supplied MyDockFinder reference screenshots, and the existing Obsidian
Glass design contract.

## Highest-risk findings

| Priority | Finding | Decision in this slice |
|---|---|---|
| P0 | Stable startup could hide/mutate Explorer while the watchdog only simulated recovery. `panic = "abort"` makes `Drop` insufficient crash recovery. | Stable builds no longer replace the Windows taskbar. The legacy path is source-gated behind `experimental-taskbar-replacement`. |
| P0 | The Apps module installed a sign-in helper and injected a DLL into Explorer on its normal path. | Stable builds use the read-only registration/process fallback. Injection and the helper are source-gated behind `experimental-tray-bridge`. |
| P1 | Settings was a fixed topmost popup with keyboard-only navigation; most sections emitted an action the owner ignored. | Settings is a normal resizable window with an adaptive typed scene, pointer hit-testing, real sections, and Apply/Cancel/Reset transactions. |
| P1 | Each monitor held an independent configuration snapshot and Settings could overwrite newer Dock order changes or silently discard another monitor's draft. | Process-local configuration writes are serialized and Settings merges only settings fields, preserving the latest Dock items/layout. Commits broadcast to every monitor; a dirty remote draft is preserved with an explicit review warning and a new Cancel baseline. |
| P1 | The global Win32 event queue was unbounded and accepted every high-frequency movement/refresh. | The queue is bounded at 1,024 entries, coalesces latest-value work per target while preserving discrete barriers, recovers poisoned locks, and rolls back failed wakes. |
| P1 | Top-bar overflow displayed `+N` but opened the system menu; focus could address modules no longer rendered. | Overflow now owns the actual hidden-module list, is focusable, opens a native keyboard-accessible menu, and routes selection through existing module intents. |
| P1 | Persisted top-bar density and appearance opacity/radius were not applied consistently. | Density respects user preference with a narrow-width safety fallback. Opacity/radius are bounded renderer preferences and support live preview/revert. |
| P1 | `shell-watchdog` only simulated lifecycle events and normal packages launched `shell-app` directly. | Packaged startup now enters the watchdog, which launches the sibling shell, monitors its heartbeat, applies bounded restart backoff, and attempts safe mode once after a crash loop. |
| P1 | Six swapchains were created per monitor even when transient windows were never opened, and shared transient icon entries had no memory bound. | Only Topbar and Dock materialize eagerly; Popover, App Menu, Preview and Settings are lazy and rematerialize only while visible. The shared native icon cache uses deterministic item and estimated-byte LRU budgets. |

## Settings vertical slice

The native Settings shell now follows a single draft transaction:

1. A control changes `draft` and validated `preview`.
2. Dock, Top bar, Quick Controls, and appearance update in the owning monitor.
3. **Apply** persists a settings-field merge and broadcasts the committed
   configuration to all monitor slots.
4. **Cancel** or dismissal restores the committed runtime configuration.
5. **Reset** creates a default settings draft without deleting a newer Dock
   item layout during persistence.

Opening another shell surface or closing Settings now follows the same dismissal
path: it cancels the draft and restores the committed preview before hiding the
window. A concurrent commit never silently replaces a dirty draft; the editor
keeps it visible and asks the user to review before applying it over the newer
baseline.

Persistence failure is non-fatal: the shell keeps the validated draft and live
preview, reports that the save failed in Settings, and allows another Apply.

Implemented pages:

- Dock: icon size, spacing, alignment, magnification, and auto-hide.
- Top bar: compact/comfortable density.
- Modules: visibility for customizable status modules.
- Quick Controls: capability-filtered visibility and keyboard reorder.
- Appearance: surface opacity and shared Dock/panel corner radius.

Sections without a complete runtime contract remain visibly unavailable with a
reason. The UI does not present nonfunctional controls as finished features.

## Windows design and platform decisions

- Settings uses a normal, long-lived top-level window and a Mica main-window
  backdrop. Microsoft describes Mica as the base material for long-lived app
  and settings windows: <https://learn.microsoft.com/en-us/windows/apps/design/style/mica>.
- The layout mirrors NavigationView principles: prominent left navigation at
  wider sizes, a content page with Back at narrow sizes, and 24/16 DIP content
  gutters: <https://learn.microsoft.com/en-us/windows/apps/develop/ui/controls/navigationview>.
- Settings organization and wording follow Microsoft's app settings guidance:
  <https://learn.microsoft.com/en-us/windows/apps/design/app-settings/guidelines-for-app-settings>.
- System backdrop selection and down-level fallback follow the desktop backdrop
  contract: <https://learn.microsoft.com/en-us/windows/apps/develop/ui/system-backdrops>.
- The Top bar remains a documented AppBar. It must register/unregister with
  `ABM_NEW`/`ABM_REMOVE`, negotiate position, and coexist with other AppBars:
  <https://learn.microsoft.com/en-us/windows/win32/shell/application-desktop-toolbars>.
- Stable code must not impersonate or inject into Explorer's notification area.
  Unsupported research paths remain opt-in and must not ship.

## Performance observations

The bounded queue prevents memory growth during bursts, and unchanged top-bar
hover events no longer present a new frame. Live configuration changes also
preserve fullscreen auto-hide, so an appearance preview cannot reveal the Dock
over a fullscreen application. Popover, app menu, preview, and Settings now
defer their swapchains until first use; visible lazy surfaces are rematerialized
across renderer rebuild while hidden ones remain absent. The shared transient
icon cache is bounded to 256 entries and about 32 MiB of estimated bitmap/key
storage. Dock icon identity, package/manifest, and filesystem source resolution
now runs on one process-wide STA worker with latest-per-monitor scheduling,
bounded target state, generation checks, and an immediate system fallback; only
bitmap decode and GPU upload remain on the UI/render path. Native and routed
events also run in alternating bounded batches so a single flood cannot
monopolize the UI thread. Desktop blur now uses one process-wide GDI worker with
fair latest-per-popover scheduling, a 16-slot request/result bound, and a 32 MiB
byte-budgeted CPU raster cache. The UI thread only performs the Direct2D upload
and falls back immediately when no raster is ready; worker creation failure
disables this optional effect instead of failing the renderer. The larger
performance work remains:

- share the D3D/D2D device across monitor slots;
- move remaining icon decode and CPU-side raster work off the UI thread;
- distinguish redraw, resize, surface recreation, and device recreation.

## Release gates still open

1. Complete the watchdog recovery boundary. External process supervision,
   heartbeat monitoring, bounded crash-loop restart, one-shot safe-mode fallback,
   the bounded/versioned write-ahead journal, and fail-closed native restoration
   of a compatible V1 journal are implemented. A bounded, canonical arming
   protocol and taskbar fingerprint now exist as pure `shell-core` primitives.
   The watchdog also has exclusive durable `Prepared` creation and an
   authenticated `Prepared` -> `Applied` transition that preserves a different
   or corrupt journal. These primitives are not connected to either process;
   arming before native mutation and crash-at-every-phase recovery testing on
   real Windows remain open.
2. Validate the implemented single-instance-per-user-and-session activation
   forwarding under concurrent launch, elevated/medium-integrity launch, RDP,
   Fast User Switching, and primary-process failure during startup.
3. Validate the Settings-only server-side UI Automation provider on native
   Windows with Narrator, Accessibility Insights, and automated clients. The
   provider now handles `WM_GETOBJECT`, fragment navigation, screen bounds,
   accessible properties, and Invoke/Toggle/RangeValue patterns through queued
   controller actions. Snapshot-diffed focus/property/structure notifications
   are implemented and remain part of the native validation gate; tooltips and
   providers for the remaining custom D2D surfaces are still open.
4. Run Windows quality gates and native soak tests across mixed DPI, monitor
   attach/detach, Explorer restart, sleep/resume, device loss, WARP, high
   contrast, and reduced motion.
5. Wire the synchronous `--restore-only` contract into supported update and
   uninstall pipelines. The checked-in scripts expose and validate the contract,
   but merely copying them into MSIX/Steam layouts does not register a platform
   lifecycle hook.

The private Windows-policy gate is closed in this revision: Night Light, Do not
disturb, and default audio-output selection now use Microsoft's documented
`ms-settings:nightlight`, `ms-settings:quiethours`, and `ms-settings:sound`
routes. Stable builds no longer compile the CloudStore dependency, WNF
quiet-hours bridge, or undocumented `IPolicyConfig` COM ABI. Documented Core
Audio enumeration, mute, master volume, and per-session volume remain native.

## Verification boundary

A pinned Rust 1.97 toolchain was bootstrapped for this review. Windows-target
`cargo check` and Clippy passed for the workspace, all targets, and all features;
the platform-independent core/configuration/diagnostics/watchdog test suites
also passed on Linux. The cross-check used diagnostic C++ bridge stubs, so it
does not validate MSVC compilation, linking, Win32/DirectComposition execution,
or native visuals. The GitHub Windows workflow remains the authoritative format,
Clippy, test, release-build, dependency, and license gate. Native visual
acceptance must be captured on the controlled Windows desktop image; the eight
reference screenshots are documentation inputs, not proof of runtime parity.
