# BitArchive – UI/UX Concept & Low-Fidelity Specification

> **Status:** Consolidated MVP UX specification  
> **Basis:** `PRODUCT.md` + `ARCHITECTURE.md`  
> **Purpose:** Canonical interaction, navigation, focus, state, and low-fidelity screen reference for prototyping, visual design, and later Slint implementation.  
> **Rule:** `DECIDED` is binding. `FAVORED` is the current preferred direction and must be validated in prototype. `OPEN` requires prototype/decision. `DEFERRED` is not an MVP requirement.

---

# 1. Responsibility and design model

`PRODUCT.md` defines **what BitArchive must do**.

`UI_UX_CONCEPT.md` defines **how users navigate and interact with those capabilities**.

`ARCHITECTURE.md` defines **how the behavior is implemented safely and consistently**.

BitArchive is a:

> **Desktop-first application with controller-first Library navigation.**

The normal game frontend and the management/settings areas intentionally use different interaction densities:

- **Frontend:** controller-centric, low chrome, no permanent sidebar.
- **Manage Library / Settings:** desktop-oriented, form/list/table patterns allowed, while remaining basically controller accessible.

The discarded HTML mockup is not a design source and is not part of this specification.

---

# 2. Global UX invariants

## 2.1 Home and terminology — DECIDED

User-facing terms:

- **Home** = root of the normal game frontend.
- **Manage Library** = content/component/readiness management.
- **Settings** = application and emulation behavior/preferences.
- **Game Options** = context-specific behavior for the current Game/Release.

Do not use `Library` as an ambiguous destination label.

## 2.2 Main navigation model — DECIDED

At Home:

```text
Horizontal = Built-in Libraries / Systems
```

Inside a game list:

```text
Vertical = Games
```

The frontend has no permanent sidebar or top navigation bar.

## 2.3 Stable focus — DECIDED

Frontend focus is stored by stable domain identity (`GameId`, system/library scope), never by transient list index.

When data changes:

- keep the same focused `GameId` if it still exists;
- otherwise select the nearest valid neighbor;
- never jump to the first item merely because a list changed.

Closing an overlay restores the exact previous focus.

## 2.4 Back behavior — DECIDED

In the frontend, `Back` follows navigation history.

Search restores query/result focus when returning.

Deep links into Manage Library carry a **Return Context**. Back from the deep-linked management entry returns to the originating Game/context.

## 2.5 Primary action — DECIDED

In Game Browser and Game Info:

```text
Primary = Play
```

`Continue` never replaces Play.

`Continue` is an additional action only when an exactly compatible Save State exists.

## 2.6 Semantic input — DECIDED

```text
Navigation
- Directional
- Primary
- Back
- PageNext
- PagePrevious
- SectionNext
- SectionPrevious

Context
- Options

Global
- GlobalMenu
- Search

Convenience
- Continue
- Favorite
- Info
- SaveStates
```

Not every convenience action requires a dedicated physical button.

## 2.7 Mouse model — DECIDED

Hover is not Focus.

For frontend rails/lists:

- first click focuses;
- double-click may activate only when the item is already focused;
- right-click opens context options where appropriate.

## 2.8 Controller ownership — DECIDED

BitArchive consumes navigation/controller actions only while BitArchive owns the foreground input context.

A foreground RetroArch session must not simultaneously navigate BitArchive.

---

# 3. Screen inventory

```text
Frontend
├─ Home
├─ Game Browser
├─ Game Info
├─ Save States
├─ Search
├─ Global Menu
├─ Game Options
├─ Now Playing
└─ Readiness / Error / Confirmation overlays

Manage Library
├─ Overview
├─ Sources
├─ Systems
├─ Scan
├─ Scraping
├─ Metadata
├─ BIOS & Firmware
├─ RetroArch & Cores
├─ Updates
└─ Maintenance

Settings
├─ General
├─ Appearance
├─ Input
├─ Emulation
├─ Language & Region
├─ Notifications
├─ Backup & Restore
└─ Advanced

Cross-cutting
├─ Onboarding
├─ Activity
├─ Empty States
├─ Loading States
├─ Offline / Permission States
└─ Session Recovery
```

---

# 4. Screen 1 — Home

**Structure:** DECIDED  
**Carousel presentation:** FAVORED

## Purpose

