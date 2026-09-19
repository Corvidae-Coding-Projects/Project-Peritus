---
name: Peritus WebUI
description: A tactile laboratory console with luminous instruments, seated controls, and steady reading surfaces.
colors:
  background: "#171c1c"
  panel: "#232a29"
  display: "#111717"
  text: "#e5e6dd"
  muted: "#a9b4ae"
  accent: "#f2a260"
  line: "#414c47"
  raised: "#303936"
  ink: "#1c211c"
  error: "#ffb2a0"
  success: "#a7c69a"
typography:
  display:
    fontFamily: "Barlow Condensed, sans-serif"
    fontSize: "2.75rem"
    fontWeight: 500
    lineHeight: 1.07
    letterSpacing: "0.008em"
  headline:
    fontFamily: "Barlow Condensed, sans-serif"
    fontSize: "1.66rem"
    fontWeight: 500
    letterSpacing: "0.01em"
  title:
    fontFamily: "Barlow Condensed, sans-serif"
    fontSize: "1.55rem"
    fontWeight: 500
  body:
    fontFamily: "Barlow, sans-serif"
    fontSize: "1rem"
    fontWeight: 400
    lineHeight: 1.7
  conversation:
    fontFamily: "Barlow, sans-serif"
    fontSize: "1.08rem"
    fontWeight: 400
    lineHeight: 1.7
  label:
    fontFamily: "Barlow Condensed, sans-serif"
    fontSize: "0.72rem"
    lineHeight: 1.5
    letterSpacing: "0.13em"
  code:
    fontFamily: "ui-monospace, monospace"
    fontSize: "0.84rem"
    fontWeight: 400
    lineHeight: 1.6
  counter:
    fontFamily: "Rajdhani, sans-serif"
    fontSize: "36px"
    fontWeight: 400
    lineHeight: 1
  counter-large:
    fontFamily: "Rajdhani, sans-serif"
    fontSize: "66px"
    fontWeight: 400
    lineHeight: 1
rounded:
  field: "3px"
  key: "4px"
  module: "5px"
  chassis: "7px"
  switch: "11px"
  tube: "11px 11px 4px 4px"
spacing:
  tight: "4px"
  inline: "8px"
  control: "12px"
  gutter: "14px"
  content: "16px"
  section: "20px"
  panel: "24px"
  group: "32px"
components:
  button-primary:
    backgroundColor: "{colors.accent}"
    textColor: "{colors.ink}"
    rounded: "{rounded.key}"
    padding: "8px 14px"
  button-primary-hover:
    backgroundColor: "color-mix(in srgb, var(--accent) 85%, white)"
  button-secondary:
    backgroundColor: "{colors.raised}"
    textColor: "{colors.text}"
    rounded: "{rounded.key}"
    padding: "8px 14px"
  button-secondary-hover:
    backgroundColor: "color-mix(in srgb, var(--raised) 85%, var(--text))"
  button-small:
    backgroundColor: "{colors.raised}"
    textColor: "{colors.text}"
    rounded: "{rounded.key}"
    padding: "5px 10px"
  button-flat:
    backgroundColor: "transparent"
    textColor: "{colors.muted}"
    rounded: "{rounded.field}"
    padding: "5px 7px"
  button-flat-hover:
    backgroundColor: "#ffffff07"
    textColor: "{colors.text}"
  field:
    backgroundColor: "{colors.display}"
    textColor: "{colors.text}"
    rounded: "{rounded.field}"
    padding: "10px 12px"
  project-tab:
    backgroundColor: "transparent"
    textColor: "{colors.muted}"
    rounded: "4px 4px 0 0"
    padding: "10px 18px 12px"
  project-tab-active:
    backgroundColor: "{colors.panel}"
    textColor: "{colors.text}"
  count-chip:
    textColor: "{colors.muted}"
    rounded: "{rounded.field}"
    padding: "1px 4px"
  target-readout:
    backgroundColor: "{colors.display}"
    textColor: "{colors.text}"
    rounded: "{rounded.field}"
    padding: "13px 12px"
  switch:
    backgroundColor: "{colors.display}"
    rounded: "{rounded.switch}"
    width: "38px"
    height: "21px"
  switch-checked:
    backgroundColor: "{colors.accent}"
  nixie-tube:
    rounded: "{rounded.tube}"
    width: "25px"
    height: "43px"
  command-result:
    backgroundColor: "transparent"
    textColor: "{colors.text}"
    rounded: "{rounded.field}"
    padding: "13px 16px"
  command-result-highlighted:
    backgroundColor: "{colors.raised}"
  control-dialog:
    backgroundColor: "{colors.panel}"
    textColor: "{colors.text}"
    rounded: "{rounded.chassis}"
    width: "700px"
