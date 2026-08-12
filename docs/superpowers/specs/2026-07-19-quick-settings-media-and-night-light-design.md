# Quick Settings Media Player and Reliable Night Light Design

> **Partially superseded (2026-08-12):** the media design remains relevant, but
> the stable Night Light behavior is now a route to the documented
> `ms-settings:nightlight` page. The private CloudStore mutation described below
> must not be restored in stable builds.

**Date:** 2026-07-19

**Status:** Approved design

**Primary surface:** Controls popover in the top bar

## 1. Objective

Replace the low-value `Nearby sharing` and `Snap windows` tiles with a compact media player that occupies their two vertical slots. Stabilize Night light so repeated clicks converge on the user's latest requested state instead of producing overlapping or indistinguishable CloudStore writes.

The work keeps the current light liquid-glass material, typography, spacing tokens, icon family, adaptive hardware filtering, and native popover window.

## 2. Fixed Product Decisions

- The player occupies the left grid column with the height of two standard tiles.
- `Focus` and `Projection` remain in the right column.
- The player uses the reference only for information hierarchy and transport controls. It does not copy the reference's color, glow, or card material.
- A paused session remains visible with its last artwork and metadata until that session ends.
- Tapping the artwork opens a manual media-session selector.
- The chosen session remains fixed until the user chooses another session or the selected application ends its session.
- When no media session exists, the player is omitted and the remaining tiles compact without a blank region.
- Night light actions are serialized and coalesced to the latest requested state.
- The interface distinguishes confirmed state, desired state, and an in-progress change.

## 3. Media Player Information Architecture

The two-slot player uses a compact editorial composition suited to the narrow column:

1. A top row contains square artwork at `46-52 DIP` and a metadata column.
2. The metadata column shows the track title and artist/source on one line each.
3. A bottom row contains Previous, Play/Pause, and Next controls with equal visual weight.

Title and artist use end ellipsis and never touch the card edge. Missing metadata falls back to the source application name. Missing artwork uses the source application icon when available, otherwise the existing media glyph in a quiet tonal well.

The player is one interaction card, not a stack of nested cards. Its background uses the same local tile surface as the grid, with the selected session expressed through content and focus rather than a second glass treatment.

## 4. Media Capability and Session Model

Windows integration uses `GlobalSystemMediaTransportControlsSessionManager` from `Windows.Media.Control`.

The platform adapter exposes a platform-neutral snapshot containing:

- stable session identity for the lifetime of the Windows media session;
- source application identifier and display name when available;
- title, artist, thumbnail, and playback status;
- availability of previous, play, pause, and next commands;
- whether the session is the manually selected session.

The adapter listens to manager and session events instead of polling:

- current-session changes;
- session-list changes;
- media-properties changes;
- playback-info changes;
- session closure.

Event callbacks enqueue lightweight platform events. Metadata and thumbnail loading occur outside the paint path. The renderer receives only cached scene data and a renderer-owned bitmap resource.

## 5. Session Selection

Tapping or keyboard-activating the artwork opens an inline contextual page in the existing popover, following the projection selector pattern.

Each row shows:

- application icon or media thumbnail;
- track title or application name;
- artist/source detail;
- playback state;
- a checkmark for the selected session.

Selecting a row returns to the main panel and preserves that choice until the session closes. If the selected session closes, the controller chooses the most recently active remaining session. If none remain, it removes the player and compacts the grid.

Escape or the back button returns to the main panel without changing the selection. Keyboard focus order follows the visual order and all rows use full-width hit targets.

## 6. Media Actions and Failure Behavior

Transport actions call the selected session's documented asynchronous methods. A control is disabled when the session reports that command unsupported.

The UI does not invent playback success. While a command is pending, the relevant control shows a restrained pressed/pending state. The playback icon changes only after the session publishes confirmed playback information. A failed command keeps the previous confirmed state and shows a short inline message without dismissing the panel.

Rapid Play/Pause activation is serialized per selected session. A later desired state supersedes an earlier queued state, preventing multiple asynchronous commands from racing.

## 7. Night Light Root Cause and State Model

The current implementation writes the global and per-device CloudStore records synchronously for every click. The underlying Night light serializer records the outer CloudStore timestamp in whole seconds. Repeated state changes inside the same second can therefore produce writes that are distinct internally but indistinguishable to the Windows settings broker. The controller also has no explicit in-flight or desired state.