Home is the root of the normal frontend. There is no separate dashboard before it.

Users can enter:

```text
Recent
Favorites
All Games
│
└─ Systems
```

## Low-fidelity structure

```text
┌──────────────────────────────────────────────────────────────┐
│                                                              │
│                       NINTENDO 64                            │
│                        187 Games                             │
│                                                              │
│   RECENT   FAVORITES   ALL GAMES   │   SNES   N64   PS1    │
│                                               ●              │
│                                                              │
│   Primary Open                           Global Menu          │
└──────────────────────────────────────────────────────────────┘
```

## Start focus

1. restore last valid position;
2. otherwise `Recent` if non-empty;
3. otherwise `All Games`.

`Recent`, `Favorites`, and `All Games` are stable Home destinations once a library exists.

A completely unconfigured/no-source installation may replace the carousel with the onboarding/empty state.

## Systems

- systems with **0 indexed Games** do not appear on normal Home;
- they remain visible in Manage Library;
- systems do **not** disappear because a Source is offline.

Example:

```text
PLAYSTATION
1124 Games
Unavailable

RetroSSD not connected
```

## Actions

```text
Left / Right  → change Home item
Primary       → open
GlobalMenu    → Global Menu
```

Back on the Home root does not create another frontend level.

## Required states

- normal;
- Favorites empty;
- Recent empty;
- no Library Sources;
- Source Offline;
- Permission Denied;
- active background job indicator;
- active session indicator;
- focused system removed after successful reconciliation.

---

# 5. Screen 2 — Game Browser

**Interaction:** DECIDED  
**Vertical cover rail:** FAVORED

## Purpose

The Game Browser is the primary game experience.

No mandatory detail screen exists before launching.

## Low-fidelity structure

```text
┌──────────────────────────────────────────────────────────────┐
│ SUPER NINTENDO                                              │
│                                                              │
│       [ previous ]                                           │
│                                                              │
│     ┌────────────┐       SUPER METROID                      │
│     │  BOX ART   │       1994 · Nintendo                   │
│     └────────────┘       Action Adventure · 1 Player       │
│                                                              │
│       [ next ]           Continue · Yesterday 21:43         │
│                          ★ Favorite                         │
│                                                              │
│ Primary Play   Continue   Info   Options   Global Menu       │
└──────────────────────────────────────────────────────────────┘
```

## Navigation

```text
Up / Down                 → Games
PageNext / PagePrevious   → page jump
SectionNext/Previous      → section jump
Search                    → global game search
```

Fast Navigation is MVP.

Section examples:

- Title → A–Z
- Release Date → years
- Recently Played → temporal groups

The list/query model must remain view-independent so a future Grid can reuse it.

## Visible information

Default density:

- title;
- system where needed (especially All Games);
- release year;
- developer or publisher;
- genre;
- players;
- short description;
- Favorite status;
- optional Last Played;
- optional Continue;
- readiness warning only when relevant.

Do not permanently show paths, hashes, config paths, launch arguments, or core versions.

## Readiness

Ready Games show no extra status.

Problems are local and concise:

```text
⚠ BIOS required
⚠ Core missing
⚠ Source offline
```

Play remains visible but is blocked with a useful recovery action.

## Multiple Releases

If multiple Releases exist, show a small indicator such as:

```text
USA · +2 Releases
```

Release complexity stays hidden when irrelevant.

## Required states

- normal;
- Continue available;
- Favorite;
- multiple Releases;
- missing Cover;
- BIOS missing;
- Core missing;
- Source Offline;
- Permission Denied;
- active filters;
- Search jump outside current filter;
- empty result;
- Game removed/reordered while focused;
- another session already active;
- return after RetroArch exits.

---

# 6. Screen 3 — Game Info

**Purpose/behavior:** DECIDED  
**Layout:** FAVORED

## Purpose

Game Info is the deeper information view. It is optional and never required before Play.

## Content

- title and system;
- Box Art;
- release date;
- developer/publisher;
- genre;
- players;
- description;
- Favorite;
- Last Played;
- Playtime;
- active Release;
- Save-State summary;
- Readiness;
- scraped Screenshots.

No Fanart, Videos, or Game Logos are required for MVP.

## Actions

```text
Primary      → Play
Continue     → latest exactly compatible State
Save States  → Save State timeline
Releases     → Release picker
Options      → Game Options
Back         → previous Game Browser/context
```