---

# Design System: Peritus WebUI

## Overview

**Creative North Star: "The Nixie Laboratory Console"**

Peritus feels like precise industrial equipment: blackened steel, gunmetal seams, ivory lettering, recessed reading wells, substantial molded keys, and warm cathodes suspended inside numeric instruments. Full physical theater is an explicit user commitment. Small highlights, contact shadows, spring-settling parts, and depressible controls explain the construction of the machine.

The equipment remains legible during sustained use. Barlow carries reading, Barlow Condensed identifies the apparatus, and Rajdhani supplies the layered digits. Nixie, Daylight, and Blueprint are three finishes of the same instrument; labels, component geometry, and action semantics remain coherent across them. Reading stays still while controls respond. System reduced-motion preferences take precedence over mechanical animation.

**Key Characteristics:**
- Blackened steel, ceramic, and blue-steel finishes on a shared component system.
- Warm numeric cathodes with real layered depth and accessible value labels.
- Recessed reading wells and molded keys with directional light and contact shadows.
- Condensed instrument lettering paired with open, steady long-form text.
- Explicit operational state, keyboard-visible focus, and reduced-motion response.

This is the first recorded world, extracted from `webui/src/app.css`, `webui/src/App.svelte`, the representative components, and `webui/src/main.ts`. The frontmatter is normative for the base Nixie stylesheet; runtime preferences can replace the documented roles. `.impeccable/design.json` is a schemaVersion 2 extension containing metadata, source-derived depth/motion, and component specimens. Its synthesized eight-step OKLCH ramps are swatch previews, not additional shipped colors.

The selected direction is `signals-instruments-nixie-laboratory-counter` (seed `ee96c4a3`). The conversation-first workspace strategy, story, and first-viewport composition remain in `.impeccable/surfaces/webui-src-app-svelte.md`. They are not templates for every future surface. The development boards `.impeccable/nixie-board.webp` and `.impeccable/nixie-hero.webp` are references only; the shipped apparatus uses CSS, live text, and authored SVG rather than a raster illustration.

## Colors

The default finish pairs green-black metal and aged ivory with a warm orange electrical signal; Daylight and Blueprint preserve the same roles in ceramic and blue steel.

### Primary

- **Warm cathode orange** (`accent`): active tabs, primary key faces, command syntax, links, caret, selection, and keyboard focus. Nixie wire glow is confined to the numeric instrument.
- **Key-face ink** (`ink`): lettering on the accent face and selected text. It reverses to a pale value in Daylight.

### Secondary

- **Signal green** (`success`): confirmed connection/success indications, clean Git state, and status dots.
- **Error coral** (`error`): inline error text and alert iconography. A status also has a readable label or message; color is not its only meaning.

### Neutral

- **Blackened chassis** (`background`): outer shell, project rail, and code backing.
- **Gunmetal plate** (`panel`): apparatus panels, dialog body, and user-message containers.
- **Reading well** (`display`): inset transcript, fields, code/readout interiors, and selected mobile panel controls.
- **Ivory lettering** (`text`): primary content and actionable labels.
- **Sage-grey engraving** (`muted`): metadata, supporting copy, unselected controls, and input placeholders.
- **Machined seam** (`line`): panel boundaries, field strokes, dividers, and tab outlines.
- **Raised metal** (`raised`): molded keys, selected segments, palette selection, and notices.

### Implemented finishes

Nixie is the default (`theme = "nixie"`); its primitive values are solely in the frontmatter. The following are the actual CSS overrides in `webui/src/app.css`, not colors sampled from review screenshots. “Inherits Nixie” means no override is declared.

| Role / CSS property | Daylight (`data-theme=daylight`) | Blueprint (`data-theme=blueprint`) |
| --- | --- | --- |
| `background` / `--background` | `#c5c9c1` | `#172533` |
| `panel` / `--panel` | `#dfe1d8` | `#233747` |
| `display` / `--display` | `#f1f1e9` | `#11212e` |
| `text` / `--text` | `#28352e` | `#e1e9e8` |
| `muted` / `--muted` | `#53614f` | `#afc0c6` |
| `accent` / `--accent` | `#9a4c1d` | `#ecc08b` |
| `line` / `--line` | `#a6aea0` | `#465e6a` |
| `raised` / `--raised` | `#e7e8dd` | `#304657` |
| `ink` / `--ink` | `#fff5e2` | Inherits Nixie |
| `error` / `--error` | `#8e3425` | Inherits Nixie |
| `success` / `--success` | `#426241` | `#adcbc1` |

