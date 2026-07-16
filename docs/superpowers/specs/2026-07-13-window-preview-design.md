# Native Window Preview Design

**Date:** 2026-07-13
**Status:** Approved design
**Scope:** Hover and keyboard previews for running applications in the Windows dock

## Goal

Add a native window-preview surface that appears above the dock, shows every window owned by the selected application, and matches the translucent visual language already used by the dock and menus. The preview must remain stable across multiple monitors, mixed DPI, auto-hide, protected content, minimized windows, and rapid pointer movement.

The previous experimental approach rendered a DWM thumbnail inside the dock's 55 DIP window. That coupled preview geometry to icon animation and could produce boxed or clipped output. The new preview uses a dedicated native window and does not resize or repurpose the dock surface.

## Product Decisions

- Preview is available only for running applications with at least one known window.
- The first pointer hover opens after a 450 ms dwell.
- While a preview is already visible, moving directly to another eligible dock item replaces its content immediately.
- Leaving the item starts a 200 ms bridge period so the pointer can enter the preview without dismissing it.
- The dock remains revealed while the preview is visible or the pointer/focus is inside it.
- Clicking a thumbnail restores and focuses that window.
- A close button appears when a card is hovered or keyboard-focused and closes only that window.
- Escape, outside interaction, or true focus loss dismisses the preview.
- Keyboard focus on a dock item can open the preview with Space.
- Arrow keys navigate preview cards, Enter activates, and Escape returns focus to the originating dock item.
- Reduced-motion mode removes the animated transition without changing behavior.

## Visual Direction

The preview is a system-level glass surface: restrained dark translucency, low-contrast luminous rim, soft depth, precise spacing, and Windows-native content. It takes visual principles from the existing macOS-inspired direction without copying Apple marks, assets, or branded controls.

### Surface

- Corner radius: 12 DIP.
- Outer work-area margin: at least 16 DIP.
- Gap above the dock: 10 DIP.
- Internal padding: 12 DIP.
- Card gap: 10 DIP.
- Border: subtle half-pixel-equivalent dark rim with an inner light response at the current monitor scale.
- Shadow: existing elevation-2 language, softened outside the card so no gray rectangular backing is visible.
- Typography: Segoe UI Variable using the existing primary and secondary text tokens.
- Motion: opacity plus 6 DIP vertical translation over approximately 160 ms; no spring overshoot.

The panel is horizontally centered on the invoking icon and then clamped to the monitor work area. Its lower visual connection points toward the invoking item but remains understated and unbranded.

### Single Window

- One primary card with a preferred thumbnail area of approximately 320 x 200 DIP.
- Window title remains on one line and uses ellipsis when necessary.
- The close action is placed in the card's upper-right chrome and appears only on hover or keyboard focus.

### Multiple Windows

- Two to four windows use a responsive two-column grid.
- Cards preserve source aspect ratio with letterboxing inside the thumbnail slot when required.
- The panel never expands beyond four visible cards.
- Five or more windows use pages of four cards.
- Pagination appears only when needed and includes previous/next controls plus a current-page indicator.
- Closing a window keeps the current page when valid and otherwise moves to the nearest valid page.

### Unavailable Content

Protected, stale, minimized-without-frame, or failed captures retain the same card geometry. The thumbnail slot shows a privacy-safe placeholder with the application icon, window title, and a short localized reason. No screenshot fallback or private window content is stored.

## Architecture

### PreviewController

Owns the interaction state machine and remains independent from HWND and Direct2D details.

States:

- `Closed`
- `Dwelling` with target item and deadline
- `Visible` with target item, page, and focused card
- `Closing` only while a non-reduced transition is active

It consumes typed pointer, keyboard, window-list, timer, and focus events. It emits intents to show, update, dismiss, focus a window, close a window, or hold/release dock reveal. Timer checks use the existing non-blocking Win32 timer path.

### PreviewScene

Carries renderer-ready data:

- originating dock item;
- application identity and runtime icon source;
- ordered window cards with stable `WindowId` values;
- title and availability state;
- current page and page count;
- hovered and keyboard-focused card;
- close-action visibility;
- reduced-motion state.

