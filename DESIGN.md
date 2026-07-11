# Obsidian Glass Design System

This document is the visual and interaction contract for the Windows-native dock application. Product UI must trace colors, type, spacing, geometry, materials, states, motion, and accessibility behavior to this file before implementation.

## 0. Research Log

- Concrete references: inspected all eight supplied screenshots at native resolution and preserved them under `docs/references/mydockfinder/`; the behavior-by-behavior extraction and legal boundary are in `docs/reference-annex.md`.
- Reference classification: the screenshots are documentation-only image references. Dock, top bar, popovers, menus, settings, text, icons, charts, and state feedback are **code UI**; installed-application icons and consented media artwork are runtime asset slots supplied by Windows. No screenshot is an implementation asset.
- Taste route: loaded the frontend design architecture, static image-to-code analysis, and premium/glass guidance. Retained dimensional glass, compact anchored surfaces, clear hierarchy, and one restrained dock-magnification signature; rejected website-scale whitespace, decorative spectacle, and non-native typography as inappropriate for a system utility.
- Direction alternatives considered: (A) neutral Fluent utility, (B) bright acrylic shelf, (C) Obsidian Glass. Chose C because dark mineral tint makes status surfaces legible over varied desktops while remaining distinct from the references; Windows-native type, icons, focus, terminology, and adaptive-material behavior keep it original.
- External research: no Lazyweb or Imagen lane was run because the user supplied a complete eight-image concrete reference set and explicitly requested an original translation. No Apple or MyDockFinder brand system was used.
- Verification artifact: `.omo/evidence/task-1-reference-contact-sheet.png` is documentation QA only.

## 1. Atmosphere & Identity

### Visual thesis

**Obsidian Glass** is a calm Windows command layer: charcoal mineral tint, cool reflected edges, and brief blue energy only where the operating system needs to communicate focus or action. The signature is the **pressure field** around the pointer: a small, local dock lift and scale response that feels weighted and precise, never elastic or theatrical.

The design is a system surface, not a website and not a macOS replica. It should feel native beside Windows 11, readable over unpredictable wallpapers, compact enough for frequent use, and trustworthy around system controls.

### Content plan

1. Dock: launch, switch, inspect windows, and expose task progress or attention.
2. Top bar: quiet glanceable status and one-click access to owned popovers.
3. Popovers: one task per surface: network, weather, audio/media, controls, calendar, or app/system menu.
4. Settings: configure appearance, behavior, modules, privacy, accessibility, and diagnostics with explicit preview/revert behavior.

### Interaction thesis

- The dock responds through a localized pressure field: pointer proximity affects at most the hovered item and two neighbors per side; keyboard focus uses elevation and focus ring without magnification.
- Popovers appear to emerge from their invoking status item, with short opacity/translate motion and exact focus restoration.
- State changes are immediate and legible. Decorative motion never competes with work, and all meaning remains when motion, transparency, or animation is disabled.

### Product principles

1. **Windows truth over visual promise:** expose only capabilities available through supported Windows APIs.
2. **Glance, act, leave:** status is concise; actions are close; popovers dismiss predictably.
3. **Material supports hierarchy:** tint, rim, and elevation separate layers; glass is never a readability tax.
4. **Privacy is visible:** network, weather, media, and diagnostics disclose data use and default to the minimum collection.
5. **Original by construction:** Windows terminology, Segoe typography, original geometry, and licensed/native icons only.

## 2. Users, Brand Foundation & Color

### Inclusive personas

| Persona | Context and needs | Primary tasks | Pass condition |
|---|---|---|---|
| Marina, keyboard-first analyst | Motor fatigue; avoids repeated pointer travel; uses 150% scale and keyboard navigation. | Launch/switch apps, inspect windows, change audio output, open calendar. | Every task is reachable in logical order, focus is always visible, Escape/back behavior is predictable, and no pointer-only control exists. |
| Rafael, multi-monitor creator | Two mixed-DPI displays; many open windows; values fast visual scanning. | Switch active apps, identify windows, manage media, move between monitors. | Dock and popovers retain size/position across per-monitor DPI changes, preview identity is unambiguous, and no surface clips at work-area edges. |
| Denise, low-vision administrator | Uses 200–250% scale, high contrast, larger text, and may disable transparency. | Read system status, adjust controls, use power/session menu safely. | Text reflows without truncating actions, contrast remains compliant, forced colors are respected, and destructive choices are clearly separated and confirmed. |
| Lucas, distraction-sensitive student | Cognitive load and motion sensitivity; uses reduced motion and focus assist. | Check weather/time, change volume, launch a small app set. | Status remains quiet, animation is optional, labels use plain language, and each popover has one clear hierarchy. |