Daylight sets `color-scheme: light`; the others retain dark native controls. Its tubes remain dark and their lit cathodes use the base Nixie accent, while ordinary controls use Daylight's darker accent. Daylight also changes the large instrument backing (`#d2d7ca`), key contact shadows, primary-key border (`#a65d32`) and lettering (`#fff2dd`), and source-code string/title (`#486b21`), keyword (`#8e361e`), and comment (`#65705f`) colors. These are local component overrides, not new shared primitives. Blueprint changes the listed roles without replacing the material construction.

The app accepts saved overrides for `background`, `panel`, `display`, `text`, `muted`, `accent`, and `line` as `#RGB` or `#RRGGBB`. `App.svelte` applies them as root custom properties after the finish; an absent override removes the inline property. Configuration validation checks syntax and recognized roles, not contrast.

### Contrast and focus

Primary and muted copy use the semantic text roles, including fully opaque placeholders. The global keyboard indicator is an accent outline (2px, offset 3px), also applied to links, fields, summaries, and focusable code. Inputs use the accent caret; selected text pairs accent with ink. The skip link exposes the composer target on focus.

Calculated from the opaque source colors, muted text against the panel is approximately 6.85:1 in Nixie, 4.98:1 in Daylight, and 6.54:1 in Blueprint; primary text against the panel is approximately 11.64:1, 9.70:1, and 9.96:1 respectively. These pairs support the WCAG 2.0 AA reading requirement; they are not a certification of every gradient, syntax token, focus condition, custom override, or assistive-technology path. Decorative seams and dim inactive cathode layers are not text-color substitutes.

**The Live Signal Rule.** Use the accent to identify an active control, available primary action, focus, command syntax, or lit numeric cathode; pair operational status colors with readable text.

## Typography

**Display / Label Font:** Barlow Condensed, with a sans-serif fallback.\
**Body Font:** Barlow, with a sans-serif fallback.\
**Numeric Instrument Font:** Rajdhani, with a sans-serif fallback.\
**Code Font:** the inherited `--font-mono` preference; the base stylesheet stack is recorded in `typography.code`.

Barlow's open counters make the console comfortable to read; the condensed face supplies equipment lettering without compressing prose. Rajdhani is a wire-shaped numeric face, not a replacement for body copy. Fontsource bundles Barlow (400/500/600), Barlow Condensed (500/600), and Rajdhani (400) through `webui/src/main.ts`. Icons come from `Icon.svelte` as inline SVG (24-unit viewBox, 1.6 stroke, round caps/joins; commonly 14–18px), with the distinct authored brand mark in `App.svelte`.

### Hierarchy

- **Display** (`typography.display`): the large ready-state heading; responsive sizes range from the phone treatment (2.2rem) to the wide-desktop treatment (3rem).
- **Headline** (`typography.headline`): active-session title, reduced on phones (1.44rem).
- **Title** (`typography.title`): dialog headings; smaller drawer/control headings use the same condensed family (1.12–1.14rem).
- **Body and conversation** (`typography.body`, `typography.conversation`): Markdown and conversation content. Reading width is bounded (75ch); conversation copy returns to the body size on phones. Markdown headings use Barlow 600 with line-height (1.3), rather than instrument lettering.
- **Label** (`typography.label`): uppercase numeric instrument captions. Short equipment labels may be tracked; prose and action labels retain ordinary case and spacing.
- **Code** (`typography.code`): preformatted Markdown blocks. Paths, keyboard legends, and receipts also use the inherited mono family at context-specific compact sizes.
- **Counters** (`typography.counter`, `typography.counter-large`): explicit pixel-sized tube digits. Compact header digits become smaller on phones (30px); large digits use phone (55px), short desktop (57px), and wide desktop (78px) variants.

### Reading preferences and source variance

The default root reading size is (14px). Saved `font_size` is validated between (12px) and (22px), so the rem-based hierarchy scales together. `font_family` replaces `--font-body`; `mono_family` replaces `--font-mono`. Label and instrument families remain dedicated to their material roles. Code wrapping and Markdown preview both default on and can be changed.