Introduce a narrow `NightLightCommandCoordinator` with:

- `confirmed`: the last state read from global and per-device records;
- `desired`: the latest user request;
- `in_flight`: the state currently being applied;
- `last_timestamp`: the greatest CloudStore timestamp observed or written.

Only one native write runs at a time. Additional clicks update `desired`; they do not start concurrent writes. When an operation completes, the coordinator starts another operation only if `desired` differs from the newly confirmed state.

## 8. Night Light Write and Confirmation Flow

1. A click inverts the latest desired state, not a stale scene snapshot.
2. The tile retains the last confirmed active appearance and changes its detail to `Turning on...` or `Turning off...`.
3. A worker reads the global and all per-device records.
4. It generates a strictly increasing CloudStore timestamp: `max(current_time, last_timestamp + 1)`.
5. It writes the same requested state to the global record and every compatible per-device record while preserving schedule configuration.
6. It re-reads all written records and reports success only when they agree with the requested state.
7. The runtime applies the confirmed capability update and redraws once.
8. If the desired state changed during the operation, the next serialized operation begins immediately with a new monotonic timestamp.

The worker never performs registry or media work inside paint, pointer, or DirectComposition callbacks. No continuous polling timer is added.

On failure, the tile returns to the last confirmed appearance and shows `Could not change Night light`. The next click retries from a fresh read.

## 9. Scene, Layout, and Renderer Changes

`QuickSettingsScene` gains an optional media-player presentation model and a media-session contextual page. The model contains text, image identity, playback state, supported actions, selection state, focus, hover, pressed, and pending state.

The layout engine owns the two-row span. It places the media card in the left column and the first two ordinary tiles in the right column. When media is absent, the existing compact two-column placement applies. The player must not introduce scrolling at the current default scale.

The renderer reuses current panel and tile brushes. Artwork is clipped to the existing rounded geometry family. Transport icons use the existing Segoe MDL2 icon source. Hover, pressed, pending, keyboard focus, forced colors, and reduced motion follow current Quick Settings tokens.

## 10. Accessibility and Text Integrity

- The player exposes source, title, artist, playback state, and selected-session state to accessibility APIs.
- Artwork activation is named `Choose media session`.
- Transport actions expose unavailable and pending states.
- Title and artist remain contained at 100%, 150%, and 200% text scaling.
- Focus is visible independently of artwork and active color.
- Reduced motion removes contextual-page translation while preserving direct state feedback.
- Missing artwork or metadata never produces an empty unnamed control.

## 11. Configuration and Adaptation

`Nearby sharing` and `Snap windows` are removed from the default visible Quick Controls arrangement. Existing saved preferences remain readable for configuration compatibility, but these controls no longer occupy the default two slots.

The media player is a capability-driven surface rather than a user-reordered tile. It appears when the Windows media-session manager reports at least one session and is absent otherwise. Wi-Fi, Bluetooth, battery, brightness, and projection keep their existing hardware-adaptive rules.

## 12. Focused Verification

Automated coverage remains proportional:

- controller tests for selected-session persistence, session closure fallback, and absent-session compaction;
- layout tests for the two-row left span, right-column tile placement, text bounds, and no-scroll default geometry;
- media adapter tests behind pure snapshots for supported and unsupported transport commands;
- Night light coordinator tests for rapid toggle coalescing, single in-flight operation, monotonic timestamps, confirmation, and failure rollback;
- existing Quick Settings renderer and controller tests;
- one fresh native build.

Manual verification covers:

- playing and paused media from at least two applications;
- manual session selection by artwork;
- previous, play/pause, and next states;
- selected-session closure and player removal when no sessions remain;
- rapid Night light clicks converging on the final requested state;
- the panel at 100% and 150% scale with long metadata;
- keyboard focus, Escape/back, hover, pressed, pending, and reduced-motion behavior.

## 13. Acceptance Criteria

- The player replaces the two marked left-column tiles without changing the panel's material language.
- Focus and Projection remain aligned in the right column.
- The selected or paused session remains visible until it closes.
- Artwork opens a usable manual session selector.
- Transport controls reflect session capabilities and confirmed state.
- Long or missing metadata never breaks layout.
- No media session leaves no blank two-slot placeholder.
- Rapid Night light clicks never create concurrent writes and converge on the latest desired state.
- Global and per-device Night light records are confirmed before the tile reports completion.
- No new continuous polling or paint-path system work is introduced.
