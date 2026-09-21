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

Ein RetroArch-Download enthält **nicht** automatisch die von BitArchive gewünschten Cores. Core-Bezug und Lizenzprüfung sind ein eigener Bereich; die erste Stufe davon ist implementiert (siehe unten). Die **Core-Auswahl** für ein Spiel bleibt davon getrennt und folgt weiterhin `Release > Game > System` ohne globalen Default.

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

### Stand der Core-Acquisition

Die erste Stufe der Core-Verwaltung ist ebenfalls implementiert. BitArchive kuratiert **genau einen** libretro-Core (mGBA) in einer reviewten Build-Identität und installiert ihn für die Architektur des Hosts:

```text
kuratierte Core-Definition / Allowlist (mgba, macos-arm64 | macos-x86_64)
    ↓
Auswahl des Host-Artefakts (keine Universal-Annahme)
    ↓
Download vom offiziellen Build-Host
    ↓
SHA-256-Verifikation (der Pin, kein TOFU)
    ↓
Extraktion genau des gepinnten Archiv-Members
    ↓
Installation als immutabler, plattform- und build-spezifischer Build
    ↓
Auflösung des Core-Library-Pfads
```

Wichtig für den Umgang mit dem Pin:

- Der offizielle Host veröffentlicht macOS-Cores nur unter einem rollierenden `latest`-Pfad. `latest` ist deshalb **keine** Version und **keine** Build-Identität, sondern nur der Transportweg; die Identität sind Build-ID, Revision und der gepinnte SHA-256.
- Wird ein Core upstream neu gebaut, passen die Bytes nicht mehr zum Pin. Die Installation wird dann **abgelehnt** (Digest-Mismatch); BitArchive übernimmt den neuen Digest nicht selbst. Ein Pin wird bewusst und reviewt angehoben. Auch ein reiner **Repack** des ZIPs (gleiche Library, neue Archiv-Bytes) ändert den gepinnten Archiv-Digest und muss deshalb reviewt nachgezogen werden; Build-ID, Revision und Library bleiben dabei unverändert (`docs/decisions/0002-managed-core-acquisition.md` §2).
- Cores werden pro Architektur getrennt installiert; ein Intel-Build wird nie für Apple Silicon eingesetzt (und umgekehrt).
- Es gibt **keine** Core-Aktivierung, keinen „aktuellen Core" und keinen globalen Core-Default. Mehrere Builds eines Cores liegen nebeneinander.

Die Entscheidungen im Detail stehen in [`docs/decisions/0002-managed-core-acquisition.md`](./docs/decisions/0002-managed-core-acquisition.md).

### Stand des ersten Managed-Launch

Der erste reale Launch über die BitArchive-eigene Runtime und den BitArchive-eigenen Core ist als Developer-Slice implementiert. Ein opt-in Developer-Befehl löst das Executable der aktiven Managed Runtime und den Library-Pfad des kuratierten mGBA-Builds aus dem Component Store auf, setzt beides mit einem vom Developer angegebenen lokalen Content-Pfad über die bestehende Launch-Kette zusammen und startet einen echten RetroArch-Prozess:

```text
Managed Runtime → Executable
Managed Core    → Library
lokaler Content (Developer)
      ↓
RetroArchLaunchInput
      ↓
RetroArchBackend::prepare_launch (-L <core> <content>)
      ↓
PreparedLaunch
      ↓
ProcessController::spawn (keine Shell)
      ↓
echter RetroArch-Prozess
      ↓
warten auf das Prozessende
```

Dabei gilt:

- Runtime und Core werden **ausschließlich** aus dem Managed-Component-System aufgelöst. Es gibt keine Developer-Pfade, keine Settings und keine Fallbacks (kein `PATH`, kein `/Applications`, keine System-RetroArch, keine beliebige lokale Core-Datei), und kein Argument benennt Runtime, Core, Version, URL oder Library.
- Der Developer gibt nur den Content-Pfad an. Content ist eine externe, user-eigene Datei: BitArchive lädt keinen Content herunter, kopiert, verschiebt, benennt oder verändert ihn nicht und implementiert hier keine Library, keinen Scan und keinen Import.
- Der Launch-Befehl lädt **nichts** herunter. Fehlt die Runtime oder der Core, nennt er den vorhandenen Acquisition-Befehl, der die Komponente installiert — es entsteht kein zweiter Acquisition-Flow.
- Der gestartete Prozess wird nur beobachtet und über den bestehenden `SpawnedProcess`-Handle abgewartet; Exit-Status und technische Fehler gehen an den Developer zurück. Es gibt keinen SessionManager, keine Persistenz, keine Playtime und kein Beenden des Prozesses durch BitArchive.
- Der Developer-Slice ist real verifiziert: Mit der BitArchive-verwalteten RetroArch-Runtime, dem kuratierten mGBA-Core und einer lokalen GBA-Datei startet der Befehl einen echten RetroArch-Prozess, der den Managed Core lädt; ein normaler RetroArch-Quit kommt als Exit-Code `0` beim Aufrufer an. Das bleibt eine Developer-Verifikation und ist **kein** Produkt-Play-Flow.
- Ein Produkt-Play-Flow ist weiterhin **nicht** implementiert: kein Play-Button, keine Library, keine Readiness-Prüfung, keine Konfigurationserzeugung, keine Save States, und aus dem normalen BitArchive-Fenster wird noch kein Spiel gestartet.

Die Entscheidung im Detail steht in [`docs/decisions/0003-managed-launch-composition.md`](./docs/decisions/0003-managed-launch-composition.md).

### Stand der Launch-Readiness und Launch-Vorbereitung

Die Produktentscheidung **vor** dem Prozessstart ist als eigener Schritt implementiert: BitArchive kann für einen konkreten Content beantworten, ob er mit dem aktuell konfigurierten und installierten Zustand gestartet werden kann, und welche konkreten Launch-Inputs daraus folgen.

```text
GameLaunchRequest
      ↓
System des Contents (kuratierte Systemliste, Format entscheidet)
      ↓
kuratierter Core + Installation (Managed Core Store)
      ↓
Managed Runtime (Managed RetroArch Runtime)
      ↓
Firmware-Readiness (Core-Anforderungen gegen vorhandene Dateien)
      ↓
effektive Launch-Konfiguration (Global → System → Game)
      ↓
PreparedGameLaunch  oder  LaunchBlocker[]
      ↓
RetroArchLaunchInput → RetroArchBackend::prepare_launch → PreparedLaunch
      ↓
        ── STOPP. Kein Prozessstart. ──
```

Der technische Übergang in die bestehende Launch-Kette nutzt heute drei der vier vorbereiteten Inputs — Managed-Executable, Managed-Core-Library und den exakten Content. Die **effektive Konfiguration** wird von `PreparedGameLaunch` mitgeführt und mit ihrer Quelle je Key ausgegeben, aber noch **nicht** an RetroArch übergeben: Dafür müsste eine `.cfg`-Datei erzeugt werden, und das ist ein späterer Launch-Artefakt-Schritt (B8 schreibt keine `.cfg`). RetroArch nutzt bis dahin seine eigenen Lookup-Regeln.

Dabei gilt:

- **Readiness ist ein Ergebnis, kein Fehler.** Ein blockierter Launch ist eine Liste strukturierter Gründe (`RuntimeUnavailable`, `CoreUnusable`, `UnsupportedSystemOrCore`, `ContentMissing`, `FirmwareMissing`, `InvalidConfiguration`) — nie eine Exception und nie ein verstecktes Boolean. Ein blockiertes Ergebnis ohne Grund ist nicht konstruierbar; die `LaunchReadiness` wird aus der Vorbereitung abgeleitet und kann ihr nicht widersprechen.
- **Die Blocker tragen strukturierte Ursachen, keine Texte.** Ein Blocker trägt Identitäten, Plattformen, Pfade und typisierte Ursachen (z. B. „nicht installiert“ vs. „installiert, aber unbrauchbar“); die Formulierung und der Reparaturhinweis entstehen erst in der Presentation-Schicht.
- **Readiness behauptet nur, was tatsächlich geprüft wurde.** Die Blocker-Menge ist eine echte Teilmenge der Kategorien aus `ARCHITECTURE.md` §21: Quellen-Verfügbarkeit, Berechtigungen, Content-Validierung, Core-Integrität und Session-Status brauchen Library, Source-Modell, Hash-Index und Session-Registry, die es noch nicht gibt. Ein unbrauchbarer gespeicherter Config-Key wird als `InvalidConfiguration` gemeldet und **nicht** als ungültiger Content — die Content-Bytes wurden nie geprüft.
- **Firmware blockiert nur, wenn sie wirklich nötig ist.** Der kuratierte mGBA-Core führt `gba_bios.bin` als **optional** — ein fehlendes GBA-BIOS blockiert einen GBA-Launch also nicht. Required-Firmware blockiert strukturell, ist aber nur gegen eine synthetische Definition getestet, weil heute kein kuratierter Core Firmware zwingend braucht. Es gibt keine Firmware-Downloads, keine BIOS-Datenbank, keine Auto-Beschaffung und keine erfundene Hash-Liste; Firmware wird ausschließlich gelesen, nie kopiert, umbenannt oder repariert.
- **Runtime und Core bleiben managed.** Dieselbe Invariante wie B7: kein `PATH`, kein `/Applications`, keine System-RetroArch, kein Developer-Executable, kein anderer Core und keine andere Architektur. „Nicht installiert“ und „installiert, aber unbrauchbar“ bleiben unterscheidbar — bei der Runtime (`RuntimeUnavailableReason::{NotInstalled, ExecutableMissing, StoreUnreadable}`) wie beim Core (`CoreUnusableReason::{NotInstalled, Unusable}`) —, weil das zwei verschiedene Reparaturen sind: Nur „nicht installiert“ bekommt den Acquisition-Hinweis, eine beschädigte Installation bekommt keinen (der Store lehnt die Neuinstallation eines bereits installierten Builds ab).
- **Konfiguration folgt `Global → System → Game`** (`game > system > global`). Die Priorität steckt im Scope selbst: Die Auflösung gruppiert die Scopes und wendet sie in der Reihenfolge `Global`, `System`, `Game` an, unabhängig davon, in welcher Reihenfolge ein Aufrufer sie übergibt. RetroArch-Settings haben bewusst **keine** Release-Ebene. Ein unbrauchbarer gespeicherter Key wird gemeldet und bleibt erhalten (Invariante 19), statt still gelöscht zu werden.
- **Der Schritt endet bei „ready + prepared“.** Es wird kein Prozess gestartet; der reale Prozessstart bleibt B7. `RetroArchBackend` bleibt der einzige Ort, der die RetroArch-CLI kennt.
- Der normale Start `cargo run -p bitarchive-desktop` bleibt unverändert; aus dem Produkt-Flow wird weiterhin kein Spiel gestartet.

Die Entscheidung im Detail steht in [`docs/decisions/0004-launch-readiness-and-preparation.md`](./docs/decisions/0004-launch-readiness-and-preparation.md).

Signierte Distribution-Manifeste sind noch nicht implementiert. Bis dahin ist der gepinnte SHA-256 der BitArchive-seitige Trust Anchor; Details stehen in [`docs/decisions/0001-managed-runtime-acquisition.md`](./docs/decisions/0001-managed-runtime-acquisition.md) und [`docs/decisions/0002-managed-core-acquisition.md`](./docs/decisions/0002-managed-core-acquisition.md).

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