`webui/src/lib/workspace.svelte.ts` uses `ui-monospace, monospace` as its bootstrap/reset code-font fallback, while the server's new-profile default in `crates/app/peritus-web/src/config.rs` is `Iosevka, monospace`. Iosevka is not among the bundled Fontsource assets. This difference is recorded as source drift, not as two normative code faces; the saved preference is the live authority. The embedded xterm instance in `Console.svelte` currently has its own fixed monospace font (13px) and dark color configuration. That implementation exception is not a shared reading token.

**The Still Reading Rule.** Keep conversation, code, and explanatory typography stationary during reading; reserve mechanical displacement for controls, entering panels, and changing numeric cathodes.

## Layout

The reusable spatial grammar is a fitted chassis containing bounded, independently scrollable wells. Structural panels meet at seams; controls sit on their plates, while long content gets a flexible middle and a measured reading width. The spacing entries record recurring source distances, not a newly imposed universal grid.

The current workspace uses a viewport-height flex shell (`100dvh`, desktop minimum 640px), outer inset (12px 14px), and a grid with a configurable file drawer, flexible center (`minmax(0,1fr)`), and control bank. Default drawer and bank widths are (248px) and (232px). Session nesting advances each rail by (10px) and file-tree depth advances by (14px). Tab rails scroll horizontally; nested session rails cap their height (26vh). Conversation messages cap their container width (900px), with a separate readable Markdown measure. These measurements describe this built surface; its composition remains owned by the surface brief.

The default row height is (33px); compact density sets it to (27px), reduces session-heading padding and message top spacing, and tightens file-label padding. The file drawer is adjustable (180–480px, UI step 8px). Both side panels have visibility preferences. `App.svelte` writes the saved drawer width inline, so that preference takes precedence over the stylesheet's root drawer-width breakpoint fallback.

### Responsive reflow

| Source media condition | Implemented behavior |
| --- | --- |
| Width at least 1600px | Roomier shell (18px 20px), taller top plate (112px), control bank (256px), and enlarged reading/instrument spacing. |
| Width at most 1220px | Control bank (210px), CSS drawer fallback (215px), tighter plate spacing, wrapped composer controls, and hidden shortcut legend on the header key. |
| Width at most 1050px | Control bank leaves the working grid; a Files / Session / Controls switch appears. Selecting Controls occupies the workspace with a two-column bank. |
| Width at most 760px | Single full-width selected panel. Files and Session alternate; Controls becomes a vertical bank. Shell inset (6px), minimum height (570px); two header counters yield to the session counter. File action buttons stay visible and file rows gain touch padding. |
| Width at most 760px, dialogs | Settings and role fields stack into one column. Dialogs keep viewport gutters (8px per side) and bounded height (`100dvh - 20px`). Console input stacks, and command results may scroll through (65vh). |
| Height at most 850px and width at least 761px | Large instrument and empty-state spacing compress vertically without removing the reading well or composer. |

On phones, message indentation drops to zero, the composer well narrows, and its textarea retains a useful minimum height (52px). The source supports a minimum viewport width (320px). Desktop dialogs are bounded by viewport gutters (14px per side) and height (`100dvh - 48px`); their headings and settings save actions remain available while bodies scroll.

## Elevation & Depth

Depth is structural and deliberately physical. Tonal separation establishes shell, plate, and well; directional gradients, inset darkness, fine top glints, and soft contact shadows establish material thickness. Keys depress into their seats. The tube has an anode mesh, ten stacked cathodes, curved glass reflections, and a metal base. This physical theater is an authorized defining feature.

### Shadow vocabulary

Source values below are also carried in the sidecar; none belongs in Stitch's eight-property component token objects.

- **Molded key:** `0 3px 4px #0005, inset 0 1px 0 #ffffff0b`; default raised controls.
- **Depressed key:** `0 1px 2px #0004, inset 0 2px 4px #0003`; secondary key active/pressed state with a downward displacement (2px).
- **Primary key:** `0 3px 5px #0006, inset 0 1px 0 #ffe1b755`; warmer face and heavier contact at rest. Pointer activation uses the depressed-key shadow.
- **Recessed field:** `inset 0 2px 4px #0003`; shared inputs, textareas, and selects. Deeper readout and composer wells use their own inset shadows.
- **Session well:** `inset 0 2px 8px #0008`; the primary reading recess.
- **Tube envelope:** `inset 1px 1px 2px #dee1c318, inset -2px -1px 3px #000, 0 3px 4px #0007`; highlights inside glass and contact beneath its base.
- **Engaged dialog:** `0 18px 70px #0009, inset 0 1px 0 #ffffff10`; a floating equipment panel over a dark backdrop (`#07100bc2`, blur 3px).
- **Daylight key:** `0 2px 4px #2d37292e, inset 0 1px 0 #ffffff5e`; softer contact on ceramic. The finish override replaces the standard and primary resting shadows; pointer activation still depresses the key.