## Releases

For multiple Releases, expose active/default/preferred Release without forcing Release complexity into ordinary browsing.

Changing Release may change:

- launch Content;
- effective Core;
- Readiness;
- Continue;
- visible Save States.

## Multi-disc

Game Info may show disc completeness when relevant.

Exact launch behavior for partially missing multi-disc Releases remains subject to the underlying Content/launch rules; the UI must not invent a universal rule.

---

# 7. Screen 4 — Save States

**Compatibility/load policy:** DECIDED  
**Timeline presentation:** FAVORED

## Purpose

RetroArch remains the technical source of Save-State files.

BitArchive provides a chronological UI over:

```text
Release + Core + Core Version
```

## Default scope

Show Save States for the active Release.

If other Releases have States:

```text
2 save states belong to other releases
```

## Compatibility

```text
Compatible
→ Load directly

VersionMismatch
→ warning + explicit Load Anyway

IncompatibleCore
→ Load disabled
```

`Continue` selects only the newest `Compatible` State for the active Release, active Core, and exact Core version.

## Timeline

Newest first.

Each row can show:

- thumbnail or placeholder;
- date/time;
- optional user name;
- technical slot as secondary information;
- compatibility status.

## Options

```text
Load / Load Anyway
Rename
Details
Delete
```

Delete always requires confirmation.

BitArchive does not create new Save States in MVP.

## Core updates

Before updating a Core, show Save-State impact.

After update, previously compatible States may become `VersionMismatch`, causing Continue to disappear.

Rollback may restore compatibility.

---

# 8. Screen 5 — Search

**Behavior:** DECIDED  
**Visual form:** FAVORED

## Purpose

Global, system-independent game search.

Search operates on Games, not duplicated Release rows.

## Flow

```text
Original Context
    ↓ Search
Search
    ↓ result
Target System Game Browser
    ↓ Back
Search (same query + result focus)
    ↓ Back
Original Context
```

## Search jump and filters

If the target Game is hidden by the current target-view filter, Search may temporarily bypass that filter to expose the result.

The original filter remains stored and is restored later.

## Empty query

Do not automatically render the entire Library.

Show a search prompt.

## Offline Games

Offline/permission-blocked indexed Games remain searchable and may be opened; Play remains blocked by readiness.

## Controller text input

Search must be usable without a physical keyboard via an on-screen keyboard or equivalent controller text-entry solution.

---

# 9. Screen 6 — Global Menu

**Destinations:** DECIDED  
**Overlay form:** FAVORED

## Purpose

Global actions that are not scoped to the focused Game.

## Entries

```text
Search
Now Playing      # only if session exists
Activity         # only if relevant
Manage Library
Settings
Quit BitArchive
```

The menu is not a permanent sidebar.

## Back

Closing it restores the exact previous context.

## Now Playing

```text
NOW PLAYING

Super Metroid
42 min

[ Switch to Game ]
[ Quit Game ]
```

Only one active session exists.

## Quit while a Game runs

MVP behavior:

- do not silently abandon the managed session;
- require ending the Game before exiting BitArchive, or cancel the app quit.

A `Keep Game Running after BitArchive exits` mode is not part of this MVP spec.

---

# 10. Screen 7 — Game Options

**Contents and scope:** DECIDED

## Purpose

Context hub for exactly one Game / active Release.

## Entries

```text
Release
Core
Emulation Settings
Core Options
Save States
Edit Metadata
Re-scrape
File Info
Effective Configuration
Favorite
Readiness
Hide from Library
```

Not every item must be shown if irrelevant.

## Core resolution

```text
System Default
  ↓
Game Override
  ↓
Release Override (optional)
```

There is no global Core default.

Every Core selector shows:

- effective Core;
- source (`System`, `Game`, `Release`);
- reset-to-inherited action when an override exists.

## Settings hierarchies

RetroArch / Emulation Settings:

```text
Global → System → Game
```

Core Options:

```text
Core Defaults → System → Game
```

No Release-specific Emulation Settings or Core Options in MVP.

## Effective Configuration

Read-only view showing effective values and their source.

## Core changes

Before changing Core, show impact on existing Save States.

Do not delete incompatible States.

## Running session

Changes affecting launch configuration apply to the next launch, not silently to the running RetroArch process.

---