### Brand foundation

- **Name:** Obsidian Glass (visual direction, not a third-party product claim).
- **Character:** precise, quiet, protective, lucid.
- **Voice:** brief Windows terminology; active verbs; no futuristic jargon, cute copy, or borrowed Apple language.
- **Signature material:** dark translucent mineral with a cool upper rim, neutral lower shadow, and a faint blue focus seam.
- **Accent discipline:** blue communicates action, selection, focus, or live progress only. Status colors communicate actual status and are always paired with shape/text.
- **Anti-references:** macOS chrome, SF typography, Apple glyphs, MyDockFinder assets, neon gamer glow, generic dark-SaaS gradients, oversized pills, nested glass cards, fake telemetry.

### Semantic palette

Tokens are logical roles. Implementations may convert the values to the platform color type, but may not introduce an unlisted raw color.

| Role | Token | Dark/default | Light | High-contrast behavior |
|---|---|---:|---:|---|
| Desktop scrim | `color.backdrop.scrim` | `#0A0D12B8` | `#FFFFFFA6` | Transparent; system canvas owns background. |
| Surface base | `color.surface.base` | `#11151BEF` | `#F7F9FCEF` | `Canvas`. |
| Surface raised | `color.surface.raised` | `#181D24F2` | `#FFFFFFF2` | `Canvas`. |
| Surface solid fallback | `color.surface.solid` | `#171B21` | `#F5F7FA` | `Canvas`. |
| Surface hover | `color.surface.hover` | `#FFFFFF12` | `#0B122012` | `Highlight` only when selected; otherwise system button face. |
| Surface pressed | `color.surface.pressed` | `#FFFFFF1F` | `#0B12201F` | `Highlight`. |
| Surface selected | `color.surface.selected` | `#2D7DFF2E` | `#0F6CBD24` | `Highlight`. |
| Text primary | `color.text.primary` | `#F5F7FA` | `#171A1F` | `CanvasText`. |
| Text secondary | `color.text.secondary` | `#B8C0CC` | `#4C5563` | `CanvasText`. |
| Text muted | `color.text.muted` | `#8892A0` | `#687180` | `GrayText`. |
| Text disabled | `color.text.disabled` | `#68717D` | `#929AA5` | `GrayText`. |
| Rim outer | `color.rim.outer` | `#FFFFFF2B` | `#FFFFFFCC` | `ButtonBorder`. |
| Rim inner | `color.rim.inner` | `#8FC2FF17` | `#0F6CBD12` | Omit decorative inner rim. |
| Divider | `color.divider` | `#FFFFFF1A` | `#121A241F` | `ButtonBorder`. |
| Accent | `color.accent.default` | `#4C9AFF` | `#0F6CBD` | `Highlight`. |
| Accent hover | `color.accent.hover` | `#71ADFF` | `#0B5CAD` | `Highlight`. |
| Accent pressed | `color.accent.pressed` | `#2D7DFF` | `#0A4A8A` | `Highlight`. |
| Focus outer | `color.focus.outer` | `#9DCAFF` | `#075EA8` | `Highlight`. |
| Focus inner | `color.focus.inner` | `#10141A` | `#FFFFFF` | `Canvas`. |
| Success | `color.status.success` | `#54C582` | `#0F7B3D` | System `Highlight` plus text/icon label. |
| Warning | `color.status.warning` | `#F2B84B` | `#9C5D00` | System `Highlight` plus text/icon label. |
| Error | `color.status.error` | `#FF7373` | `#C42B1C` | System `Highlight` plus text/icon label. |
| Information | `color.status.info` | `#65A7FF` | `#0F6CBD` | System `Highlight` plus text/icon label. |

### Color rules

- Body text meets WCAG 2.2 AA 4.5:1; large text, icons, focus indicators, charts, and component boundaries meet 3:1 against adjacent colors.
- Transparency is composited against a guaranteed solid tint. Never place text directly over uncontrolled wallpaper pixels.
- Accent coverage stays below roughly 10% of a resting surface. It does not decorate headings or idle glass.
- Success, warning, and error never rely on hue alone; pair with text and an icon or structural state.
- Public IP/location, diagnostics, or other sensitive values use primary text only after explicit disclosure/consent; color must not imply trustworthiness.