### Mechanical response

The shared spring is `cubic-bezier(.2,1.7,.35,1)`. Molded keys move over (200ms), with face changes (140ms) and shadow changes (160ms). Session tabs latch over (280ms): enter from (-5px), settle through (+1px), then seat at zero. Dialogs engage over (250ms) from (+12px) and scale (.985). Nixie cathodes change opacity (200ms), depth (230ms, spring), and brightness (180ms); inactive layers sit behind the lit wire. Busy indicators pulse over (1.8s); notices enter over (230ms). These responses accompany real state changes rather than animate the reading text.

Mechanical motion defaults on; synthesized control sounds default off. Enabling sound adds a brief triangle-oscillator click to direct button actions. The system reduced-motion media query removes animations, transitions, smooth scrolling, cathode transforms, and modal-backdrop blur. The app's motion-off preference removes animations, transitions, and smooth scrolling while retaining static state geometry.

**The Seated Hardware Rule.** Use inset darkness for wells, soft contact shadows beneath raised parts, and a fine top highlight to express material thickness; preserve the key's depressed position as its active feedback.

**The Instrument Glass Rule.** Confine glass reflections and layered luminous wire to numeric instruments; ordinary panels stay opaque, while the modal backdrop may soften the scene behind an engaged panel.

**The Reduced Motion Wins Rule.** Disable animation and transition when either the system requests reduced motion or the app's mechanical-motion setting is off; keep selected, pressed, busy, and focus states understandable without movement.

## Shapes

Small radii belong to machined hardware, with broader corners reserved for the chassis and instrument. The frontmatter distinguishes field, key, module, and chassis radii. Tabs can open their bottom edge into a rail; recessed wells use fine borders and compact corners. Switch housings are rounded around a traveling circular thumb. Only lamps, small fasteners, and switch thumbs are fully circular.

Tube silhouettes are rounded at the crown and flatter at the base; their large forms increase the crown radius with their dimensions. The mesh, cathode stack, shine, and base are separate layers, not a flattened illustration. Engraved plates and screw heads are quiet physical details; they do not replace readable control labels. The custom SVG icon system carries functional pictograms, while keyboard legends retain actual key names and symbols.

## Components

The sidecar contains ten self-contained, `ds-`-scoped specimens: primary key, secondary key, flat action, text field, project navigation, count chip, target readout, switch, Nixie bank, and command result. They inherit source custom properties with base-finish fallbacks. States are CSS/HTML representations; event dispatch and live data remain the responsibility of the application.

### Molded and flat buttons

**Character:** substantial keys for execution, lighter text controls for secondary navigation.

Primary and secondary keys share the key radius, face gradient, bordered lower edge, and minimum height (38px). Primary labels use weight (600). Hover lightens the face; active/pressed keys travel down (2px). Small keys use minimum height (31px), smaller text (.88rem), and the compact padding token. Disabled buttons retain native disabled semantics, unavailable cursor, and reduced opacity (.42). Flat actions use muted labels at rest and a subtle light overlay on hover; they do not acquire a molded key's shadow. Icon-only controls retain accessible names.

The composer Send key is the vertical primary-key variant; its minimum dimensions are (61px × 59px) on desktop and (46px × 49px) on phones. Pending and empty-draft states disable it, and pending text reads “Sending”. Settings and Git use the same primary grammar for their actual save/commit states.

### Inputs and switches

**Character:** recessed entry wells and a sliding mechanical toggle.

Fields use display backing, a seam border, inherited reading type, muted placeholders, and the global focus outline. Textareas support deliberate line height and bounded growth; the composer field is borderless inside its separately recessed well. The file filter uses the same display color inside a framed search enclosure. Settings switches move a small circular thumb (17px travel) between muted/off and ink/on states, with the accent filling the checked housing. Inputs remain explicitly labeled.

### Navigation and counts

**Character:** channels that engage a rail, with readable identity before decoration.

Project tabs expose the selected project using `aria-current`, a panel-colored face, accent top inset, channel-number accent, and a small signal dot. Hover adds the panel color. Nested session tabs use `role=tab`, `aria-selected`, active-lineage styling, and a distinct current-session border. Arrow keys, Home, and End select within a session row and move focus. New session tabs use the latch animation. Content and drawer tabs instead mark selection with a thin accent rule. The count chip is a compact bordered numeric label; it is not an action and gains no synthetic hover state.

