# Transparent Liquid Glass Top Bar and System Panel Design

**Date:** 2026-07-17
**Status:** Approved design
**First delivery surface:** top bar plus the `Minha UI` system panel

## 1. Objective and Scope

Make the top bar visually transparent and bring the dock's balanced liquid-glass language into the top-bar interaction model. The first refined panel is the existing `Minha UI` system panel. It establishes the shared material, geometry, grouping, focus, and motion rules that later panels can adopt one at a time.

This delivery changes presentation and information hierarchy. It does not add, remove, rename, or reroute system actions. All current `Minha UI` options remain available and retain their existing behavior.

## 2. Reference Translation

The supplied references contribute the following principles:

- a transparent resting menu bar;
- compact violet-blue active modules;
- panels anchored precisely to the invoking module;
- dark translucent material with a fine luminous rim;
- restrained separators, full-row selection, and clear information groups;
- enough width and vertical rhythm to make system actions easy to scan.

The implementation remains an original Windows-native Obsidian Glass surface. It keeps Segoe UI Variable, the existing icon system, Windows terminology, current action routes, existing accessibility behavior, and native Direct2D/DirectComposition rendering.

## 3. Alternatives Considered

### A. Sober crystal

A more opaque panel with minimal reflections. It provides the strongest readability but does not sufficiently connect the panel to the dock's liquid-glass signature.

### B. Balanced liquid glass — selected

A controlled translucent mineral base, fine bright rim, soft static refraction, restrained top highlight, and deep neutral shadow. It preserves readability while clearly belonging to the dock's visual family.

### C. Expressive glass

Stronger violet cast, bloom, and refraction. It is visually distinctive but creates unnecessary contrast risk over bright or saturated wallpapers.

### Technical backdrop alternatives

The Windows DWM transient system backdrop was prototyped and rejected. Desktop Acrylic is applied to the rectangular bounds of the top-level window, so the system-painted layer remains visible behind the custom rounded panel even when the application content and Win32 window region are clipped.

The selected implementation uses a Windows UI Composition visual tree. A blurred-wallpaper backdrop brush is confined to a rounded `SpriteVisual`, while the existing premultiplied-alpha swap-chain content is hosted above it as a composition surface. This keeps the pixels outside the panel contour transparent without capturing desktop icons or application windows.

Undocumented accent-policy Acrylic and manual desktop capture were rejected. The former is version-sensitive and can reproduce the same rectangular artifact; the latter adds privacy, synchronization, multi-monitor, and performance complexity that is unnecessary for this surface.

## 4. Visual Thesis

**The bar disappears into the desktop; glass appears only where interaction exists.**

At rest, the top-bar canvas has no continuous plate or tint. Icons and labels sit directly over the desktop with a subtle contrast treatment. Opening or focusing a module introduces a compact violet-blue glass capsule. Its panel then extends the same material language at a heavier depth.

The design avoids neon glow, milky blur, nested glass cards, and decorative gradients that do not communicate state.

## 5. Material System

### 5.1 Top bar

- Clear the top-bar target to transparent and do not draw a full-width material fill.
- Keep the bar's content aligned to the existing 32-DIP system layer.
- Apply a short, low-opacity shadow or outline to text and icons.
- Strengthen the contrast treatment when Windows theme or accessibility settings indicate increased protection is appropriate.
- Do not capture the desktop, sample pixels per frame, or add a wallpaper-analysis timer.
- The protected state may introduce only a local, near-imperceptible veil around content. It must not read as a continuous opaque bar.

### 5.2 Active module

- Use a compact rounded rectangle derived from the module bounds.
- Fill it with a violet-blue liquid-glass profile rather than the resting hover brush.
- Add a fine inner highlight and a restrained local glow.
- Keep the active treatment visible while its panel is open.
- Keyboard focus remains visible independently of color and must not depend on glow alone.

### 5.3 System panel

- Use the selected balanced liquid-glass profile: mineral translucent base, fine outer rim, restrained inner highlight, soft static refraction, and deep neutral shadow.
- Calibrate the stacked panel layers to an effective smoked-glass opacity of approximately `72%` in the central reading area, leaving about `28%` wallpaper transmission. This target applies to the final composite, not to each individual brush alpha.
- Keep desktop details perceptible only as subdued shapes; icons and wallpaper text must not compete with menu labels or shortcuts.
- Keep the DWM system backdrop disabled for the popover HWND. The renderer must not rely on the top-level window backdrop for panel blur.
- For a `SystemPanel`, host the existing DXGI swap chain in a Windows UI Composition desktop target and place a blurred-wallpaper `SpriteVisual` behind it.
- Size and round the backdrop visual to the panel body only. The window area outside that visual, including the transparent area around the anchor notch and rounded corners, must retain zero alpha.
- Compact popovers and dock context menus continue to use their existing composition path without the blurred-wallpaper visual.
- Keep refraction static. The panel does not track the pointer with a moving specular highlight.
- Preserve the existing `72%` smoked-glass rendering when the blurred-wallpaper brush is unavailable or rejected. Preserve a solid Obsidian Glass fallback when transparency is disabled, forced colors are active, or material creation fails. Neither fallback may enable a rectangular DWM backdrop.
- Use one continuous panel surface. Rows are not individual cards.