## 3. Typography

### Font stack

- Primary and display: `Segoe UI Variable, Segoe UI, sans-serif`.
- Symbol fallback: `Segoe Fluent Icons` only for Windows-owned glyph rendering; prefer the licensed code icon family in Section 8 for product icons.
- Monospace values when truly needed: `Cascadia Mono, Consolas, monospace`.
- No SF Pro, San Francisco, Helvetica, Inter, Roboto, or downloaded imitation font.

### Type scale (device-independent pixels)

| Token | Size / line | Weight | Tracking | Use |
|---|---:|---:|---:|---|
| `type.display` | 28 / 36 | 600 | `-0.01em` | Rare settings empty state or onboarding title. |
| `type.title` | 20 / 28 | 600 | `-0.005em` | Popover/settings page title. |
| `type.subtitle` | 16 / 24 | 600 | `0` | Section title, app/window title. |
| `type.body` | 14 / 20 | 400 | `0` | Default UI copy. |
| `type.body.strong` | 14 / 20 | 600 | `0` | Selected item, primary value. |
| `type.caption` | 12 / 16 | 400 | `0.01em` | Metadata, secondary value. |
| `type.label` | 12 / 16 | 600 | `0.01em` | Compact control label. |
| `type.metric` | 24 / 28 | 600 | `-0.01em` | Weather/volume hero value only. |

### Typography rules

- Windows text scaling to 200% is supported independently of display DPI. Essential actions do not truncate; values wrap or reflow before labels.
- Minimum body text is 12 DIP only for nonessential metadata; controls and primary information use 14 DIP or larger.
- Use sentence case. Uppercase is reserved for short technical group labels where Windows convention demands it.
- Tabular numerals are allowed for clocks, transfer rates, dates, and timelines; do not use monospace for ordinary prose.
- Ellipsis is permitted for runtime app/window/media names only, with accessible full text and tooltip after truncation.

## 4. Spacing, Geometry, Layout, Density & DPI

### Four-pixel grid

All dimensions are DIPs and derive from 4px unless a 1px optical stroke or a mathematically centered icon requires an exception.

| Token | Value | Typical use |
|---|---:|---|
| `space.0` | 0 | No gap. |
| `space.1` | 4 | Tight icon/badge alignment. |
| `space.2` | 8 | Icon-label gap, compact inset. |
| `space.3` | 12 | Row gap, compact padding. |
| `space.4` | 16 | Standard popover padding. |
| `space.5` | 20 | Comfortable group padding. |
| `space.6` | 24 | Major group separation. |
| `space.8` | 32 | Settings section separation. |
| `space.10` | 40 | Settings page top/bottom inset. |
| `space.12` | 48 | Large empty-state rhythm. |

### Radius

| Token | Value | Use |
|---|---:|---|
| `radius.control` | 6 | Inputs, compact buttons, selected rows. |
| `radius.tile` | 10 | Quick-setting tiles, media rows. |
| `radius.popover` | 14 | Popovers and preview cards. |
| `radius.dock` | 18 | Dock shell at default size. |
| `radius.round` | 999 | Circular icon button or slider thumb only. |

Nested radii are concentric: inner radius equals outer radius minus inset. Do not apply pill geometry to ordinary panels, list rows, or text buttons.

### System-layer geometry

- Top bar height: `32 DIP` default, optionally `36 DIP` compact-touch mode. Minimum target per item: `32 × 32 DIP`; settings touch mode raises targets to `40 × 40 DIP`.
- Dock icon slot: `44 DIP` compact, `52 DIP` default, `60 DIP` large. Shell padding: `8 DIP`; item gap: `4 DIP`; separators have `16 DIP` visual height and `1 DIP` stroke.
- Dock maximum width: work-area width minus `32 DIP` per side. Overflow becomes a labeled “More apps” item; never shrink targets below configured compact size.
- Popover widths: `320 DIP` compact, `360 DIP` standard, `440 DIP` detailed. Maximum height: work-area height minus `32 DIP`; internal content scrolls while header/footer remain stable.
- Preview card: `320 × 200 DIP` preferred, up to `400 × 240 DIP`; preview groups wrap into a bounded grid before scrolling.
- Settings: navigation rail `232 DIP`, content max `760 DIP`, outer gutters `24 DIP` at standard and `16 DIP` below `720 DIP` available width. Collapse to a navigation page rather than icon-only rail when narrow.