- ein Cargo-Workspace mit den Crates `bitarchive-domain` (fachliche Identitäten, Launch-Pfad-Modell, Core-Resolution-Policy, kuratierte Systemliste, Firmware-Anforderungen, Konfigurations-Hierarchie sowie gepinnte Runtime- und Core-Definition), `bitarchive-application` (Launch-Verträge, Launch-Readiness und Launch-Vorbereitung, Runtime- und Core-Acquisition-Ports und Orchestrierungsschicht), `bitarchive-infrastructure` (HTTP-Download, Verifikation, ZIP-Extraktion, Staging, Component Stores und die Firmware-Inventarisierung des einen Firmware-Ordners), `bitarchive-emulation` (RetroArch-Launch-Aufbereitung, gepinnte RetroArch-Runtime, Auflösung des verwalteten Executables und die Zustandsauflösung für die Launch-Readiness), `bitarchive-platform` (plattformspezifische Dienste: Prozessstart, App-Pfade und das Lesen des macOS-Disk-Images), `bitarchive-ui` (Slint-Presentation-Layer) und `bitarchive-desktop` (Composition Root und Einstiegspunkt),
- ein startbares Desktop-Binary, das ein minimales Slint-Fenster öffnet,
- ein Platzhalter-Fenster, das ausschließlich anzeigt, dass das Fundament läuft.

Im Launch-Pfad existieren bisher die ersten Verträge und die ersten beiden konkreten Schritte: ein Launch-Wunsch (`GameId` + `Play`/`Continue`), strukturierte Launch-Readiness-Kategorien, die deterministische Core-Resolution-Policy (`Release > Game > System`, ohne globalen Core-Default), der backend-neutrale Prozessvertrag `PreparedLaunch`, die deterministische Übersetzung bereits aufgelöster RetroArch-Eingaben in Prozessargumente (`-L <core> <content>`, optional `--config <config>`) und der direkte Prozessstart dieses `PreparedLaunch` über `std::process::Command`. Der Prozessstart benutzt keine Shell: Executable und Argumente werden einzeln an die Prozess-API übergeben, Environment-Overrides ergänzen das geerbte Parent Environment, und ein Working Directory wird nur gesetzt, wenn `PreparedLaunch` eines vorgibt. Der gestartete Prozess bleibt über einen eigenen Handle (`ProcessController` → `SpawnedProcess`) beobachtbar; beendet oder überwacht wird er dabei nicht automatisch. RetroArch-spezifische Typen kennt die Platform-Crate nicht.

Für die **RetroArch-Runtime** existiert die erste kontrollierte Management-Stufe: eine im Code gepinnte Runtime-Definition (Version, offizielle URL, SHA-256, Artefaktart, erwarteter Executable-Pfad sowie Upstream- und Lizenzmetadaten), ein HTTPS-Download ausschließlich vom offiziellen Host, eine SHA-256-Verifikation streaming während des Schreibens, ein versionierter Component Store (`components/runtime/<platform>/<version>/`), eine Aktivierungs-Registry, die ausschließlich auf installierte Versionen zeigen kann, und die Auflösung des konkreten RetroArch-Executable-Pfads. Ein Hash-Mismatch bricht die Installation ab und lässt die aktive Runtime unangetastet; bereits installierte Versionen sind immutable. Die Installation ist ein einzelner Rename innerhalb des Component Stores; ein finaler Versionspfad entsteht also nur als vollständige Installation, und ein fehlgeschlagener Move hinterlässt dort nichts. Kein Shell-Download, keine `latest`-Auflösung, kein beliebiger User-RetroArch-Pfad.