No raw HWND is exposed to the renderer. The platform adapter translates stable window identifiers only at the native boundary.

### PreviewLayout

Pure DIP-based layout computes:

- panel size for one card or a 2 x 2 page;
- card, thumbnail, title, close button, and pagination bounds;
- anchor position from the invoking dock item's physical location;
- work-area clamping for negative monitor coordinates and mixed DPI;
- hit-test regions for cards and controls.

Layout is recalculated before presentation when the monitor, DPI, page, window count, or invoking item changes.

### PreviewHost

Each monitor slot owns one hidden preview HWND and renderer surface. It is separate from the existing top-bar popover and context-menu surface so those focus scopes and dismissal rules cannot conflict.

The host:

- creates and positions the preview window;
- renders glass, titles, placeholders, controls, and pagination;
- registers one DWM thumbnail for each visible capturable card;
- updates DWM destination rectangles after every layout change;
- unregisters thumbnails through RAII on replacement, error, dismissal, device rebuild, or shutdown;
- routes pointer, keyboard, activation, DPI, display, and timer events to `PreviewController`.

The existing DWM registration wrapper is reused and generalized from one thumbnail to a bounded collection of visible thumbnails.

## Data Flow

1. Window discovery groups observed windows by dock application and records title, minimized/focused state, and preview availability.
2. Pointer dwell or keyboard Space selects a dock application.
3. `PreviewController` requests the application's current window group.
4. `PreviewLayout` produces the panel and visible card geometry for the current monitor and page.
5. `PreviewHost` displays the HWND, renders its code UI, and registers DWM thumbnails into the calculated slots.
6. A card click emits the existing focus/restore action; its close button emits the existing close action.
7. Window-discovery changes update the visible scene in place without hiding and recreating the host.

## Dismissal and Auto-hide

The preview and originating dock item form one pointer/focus region. Entering either cancels dismissal. Leaving both starts the 200 ms bridge deadline. A context menu opened from a preview card temporarily owns the same reveal hold.

The preview must never cause the dock or top bar to blink, rebuild device resources, or resize every animation frame. Show/hide and content changes are visual-state updates unless DPI, monitor placement, or panel dimensions actually change.

## Privacy and Failure Behavior

- Capture-restricted windows never receive a DWM thumbnail registration after restriction is known.
- A registration or update failure replaces only the affected card with an unavailable state.
- Other cards remain usable when one thumbnail fails.
- No thumbnail pixels are persisted to disk or exported.
- Stale `WindowId` actions are ignored safely after discovery confirms the window no longer exists.
- High-contrast and solid-material modes replace translucency without removing titles or actions.

## Accessibility

- Preview host is keyboard reachable from the dock.
- Each card exposes application name, window title, focused/minimized state, and action names.
- Close is never the default action and requires its own control.
- Focus order follows visual order and remains stable while paging.
- Protected and unavailable states expose the textual reason.
- Reduced motion preserves the exact interaction sequence without animation.

## Verification

Automated coverage must include:

- 450 ms initial dwell and immediate cross-item replacement while visible;
- 200 ms bridge between dock item and preview host;
- one-card and 2 x 2 layouts;
- pagination and page correction after a window closes;
- work-area clamping on negative coordinates and 100-250 percent DPI;
- protected and failed DWM capture states;
- focus, close, Escape, outside interaction, and keyboard navigation;
- dock reveal hold while preview or its context menu is active;
- RAII cleanup when any DWM registration/update step fails.

Native QA must exercise the release build on both monitors with real running applications, including an application with multiple windows. Captures must verify glass clipping, title containment, correct icon anchoring, no black rectangle, no dock/top-bar flicker, and clean dismissal.

## Out of Scope

- Persistent preview image caching.
- Server accounts, synchronization, or telemetry.
- Replacing Windows-provided application icons or thumbnail content.
- Media-session controls; those remain a separate preview variant.
- Arbitrary preview resizing by the user in the first version.