# 11. Screen 8 — Manage Library: Overview & Sources

**Structure:** DECIDED

## Management shell

A persistent local sidebar/list is allowed inside Manage Library.

```text
Overview
Sources
Systems
Scan
Scraping
Metadata
BIOS & Firmware
RetroArch & Cores
Updates
Maintenance
```

## Overview

Show actionable health summaries:

- source problems;
- missing firmware;
- missing cores;
- relevant scraping issues;
- component/update issues;
- activity needing attention.

Do not expose internal implementation metrics as primary content.

## Sources

Source statuses:

```text
Available
Offline
Permission Denied
Missing
```

Offline and Permission Denied do not delete indexed Games.

### Source actions

```text
Add Source
Re-scan
Grant Access
Locate Source
Open in Finder
Remove Source
```

Removal explicitly does **not** delete user game files.

### Detection

A Source may use:

```text
Automatic Detection
Fixed System
```

Overlapping Sources may warn; whether they are prohibited is not an MVP requirement.

---

# 12. Screen 9 — Manage Library: Systems & Scan

## Systems

Show all configured/known Systems, including zero-game Systems.

User-facing status examples:

```text
Ready
BIOS / Firmware Missing
Core Missing
Source Offline
Permission Required
No Games
```

No generic `Disabled` status is defined for MVP.

## System Details

May expose:

- Game count;
- Source(s);
- System Default Core;
- firmware readiness;
- last scan;
- System-scope Emulation Settings;
- System-scope Core Options;
- Effective Configuration.

## Scan

Scan and Scraping are separate workflows.

Scan discovers/indexes/reconciles local Content.

It does not fetch metadata/media.

### Scope

MVP requires:

```text
All Sources
Selected Source
```

### Safety

Destructive reconciliation only occurs after a successful full scan.

A failed or cancelled scan must not mass-remove existing Games.

### Review issues

Possible review cases:

- uncertain system assignment;
- invalid/corrupt Content;
- incomplete multi-disc Release;
- unsupported archive structure;
- ignored Content.

### Incremental visibility — OPEN

Newly discovered Games may become visible before the full scan finishes **only if** the implementation can do so safely without violating reconciliation guarantees.

Do not make this an MVP dependency.

---

# 13. Screen 10 — Manage Library: Scraping & Metadata

## Scraping

Scraping is manually started.

MVP provider:

```text
ScreenScraper
```

### Scope

- Games without metadata;
- fill missing fields;
- re-scrape all;
- selected System;
- current Game via deep link.

### Modes

```text
Automatic
Interactive
```

Interactive scraping may enter `AwaitingInput`.

### Job actions

Scraping supports:

```text
Pause
Resume
Cancel
```

### MVP data

Metadata:

- description;
- release date;
- developer;
- publisher;
- genre;
- players;
- region.

Media:

- Box Art;
- Screenshots.

Not MVP:

- Game Logos / Wheel Art;
- Fanart;
- Videos.

### Credentials

Stored through the OS secret-storage boundary.

Never expose secrets in logs/errors.

## Metadata

Manual overrides always win over provider values.

Scraping must not silently overwrite manual overrides.

Each editable field can expose provenance and:

```text
Reset to provider value
```

Game-level and Release-level metadata must remain separate in the model even if the UI simplifies editing.

---

# 14. Screen 11 — BIOS & Firmware + RetroArch & Cores

## BIOS & Firmware

One central firmware folder.

BitArchive does not distribute, silently copy, or silently rename user firmware.

Statuses include:

```text
Ready
Missing
Correct Content / Wrong Filename
Wrong Content
Optional Missing
```

Firmware requirements belong to Cores.

Recovery may include:

```text
Open in Finder
Change Folder
Re-scan Firmware
Grant Access
```

## RetroArch & Cores

Runtime and Cores are separate managed, versioned components.

Runtime details:

- active version;
- integrity;
- update;
- rollback.

Core details:

- installed version;
- integrity;
- supported systems;
- usage;
- update;
- update impact;
- rollback.

Cores are installed on demand.

Do not require installing all Cores.

### Core updates

Before confirmation show:

- Save States becoming `VersionMismatch`;
- Core Option overrides requiring attention;
- rollback availability.

Do not update/rollback an in-use Core underneath an active session.

### Core uninstall — DEFERRED