Für **Cores** existiert die erste kontrollierte Management-Stufe ebenfalls, bewusst auf genau einen kuratierten Core begrenzt: eine Allowlist mit einer reviewten mGBA-Definition (Komponenten-ID, libretro-Core-Name, Build-ID, Revision, Architektur, offizielle URL, gepinnter SHA-256, erwartetes Archiv-Member, Library-Pfad sowie Upstream- und Lizenzmetadaten), ein Download über dieselbe HTTP-Implementierung wie die Runtime, eine member-genaue ZIP-Extraktion (nur das gepinnte Member wird geschrieben; absolute Pfade, `..`, Verzeichnis- und Symlink-Einträge werden abgelehnt), ein plattform- und build-spezifischer Core Store (`components/cores/<component>/<platform>/<build>/`) mit einem einzelnen Rename als Installationsschritt und **ohne** jegliche Aktivierung. Ein Core, für den keine kuratierte Definition existiert, wird nicht installiert — es gibt keinen User-URL-, Core-Namen- oder Library-Pfad-Parameter. Vom offiziellen Build-Host stammt nur der rollierende `latest`-Pfad; Identität und Vertrauensanker sind Build-ID, Revision und der gepinnte SHA-256.

Für die **Launch-Readiness und Launch-Vorbereitung** existiert der erste produktnahe Orchestrierungsschritt: ein `GameLaunchRequest` (Game-Identität + Content) wird gegen die kuratierte Systemliste, die Managed-Components-Stores, den einen Firmware-Ordner und die Konfigurations-Hierarchie `Global → System → Game` aufgelöst und ergibt entweder ein `PreparedGameLaunch` (Managed-Executable, Managed-Core-Library, exakter Content, effektive Konfiguration) oder eine Liste strukturierter Blocker. Der Schritt startet keinen Prozess: Er entscheidet und bereitet vor, der reale Start bleibt B7.

Noch **nicht** implementiert sind unter anderem: Home, Game Browser, Game Info, Suche, Global Menu, Game Options, Save States, Manage Library, Settings, Onboarding, Activity, Datenbank, Library-Scan, Scraping, Start eines echten Spiels aus dem Produkt-Flow, Play-Button, Content-Resolution über den vom Aufrufer gelieferten Pfad hinaus, Release-Resolution, Core-Auswahl und Core-Katalog über den einen kuratierten Core hinaus, Core-Options, RetroArch-Konfigurationserzeugung, Hash-basierte Firmware-Identifikation, Session-Management, Prozess-Lifecycle (geordnetes Beenden, Force Kill), Launch-Log-Artefakte, Controller-Input, Localization, Rollback sowie Runtime-Update-UI und Packaging.

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

Die normalen Tests laden **kein** echtes RetroArch-Artefakt und **keinen** echten Core herunter. Sie verwenden Fake-Downloader, einen lokalen HTTP-Server auf der Loopback-Adresse, selbst gebaute ZIP-Fixtures und temporäre Component Roots. Die Tests, die ein echtes Disk-Image brauchen, sind `#[ignore]`d.

### LIVE RUNTIME ACQUISITION (Developer, opt-in)

Der erste Befehl, der Netzwerkzugriffe ausführt, ist ein expliziter Developer-Befehl. Er lädt die gepinnte RetroArch-Runtime vom offiziellen Host, verifiziert sie gegen den gepinnten SHA-256, installiert sie in den Component Store, aktiviert sie und löst den Executable-Pfad auf:

```bash
cargo run -p bitarchive-desktop -- acquire-retroarch-runtime --dry-run
cargo run -p bitarchive-desktop -- acquire-retroarch-runtime --root /tmp/bitarchive-runtime-check
```

- `--dry-run` zeigt nur die gepinnte Definition und lädt nichts herunter.
- `--root <dir>` richtet den gesamten Lauf auf ein Wegwerf-Verzeichnis; ohne `--root` wird in die regulären App-Datenpfade installiert.
- Der Befehl startet **kein** Spiel und installiert **keinen** Core.
- CI führt ihn nicht aus; die normalen Tests hängen nicht davon ab.

