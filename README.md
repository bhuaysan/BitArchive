# BitArchive

> **Status:** Early Implementation  
> Die Implementierung hat begonnen. Aktuell existiert ein minimales Rust-/Slint-Fundament ohne Produktfunktionen. Produkt-, Architektur- und Schnittstellenentscheidungen können sich weiter ändern.

BitArchive ist ein geplantes lokales Emulation-Frontend und eine Verwaltungsbibliothek, die RetroArch als verwaltetes Emulations-Backend verwendet. Ziel ist eine moderne, nachvollziehbar konfigurierte Oberfläche für Spielebibliothek, Metadaten, Medien, Core-Zuordnung, Save States und den Start bzw. Wiedereinstieg in Spiele.

## Ziele

BitArchive soll insbesondere:

- ROM-Bibliotheken strukturiert verwalten und darstellen,
- Systeme und Spiele übersichtlich organisieren,
- RetroArch als Emulations-Backend verwenden,
- pro System einen passenden RetroArch-Core verwalten und bei Bedarf Game-/Release-Overrides ermöglichen,
- Spiele direkt über **Play** starten und über **Continue** in kompatible Save States zurückkehren,
- BIOS-Dateien über einen zentral definierten BIOS-Ordner bereitstellen,
- Metadaten und Medien über Scraping ergänzen,
- RetroArch-Konfigurationen hierarchisch überschreiben können,
- die eigentliche Emulation bewusst RetroArch überlassen.

BitArchive soll kein eigener Emulator sein.

## MVP

Für den ersten funktionsfähigen Umfang sind aktuell unter anderem folgende Funktionen vorgesehen:

- Verwaltung einer lokalen Spielebibliothek
- Unterstützung mehrerer Emulationssysteme
- Zuordnung von RetroArch-Cores
- Start eines Spiels über RetroArch
- zentral konfigurierbarer BIOS-Ordner
- Scraping von Metadaten und Medien
- manuell startbarer Scraping-Durchlauf unter **Manage Library**
- RetroArch-Konfigurationshierarchie:
  - global
  - pro System
  - pro Spiel
- Save-State-Verwaltung über eine BitArchive-Metadaten- und Kompatibilitätsebene auf den von RetroArch erzeugten State-Dateien

### Bewusst nicht Teil des MVP

- eigener TV-Modus
- eigene Savegame-Verwaltung außerhalb von RetroArch
- Telemetrie
- automatische Medienkonvertierung

Der verbindliche Produktumfang wird in [`PRODUCT.md`](./PRODUCT.md) gepflegt. Interaktions- und Navigationsentscheidungen stehen in [`docs/UI_UX_CONCEPT.md`](./docs/UI_UX_CONCEPT.md).

## RetroArch-Integration

RetroArch übernimmt die eigentliche Emulation.

BitArchive ist für die darüberliegende Verwaltung und Orchestrierung zuständig, beispielsweise für:

- Spielebibliothek
- Systemzuordnung
- Core-Auswahl
- BIOS-Pfade
- Scraping
- Startkonfiguration
- Konfigurations-Overrides

Für RetroArch-Konfigurationen ist folgende Priorität vorgesehen:

```text
Global
  ↓
System
  ↓
Spiel
```

Spezifischere Einstellungen überschreiben dabei allgemeinere Einstellungen.

RetroArch bleibt die technische Quelle der Save-State-Dateien.

BitArchive indexiert und verwaltet darüber Metadaten, Kompatibilität, Timeline, `Continue`, Laden, Benennen und bestätigtes Löschen der State-Dateien. Normale SRAM-/Memory-Card-Saves bleiben außerhalb des BitArchive-Scopes.

## Projektstatus

Produktanforderungen, Architektur, UI/UX-Konzept und Entwicklungsprozess sind dokumentiert. Die Implementierung hat begonnen.

Aktuell existiert bewusst nur ein minimales, lauffähiges Fundament:

- ein Cargo-Workspace mit den Crates `bitarchive-domain` (fachliche Identitäten, Launch-Pfad-Modell und Core-Resolution-Policy), `bitarchive-application` (Launch-Verträge und Orchestrierungsschicht), `bitarchive-ui` (Slint-Presentation-Layer) und `bitarchive-desktop` (Composition Root und Einstiegspunkt),
- ein startbares Desktop-Binary, das ein minimales Slint-Fenster öffnet,
- ein Platzhalter-Fenster, das ausschließlich anzeigt, dass das Fundament läuft.

Im Launch-Pfad existieren bisher nur die ersten fachlichen Verträge: ein Launch-Wunsch (`GameId` + `Play`/`Continue`), strukturierte Launch-Readiness-Kategorien und die deterministische Core-Resolution-Policy (`Release > Game > System`, ohne globalen Core-Default).