### Anchoring and safe areas

- Surfaces use monitor work area, not full display bounds; respect taskbar and auto-hide reserved edges.
- Popovers align their anchor notch/edge to the invoking item, then clamp to `16 DIP` work-area margins. They flip above/below or left/right when needed.
- Only one top-level popover is open per host window. Submenus remain within the same focus scope.
- RTL reverses horizontal order and anchoring where Windows conventions require it; charts retain chronological direction appropriate to locale.

### DPI and density

- The process is Per-Monitor V2 DPI aware. Recalculate layout, raster sizes, and popover placement on monitor transition before presenting the next frame.
- Verify at 100%, 125%, 150%, 175%, 200%, and 250%. Strokes snap to physical pixels; text and icon layout remain DIP-based.
- Runtime app icons request the nearest Windows-provided size at the monitor scale; never upscale a low-resolution icon when a higher-resolution source exists.
- Compact density reduces whitespace but never font size, focus visibility, hit-target minimums, or information required for safe actions.

## 5. Components and State Contracts

### Global state language

Every interactive primitive implements the applicable states below. State differences use at least two signals for critical meaning.

| State | Visual and behavioral contract |
|---|---|
| Default | Primary/secondary hierarchy from semantic tokens; no decorative accent. |
| Hover | `color.surface.hover`, tooltip after 500–700ms for unlabeled icons; no layout shift. |
| Pressed | `color.surface.pressed`, transform scale at most 0.98 where appropriate, immediate input acknowledgment. |
| Focus-visible | Two-part 2 DIP ring using `focus.outer` and `focus.inner`; never hidden by clipping or transparency. |
| Selected/on | `color.surface.selected`, explicit check/indicator or label, and current state announced. |
| Disabled/unavailable | Disabled token plus retained label; not focusable unless focus is required to explain unavailability, in which case expose a read-only explanatory item. |
| Loading | Preserve geometry, announce progress/status once, show indeterminate motion only when reduced motion allows. |
| Empty | Plain-language reason and one next action where possible. |
| Offline | Retain cached safe values with “Last updated” or explain connection requirement; never present stale data as live. |
| Error | Located message, status icon/text, retry or route to fix; do not erase the user's prior selection. |
| Attention | Restrained badge/pulse once; no infinite bounce. Screen readers receive a non-repeating status update. |
| Drag target | Clear insertion/target outline, valid/invalid state, keyboard reorder alternative. |

### Dock shell and dock item

- **Structure:** edge sensor → dock shell → pinned/running groups → separators → owned utility actions/overflow.
- **Variants:** compact/default/large; bottom/left/right edge only if the implementation supports each completely; auto-hide/on-top.
- **Item anatomy:** runtime app icon, accessible name, running state, active window indicator, optional progress or count badge, context menu.
- **Pointer:** local pressure field scales hovered item to at most 1.22×, adjacent items to at most 1.10×, second neighbors to at most 1.04×; center positions shift through transform only. Magnification must not obscure previews or cross work-area bounds.
- **Keyboard:** arrow keys move in visual order, Home/End jump boundaries, Enter launches/activates, Space opens preview when applicable, Shift+F10 opens context menu, Escape closes child surface. Keyboard focus does not magnify.
- **Drag/reorder:** hold threshold prevents accidental drags; insertion position is visible; Escape cancels; keyboard reorder is available in context menu/settings.
- **States:** default, hover, pressed, focused, running, active, launching, progress, attention, unavailable, drag source, valid/invalid target.
- **Accessibility:** item name includes app and state, not raw executable path. Badges have equivalent accessible text.

### Window preview and media preview

- **Structure:** app/group heading → thumbnail asset slot → title/status → contextual actions.
- **Behavior:** opens after a deliberate hover dwell or explicit keyboard action; remains open while pointer/focus is within dock item or preview; closes with Escape/outside interaction and restores focus.
- **Privacy:** secure/protected windows show a labeled placeholder, never a captured frame. Minimized/unavailable thumbnails retain window identity.
- **Media:** use Windows-provided session metadata/artwork; show only supported transport actions. Timeline is keyboard-operable when seeking is supported.
- **States:** loading thumbnail, ready, multiple windows, protected, empty, stale, error, closing.