### LIVE CORE ACQUISITION (Developer, opt-in)

Der zweite Developer-Befehl lädt den kuratierten Core (mGBA) für die Architektur des Hosts, verifiziert ihn gegen den gepinnten SHA-256, extrahiert genau das gepinnte Library-Member, installiert ihn als immutablen Build und löst den Library-Pfad auf:

```bash
cargo run -p bitarchive-desktop -- acquire-core --dry-run
cargo run -p bitarchive-desktop -- acquire-core --root /tmp/bitarchive-core-check
```

- `--dry-run` zeigt die kuratierte Definition (inklusive Build-ID, Revision, Archiv-Member, Lizenz und Store-Pfad) und lädt nichts herunter.
- `--root <dir>` richtet den gesamten Lauf auf ein Wegwerf-Verzeichnis.
- Der Befehl akzeptiert **kein** Argument für Core, URL, Version oder Library-Pfad: was installiert wird, entscheidet allein die Allowlist.
- Er startet **kein** Spiel, verändert **keine** Runtime und aktiviert **nichts** (es gibt keinen aktiven Core).
- Schlägt die Verifikation fehl, weil der Host den Core neu gebaut hat, wird nichts installiert; der Pin muss dann reviewt angehoben werden (siehe [`docs/decisions/0002-managed-core-acquisition.md`](./docs/decisions/0002-managed-core-acquisition.md)).
- CI führt ihn nicht aus; die normalen Tests hängen nicht davon ab.

### MANAGED RETROARCH LAUNCH (Developer, opt-in)

Der dritte Developer-Befehl startet echten lokalen Content mit der BitArchive-verwalteten RetroArch-Runtime und dem kuratierten mGBA-Core. Runtime und Core werden ausschließlich aus dem Component Store aufgelöst; der Developer gibt nur den Content-Pfad an:

```bash
cargo run -p bitarchive-desktop -- launch /pfad/zu/lokalem/content
cargo run -p bitarchive-desktop -- launch /pfad/zu/lokalem/content --root /tmp/bitarchive-launch-check
```

- `<content>` ist der einzige Launch-Input des Developers und muss bereits lokal existieren. BitArchive lädt keinen Content herunter und kopiert oder verändert ihn nicht.
- `--root <dir>` löst Runtime und Core aus einem anderen Store-Root auf (dieselbe Option wie bei den Acquisition-Befehlen). Es benennt keine Komponente, sondern einen Store.
- Es gibt **kein** Argument für Runtime, Core, Version, URL oder Library-Pfad: `--retroarch`, `--runtime`, `--core`, `--core-library`, `--core-path`, `--core-url`, `--core-version` und `--runtime-version` werden abgelehnt.
- Fehlt die Runtime oder der Core, bricht der Befehl ab und nennt `acquire-retroarch-runtime` bzw. `acquire-core` — mit demselben `--root`, falls der Lauf mit einem gestartet wurde, damit der Hinweis den Store repariert, aus dem `launch` aufgelöst hat. Der Launch-Befehl lädt **nichts** herunter und legt keinen zweiten Acquisition-Flow an.
- Eine *beschädigte* Installation (Build-Verzeichnis vorhanden, Library fehlt) wird als solche gemeldet und bekommt **keinen** Acquisition-Hinweis: Der Store lehnt die Installation eines bereits installierten Builds ab, `acquire-core` könnte diesen Zustand also nicht reparieren.
- Der Befehl startet RetroArch direkt über die Prozess-API (keine Shell, Executable und Argumente getrennt), meldet die PID und wartet auf das Prozessende. Der Exit-Code des Prozesses wird zum Exit-Code des Befehls (z. B. `3` → `3`); ein Code außerhalb des von `ExitCode` darstellbaren Bereichs oder ein Prozess ganz ohne Exit-Code (z. B. Signaltermination) ergibt einen generischen Failure. Der Befehl beendet oder signalisiert RetroArch nicht selbst.
- Der normale Start `cargo run -p bitarchive-desktop` startet weiterhin die Desktop-Anwendung; aus dem Produkt-Flow wird noch kein Spiel gestartet.
- CI führt ihn nicht aus; die normalen Tests hängen nicht davon ab. Der reale Launch mit echter Runtime und echtem Core ist eine manuelle Developer-Verifikation.