Core removal is not required by current MVP scope and must not be introduced as an implementation dependency.

---

# 15. Screen 12 — Updates, Maintenance & Activity

## Updates

Separate channels:

```text
BitArchive App
RetroArch Runtime
Cores
```

Automatic installation is not allowed.

App updates do not implicitly update Runtime/Cores.

Core updates require impact review when relevant.

## Maintenance

May expose:

- Hidden Games;
- Ignored Content;
- Library rebuild;
- managed-media cleanup;
- diagnostics/logs.

Maintenance may only delete files clearly owned by BitArchive.

Never use generic cleanup to delete external ROMs, ISOs, BIOS, firmware, or unrelated user files.

A complete BitArchive reset belongs to `Settings → Advanced`.

## Activity

Global job view.

Job states:

```text
Queued
Running
AwaitingInput
Paused
Completed
Cancelled
Failed
```

Typical jobs:

- Scan;
- Scraping;
- Runtime/Core downloads;
- component install;
- firmware scan;
- Backup;
- Restore.

`AwaitingInput` may be user-facing as `Needs Attention`.

Failed jobs remain visible until handled/dismissed.

---

# 16. Screen 13 — Settings

**Information architecture:** DECIDED

```text
General
Appearance
Input
Emulation
Language & Region
Notifications
Backup & Restore
Advanced
```

## General

Keep this small.

Architecture invariants such as single-session ownership are not exposed as arbitrary toggles.

## Appearance

MVP requires semantic Light/Dark support.

Preferred UI control:

```text
System
Light
Dark
```

Reduced Motion must respect the OS preference by default.

### UI Scale — OPEN / non-blocking

Custom UI scale is not a binding MVP requirement unless later explicitly adopted.

## Input

Show connected controller/device state and relevant preferences.

BitArchive is not the RetroArch remapping UI.

### Controller Test — FAVORED

Useful for diagnostics/prototyping, but not a binding MVP requirement unless adopted.

## Emulation

Global level of:

```text
Global → System → Game
```

Core Options remain separate.

## Language & Region

Keep separate:

- interface language;
- metadata language;
- fallback language;
- preferred region.

MVP UI languages: German and English.

## Notifications

Only useful local notifications, such as:

- long job complete;
- job needs attention;
- update available;
- important readiness issue.

Do not turn this into telemetry or marketing infrastructure.

## Backup & Restore

Backup covers BitArchive-managed state.

Save-State files may be optionally included as defined by product scope.

External ROMs/ISOs/BIOS/firmware are not silently backed up.

Restore is staged and requires a controlled app restart after successful activation.

## Advanced

May contain:

- Network & Privacy;
- Diagnostics;
- technical managed paths where useful;
- Reset BitArchive.

A general reset must not silently delete external user-owned files.

Save-State file deletion must remain explicit rather than being an ambiguous side effect of a generic reset.

---

# 17. Screen 14 — Onboarding and cross-cutting system states

## Onboarding — DECIDED

```text
Welcome
  ↓
UI Language
  ↓
Game Language / Region
  ↓
Library Sources
  ↓
Optional Fixed System per Source
  ↓
System Detection
  ↓
BIOS / Firmware Location
  ↓
Bundled Runtime Check
  ↓
Recommended Cores
  ↓
Initial Scan
  ↓
Ready
```

Scraping is optional after setup.

Missing firmware/core for one System must not block usable Systems.

## Launch Readiness — DECIDED

Readiness is checked before Play and Continue.

Ready launches directly with no redundant “Everything OK” screen.

Blocking issues produce local recovery UI.

Possible issue categories include:

- Core Missing;
- Core integrity failure;
- Runtime unavailable;
- Firmware missing/wrong content/wrong filename;
- Content unavailable;
- Source Offline;
- Permission Denied;
- unsupported format;
- invalid Content;
- active session.

## Launch transition

Normal launches should remain simple:

```text
Starting Super Metroid…
```

Technical phases are exposed only when needed.

## Running Session — DECIDED

Exactly one active session.

On session end:

- return BitArchive to foreground;
- finalize session/playtime;
- update Recent;
- refresh Save States;
- recalculate Continue;
- focus the played `GameId`.

## Session recovery

If a managed RetroArch session is recoverable after BitArchive restarts, expose it as `Now Playing` rather than silently losing session state.

## Confirmations