### 5.4 Shared renderer resource

Generalize the current dock-specific liquid-glass resource into a renderer-owned material resource with explicit surface profiles:

- `Dock`: dynamic hover-linked specular behavior;
- `Panel`: static refraction and highlight;
- `ActiveModule`: compact violet-blue state treatment;
- `Disabled/Solid`: no transparent overlay.

Resources remain cached per native surface and are recreated only for surface creation, resize, DPI change, device loss, or material-mode change. The change must not introduce continuous timers, desktop capture, or per-frame bitmap generation.

## 6. System Panel Information Architecture

The panel title becomes `Minha UI`, matching its invoking top-bar module. The seven actions currently supplied by `system_rows()` are reorganized into three visual groups without changing their labels, confirmation rules, or destinations.

### Preferences

- Settings
- Task Manager

### Session

- Lock
- Sleep
- Sign out

### Power

- Restart
- Shut down

Group names describe the specification structure and are not rendered as visible section labels. The panel communicates the groups through spacing and fine dividers.

The panel uses a `288 DIP` default width instead of the current `244 DIP` menu width so long labels and shortcuts have room to breathe. It may grow for supported text scaling and must remain clamped to the monitor work area.

Rows use full-width hit targets, left-aligned icons in small glass wells, labels in the primary column, and shortcuts or trailing values aligned right. Power actions remain visually separated at the bottom. They do not use warning red at rest.

## 7. Interaction and Motion

- Pointer-down immediately changes the invoking module to its active glass state.
- The panel opens from the anchor notch using a short opacity, vertical translation, and subtle scale transition.
- Motion remains interruptible and does not move or resize the top bar.
- Hover and keyboard focus affect only the current row.
- `Escape` and outside click dismiss the panel and restore focus to the invoking module.
- Keyboard navigation follows the visual row order and preserves current shortcuts.
- Reduced-motion mode removes translation and scale, retaining only a direct state change or short opacity transition.
- Existing confirmation behavior for system actions is preserved. This design does not add or remove confirmation steps.

## 8. Scene and Layout Boundaries

The generic popover scene must be able to express:

- panel title;
- ordered groups;
- group boundaries or separators;
- row icon, label, detail or shortcut, enabled state, and focus state;
- scroll offset when the panel cannot fit the work area.

The renderer consumes this presentation model and does not own system-action routing. Existing controllers and adapters continue to own actions and Windows integration. The layout module owns row geometry, panel padding, section spacing, hit testing, and work-area constraints.

Only the `Minha UI` scene adopts the new grouping in the first delivery. Other panels keep their current content and geometry until their individual refinement pass, while using compatible shared material primitives where safe.

## 9. Accessibility and Adaptive Behavior

- Preserve visible focus at 3:1 contrast or better against the adjacent material.
- Keep text and essential icons at WCAG 2.2 AA contrast against the composited panel background.
- Do not rely on violet, glow, or transparency alone to communicate active or focused state.
- Support 100%, 150%, and 200% display/text scaling without clipping essential labels or shortcuts.
- Respect reduced motion, transparency-disabled, high-contrast, and forced-color settings.
- The top bar must retain readable content over representative light, dark, detailed, and saturated wallpapers.
- When a transparent treatment cannot guarantee readability, prefer the protected or solid fallback instead of weakening text contrast.

## 10. Verification

### Automated

- Unit tests for grouped row layout, section spacing, scrolling, hit testing, and keyboard order.
- Scene tests proving that existing `Minha UI` labels and actions are preserved.
- Renderer tests proving the top bar does not receive a full-surface fill in transparent mode.
- Renderer tests proving that only `SystemPanel` requests the blurred-wallpaper composition layer and that all fallbacks leave the DWM backdrop disabled.
- Material-mode tests for dynamic, static, disabled, WARP, reduced-motion, and solid fallback behavior.
- Geometry tests proving active-module material remains within module bounds and panel placement remains clamped to the work area.
- Regression tests for DPI changes and device-loss resource recreation.

### Visual and interactive

- Capture the resting transparent bar, active `Minha UI` module, and open panel over light and dark wallpapers.
- Inspect at 100%, 150%, and 200% scale.
- Verify pointer, keyboard, `Escape`, outside-click dismissal, and focus restoration.
- Verify transparency-disabled, reduced-motion, and high-contrast modes.
- Compare the panel and dock together to confirm they share material character without using identical motion behavior.
- Confirm that the panel corners and the area beside the anchor notch show the untouched desktop, with no rectangular gray backdrop.
- Confirm there is no new background timer, desktop capture path, or continuous redraw when the UI is idle.

## 11. Delivery Boundary

This design is complete when the transparent top bar, active `Minha UI` module, and refined `Minha UI` panel are implemented and verified. Search, network, battery/energy, control center, display, profile, calendar, audio, and background-app panels remain separate follow-up design and implementation passes.
