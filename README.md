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

### RetroArch ist eine von BitArchive verwaltete Runtime

BitArchive setzt **keine** beliebig lokal installierte RetroArch-Version voraus und verwendet **keinen** frei konfigurierbaren RetroArch-Pfad.

RetroArch ist eine von BitArchive verwaltete, versionierte Runtime-Komponente:

- BitArchive pinnt eine konkrete RetroArch-Version, ein konkretes offizielles Artefakt und dessen SHA-256.
- Die Runtime wird aus der offiziellen Quelle bezogen, gegen den gepinnten SHA-256 verifiziert und in einem versionierten Component Store installiert.
- Nutzer sollen für den normalen Betrieb **keine eigene RetroArch-Installation bereitstellen oder pflegen** müssen. Eine bestehende RetroArch-Installation wird weder verwendet noch verändert.
- Beliebige externe RetroArch-Pfade sind **kein normales Produktmodell**; es gibt keinen User-Path-Fallback.
- Die installierte Runtime liegt außerhalb des App-Bundles in den regulären App-Datenpfaden, nicht in `/Applications/RetroArch.app`.

### Runtime und Cores bleiben getrennte Komponenten

Runtime und Cores sind getrennt versionierte Komponenten:

- Die **Runtime** ist RetroArch selbst.
- **Cores** sind libretro-Bibliotheken mit eigenen Versionen und eigenen Lizenzen.

Ein RetroArch-Download enthält **nicht** automatisch die von BitArchive gewünschten Cores. Core-Bezug, Core-Auswahl und Lizenzprüfung sind ein eigener Bereich und werden in einem separaten Schritt umgesetzt.

### Stand der Runtime-Acquisition

Die erste Stufe der Runtime-Verwaltung ist implementiert:

```text
gepinnte Runtime-Definition
    ↓
Download vom offiziellen Host
    ↓
SHA-256-Verifikation
    ↓
Staging
    ↓
Installation in den versionierten Component Store
    ↓
Auflösung des installierten RetroArch-Executables
```

Der echte **Start eines Spiels** folgt erst nach dem Core Management: Die Launch-Kette kann heute eine bereits vorbereitete Eingabe in einen Prozessstart übersetzen, aber es wird noch kein Spiel aus dem Produkt-Flow gestartet, und es wird noch kein Core installiert.

Signierte Distribution-Manifeste sind noch nicht implementiert. Bis dahin ist der gepinnte SHA-256 der BitArchive-seitige Trust Anchor; Details stehen in [`docs/decisions/0001-managed-runtime-acquisition.md`](./docs/decisions/0001-managed-runtime-acquisition.md).

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

- ein Cargo-Workspace mit den Crates `bitarchive-domain` (fachliche Identitäten, Launch-Pfad-Modell, Core-Resolution-Policy und gepinnte Runtime-Definition), `bitarchive-application` (Launch-Verträge, Runtime-Acquisition-Ports und Orchestrierungsschicht), `bitarchive-infrastructure` (HTTP-Download, Verifikation, Staging und Component Store), `bitarchive-emulation` (RetroArch-Launch-Aufbereitung, gepinnte RetroArch-Runtime und Auflösung des verwalteten Executables), `bitarchive-platform` (plattformspezifische Dienste: Prozessstart, App-Pfade und das Lesen des macOS-Disk-Images), `bitarchive-ui` (Slint-Presentation-Layer) und `bitarchive-desktop` (Composition Root und Einstiegspunkt),
- ein startbares Desktop-Binary, das ein minimales Slint-Fenster öffnet,
- ein Platzhalter-Fenster, das ausschließlich anzeigt, dass das Fundament läuft.

Im Launch-Pfad existieren bisher die ersten Verträge und die ersten beiden konkreten Schritte: ein Launch-Wunsch (`GameId` + `Play`/`Continue`), strukturierte Launch-Readiness-Kategorien, die deterministische Core-Resolution-Policy (`Release > Game > System`, ohne globalen Core-Default), der backend-neutrale Prozessvertrag `PreparedLaunch`, die deterministische Übersetzung bereits aufgelöster RetroArch-Eingaben in Prozessargumente (`-L <core> <content>`, optional `--config <config>`) und der direkte Prozessstart dieses `PreparedLaunch` über `std::process::Command`. Der Prozessstart benutzt keine Shell: Executable und Argumente werden einzeln an die Prozess-API übergeben, Environment-Overrides ergänzen das geerbte Parent Environment, und ein Working Directory wird nur gesetzt, wenn `PreparedLaunch` eines vorgibt. Der gestartete Prozess bleibt über einen eigenen Handle (`ProcessController` → `SpawnedProcess`) beobachtbar; beendet oder überwacht wird er dabei nicht automatisch. RetroArch-spezifische Typen kennt die Platform-Crate nicht.