### Top bar and status item

- **Structure:** optional app/menu cluster, flexible spacer, owned status items, clock. The surface never impersonates the Windows notification area.
- **Status item:** icon plus concise value where useful; accessible name includes state and unit. Frequent values use tabular numerals and fixed minimum width to avoid jitter.
- **Behavior:** click/Enter opens its owned popover; arrow keys traverse neighboring status items; clock opens calendar.
- **States:** default, hover, pressed, focused, active-popover, stale, unavailable, attention.

### Popover frame

- **Structure:** optional header → one primary content region → optional pinned footer action. Avoid card-inside-card nesting; use spacing/dividers first.
- **Material:** `glass.popover` recipe from Section 7. Only the outer frame owns blur, rim, radius, and shadow.
- **Focus:** focus enters the first meaningful control or selected item, remains logically contained, and returns to the invoker on close. Escape closes the deepest layer first.
- **Placement:** anchor, flip, and clamp rules from Section 4. Arrow/notch is optional; do not distort the radius to force one.
- **States:** opening, ready, internal loading, partial data, empty, offline, error, closing.

### Network popover

- Summary, active adapter, connection type, transfer rates, accessible history table/summary, totals, last update, and route to Windows network settings.
- Public IP/location is hidden by default and requires explicit opt-in. Graphs are supplemental, not sole information.
- Update animation is numeric/opacity only; the graph may append without sweeping motion.

### Weather popover

- Location, current condition/value, high/low, forecast list, unit selector/settings route, last update.
- Permission/configuration, loading, cached/offline, provider error, and no-data states are first-class.
- Weather icons come from the approved icon/asset policy; no reference artwork is reused.

### Audio and media popover

- Volume slider, mute, current output, output list, optional system sounds/spatial audio route, current media, supported transport controls, route to Windows sound settings.
- Slider exposes name, numeric value, increment, and mute state. Output change reports success/failure without closing the surface unexpectedly.
- Device names may wrap to two lines; never compress primary controls to preserve one-line layout.

### Tray/integration overflow

- V1 contract is app-owned integrations only. It is not a Windows tray mirror.
- Each row has licensed/runtime icon, application name, status, and an action explicitly provided by the integration.
- Unsupported third-party tray items do not appear. Empty state explains that only supported integrations are listed.

### Control Center

- **Structure:** quick-setting group → display slider → audio slider → optional media strip. Use grouped layout and dividers, not a mosaic of nested cards.
- Tiles display label and explicit on/off/unavailable state. Long labels wrap; icons never replace state text.
- Settings that cannot be changed through supported APIs route to the corresponding Windows Settings page.
- Layout: two columns above `344 DIP` content width, one column below or at large text scales.

### Calendar

- Semantic month heading and grid; today, selected date, keyboard focus, outside-month, and unavailable states are visually distinct.
- Arrow keys navigate days; Home/End week boundaries; PageUp/PageDown months; Ctrl+PageUp/PageDown years when supported; month changes are announced politely.
- Locale controls language, first day of week, numerals, and date order. No events in V1 unless separately authorized.

### App/system menu

- Groups: app/about/help; settings/integrations; task/session; power. Separators encode consequence, not decoration.
- Destructive/disruptive actions use text labels and appropriate Windows confirmation. Default focus never lands on a destructive command.
- Commands unavailable under policy show disabled state and a reason; they do not fail silently.

### Settings shell and settings primitives

- **Shell:** navigation page/rail, title, searchable content only if search is complete and indexed, scrollable content region, status/toast region.
- **Primitives:** section heading, description, toggle, segmented choice, dropdown, slider with numeric value, text/path picker, shortcut recorder, radio group, inline callout, preview, reset/revert command.
- **Apply model:** immediate for reversible appearance choices with a 10-second “Keep changes / Revert” guard when visibility could be impaired; explicit Apply for startup/system integration changes; destructive reset requires confirmation.
- **States:** default, hover, pressed, focused, selected, modified, applying, applied, validation error, permission required, unavailable, restart required.
- **Accessibility:** labels precede controls in reading order; help text is programmatically associated; error summary links to fields; shortcut capture has a cancel path and describes conflicts.

### Feedback primitives