Noch **nicht** implementiert sind unter anderem: Home, Game Browser, Game Info, Suche, Global Menu, Game Options, Save States, Manage Library, Settings, Onboarding, Activity, Datenbank, Library-Scan, Scraping, RetroArch-Integration, Firmware- und Core-Verwaltung, Session-Management, Controller-Input, Localization und Packaging.

Weiteres wird als GitHub Issue geplant und umgesetzt.

## Projektstruktur

```text
.
├── Cargo.toml
├── rust-toolchain.toml
├── README.md
├── AGENTS.md
├── PRODUCT.md
├── ARCHITECTURE.md
├── apps/
│   └── bitarchive-desktop/
├── crates/
│   ├── bitarchive-application/
│   ├── bitarchive-domain/
│   └── bitarchive-ui/
└── docs/
    ├── DEVELOPMENT.md
    ├── UI_UX_CONCEPT.md
    └── decisions/
```

### Dokumente

| Datei | Zweck |
|---|---|
| [`README.md`](./README.md) | Einstieg und Projektüberblick |
| [`PRODUCT.md`](./PRODUCT.md) | Produktfähigkeiten, MVP, Verhalten und Scope |
| [`docs/UI_UX_CONCEPT.md`](./docs/UI_UX_CONCEPT.md) | Informationsarchitektur, Interaktion, Navigation und UI/UX-Entscheidungsstatus |
| [`ARCHITECTURE.md`](./ARCHITECTURE.md) | Technische Architektur, Subsystemgrenzen und Invarianten |
| [`AGENTS.md`](./AGENTS.md) | Regeln und Kontext für AI-gestützte Entwicklung |
| [`docs/DEVELOPMENT.md`](./docs/DEVELOPMENT.md) | Entwicklungsworkflow, Git, Issues, Tests, PRs und Reviews |
| [`docs/decisions/`](./docs/decisions/) | Architecture Decision Records für langlebige Querschnittsentscheidungen |

## Entwicklung

GitHub dient als Remote Repository und als zentrales Issue-System.

Die Entwicklung erfolgt ticketbasiert. `main` bleibt stabil; Änderungen werden über eigene Branches und Pull Requests integriert.

Der vorgesehene Ablauf:

```text
Issue / Task
    ↓
Branch
    ↓
AI implementiert
    ↓
Tests
    ↓
Commit
    ↓
Push
    ↓
Pull Request
    ↓
Review
    ↓
Merge nach main
```

Am Ende einer Implementierung wird ein **Handover-Protokoll** erstellt. Dieses dient insbesondere dem Review im ChatGPT-Chat und dokumentiert die vorgenommenen Änderungen, Tests, bekannte Einschränkungen und offene Punkte.

Die vollständigen Entwicklungsregeln stehen in [`docs/DEVELOPMENT.md`](./docs/DEVELOPMENT.md).

## Repository

Das Projekt liegt unter <https://github.com/bhuaysan/BitArchive>.

Ein installierbares Paket existiert noch nicht. Das Fundament kann bisher nur aus dem Quellcode gebaut und gestartet werden.

## Voraussetzungen

Für den Entwicklungsworkflow werden benötigt:

- Git
- GitHub-Zugang und GitHub CLI (`gh`) für Issues und Pull Requests
- Rust über [rustup](https://rustup.rs/). Die Toolchain wird über `rust-toolchain.toml` auf den stabilen Release-Kanal festgelegt; `rustfmt` und `clippy` sind Bestandteil der Validierung.

Plattform-, Packaging- und Signing-Voraussetzungen für eine verteilbare macOS-App werden ergänzt, sobald Packaging Teil der Implementierung ist.

## Build und Start

Alle Befehle werden aus dem Repository-Stamm ausgeführt.

Desktop-Anwendung starten:

```bash
cargo run -p bitarchive-desktop
```

Es öffnet sich ein minimales Platzhalter-Fenster. Das Schließen des Fensters beendet den Prozess.

Validierung:

```bash
cargo fmt --all -- --check
cargo check --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

`Cargo.lock` wird bewusst versioniert, da BitArchive eine Anwendung ist.

## Beiträge

Änderungen sollten grundsätzlich über ein GitHub Issue nachvollziehbar sein und dem in [`docs/DEVELOPMENT.md`](./docs/DEVELOPMENT.md) beschriebenen Workflow folgen.

Direkte Änderungen an `main` sind nicht vorgesehen.

## Datenschutz

Für den aktuellen Projektumfang ist **keine Telemetrie** vorgesehen.

## Lizenz

**TBD**

Vor einer öffentlichen Veröffentlichung des Projekts muss eine Lizenz festgelegt und als `LICENSE`-Datei ergänzt werden.