Für die **RetroArch-Runtime** existiert die erste kontrollierte Management-Stufe: eine im Code gepinnte Runtime-Definition (Version, offizielle URL, SHA-256, Artefaktart, erwarteter Executable-Pfad sowie Upstream- und Lizenzmetadaten), ein HTTPS-Download ausschließlich vom offiziellen Host, eine SHA-256-Verifikation streaming während des Schreibens, ein versionierter Component Store (`components/runtime/<platform>/<version>/`), eine Aktivierungs-Registry, die ausschließlich auf installierte Versionen zeigen kann, und die Auflösung des konkreten RetroArch-Executable-Pfads. Ein Hash-Mismatch bricht die Installation ab und lässt die aktive Runtime unangetastet; bereits installierte Versionen sind immutable. Kein Shell-Download, keine `latest`-Auflösung, kein beliebiger User-RetroArch-Pfad.

Noch **nicht** implementiert sind unter anderem: Home, Game Browser, Game Info, Suche, Global Menu, Game Options, Save States, Manage Library, Settings, Onboarding, Activity, Datenbank, Library-Scan, Scraping, Start eines echten Spiels aus dem Produkt-Flow, Core-Verwaltung (Download, Installation, Auswahl, Allowlist), RetroArch-Konfigurations- und Core-Options-Erzeugung, Firmware-Readiness, Session-Management, Prozess-Lifecycle (geordnetes Beenden, Force Kill), Launch-Log-Artefakte, Controller-Input, Localization, Rollback sowie Runtime-Update-UI und Packaging.

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
│   ├── bitarchive-emulation/
│   ├── bitarchive-infrastructure/
│   ├── bitarchive-platform/
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
cargo check --workspace --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --locked
cargo build --workspace --locked
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
```

`Cargo.lock` wird bewusst versioniert, da BitArchive eine Anwendung ist.

Die normalen Tests laden **kein** echtes RetroArch-Artefakt herunter. Sie verwenden Fake-Downloader, einen lokalen HTTP-Server auf der Loopback-Adresse und temporäre Component Roots. Die Tests, die ein echtes Disk-Image brauchen, sind `#[ignore]`d.

### LIVE RUNTIME ACQUISITION (Developer, opt-in)

Der einzige Befehl, der Netzwerkzugriffe ausführt, ist ein expliziter Developer-Befehl. Er lädt die gepinnte RetroArch-Runtime vom offiziellen Host, verifiziert sie gegen den gepinnten SHA-256, installiert sie in den Component Store, aktiviert sie und löst den Executable-Pfad auf:

```bash
cargo run -p bitarchive-desktop -- acquire-retroarch-runtime --dry-run
cargo run -p bitarchive-desktop -- acquire-retroarch-runtime --root /tmp/bitarchive-runtime-check
```

- `--dry-run` zeigt nur die gepinnte Definition und lädt nichts herunter.
- `--root <dir>` richtet den gesamten Lauf auf ein Wegwerf-Verzeichnis; ohne `--root` wird in die regulären App-Datenpfade installiert.
- Der Befehl startet **kein** Spiel und installiert **keinen** Core.
- CI führt ihn nicht aus; die normalen Tests hängen nicht davon ab.

Wo die Daten liegen (macOS, siehe `ARCHITECTURE.md` §35):

```text
~/Library/Application Support/BitArchive/components/runtime/retroarch/macos-universal/<version>/
```

## Beiträge

Änderungen sollten grundsätzlich über ein GitHub Issue nachvollziehbar sein und dem in [`docs/DEVELOPMENT.md`](./docs/DEVELOPMENT.md) beschriebenen Workflow folgen.

Direkte Änderungen an `main` sind nicht vorgesehen.

## Datenschutz

Für den aktuellen Projektumfang ist **keine Telemetrie** vorgesehen.

## Lizenz

**TBD**

Vor einer öffentlichen Veröffentlichung des Projekts muss eine Lizenz festgelegt und als `LICENSE`-Datei ergänzt werden.