- Tooltips name unlabeled controls only; they do not contain required instructions or interactive content.
- Toasts confirm asynchronous outcomes, persist long enough to read, support keyboard dismissal, and never carry the only error detail.
- Inline callouts contain persistent context: information, warning, error, or privacy disclosure.
- Progress is determinate when measurable; taskbar/dock progress and textual percentage agree.

## 6. Motion & Interaction

### Timing and easing

| Token | Duration | Easing | Use |
|---|---:|---|---|
| `motion.instant` | 0ms | none | Reduced motion, direct state synchronization. |
| `motion.micro` | 90ms | `cubic-bezier(0.2, 0, 0, 1)` | Press feedback, hover tint. |
| `motion.fast` | 140ms | `cubic-bezier(0.2, 0, 0, 1)` | Tooltip/popover fade, focus-adjacent response. |
| `motion.standard` | 200ms | `cubic-bezier(0.16, 1, 0.3, 1)` | Popover open/close, tile state. |
| `motion.emphasis` | 280ms | `cubic-bezier(0.16, 1, 0.3, 1)` | Dock reveal and settings preview transition. |

### Motion rules

- Animate compositor-friendly `transform` and `opacity`. Color changes may interpolate only on small controls; never animate layout, blur radius, shadow spread, width, or height continuously.
- Popover open: opacity 0→1 and translate 4 DIP toward anchor→0 over `motion.standard`; close uses `motion.fast` and no scale flourish.
- Dock reveal: translate from edge by at most 12 DIP plus opacity; pressure-field scale uses `motion.fast` and settles without overshoot.
- Attention: one 280ms lift or rim-brightening sequence, maximum twice per user-relevant event; no infinite bounce or ambient loop.
- Loading indicators run only while work is active and expose accessible status independent of animation.

### Reduced motion

- With Windows animation effects off or `prefers-reduced-motion`, set transforms to final state immediately, remove magnification interpolation and attention pulses, and use opacity changes of 90ms or less only when they aid continuity.
- Dock hover uses static rim/selection feedback; popovers appear at final position; charts update without sweep; loading uses a textual state and optionally a non-moving progress fill.
- Reduced motion never disables functionality, focus feedback, or live data.

## 7. Material, Elevation & Visual Performance

### Material recipe

The strategy is **mixed tonal shift plus restrained elevation**. Glass is a five-layer recipe, not a blur effect:

1. Solid fallback: `color.surface.solid` guarantees contrast.
2. Tint: `color.surface.base` or `color.surface.raised` composited over the captured desktop when supported.
3. Backdrop material: Windows system backdrop/acrylic equivalent with bounded blur; only top-level dock/top bar/popover windows own it.
4. Rim: 1 physical-pixel outer `color.rim.outer` plus optional inner `color.rim.inner` highlight on the top/leading edge.
5. Separation: elevation shadow from the table below; never use glow as a substitute for focus.

No scrolling child panel applies backdrop blur. Internal grouping uses tonal shifts, spacing, and dividers.

### Elevation

| Token | Windows/CSS-equivalent intent | Use |
|---|---|---|
| `elevation.0` | none | Internal rows and settings content. |
| `elevation.1` | `0 2px 8px #00000024, 0 1px 2px #0000002E` | Top bar and resting dock. |
| `elevation.2` | `0 8px 24px #00000038, 0 2px 6px #00000030` | Popovers and preview cards. |
| `elevation.3` | `0 16px 40px #0000004A, 0 4px 10px #00000033` | Modal confirmation only. |

### Visual-performance degradation ladder

The renderer selects the highest stable level per monitor/window and can step down without changing layout or behavior.

| Level | Material | Motion/imagery | Trigger and guarantee |
|---|---|---|---|
| L0 Full | Bounded system backdrop, tint, rim, `elevation.1/2`, optional live previews. | Full approved transforms/opacity; runtime icons at native scale. | Default on capable hardware. Target smooth interaction with no sustained frame misses. |
| L1 Reduced effects | Lower-cost backdrop or reduced blur area, tint and rim retained, simplified shadow. | Dock pressure field limited to hovered + immediate neighbors; preview refresh rate reduced. | Automatic after repeated missed frames, remote session, battery saver, or user preference. |
| L2 Solid dimensional | `color.surface.solid`, rim, tonal grouping, static shadow; no backdrop sampling. | No magnification interpolation; thumbnails refresh only on demand. | Transparency disabled, high GPU pressure, unsupported compositor, or explicit user setting. |
| L3 Essential/high contrast | System colors, no transparency, no decorative shadow/rim. | No nonessential motion or live thumbnail; text/icon state remains complete. | Forced colors/high contrast, severe fallback, or accessibility mode. |

