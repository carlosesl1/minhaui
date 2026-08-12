# Adaptive Quick Settings Panel Design

**Date:** 2026-07-18

**Status:** Approved design

**Primary surface:** top-bar controls module and its native popover

**Configuration surface:** Settings > Quick Controls

## 1. Objective

Replace the current row-based Quick Settings popover with a native, adaptive control panel that shows only capabilities the current Windows device can actually use. The top-bar controls icon opens this panel directly. The Settings application configures the visibility and order of compatible controls; it does not duplicate the live controls.

The panel must feel like a compact system surface rather than a settings page: immediate status, direct adjustment when Windows exposes a stable public API, and a precise Windows Settings route when direct control is unavailable or requires permissions the running package does not have.

## 2. Product Decisions

The following decisions are fixed for this delivery:

- The existing top-bar controls slot is the direct entry point to the adaptive panel.
- The persisted module identifier remains compatible with the current `TopbarModuleKind::Notifications` configuration, but its visible role becomes **Controls**, with a sliders-style icon and Quick Settings intent.
- Apple-specific concepts are translated into Windows-native equivalents: AirDrop becomes **Nearby sharing** and Stage Manager becomes **Multitasking / Snap layouts**.
- Unsupported hardware is absent instead of disabled. Hardware that exists but is currently off remains visible and communicates its off state.
- Settings lists only controls supported by the current capability snapshot. If hardware appears later, its control enters the available list automatically.
- A temporarily absent control keeps its saved visibility and order preference internally, so reconnecting hardware restores the previous arrangement.
- The approved layout is a fluid two-column grid. Tiles compact into the next available position; an unpaired final tile spans the full width.
- Direct actions use stable, documented Windows APIs only. Otherwise the control opens the most specific documented `ms-settings:` destination available.
- The interface never invents availability, reports a toggle as successful before Windows confirms it, or presents a decorative control with no action.

## 3. Scope

### Included

- a dedicated Quick Settings scene and native renderer;
- capability detection for networking, Bluetooth, display brightness, sound, battery, projection, and device-class-dependent controls;
- a new Quick Controls section in Settings for compatible-item visibility and ordering;
- direct status and adjustment where public Windows APIs make it reliable;
- documented Windows Settings fallbacks;
- pointer, keyboard, high-DPI, and accessibility behavior;
- configuration migration that preserves existing user settings;
- event-driven refresh when relevant hardware or system state changes.

### Excluded

- a notification center or notification history;
- undocumented registry toggles or private Windows shell protocols;
- continuous desktop, hardware, WMI, WLAN, or Bluetooth polling;
- network scanning, BSSID collection, or nearby-device discovery merely to decorate the panel;
- fake sample hardware in the production UI;
- support for monitors whose DDC/CI implementation cannot be verified safely;
- a separate top-level window for every control group.

## 4. Visual Thesis

**One calm sheet of responsive glass, with controls organized by intent rather than by hardware inventory.**

The panel reuses the refined `Panel` liquid-glass profile already established for the system panel: one continuous surface, restrained edge lighting, a soft blurred background, and contrast tokens that adapt to the material. It must not look like a stack of opaque cards or add another rectangular system backdrop behind the rounded panel.

The approved reference is translated into Windows-native hierarchy:

1. compact connection and productivity tiles;
2. full-width Display section;
3. full-width Sound section;
4. conditional full-width Energy section;
5. a quiet **Edit controls...** footer action.

The panel width follows the existing popover constraints. Height is content-driven, clamped to the monitor work area, and becomes vertically scrollable only when content cannot fit because of capability count, text scaling, or reduced work area.

## 5. Information Architecture and Layout

### 5.1 Adaptive tile grid

The first region contains supported compact controls in the user's saved order:

- Wi-Fi;
- Bluetooth;
- Nearby sharing;
- Focus;
- Multitasking / Snap layouts;
- Projection.

The layout engine filters unsupported controls before placement. Remaining tiles fill two columns from left to right and top to bottom. If the final row has one tile, that tile spans both columns. Removing Wi-Fi or Bluetooth therefore closes the gap instead of leaving an empty slot.

Each tile exposes:

- one recognizable icon from the existing icon system;
- a concise primary label;
- a short state or destination label;
- hover, pressed, keyboard-focus, unavailable-in-the-moment, and active states;
- a direct action or a specific Settings route.

### 5.2 Display

Display is rendered only when the device has a meaningful display capability. It may contain:

- a brightness slider when at least one target display exposes a verified, controllable brightness interface;
- Dark mode;
- Night light.

Brightness is not inferred from laptop form factor. A desktop with a verified controllable monitor may receive the slider; a notebook whose display driver does not expose a supported control does not.

### 5.3 Sound

Sound appears when Windows exposes a default render endpoint. It contains:

- a master-volume slider;
- mute state;
- the current output endpoint name when available;
- a route to output-device settings when switching cannot be performed reliably from the initial implementation.

### 5.4 Energy

Energy appears only when Windows reports a real system battery. It contains:

- battery percentage;
- charging or discharging state;
- Energy saver status or route when available.

UPS-style short-term batteries do not make a desktop look like a notebook unless Windows reports them as a genuine system battery appropriate for this surface.

### 5.5 Footer

**Edit controls...** opens Settings directly at the Quick Controls section. It does not put the panel into an inline edit mode.

## 6. Interaction Model

Opening the controls module creates one capability snapshot, builds the supported-control model, lays it out, and shows the existing native popover window with a `QuickSettingsScene`. The panel remains open while the user drags a slider. A successful tile action updates from confirmed system state rather than optimistic decoration.

Keyboard behavior follows native menu expectations:

- Tab and Shift+Tab traverse interactive elements;
- arrow keys adjust a focused slider in meaningful increments;
- Enter or Space activates a focused tile;
- Escape closes the popover;
- visible focus does not depend on color or glow alone.

Hover is a local tonal lift within the continuous panel surface. Active state is stronger than hover and includes text/icon state. Disabled-in-the-moment is reserved for a supported control that cannot currently act, such as projection with no valid secondary target; truly unsupported controls are omitted.

## 7. Capability Model

### 7.1 Stable control identity

`shell-core` introduces a stable `QuickControlKind` enum for persistence, ordering, rendering, and action routing. It is independent of Windows handles and API structures.

The initial kinds are:

- `Wifi`
- `Bluetooth`
- `NearbySharing`
- `Focus`
- `Multitasking`
- `Projection`
- `Brightness`
- `DarkMode`
- `NightLight`
- `Volume`
- `Battery`
- `EnergySaver`

Grouping is scene metadata, not part of the persisted identity, so a future layout change does not invalidate user order.

### 7.2 Capability snapshot

`shell-platform-windows` produces a `WindowsQuickSettingsCapabilities` snapshot containing support, current state, action mode, status text, and any relevant target identity. The snapshot distinguishes:

- **unsupported:** no usable device or OS capability; hide it;
- **supported/off:** hardware exists and can be represented; show it as off;
- **supported/on:** show current active state;
- **supported/route-only:** show it and open a documented Settings page;
- **temporarily unavailable:** show a disabled-in-the-moment state only when hiding it would make the panel unstable during a transient condition.

No renderer code calls Windows hardware APIs. The renderer consumes a platform-neutral scene model produced by the controller.

## 8. Windows Capability and Action Matrix

| Control | Presence / status source | Primary action | Fallback |
| --- | --- | --- | --- |
| Wi-Fi | `GetIfTable2` / `MIB_IF_ROW2` for IEEE 802.11 hardware; Native Wi-Fi interface state for enabled adapters | documented WLAN operation where permitted | Network & internet Wi-Fi Settings |
| Bluetooth | `BluetoothFindFirstRadio` for radio presence | `Windows.Devices.Radios` only when package identity, `radios` capability, and user access are available | Bluetooth & devices Settings |
| Nearby sharing | supported Windows version and documented Settings destination | route-only initially | Nearby sharing Settings |
| Focus | supported Windows version and documented Settings destination | route-only initially | Focus Settings |
| Multitasking | supported Windows version and documented Settings destination | route-only initially | Multitasking Settings |
| Projection | `QueryDisplayConfig` and valid display paths | `SetDisplayConfig` for supported topology changes | Display / projection Settings |
| Brightness | WMI brightness interface for internal panels; verified DDC/CI capability for external monitors | WMI or high-level monitor API | Display Settings |
| Dark mode | supported Windows version | route-only initially | Colors / personalization Settings |
| Night light | supported Windows version and documented Settings destination | route-only initially | Night light Settings |
| Volume | default render endpoint through Core Audio | `IAudioEndpointVolume` | Sound Settings |
| Battery | `GetPwrCapabilities` / `SYSTEM_POWER_CAPABILITIES`, plus live power status | status display | Power & battery Settings |
| Energy saver | real system battery and supported Windows destination/API | documented action if available | Power & battery Settings |

### 8.1 Wi-Fi privacy boundary

Presence detection does not require scanning networks. The panel avoids SSID/BSSID scans and location-sensitive WLAN data unless a later user-facing feature explicitly needs them and obtains the required consent. `WlanEnumInterfaces` alone is not used as the only hardware-presence test because it enumerates currently enabled wireless interfaces, not every installed wireless adapter.

### 8.2 Bluetooth packaging boundary

The ordinary portable debug executable is expected to use the Bluetooth Settings fallback unless it has package identity, declares the `radios` capability, and receives user access. The UI must not imply a direct toggle when those prerequisites are absent. A packaged build may enable the direct path without changing the scene contract.