Use confirmations only for meaningful consequences.

Buttons name actions explicitly:

```text
Delete / Cancel
Quit Game / Cancel
Restore / Cancel
```

Avoid generic `Yes / No`.

Destructive confirmation defaults to the safe option.

## Errors

User-facing explanation first.

Technical details behind `Details`.

Secrets are always redacted.

## Empty states

Every empty state should explain:

1. why it is empty;
2. what the user can do.

Required examples:

- no Library Sources;
- Favorites empty;
- Recent empty;
- no Search result;
- no filtered result;
- no Save States.

## Loading

Load local text/state first.

Artwork and screenshots may appear asynchronously without reflowing the layout.

Do not use a fullscreen spinner for minor media loads.

## Offline behavior

The local Library remains functional without internet.

Internet-dependent functionality such as Scraping/update checks may fail independently.

## Permissions

Permission loss never means Content deletion.

Keep indexed Games visible and provide recovery.

## Accessibility

Minimum:

- visible focus;
- focus not encoded only by color;
- sufficient contrast;
- textual status in addition to iconography;
- Reduced Motion;
- keyboard support;
- controller support for core flows;
- no essential information available only via hover.

---

# 18. Canonical navigation map

```text
HOME
│
├─ Recent
├─ Favorites
├─ All Games
└─ Systems
     │
     ▼
GAME BROWSER
│
├─ Primary ───────────► PLAY
├─ Continue ──────────► CONTINUE
├─ Info ──────────────► GAME INFO
├─ Save States ───────► SAVE STATES
└─ Options ───────────► GAME OPTIONS
                         │
                         └─ Deep Links ─► MANAGE LIBRARY / SETTINGS
                                         │
                                         └─ Back → exact return context

GLOBAL MENU
├─ Search
├─ Now Playing
├─ Activity
├─ Manage Library
├─ Settings
└─ Quit
```

---

# 19. Prototype questions

These are the remaining items that should be decided by interaction prototyping rather than more abstract specification.

## P1 — Physical controller mapping

Validate concrete buttons for:

- Global Menu;
- Search;
- Page Jump;
- Section Jump;
- Options;
- convenience actions such as Continue / Info / Favorite / Save States.

The semantic actions are already fixed.

## P2 — Lateral System Switching

Test:

```text
Left / Right inside Game Browser
→ switch System directly
```

versus:

```text
Back → Home → next System
```

If direct switching is adopted, it replaces the current Browse history entry rather than pushing a new one.

## P3 — Home carousel wrap-around

Test wrap vs hard ends.

## P4 — Fixed-focus cover rail

Validate:

- controller feel;
- mouse first-click focus;
- double-click safety;
- trackpad momentum;
- wheel snapping;
- hover without focus mutation.

## P5 — Section Jump visualization

Behavior is decided; visual form is not.

Candidates include:

- transient letter/year overlay;
- horizontal section strip;
- compact jump index.

## P6 — Now Playing / Activity presence

Determine how much passive status belongs on Home/Game Browser versus only in Global Menu.

## P7 — Minimum window size and density

Determine during visual/prototype work.

## P8 — Keyboard type-ahead vs explicit Search

Title-list letter keys may perform section/type-ahead navigation, while `Cmd+F` remains Search. Validate interaction before binding final shortcuts.

## P9 — Optional Grid View

Still OPEN and not required for MVP.

The query/focus architecture already preserves the option.

---

# 20. Explicitly deferred / not MVP

The UX must not accidentally depend on:

- Collections;
- TV / Big Picture mode;
- videos;
- Fanart backgrounds;
- Game Logo / Wheel Art scraping;
- telemetry;
- cloud accounts;
- automatic media conversion;
- BitArchive-created Save States;
- normal SRAM / memory-card management;
- arbitrary live mutation of an active RetroArch session;
- Core uninstall;
- optional Grid View.

---

# 21. Readiness for the next design phase

With this consolidated specification:

- product flows are defined;
- screen inventory is complete;
- Back/focus behavior is defined;
- major empty/error/offline states are defined;
- management responsibility is defined;
- controller semantics are defined;
- remaining unknowns are prototype questions rather than product ambiguities.

**Status: READY FOR INTERACTION PROTOTYPING AND VISUAL WIREFRAMES.**

The next design work should validate P1–P8 without reopening settled product architecture.
