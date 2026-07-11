# Frontend Design State

Updated: 2026-07-11

## Current Objective

Define the decision-complete visual and interaction contract for an original Windows-native dock, top bar, popovers, and settings experience, while preserving eight supplied screenshots as documentation-only references.

## Locked Decisions

- Direction: **Obsidian Glass**, a restrained Windows system surface with mineral tint, cool rim, compact density, and one local dock pressure-field interaction.
- Typography: Segoe UI Variable with Segoe UI fallback; no SF Pro or imitation typography.
- Grid: 4 DIP spacing system; Per-Monitor V2 DPI behavior; verify 100–250% scaling.
- Accessibility outranks glass fidelity. High contrast/forced colors use system colors and L3 essential rendering.
- References are not shipped and are not a pixel-match contract. No Apple, macOS, SF Symbols, SF Pro, MyDockFinder, screenshot-crop, or scraped tray assets.
- Tray reference is visual-density evidence only; V1 supports app-owned integrations, not arbitrary Windows tray mirroring.

## Source Inputs

- Contract: `DESIGN.md`
- Reference mapping: `docs/reference-annex.md`
- Preserved screenshots: `docs/references/mydockfinder/01-dock-previews.png` through `08-system-menu.png`
- Documentation QA: `.omo/evidence/task-1-reference-contact-sheet.png`
- Baseline: `.omo/evidence/task-1-baseline.txt`

## Design Brief

- Product: compact native Windows command layer for launching/switching apps and accessing owned system/status surfaces.
- Primary journeys: launch/switch/preview windows; inspect network/weather/time; choose audio output and control media; change supported quick settings; use calendar and safe system/app commands; configure behavior and accessibility.
- Information hierarchy: dock/topbar glance → single-purpose anchored popover → Windows Settings or app settings for deeper work.
- Taste: dark mineral glass, quiet depth, original Windows geometry, precise typography, low accent coverage.
- Anti-references: macOS imitation, generic dark-SaaS glow, neon gamer chrome, nested cards, fake telemetry, unlabeled icon-only consequence actions.
- Success: readable over arbitrary desktops, keyboard-complete, stable under mixed DPI, degradable without loss of meaning, and legally original.

## Inclusive Personas

| Persona | Constraint | Pass criteria |
|---|---|---|
| Marina | Keyboard-first, motor fatigue, 150% scale. | Complete core journeys with visible focus and predictable Escape/focus return. |
| Rafael | Multi-monitor creator with mixed DPI. | Stable geometry, correct preview identity, no work-area clipping during monitor moves. |
| Denise | Low vision, 200–250%, high contrast. | Reflow without lost actions; system colors and readable focus/destructive hierarchy. |
| Lucas | Motion/cognitive sensitivity. | Reduced-motion parity, stable layout, plain labels, one task per popover. |

## Adaptive Preferences

- Respect Windows animation and transparency settings, forced colors/high contrast, text scaling, locale/RTL, keyboard, screen reader, color filters, and remote/battery constraints.
- L0–L3 performance/material ladder is behavioral parity, not a feature reduction.
- Reduced motion removes dock interpolation, popover translation, chart sweeps, and attention pulses while retaining state feedback.

## Taste and Design Principles

1. Windows truth over visual promise.
2. Glance, act, leave.
3. Material supports hierarchy, never readability debt.
4. Privacy choices are visible and default-minimal.
5. Original by construction.

## Decisions Log

| Decision | Rationale |
|---|---|
| Use restrained Ethereal Glass adapted to system utility density. | Preserves dimensionality from the visual references without importing website-scale spectacle or macOS identity. |
| Use a five-layer glass recipe and performance ladder. | Prevents single-blur generic glass and guarantees stable fallbacks. |
| One popover owner at a time with anchor/flip/clamp and focus restoration. | Makes transient surfaces predictable for keyboard, low-vision, and multi-monitor users. |
| Use app-owned integrations instead of tray mirroring. | Avoids unsupported APIs, impersonation, inaccessible third-party menus, and misleading parity claims. |
| Keep reference images documentation-only. | Supports auditability without licensing or shipping ambiguity. |

## Verification Matrix

| Gate | Evidence / expected result | Status |
|---|---|---|
| Failing-first | Stable reference directory and contact sheet absent before work. | Recorded in `.omo/evidence/task-1-baseline.txt`. |
| Reference preservation | Eight stable files, nonzero, decodable, distinct source hashes. | Record in `.omo/evidence/task-1-verification.txt`. |
| Link integrity | Every Markdown image/document link resolves. | Record in verification evidence. |
| Visual documentation QA | Contact sheet contains all eight labeled references and is visually inspected. | Artifact: `.omo/evidence/task-1-reference-contact-sheet.png`. |
| Primitive showcase | All primitive states at representative DPI/widths before product screens. | Pending implementation; DD-05. |
| Runtime visual QA | Real app at mixed DPI, keyboard, high contrast, reduced motion, L0–L3. | Pending implementation; not applicable to Task 1 documentation. |
| Review handoff | Contract, state, evidence, debt passed to implementation/review owners. | Ready after Task 1 commit. |

## Design Debt Register

| ID | Severity | Affected users | Suggested fix / owner | Status / notes |
|---|---|---|---|---|
| DD-01 | Minor | Low-power, remote, battery users. | Renderer owner profiles and sets L0–L3 thresholds. | Open; behavior fixed, thresholds pending. |
| DD-02 | Minor | Users of packaged/legacy apps with missing icons. | Shell owner validates icon fallback matrix. | Open; labeled placeholder required. |
| DD-03 | Minor | Weather and privacy-sensitive users. | Product/privacy owner selects provider and consent/attribution. | Open; network weather remains off. |
| DD-04 | Note | Users expecting notification-area parity. | Product owner revisits only with supported API evidence. | Accepted scope exclusion, not accessibility debt. |
| DD-05 | Minor, pre-implementation | All personas. | UI owner builds and visually verifies primitive state harness. | Open; blocks product-screen composition, not Task 1 contract. |

No accessibility debt is accepted.

## Evidence Index and Handoff

- Baseline: `.omo/evidence/task-1-baseline.txt`
- Contact sheet: `.omo/evidence/task-1-reference-contact-sheet.png`
- Verification receipt: `.omo/evidence/task-1-verification.txt`
- Next owner must read `DESIGN.md` before UI code, exclude documentation references from packages, build the primitive showcase, and validate keyboard/high-contrast/reduced-motion/mixed-DPI behavior before product screens.
