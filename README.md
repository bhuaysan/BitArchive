# BitArchive

> **Status:** Planning / Pre-Implementation  
> Die Implementierung hat noch nicht begonnen. Inhalte und Schnittstellen können sich während der Planungsphase noch ändern.

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

Das Projekt befindet sich aktuell vollständig in der **Planungsphase**.

Es existiert noch keine produktive Implementierung. Vor Beginn der Entwicklung werden Produktanforderungen, Architektur, Entwicklungsprozess und Agentenregeln dokumentiert.

## Projektdokumentation

```text
.
├── README.md
├── AGENTS.md
├── PRODUCT.md
├── ARCHITECTURE.md
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

Das GitHub-Repository wird im Rahmen der Projektvorbereitung erstellt.

Bis dahin sind Repository-URL, Build-Anweisungen und Installationsschritte noch nicht verfügbar.

## Voraussetzungen

Da die Implementierung noch nicht begonnen hat, existieren noch keine vollständigen Build- oder Installationsanweisungen.

Die technische Architektur ist jedoch festgelegt auf Rust + Slint + SQLite. Für den Entwicklungsworkflow werden mindestens benötigt:

- Git
- GitHub-Zugang
- GitHub CLI (`gh`)
- Rust-Toolchain / Cargo, sobald die Implementierung startet

Konkrete Plattform-, Packaging- und Build-Voraussetzungen werden mit dem initialen Repository-Setup ergänzt.

## Beiträge

Änderungen sollten grundsätzlich über ein GitHub Issue nachvollziehbar sein und dem in [`docs/DEVELOPMENT.md`](./docs/DEVELOPMENT.md) beschriebenen Workflow folgen.

Direkte Änderungen an `main` sind nicht vorgesehen.

## Datenschutz

Für den aktuellen Projektumfang ist **keine Telemetrie** vorgesehen.

## Lizenz

**TBD**

Vor einer öffentlichen Veröffentlichung des Projekts muss eine Lizenz festgelegt und als `LICENSE`-Datei ergänzt werden.