Die Entscheidung im Detail steht in [`docs/decisions/0003-managed-launch-composition.md`](./docs/decisions/0003-managed-launch-composition.md).

Wo die Daten liegen (macOS, siehe `ARCHITECTURE.md` §35):

```text
~/Library/Application Support/BitArchive/components/runtime/retroarch/macos-universal/<version>/
~/Library/Application Support/BitArchive/components/cores/mgba/<platform>/<build-id>/mgba_libretro.dylib
```

### GAME LAUNCH PREPARATION (Developer, opt-in)

Der vierte Developer-Befehl beantwortet die Produktentscheidung vor dem Start und bereitet die Launch-Inputs vor, **ohne** einen Prozess zu starten:

```bash
cargo run -p bitarchive-desktop -- prepare /pfad/zu/lokalem/content
cargo run -p bitarchive-desktop -- prepare /pfad/zu/lokalem/content --root /tmp/bitarchive-prepare-check
cargo run -p bitarchive-desktop -- prepare /pfad/zu/lokalem/content --global video_vsync=true --system gba video_vsync=false --game video_fullscreen=true
```

- `<content>` ist der einzige Pflicht-Input und muss bereits lokal existieren. BitArchive lädt keinen Content herunter und kopiert oder verändert ihn nicht.
- `--root <dir>` löst Runtime, Core und den Firmware-Ordner aus einem anderen Store-Root auf (dieselbe Option wie bei den anderen Developer-Befehlen). Es benennt keine Komponente, sondern einen Store.
- `--global`, `--system <key>` und `--game` setzen Konfigurations-Overrides. Die Priorität ist `game > system > global`; ein `--system`-Override gilt nur für genau dieses System. Es gibt noch keine Persistenz: Die Overrides existieren nur für diesen Lauf.
- Es gibt **kein** Argument für Runtime, Core, Version, URL oder Library-Pfad. Der Befehl lädt **nichts** herunter: Fehlt eine Komponente, nennt er den vorhandenen Acquisition-Befehl mit demselben `--root`; eine *beschädigte* Installation bekommt bewusst keinen Acquisition-Hinweis.
- Der Befehl startet **keinen** Prozess. Er zeigt die aufgelösten Werte, die effektive Konfiguration mit ihrer Quelle je Key und die Argumente, die der spätere Play-Flow verwenden würde. Exit-Code `0` bedeutet „ready und prepared“, `1` bedeutet „blockiert“.
- CI führt ihn nicht aus; die normalen Tests hängen nicht davon ab, und für die Readiness-Tests werden ausschließlich Fakes verwendet (kein Netzwerk, kein RetroArch, keine ROMs, keine BIOS-Dumps, keine GUI).

Die Entscheidung im Detail steht in [`docs/decisions/0004-launch-readiness-and-preparation.md`](./docs/decisions/0004-launch-readiness-and-preparation.md).

## Beiträge

Änderungen sollten grundsätzlich über ein GitHub Issue nachvollziehbar sein und dem in [`docs/DEVELOPMENT.md`](./docs/DEVELOPMENT.md) beschriebenen Workflow folgen.

Direkte Änderungen an `main` sind nicht vorgesehen.

## Datenschutz

Für den aktuellen Projektumfang ist **keine Telemetrie** vorgesehen.

## Lizenz

**TBD**

Vor einer öffentlichen Veröffentlichung des Projekts muss eine Lizenz festgelegt und als `LICENSE`-Datei ergänzt werden.