Step-down must be observable in diagnostics but silent in ordinary use. Never lower text contrast, hit targets, data freshness labels, keyboard behavior, or safe confirmations to preserve visual effects.

## 8. Accessibility Constraints, Asset Policy & Accepted Debt

### Accessibility constraints

- Target WCAG 2.2 AA for applicable desktop UI plus Windows accessibility conventions.
- Full keyboard reachability, logical reading/focus order, visible focus, accessible names/roles/states/values, and focus restoration are release blockers.
- Minimum pointer target is 24 × 24 DIP with 32 × 32 DIP preferred; touch density option uses 40 × 40 DIP minimum.
- Do not block Windows text scaling, high contrast/forced colors, animation settings, transparency settings, color filters, or screen magnification.
- Screen reader announcements are polite and deduplicated for status; errors and destructive actions are assertive only when immediate attention is necessary.
- Charts have textual equivalents. Icons never carry the sole label for unfamiliar or consequential actions.
- Localization supports expansion of at least 40%, RTL where applicable, locale-aware dates/numbers/units, and no text baked into images.
- Cognitive accessibility: one primary task per popover, plain labels, stable control positions, no surprise dismissal during device/data updates, and reversible settings changes.

### Icon and asset policy

- Product icons: use one thin-to-regular Windows-compatible family, preferably Microsoft Fluent UI System Icons under its applicable open-source license. Pin the package/version and retain required notices.
- Platform glyphs: `Segoe Fluent Icons` may be used only through Windows font availability and documented glyph semantics; provide a licensed fallback and accessible label.
- Runtime app icons: extract through supported Windows shell APIs from installed apps. Cache by application identity and DPI; do not redistribute them in the installer.
- Media artwork and thumbnails: display only data returned by Windows APIs for the active session/window, honor protected content, cache ephemerally, and provide privacy-safe placeholders.
- Weather artwork: use licensed original or approved icon-family glyphs with attribution/notice as required.
- No emojis as product icons. No ad hoc traced SVGs, CSS-drawn copies, scraped tray icons, Apple glyphs/logos, SF Symbols, SF Pro, MyDockFinder imagery, screenshot crops, or copied third-party chrome.
- The eight files under `docs/references/mydockfinder/` and the contact sheet under `.omo/evidence/` are documentation-only; build and packaging rules must exclude them.

### Accepted design debt

These are contract-level unknowns, not permission to ship inaccessible behavior. No accessibility debt is accepted.

| ID | Item and location | Severity / affected users | Why accepted now | Owner / exit condition |
|---|---|---|---|---|
| DD-01 | Exact Windows backdrop API and blur cost thresholds for dock/top bar/popovers. | Minor; low-power, remote-session, and battery users. | Task 1 defines the design contract before renderer selection. The L0–L3 behavior is fixed even though thresholds are not measured yet. | Rendering owner; close with per-monitor profiling and documented automatic step-down thresholds. |
| DD-02 | Final runtime icon fallback coverage for packaged, unpackaged, and legacy Win32 apps. | Minor; users of apps with missing/corrupt icons. | Windows identity/API investigation belongs to implementation. A labeled placeholder is already required. | Shell integration owner; close with fixture matrix and licensed placeholder. |
| DD-03 | Weather provider attribution placement and exact consent copy. | Minor; weather users and privacy-sensitive users. | No provider is selected. Weather stays unconfigured/off until selection and disclosure. | Product/privacy owner; close before enabling weather network calls. |
| DD-04 | Tray/integration scope beyond app-owned integrations. | Note; users expecting full notification-area parity. | Arbitrary tray mirroring is deliberately excluded from V1 due to API, trust, and accessibility risk. | Product owner; revisit only with documented Windows API feasibility and a new design review. |
| DD-05 | Primitive showcase and rendered state validation. | Minor pre-implementation debt; all personas. | No UI implementation exists in Task 1. The contract defines the required harness and states. | UI owner; must close before composing product screens. |

### Design-system change rule

New raw colors, arbitrary spacing, font families, radii, material recipes, motion timings, or reusable component patterns are prohibited. Update this document first, state the user/persona need, and remove an obsolete option where possible. Accessibility constraints outrank visual fidelity; user task completion outranks taste.