Every session tab includes a persistently visible X, with a session-specific accessible name and reserved title spacing on desktop and phones. Close controls sit outside the semantic tablist while sharing its visual grid. Closing returns focus to the selected tab or Add session and remains available when the project directory is missing; conversation records and nesting are retained.

Project tabs pair their selection button with a separate, persistently visible X and a project-specific accessible name. Closing retains the project and session records, updates channel counts and numbered shortcuts, and returns focus to the selected project or Open project. Closing the final project exposes an open-project empty state. Reopening a saved session also restores its project tab.

### Containers and readouts

**Character:** bounded operational compartments, not floating content cards everywhere.

The target readout uses a display-colored recess, compact corner, seam border, and inset shadow. A condensed section title introduces the state and wrapping monospace path; a labeled flat action leads to workspace configuration. User messages get a panel-colored outlined container while assistant prose stays on the reading well. Dialogs reuse opaque panel material, engaged depth, sticky heading, scrollable body, and explicit actions. Notices pair icons with text and use `status` or `alert` semantics.

### Nixie numeric bank

**Character:** the reusable signature instrument, showing actual observed values.

`Nixie.svelte` left-pads nonnegative values to the requested digit count and renders all ten numeric cathodes behind each active digit. The lit wire moves forward (2px), uses accent stroke and warm glow, and increases brightness (1.2); inactive wires stay faint and behind the anode mesh. Tube dimensions, typography, and transitions scale with the source variants. The decorative tube bank is `aria-hidden`; its outer accessible label reports the real label and value. Glass, glow, and spring response are intentional material grammar.

### Command directory

**Character:** a searchable command instrument with selection that remains visible.

The modal has an accent search icon, a borderless search field, scrollable result well, and compact key legends. Each result separates a strong command name, muted explanation, and accent slash syntax. Highlighted and hovered rows use raised metal. Search is a labeled combobox controlling a listbox with stable option IDs, `aria-activedescendant`, and `aria-selected`. Arrow navigation clamps to the available result range; Enter dispatches the selected command. The selected row is scrolled into view with `block: 'nearest'` after updates, including beyond the initial viewport.

The header command key formats the configured binding, including platform-specific Mod and Alt names. The actual default is `Mod+k`; the reviewed `Mod+Shift+p` binding was a temporary saved test preference, and the user's original TOML was restored. It is not a new default. The finishing review reports both command-selection visibility and configured shortcut legend resolved, with 16-row keyboard navigation captured and final verdict **ship**. Those findings establish the corrected behavior, not an alternative token set.

### Explorer, Git, settings, and console

These reuse the same fields, keys, list rows, and recessed panels. File rows distinguish selection with an accent/display mix and indentation; their overflow action is exposed on hover/focus and persistently on phones. Git separates staged and unstaged groups, uses actual status letters and counts, and disables actions during work. Settings uses a two-column fieldset layout that becomes one column on phones, with persistent save actions and validated, resettable preferences. The embedded terminal supplies its own accessible runtime surface; its fixed styling remains local to `Console.svelte`.

## Do's and Don'ts

### Do:

- Do reuse the semantic finish roles and inherited custom properties for panels, reading wells, controls, and text.
- Do preserve the authored Nixie layers, material depth, and spring response when extending the instrument world.
- Do keep long-form reading still, use the configured reading dimensions, and honor reduced motion before animation.
- Do show operational labels alongside status colors and keep keyboard focus visible on interactive controls.
- Do reflow auxiliary panels and settings into the implemented single-panel or single-column mobile patterns.
- Do keep command selection in view and render shortcut legends from the active saved binding.
- Do verify text and focus contrast after custom palette overrides; syntax validation alone does not establish readable contrast.

### Don'ts:

- Don't spread glass reflections or cathode glow onto ordinary reading panels and text.
- Don't flatten the approved mechanical controls into generic shadowless controls or remove their static selected/pressed cues.
- Don't animate conversation or code text as part of mechanical response.
- Don't use a numeric instrument, lamp, or status decoration to imply observations the daemon has not supplied.
- Don't ship the development reference boards as product illustration or replace authored SVG icons with glyph approximations.
- Don't turn a temporary screenshot preference, a local terminal override, or the code-font default mismatch into a new shared default.
- Don't promote the workspace's first-viewport strategy into a mandatory composition for unrelated surfaces.