### 8.3 Brightness safety boundary

Internal-panel WMI brightness is used only when the monitor exposes the appropriate WMI method. External DDC/CI is used only after capability verification. Because monitors may report or implement DDC/CI incorrectly, an unverified external monitor does not receive a live slider in the first release.

## 9. Component Boundaries

### `shell-core`

- owns `QuickControlKind` and platform-neutral control/state concepts;
- adds `Popover::QuickSettings` for semantic routing;
- preserves the existing persisted top-bar module identity for configuration compatibility.

### `shell-config`

- adds `QuickSettingsSettings` with ordered kinds and visibility preferences;
- uses backward-compatible Serde defaults;
- preserves preferences for temporarily absent controls;
- normalizes duplicates and unknown/removed values without discarding the rest of the configuration.

### `shell-platform-windows`

- owns Windows capability readers and action adapters;
- owns `QuickSettingsController` and the snapshot-to-scene mapping;
- routes Settings URIs through the existing safe system-action path;
- subscribes to targeted lifecycle events and requests a refresh;
- never exposes Win32 handles or COM interfaces to `shell-renderer`.

### `shell-renderer`

- owns `QuickSettingsScene`, layout, hit testing, focus order, and Direct2D drawing;
- renders the scene on the existing popover HWND;
- reuses the shared `Panel` liquid-glass profile and rounded capture/blur path;
- adds no continuous animation or capture loop.

### Settings application

- adds a **Quick Controls** section;
- shows only controls present in the latest capability snapshot;
- lets the user show, hide, and reorder compatible controls;
- saves changes through the existing configuration controller;
- explains route-only controls without presenting them as direct toggles.

## 10. Data Flow

1. The user activates the top-bar controls module.
2. `QuickSettingsController` requests a fresh capability snapshot.
3. Saved order and visibility are applied to supported controls.
4. The controller builds a platform-neutral `QuickSettingsScene`.
5. The existing popover HWND is positioned and rendered with the panel material.
6. A user action is routed to a direct adapter or a documented Settings URI.
7. The controller re-reads affected state and publishes the confirmed scene.
8. Settings edits update configuration; an open panel rebuilds its scene without recreating the whole shell.

## 11. Refresh, Performance, and Privacy

The capability model refreshes:

- when the panel opens;
- after a control action;
- on relevant display, power, audio-endpoint, network, or device-change events;
- when configuration changes;
- after DPI or monitor placement changes when layout must be recomputed.

`WM_DEVICECHANGE` is a refresh trigger, not a source of truth. The controller coalesces bursts and re-queries the relevant subsystem. WLAN notifications may be used for connection/interface changes that do not require location-sensitive scan data. COM and WinRT work runs outside the paint path. Expensive queries are bounded, cached for the open session, and invalidated by events.

The implementation does not continuously poll hardware, enumerate nearby devices, retain network scan results, or persist captured desktop pixels. The existing bounded background capture for panel blur remains tied to popover opening/placement and is not coupled to hardware refresh.

## 12. Failure Behavior

- A failed direct action keeps the last confirmed visual state.
- A short inline status or existing transient diagnostic indicates that the action could not be completed.
- When appropriate, the same control offers its documented Settings fallback.
- Failure of one capability reader does not suppress unrelated sections.
- Missing permissions are represented as route-only, not as missing hardware.
- If the liquid-glass resource fails, the existing protected solid panel fallback preserves readability and rounded geometry.
- If no configurable controls are supported, Settings shows a concise empty state instead of a blank reorder list.

## 13. Accessibility and Localization

- Every control has a programmatic name, role, current value/state, and action description.
- Status is not conveyed by color alone.
- Text and icons use panel-aware contrast tokens rather than hardcoded light or dark foregrounds.
- The layout supports Windows text scaling and localized strings without truncating critical state.
- Sliders expose keyboard increments and meaningful value text.
- Reduced motion disables nonessential transitions; forced colors and disabled transparency use the protected solid material.
- The odd-tile full-width rule remains deterministic for keyboard order and screen-reader traversal.

## 14. Configuration Migration

Existing configurations load unchanged. The new Quick Settings settings object is created with defaults when absent. The default order follows the information architecture in this document, while visibility is filtered at runtime by capabilities.

The current persisted `Notifications` top-bar module key is not renamed during this delivery. Its display metadata and activation intent change to Controls, avoiding a destructive migration. A future notification-center feature must receive a new semantic module or an explicit migration rather than silently taking back this slot.

## 15. Focused Verification

Verification is intentionally proportional to the change:

- unit coverage for capability filtering, stable ordering, odd-tile spanning, and configuration normalization;
- controller coverage for unsupported, supported/off, route-only, and temporarily unavailable states;
- targeted Windows adapter checks behind interfaces so tests do not require specific hardware;
- one native build and focused test run for touched crates;
- a manual pass on the available desktop hardware plus simulated capability snapshots for notebook, no-Wi-Fi, no-Bluetooth, battery, external-display, and high-DPI layouts;
- visual inspection of the real native panel for rounded blur, hover, focus, sliders, empty-gap compaction, and readable contrast.

The verification does not require an exhaustive full-workspace review or long-running hardware polling test.

## 16. Acceptance Criteria

The design is complete when all of the following are true:

- activating the top-bar controls icon opens the adaptive Quick Settings panel directly;
- unsupported Wi-Fi, Bluetooth, brightness, battery, and projection controls are not shown;
- supported-but-off hardware remains visible with an accurate state;
- the tile grid compacts without holes and an odd final tile spans the full row;
- Display, Sound, and Energy sections appear only when their capability rules are satisfied;
- Settings shows only currently compatible controls and can show, hide, and reorder them;
- reconnecting supported hardware restores its saved preference and relative order;
- direct controls update only after confirmed Windows state;
- route-only controls open the correct documented Windows Settings destination;
- no continuous hardware polling, network scanning, or desktop capture loop is introduced;
- the panel uses the shared rounded liquid-glass material without a rectangular backdrop artifact;
- keyboard, high-DPI, forced-colors, reduced-motion, and transparency-disabled fallbacks remain usable;
- the native executable builds and can be handed to the user for testing.

## 17. Authoritative References

- [MIB_IF_ROW2 interface type, hardware, administrative, and operational state](https://learn.microsoft.com/en-us/windows/win32/api/netioapi/ns-netioapi-mib_if_row2)
- [GetIfTable2](https://learn.microsoft.com/en-us/windows-hardware/drivers/network/getiftable2)
- [WlanEnumInterfaces](https://learn.microsoft.com/en-us/windows/win32/api/wlanapi/nf-wlanapi-wlanenuminterfaces)
- [WLAN interface query opcodes](https://learn.microsoft.com/en-us/windows/win32/api/wlanapi/ne-wlanapi-wlan_intf_opcode)
- [WLAN radio state](https://learn.microsoft.com/en-us/windows/win32/api/wlanapi/ns-wlanapi-wlan_phy_radio_state)
- [WlanQueryInterface](https://learn.microsoft.com/en-us/windows/win32/api/wlanapi/nf-wlanapi-wlanqueryinterface)
- [Wi-Fi access and location changes](https://learn.microsoft.com/en-us/windows/win32/nativewifi/wi-fi-access-location-changes)
- [WlanRegisterNotification](https://learn.microsoft.com/en-us/windows/win32/api/wlanapi/nf-wlanapi-wlanregisternotification)
- [BluetoothFindFirstRadio](https://learn.microsoft.com/en-us/windows/win32/api/bluetoothapis/nf-bluetoothapis-bluetoothfindfirstradio)
- [Radio.RequestAccessAsync](https://learn.microsoft.com/en-us/uwp/api/windows.devices.radios.radio.requestaccessasync)
- [Radio.SetStateAsync](https://learn.microsoft.com/en-us/uwp/api/windows.devices.radios.radio.setstateasync)
- [App capability declarations](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/app-capability-declarations)
- [Package identity for non-packaged desktop apps](https://learn.microsoft.com/en-us/windows/apps/desktop/modernize/grant-identity-to-nonpackaged-apps-overview)
- [SYSTEM_POWER_CAPABILITIES](https://learn.microsoft.com/en-us/windows/win32/api/winnt/ns-winnt-system_power_capabilities)
- [WmiSetBrightness](https://learn.microsoft.com/en-us/windows/win32/wmicoreprov/wmisetbrightness-method-in-class-wmimonitorbrightnessmethods)
- [GetMonitorCapabilities](https://learn.microsoft.com/en-us/windows/win32/api/highlevelmonitorconfigurationapi/nf-highlevelmonitorconfigurationapi-getmonitorcapabilities)
- [IMMDeviceEnumerator::GetDefaultAudioEndpoint](https://learn.microsoft.com/pt-br/windows/win32/api/mmdeviceapi/nf-mmdeviceapi-immdeviceenumerator-getdefaultaudioendpoint)
- [IAudioEndpointVolume::SetMasterVolumeLevelScalar](https://learn.microsoft.com/en-us/windows/win32/api/endpointvolume/nf-endpointvolume-iaudioendpointvolume-setmastervolumelevelscalar)
- [QueryDisplayConfig](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-querydisplayconfig)
- [SetDisplayConfig](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setdisplayconfig)
- [Launch Windows Settings](https://learn.microsoft.com/en-us/windows/apps/develop/launch/launch-settings)
- [WM_DEVICECHANGE](https://learn.microsoft.com/en-us/windows/win32/devio/wm-devicechange)
