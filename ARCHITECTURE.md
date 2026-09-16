# BitArchive – Technical Architecture v0.1

**Status:** Architektur für den MVP definiert  
**Basis:** `PRODUCT.md` / Product Specification v0.1  
**Primäre Zielplattform:** macOS  
**Spätere Zielplattformen:** Linux, perspektivisch Android  
**Technologie-Stack:** Rust + Slint + SQLite

---

## 1. Ziel der Architektur

BitArchive ist eine lokale Verwaltungsbibliothek und ein Emulation-Frontend für Retrospiele. Die Architektur muss die Produktgrenzen aus `PRODUCT.md` technisch erzwingen und gleichzeitig genügend Flexibilität für spätere Plattformen und Emulations-Backends erhalten.

Die Architektur verfolgt insbesondere folgende Ziele:

- native Desktop-Anwendung ohne WebView
- Local-first ohne Cloud-Backend
- klare Trennung zwischen `Game`, `Release` und physischem Content
- keine Besitzübernahme über ROMs, ISOs oder BIOS-/Firmware-Dateien
- verwaltete, versionierte RetroArch-Runtime
- verwaltete, versionierte Cores
- backend-neutrale Emulationsarchitektur
- provider-neutrale Metadatenarchitektur
- deterministische Launch-Konfiguration
- robuste Library-Scans und Wiedererkennung verschobener Dateien
- konservative Save-State-Kompatibilität
- Controller-fähige UI
- macOS-first, ohne Linux oder eine spätere Android-Portierung unnötig zu verbauen
- testbare, modularisierte Rust-Codebasis

---

## 2. Architekturprinzipien

### 2.1 Local-first

Alle fachlich relevanten Daten liegen lokal:

- Bibliotheksindex
- Metadaten
- Einstellungen
- Favoriten
- Statistiken
- Save-State-Metadaten
- Core-/Runtime-Zustand
- Scraping-Zustand
- Backup-Metadaten

Netzwerkzugriffe existieren nur für explizite Funktionen wie:

- ScreenScraper
- BitArchive-Updates
- Runtime-Updates
- Core-Downloads und Core-Updates

Es gibt im MVP kein Benutzerkonto, kein Cloud-Backend und keine Telemetrie.

### 2.2 Originaldateien bleiben externe Ressourcen

ROMs, ISOs und BIOS-/Firmware-Dateien werden als externe, nutzerverwaltete Ressourcen behandelt.

BitArchive:

- verschiebt sie nicht automatisch
- benennt sie nicht automatisch um
- konvertiert sie nicht
- reorganisiert sie nicht
- löscht sie nicht

Ausnahmen gelten nur dort, wo das Produkt dies ausdrücklich vorsieht, z. B. beim bestätigten Löschen einer RetroArch-Save-State-Datei.

### 2.3 Fachliche Identität ist nicht gleich Dateipfad

Die Architektur unterscheidet explizit:

```text
Game
  != Release
  != physischer Content
```

Ein Release kann mehrere Content-Dateien besitzen. Mehrere physische Dateien können denselben Inhalt repräsentieren.

Dateipfade sind daher niemals primäre fachliche Identitäten.

### 2.4 Ports-and-Adapters

Domain und Application dürfen nicht direkt von:

- SQLite
- Slint
- macOS-APIs
- RetroArch-Prozessdetails
- ScreenScraper
- Keychain
- konkreten HTTP-Bibliotheken

abhängen.

Äußere Adapter implementieren Ports, die von inneren Schichten definiert werden.

### 2.5 Plattformabhängigkeit bleibt außen

Plattformspezifische Funktionen werden hinter Abstraktionen gekapselt:

- Filesystem
- Prozesssteuerung
- Controller
- Secret Store
- App-Pfade
- App-Installer
- Dateiauswahl
- Notifications
- OS-Aktivierung / Fensterfokus

### 2.6 Generierte Dateien sind keine Source of Truth

Generierte Artefakte wie:

- RetroArch-Konfigurationen
- Core-Options-Dateien
- interne `.m3u`-Playlists
- Thumbnail-Caches
- Launch-Artefakte

werden aus persistenten Modellen erzeugt und können rekonstruiert werden.

---

## 3. Technologie-Stack

### 3.1 Primärtechnologien

| Bereich | Technologie |
|---|---|
| Sprache | Rust |
| UI | Slint |
| Async Runtime | Tokio |
| Datenbank | SQLite |
| SQLite-Zugriff | rusqlite |
| Serialisierung | serde |
| HTTP | reqwest |
| Logging / Tracing | tracing + tracing-subscriber |
| UUID | UUIDv7 |
| Hashing | SHA-256 |
| Property Tests | proptest |
| Build | Cargo |
| Projektstruktur | Cargo Workspace |

### 3.2 Warum Rust + Slint

Rust wird verwendet für:

- Domainlogik
- Filesystem-Scanning
- Hashing
- SQLite-Zugriff
- Netzwerkzugriffe
- Runtime-/Core-Management
- Prozesssteuerung
- Launch-Orchestrierung
- Backup/Restore
- Plattformadapter

Slint ist ausschließlich Presentation Layer.

Es gibt:

- keine WebView
- kein JavaScript
- kein Node/npm als Runtime-Abhängigkeit
- keine IPC-Grenze zwischen Frontend und Backend

### 3.3 Plattformstrategie

MVP:

```text
macOS
```

später:

```text
Linux
```

perspektivisch:

```text
Android
```

Android ist kein MVP-Ziel. Die Architektur vermeidet jedoch bewusst Desktop-Annahmen in Domain und Application.

---

## 4. High-Level Architecture

```text
┌─────────────────────────────────────────────┐
│                  Slint UI                   │
│              Presentation Layer             │
└──────────────────────┬──────────────────────┘
                       │ ViewModels / Commands
                       ▼
┌─────────────────────────────────────────────┐
│              Application Layer              │
│ Use Cases / Services / Jobs / Orchestration │
└──────────────────────┬──────────────────────┘
                       │
                       ▼
┌─────────────────────────────────────────────┐
│                  Domain                     │
│ Game / Release / Content / Rules / Policies │
└──────────────────────▲──────────────────────┘
                       │ Ports
        ┌──────────────┼───────────────┐
        ▼              ▼               ▼
┌──────────────┐ ┌──────────────┐ ┌──────────────┐
│Infrastructure│ │  Emulation   │ │   Platform   │
│SQLite / HTTP │ │  RetroArch   │ │ macOS / ...  │
└──────────────┘ └──────────────┘ └──────────────┘
```

Abhängigkeiten zeigen grundsätzlich nach innen.

---

## 5. Cargo-Workspace

BitArchive verwendet einen Cargo Workspace mit wenigen größeren Crates. Zu viele kleinteilige Crates werden vermieden.

Empfohlene Struktur:

```text
bitarchive/
├── Cargo.toml
├── rust-toolchain.toml
├── apps/
│   └── bitarchive-desktop/
│       ├── Cargo.toml
│       └── src/
│
├── crates/
│   ├── bitarchive-domain/
│   ├── bitarchive-application/
│   ├── bitarchive-infrastructure/
│   ├── bitarchive-emulation/
│   ├── bitarchive-platform/
│   └── bitarchive-ui/
│
├── resources/
│   ├── catalog/
│   ├── i18n/
│   └── assets/
│
├── migrations/
├── fixtures/
└── tools/
```

### 5.1 `bitarchive-domain`

Enthält:

- Domain-IDs
- Game
- Release
- Content
- Library Source
- Firmware
- Core
- Runtime
- Save State
- Settings
- Metadata
- Domain Policies
- Resolver
- fachliche Fehler

Keine Abhängigkeit von SQLite, Slint oder OS-APIs.

### 5.2 `bitarchive-application`

Enthält:

- Use Cases
- Application Services
- Repository-Ports
- Filesystem-Ports
- Metadata-Provider-Port
- EmulationBackend-Port
- JobManager
- Query Services
- Launch-Orchestrierung
- Backup/Restore-Orchestrierung

### 5.3 `bitarchive-infrastructure`

Enthält:

- SQLite-Repositories
- MigrationRunner
- HTTP
- ScreenScraper
- Media Store
- Backup-Dateiformat
- Manifest-Verifikation
- Download-/Staging-Infrastruktur

### 5.4 `bitarchive-emulation`

Enthält:

- RetroArchBackend
- RuntimeManager
- CoreManager
- Config Generator
- Core Option Introspection
- Save-State Discovery
- Launch Content Resolution
- Session-spezifische Emulationsadapter

### 5.5 `bitarchive-platform`

Enthält plattformspezifische Implementierungen:

- Filesystem
- ProcessController
- ControllerService
- SecretStore
- AppPaths
- AppInstaller
- Notifications
- Window Activation
- VolumeMonitor / SourceAvailabilityEvents
- Volume / Source Availability Events

Unter macOS insbesondere Keychain- und Prozessintegration.

### 5.6 `bitarchive-ui`

Enthält:

- Slint-Komponenten
- Designsystem
- ViewModels
- Presentation Controller
- Navigation
- Focus-/Controller-Navigation
- Localization Adapter

### 5.7 `bitarchive-desktop`

Composition Root und Desktop Entry Point.

Es verbindet alle konkreten Implementierungen.

---

## 6. Dependency Injection und Bootstrap

BitArchive verwendet keine DI-Frameworks und keine Service Locator.

Abhängigkeiten werden explizit über Konstruktoren übergeben.

Beispiel:

```rust
pub struct LaunchService {
    releases: Arc<dyn ReleaseRepository>,
    firmware: Arc<FirmwareReadinessService>,
    backend: Arc<dyn EmulationBackend>,
    sessions: Arc<SessionManager>,
}
```

### 6.1 Startup-Reihenfolge

```text
1. AppPaths bestimmen
2. Logging initialisieren
3. Curated Catalog laden und validieren
4. SQLite öffnen
5. Migrationen ausführen
6. Settings laden
7. Platform Services initialisieren
8. Component Registry validieren
9. Recovery durchführen
   - unvollständige Jobs
   - Staging
   - aktive Session
10. Application Services erzeugen
11. Presentation Layer erzeugen
12. Slint UI starten
```

Die normale UI erhält erst Zugriff auf Repositories und Services, wenn Bootstrap, Migration und Recovery erfolgreich abgeschlossen sind.

---

## 7. Identitäten

Domain-Entitäten erhalten stabile UUIDv7-IDs.

Beispiele:

```text
GameId
ReleaseId
ContentId
LibrarySourceId
SaveStateId
SessionId
ScanRunId
ScrapeRunId
MediaAssetId
```

UUIDs werden in SQLite als 16-Byte-`BLOB` gespeichert.

Hashes sind niemals Primärschlüssel.

---

## 8. Persistenz

### 8.1 SQLite

SQLite ist die lokale Primärdatenbank.

Konfiguration:

- WAL Mode
- Foreign Keys aktiviert
- explizite Transaktionen
- Batch Writes bei Scan-/Scrape-Vorgängen
- versionierte Schema-Migrationen

### 8.2 rusqlite

`rusqlite` wird als dünne SQLite-Zugriffsschicht verwendet.

Asynchrone Application Tasks blockieren nicht den UI-Thread. Datenbankarbeit wird über kontrollierte Worker bzw. `spawn_blocking`/dedizierte DB-Ausführung abgewickelt.

### 8.3 Database Ownership

Es wird kein frei geteilter `Arc<Mutex<Connection>>` durch die Anwendung gereicht.

Empfohlen ist ein kontrollierter DB-Executor bzw. Repository-Pool mit klaren Transaktionsgrenzen.

Schreibintensive Pipeline-Schritte verwenden Batch-Transaktionen.

---

## 9. Datenbankmigrationen

Migrationen sind:

- sequenziell
- versioniert
- ausschließlich vorwärtsgerichtet

Beispiel:

```text
0001_initial.sql
0002_release_types.sql
0003_scrape_runs.sql
```

Migrationen dürfen SQL und Rust-basierte Datentransformationen kombinieren.

Es gibt keine generischen Down-Migrations.

Eine ältere BitArchive-Version darf eine inkompatibel neuere Datenbank nicht öffnen.

Der gleiche `MigrationRunner` wird bei Restore alter Backups verwendet.

---

## 10. Library Sources und Filesystem

### 10.1 Library Source

Library Sources sind stabile Domain-Entitäten.

```rust
pub struct LibrarySource {
    pub id: LibrarySourceId,
    pub display_name: String,
    pub location: SourceLocation,
    pub fixed_system: Option<SystemId>,
}
```

### 10.2 Content Location

Content wird primär relativ zu einer Source gespeichert:

```text
LibrarySource
└── /Volumes/RetroSSD/Nintendo/

Content
└── NES/Mario.nes
```

statt ausschließlich:

```text
/Volumes/RetroSSD/Nintendo/NES/Mario.nes
```

### 10.3 Source Availability

```rust
pub enum SourceAvailability {
    Available,
    Offline,
    PermissionDenied,
    Missing,
}
```

Eine offline Source führt nicht zur Löschung zugehöriger Spiele.

Source Availability wird beim Start und zusätzlich über plattformspezifische Volume-/Permission-Ereignisse aktualisiert, soweit die Plattform dies unterstützt.

Statusänderungen erzeugen `SourceAvailabilityChanged`-Events.

### 10.4 Plattformabstraktion

Filesystemzugriffe laufen über einen Port.

Desktop verwendet lokale Pfade. Eine spätere Android-Implementierung kann z. B. Document-Tree-Handles verwenden, ohne das Domainmodell zu ändern.

---

## 11. Hashing und Content Fingerprints

SHA-256 ist der kanonische Content-Hash.

Hashing erfolgt streaming-basiert.

Zur Vermeidung unnötiger Voll-Hashes dienen:

- Dateigröße
- mtime

als Cache-/Invalidierungsmerkmale.

Zusätzliche Hashes wie CRC32, MD5 oder SHA-1 werden nur berechnet, wenn externe Provider oder Referenzdaten sie erfordern.

### 11.1 ZIP

Bei ZIP zählt der Hash des enthaltenen spielbaren ROM-Inhalts, nicht des Containers.

### 11.2 Firmware

Firmware wird immer anhand echter Content-Hashes verifiziert.

---

## 12. Library Scan Pipeline

Library Scans sind cancellable und mehrstufig:

```text
Library Source
      │
      ▼
1. Discovery
      │
      ▼
2. Classification
      │
      ▼
3. Technical Validation
      │
      ▼
4. Fingerprint / Hash
      │
      ▼
5. Content Matching
      │
      ▼
6. Release Resolution
      │
      ▼
7. Persistence
      │
      ▼
8. Reconciliation
      │
      ▼
Scan Result
```

### 12.1 Parallelität

Teure Arbeit läuft mit begrenzter Parallelität.

Keine unbeschränkten `tokio::spawn()`-Aufrufe pro Datei.

Bounded Channels und Semaphoren begrenzen:

- Disk I/O
- Hashing
- Parsing

### 12.2 Reconciliation

Destruktive Reconciliation erfolgt nur nach einem erfolgreich abgeschlossenen vollständigen Scan.

Ein abgebrochener oder fehlgeschlagener Scan darf keine Inhalte als entfernt markieren.

### 12.3 Scan Runs

Persistente Scan Runs enthalten:

- ID
- Source
- Startzeit
- Endzeit
- Status
- Ergebnisstatistik

Scanning und Metadata Scraping bleiben getrennte Subsysteme.

---

## 13. Game-, Release- und Content-Modell

### 13.1 Game

Repräsentiert das logische Werk.

### 13.2 Release

Repräsentiert eine konkrete Veröffentlichung / Variante.

Ein Game kann mehrere Releases besitzen.

### 13.3 Content

Repräsentiert indexierte physische oder containerinterne Inhalte.

Ein Release kann mehrere Content-Einträge besitzen.

### 13.4 Default Release Resolver

Priorität:

1. expliziter Game-Override
2. bevorzugte Spielsprache
3. bevorzugte Region
4. definierte Fallback-Regel

Diese Logik ist ein deterministischer, separat testbarer Resolver.

---

## 14. Content Locations und Launch Content

Indexierter Content und startbarer Content werden getrennt.

### 14.1 Content Location

```rust
pub enum ContentLocation {
    File {
        source_id: LibrarySourceId,
        relative_path: RelativePath,
    },
    ArchiveEntry {
        archive_id: ContentId,
        entry_path: String,
    },
}
```

### 14.2 LaunchContent

```rust
pub enum LaunchContent {
    File(ContentId),
    ArchiveEntry {
        archive: ContentId,
        entry: ArchiveEntryId,
    },
    ExistingPlaylist {
        playlist: ContentId,
    },
    ManagedPlaylist {
        path: PathBuf,
        members: Vec<ContentId>,
    },
}
```

### 14.3 Multi-Disc

Priorität:

1. gültige vorhandene `.m3u`
2. bekannte Multi-Disc-Struktur
3. von BitArchive verwaltete `.m3u`

Generierte Playlists liegen ausschließlich im BitArchive-Datenbereich und sind rebuildbar.

### 14.4 ZIP

Nur ZIPs mit genau einem eindeutig spielbaren Inhalt werden regulär unterstützt.

Kein dauerhaftes Entpacken.

---

## 15. Curated Catalog

BitArchive liefert einen versionierten Curated Catalog mit der App aus.

Beispiel:

```text
resources/catalog/
├── systems/
│   ├── nes.toml
│   ├── snes.toml
│   └── psx.toml
└── cores/
    ├── snes9x.toml
    └── mgba.toml
```

Der Catalog beschreibt:

- Systeme
- Hersteller
- Erscheinungsjahr
- Formate
- Detection Rules
- empfohlene Cores
- alternative Cores
- Capabilities
- Firmware-Anforderungen
- Launch-Regeln

Catalog-Daten sind datengetrieben und werden in CI validiert.

Der Curated Catalog ist fachlich strikt von Online-Distribution-Manifests getrennt.

---

## 16. Metadatenarchitektur

Metadaten sind provider-neutral.

### 16.1 Provider-Port

```rust
pub trait MetadataProvider {
    fn id(&self) -> ProviderId;

    async fn search(
        &self,
        query: MetadataQuery,
    ) -> Result<Vec<MetadataCandidate>, ProviderError>;

    async fn fetch(
        &self,
        candidate: &ProviderItemId,
    ) -> Result<ProviderMetadata, ProviderError>;
}
```

Im MVP:

```text
MetadataProvider
└── ScreenScraperProvider
```

### 16.2 Provider Values und Overrides

Provider-Werte und manuelle Overrides werden getrennt gespeichert.

Effektive Werte werden über einen `MetadataResolver` bestimmt.

Priorität:

```text
Manual Override
   ↓
Provider Value
   ↓
Local/Fallback Value
```

Overrides sind feldweise.

### 16.3 Provenance

Metadaten behalten ihre Herkunft:

```text
Manual
Provider(ScreenScraper)
Local
```

### 16.4 Scrape Runs

Scraping-Läufe sind persistente Jobs mit Status pro Spiel.

Sie unterstützen:

- Automatic
- Interactive
- Pause
- Resume
- Cancel

API-Limits und Parallelität werden vom Provider-Adapter respektiert.

---

## 17. ScreenScraper

ScreenScraper ist der einzige MVP-Metadatenprovider.

### 17.1 Rate Limiting

Der Adapter behandelt:

- erlaubte Parallelität
- API-Limits
- Retries
- Backoff
- Cancellation

Die Thread-/Parallelitätszahl ist kein User Setting.

### 17.2 Credentials

App-/Developer-Credentials und User-Credentials sind getrennt.

User-Secrets liegen unter macOS im Keychain.

Developer-Credentials, die clientseitig vorhanden sein müssen, werden nicht als wirklich geheim betrachtet.

Secrets erscheinen niemals in Logs oder Fehlermeldungen.

---

## 18. Managed Media Store

Gescrapte Medien werden nicht als beliebig löschbarer Cache behandelt.

### 18.1 Persistent Managed Media Store

```text
Application Support/BitArchive/media/
├── covers/
└── screenshots/
```

Speicherung content-addressed per SHA-256.

SQLite speichert nur Metadaten und Referenzen.

### 18.2 Transient Image Cache

Abgeleitete Thumbnails und Render-Caches liegen unter dem OS-Cache-Pfad und dürfen jederzeit neu erzeugt werden.

### 18.3 Garbage Collection

Nicht mehr referenzierte Media-Blobs werden ausschließlich über einen kontrollierten Garbage-Collection-Schritt entfernt.

---

## 19. Emulation Backend

Emulation ist backend-neutral.

```rust
pub trait EmulationBackend {
    fn id(&self) -> EmulationBackendId;

    fn check_readiness(
        &self,
        request: &LaunchRequest,
    ) -> Result<LaunchReadiness, EmulationError>;

    fn prepare_launch(
        &self,
        request: &LaunchRequest,
    ) -> Result<PreparedLaunch, EmulationError>;

    fn launch(
        &self,
        prepared: PreparedLaunch,
    ) -> Result<SessionHandle, EmulationError>;
}
```

MVP:

```text
EmulationBackend
└── RetroArchBackend
```

Spätere Standalone-Backends können denselben Port implementieren.

---

## 20. Launch Flow

```text
Play / Continue
      ↓
Release Resolution
      ↓
Core Resolution
      ↓
LaunchContent Resolution
      ↓
Firmware Readiness
      ↓
Config Resolution
      ↓
Core Option Resolution
      ↓
PreparedLaunch
      ↓
Process Spawn
      ↓
SessionManager
```

### 20.1 LaunchRequest

Enthält fachliche IDs und gewünschte Aktion.

### 20.2 PreparedLaunch

Enthält konkrete Infrastructure-Details:

- Executable
- Core
- Content
- Argumente
- Environment
- Working Directory
- Config Paths
- optionalen Save State

### 20.3 Kein Shell-Launch

RetroArch wird direkt über Prozess-APIs gestartet.

Keine Shell-Interpolation.

---

### 20.4 Core Resolution

Core-Auswahl ist deterministisch:

```text
System Default
  ↓
Game Override
  ↓
Release Override (optional)
```

Der spezifischste vorhandene Wert gewinnt.

Es gibt keinen globalen Core-Default.

Release-spezifische Core Overrides sind ausschließlich Teil der Core Resolution. RetroArch Settings und Core Options bleiben im MVP auf ihren eigenen `Global/System/Game`- bzw. `Core Defaults/System/Game`-Scopes.

Der Resolver ist separat testbar und liefert neben dem effektiven Core auch dessen Quelle (`System`, `Game`, `Release`).

---

## 21. Launch Readiness

Readiness wird strukturiert modelliert.

```rust
pub enum ReadinessIssue {
    CoreMissing,
    CoreIntegrityFailure,
    RuntimeUnavailable,

    FirmwareMissing,
    FirmwareWrongContent,
    FirmwareWrongFilename,

    ContentUnavailable,
    SourceOffline,
    SourcePermissionDenied,

    UnsupportedFormat,
    InvalidContent,

    SessionActive,
}
```

Die Backend-/Application-Schicht liefert Zustände und Recovery-Metadaten. Die UI entscheidet über Darstellung und konkrete Aktionen.

Readiness wird vor `Play` und `Continue` geprüft.

Eine offline Source oder fehlende Berechtigung verändert niemals die fachliche Identität des Games und entfernt es nicht aus der Library.

## 22. Session Management

BitArchive erlaubt genau eine aktive Emulationssession.

Ein separater `SessionManager` verantwortet:

- Exklusivität
- Session-Persistenz
- Playtime
- Startzeit
- Exit
- Prozessstatus
- Recovery

### 22.1 Process Identity

Nicht nur PID, sondern mindestens:

- PID
- Prozessstartzeit
- Executable Identity

werden für Recovery genutzt.

### 22.2 Recovery

Beim App-Start:

```text
persistierte Session vorhanden?
        ↓
Prozessidentität prüfen
        ↓
läuft noch?
  ├── ja → Session wieder aufnehmen
  └── nein → Session sauber finalisieren
```

### 22.3 Beenden

Zuerst geordnetes Beenden.

Force Kill nur nach Fehler und expliziter Nutzeraktion.

---

## 23. Runtime- und Core-Management

RetroArch-Runtime und Cores sind versionierte immutable Komponenten.

### 23.1 Component Store

```text
components/
├── runtime/
│   ├── <version>/
│   └── ...
└── cores/
    └── <core-id>/
        ├── <version>/
        └── ...
```

Die Plattform steht unterhalb der Komponentenklasse, damit eine spätere Plattform
keine Installation einer anderen stillschweigend mitbenutzt und `cores/<core-id>/`
davon unberührt bleibt:

```text
components/
├── runtime/<platform>/<version>/
└── cores/<core-id>/<version>/
```

Welche Runtime-Version aktiv ist, steht pro Runtime in einer kleinen Registry
(`components/runtime/<id>/active`). Sie zeigt ausschließlich auf eine installierte
Version; die Menge der installierten Versionen ergibt sich aus den
Versionsverzeichnissen.

### 23.2 Update Flow

Vor der Nutzerbestätigung wird ein `ComponentUpdateImpact` ermittelt.

Für Core-Updates enthält dieser mindestens:

- Anzahl vorhandener Save States, die mit der neuen Core-Version zu `VersionMismatch` werden
- Core-Option-Overrides, die gegen das neue Schema ungültig werden könnten
- Verfügbarkeit der vorherigen Core-Version für Rollback

Danach:

```text
Manifest laden
    ↓
Signatur prüfen
    ↓
Artefakt herunterladen
    ↓
SHA-256 prüfen
    ↓
Staging
    ↓
Validierung
    ↓
atomare Installation
    ↓
Aktivierung
```

Aktive Versionen werden nicht in-place überschrieben.

Staging liegt innerhalb des Component Stores, damit die Installation ein Rename
innerhalb eines Dateisystems und damit tatsächlich atomar ist. Der
Staging-Bereich im Cache-Verzeichnis (§35) bleibt für Arbeit, die nicht atomar
mit dem Store sein muss. Staging ist auf allen Pfaden transient und wird nach
Erfolg wie nach Fehler entfernt.

### 23.3 Rollback

Standardmäßig werden behalten:

- active
- previous

Damit ist ein lokaler Ein-Schritt-Rollback möglich.

### 23.4 Gemeinsame Infrastruktur

Runtime und Cores teilen:

- Download
- Verify
- Stage
- Install
- Activate
- Rollback

bleiben aber fachlich getrennte Komponententypen.

---

### 23.5 Core Selection Overrides

Core-Auswahl ist getrennt von RetroArch Settings und Core Options zu persistieren.

Scopes:

```text
System
Game
Release (optional)
```

Ein Release Override darf nur existieren, wenn der Core mit System und Content kompatibel ist.

Der effektive Core wird ausschließlich über den Core Resolver bestimmt.

---

## 24. Signierte Distribution-Manifests

Runtime-/Core-Manifeste sind signiert.

Empfohlener Signaturalgorithmus:

```text
Ed25519
```

Manifest und Signatur werden getrennt verteilt.

Der öffentliche Verifikationsschlüssel liegt in BitArchive.

Der private Signaturschlüssel liegt ausschließlich in der Release-Infrastruktur.

Artefakte werden zusätzlich per SHA-256 verifiziert.

---

## 25. Firmware

Es existiert im MVP genau ein zentraler Firmware-Ordner.

### 25.1 Firmware Index

BitArchive indexiert:

- relativen Pfad
- Dateiname
- Größe
- mtime
- SHA-256

### 25.2 Core Requirements

Firmware-Anforderungen hängen am Core.

```rust
pub struct FirmwareRequirement {
    pub accepted_hashes: Vec<Sha256Digest>,
    pub expected_filenames: Vec<String>,
    pub requirement: RequirementLevel,
}
```

### 25.3 Readiness

```text
Ready
CorrectContentWrongFilename
WrongContent
Missing
```

Readiness wird dynamisch aus Firmware Index und Core-Anforderungen abgeleitet.

BitArchive benennt oder kopiert Firmware niemals automatisch.

---

## 26. RetroArch Settings

RetroArch-Settings sind strukturierte, typisierte Daten.

Keine `.cfg` ist Source of Truth.

### 26.1 Vererbung

```text
Global
  ↓
System
  ↓
Game
```

Nur explizite Overrides ersetzen geerbte Werte.

### 26.2 Typed Settings

```rust
pub enum SettingValue {
    Bool(bool),
    Integer(i64),
    Float(f64),
    Enum(String),
    String(String),
}
```

Eine `SettingDefinition` beschreibt Typ, Default und unterstützte Scopes.

---

## 27. Core Options

Core Options sind eine separate Konfigurationsdomäne.

Vererbung:

```text
Core Defaults
  ↓
System
  ↓
Game
```

### 27.1 CoreOptionSchema

Core Options gehören zur konkreten Core-Version.

```rust
pub struct CoreOptionSchema {
    pub core_id: CoreId,
    pub core_version: Version,
    pub options: Vec<CoreOptionDefinition>,
}
```

### 27.2 Core Introspection

Eine `CoreIntrospector`-Abstraktion ermittelt die tatsächlich angebotenen Optionen einer installierten Core-Version.

Introspection läuft nach Installation/Aktivierung einer Core-Version, nicht erst beim Öffnen der Settings.

Die technische Implementierung kann über einen kleinen Libretro-Host oder eine kontrollierte RetroArch-Introspection erfolgen. Diese Entscheidung ist intern austauschbar.

### 27.3 Override Validation

Gespeicherte Overrides referenzieren technische Option Keys.

Nach Core-Updates werden Overrides gegen das neue Schema validiert.

Ungültig gewordene Overrides werden:

- nicht gelöscht
- nicht angewendet
- als Warnung markiert

---

## 28. Generierte Launch-Konfiguration

Pro Session:

```text
sessions/<session-id>/
├── retroarch.cfg
├── core-options.cfg
├── launch.json
├── stdout.log
├── stderr.log
└── content/
```

Diese Dateien sind Session-Artefakte und nicht persistent fachliche Wahrheit.

---

## 29. Save States

RetroArch bleibt technische Quelle aller Save-State-Dateien.

BitArchive indexiert sie.

### 29.1 Zuordnung

Save States gehören zu:

```text
Release
+ Core
+ Core-Version
```

### 29.2 Discovery

Discovery erfolgt:

- beim App-Start
- nach Ende einer Emulationssession
- bei gezieltem Refresh / Game Detail

Kein permanenter File Watcher im MVP.

### 29.3 Compatibility Resolver

```text
same core + same version
→ Compatible

same core + different version
→ VersionMismatch

different core
→ IncompatibleCore
```

### 29.4 Load Policy und Continue

Load Policy:

```text
Compatible
→ Load erlaubt

VersionMismatch
→ Load nur nach expliziter Nutzerbestätigung

IncompatibleCore
→ Load blockiert
```

`Continue` wählt ausschließlich den neuesten `Compatible` State für aktives Release, aktiven Core und aktive Core-Version.

Die Standard-Timeline einer Game-Ansicht fragt States des aktiven Releases ab; States anderer Releases können separat gezählt bzw. angezeigt werden.

### 29.5 Namen und Thumbnails

Anzeigenamen liegen nur in BitArchive.

Physische State-Dateien werden für Anzeigenamen nicht umbenannt.

Thumbnails bleiben technisch getrennte Dateien.

### 29.6 Delete

Beim bestätigten Löschen:

1. tatsächliche State-Datei löschen
2. Thumbnail löschen, falls vorhanden
3. DB-Index aktualisieren

DB-Einträge werden nicht entfernt, wenn die Dateisystemoperation fehlgeschlagen ist.

---

## 30. Background Jobs

Tokio ist die zentrale Async Runtime.

Ein Application-Level `JobManager` koordiniert:

- Scans
- Scraping
- Downloads
- Component Installation
- Firmware Scan
- Backup
- Restore

### 30.1 Job State

```text
Queued
Running
AwaitingInput
Paused
Completed
Cancelled
Failed
```

Nicht jeder Job unterstützt jeden Zustand.

### 30.2 Cancellation

Cancellation erfolgt über `CancellationToken`.

### 30.3 Resource Limits

Zentrale Limits existieren für:

- Disk I/O
- Hashing
- HTTP
- Downloads

Keine unbeschränkten Worker.

### 30.4 Persistenz

Nur Jobs mit Resume-/Recovery-Bedarf werden persistiert.

Recovery erfolgt fachlich pro Job-Typ und nicht durch Versuch, Futures wiederzubeleben.

---

## 31. Netzwerk

Ein zentraler Network Layer kapselt HTTP.

`reqwest` ist die empfohlene Implementierung.

Der Layer behandelt:

- TLS
- Timeouts
- User-Agent
- Retries
- Backoff
- Cancellation
- Rate-Limits
- Redaction sensibler Header

Kein Modul erzeugt unkontrolliert eigene HTTP-Clients.

---

## 32. Secret Management

Ein plattformneutraler `SecretStore` kapselt Secrets.

macOS:

```text
Keychain
```

später Linux:

```text
Secret Service / libsecret
```

SQLite speichert keine Passwörter oder Tokens im Klartext.

Secret-Typen redigieren sich in `Debug`/Logging automatisch.

---

## 33. Logging und Diagnose

BitArchive verwendet `tracing`.

Log-Level:

- ERROR
- WARN
- INFO
- DEBUG
- TRACE

Logs bleiben lokal und rotieren.

### 33.1 Fehlerarchitektur

Drei Ebenen:

```text
User-facing error
    ↓
Application/domain error
    ↓
Technical cause
```

Domain- und Application-Fehler sind typisiert.

### 33.2 Launch Diagnose

Pro Session können gespeichert werden:

- Core
- Core-Version
- Content
- Runtime-Version
- Startzeit
- Prozessstatus
- Exit-Code
- relevante RetroArch-Ausgaben

Keine automatische Übertragung.

---

## 34. Backup und Restore

BitArchive verwendet ein eigenes versioniertes Backupformat.

Beispiel:

```text
backup.bitarchive-backup
├── manifest.json
├── database.sqlite
├── settings/
└── save-states/       # optional
```

Das Manifest enthält mindestens:

- Backup Format Version
- Database Schema Version
- BitArchive Version
- Erstellungszeit
- Save-State-Inclusion

### 34.1 SQLite Snapshot

Backups verwenden die SQLite Backup API bzw. einen konsistenten DB-Snapshot, nicht einfach eine Kopie der laufenden WAL-DB.

### 34.2 Restore

```text
Backup öffnen
   ↓
Manifest validieren
   ↓
Staging
   ↓
DB-Integrität prüfen
   ↓
Migration
   ↓
Restore validieren
   ↓
atomarer Austausch
```

Vor Restore wird intern ein Safety Snapshot des aktuellen Zustands erstellt.

Nach erfolgreichem atomarem Austausch wird kein bestehender Repository-/Service-State weiterverwendet. Die Anwendung fordert einen kontrollierten Neustart an und durchläuft anschließend den normalen Bootstrap inklusive Migration, Recovery und Service-Aufbau.

Library Sources dürfen nach Restore offline sein.

### 34.3 Nicht im Backup

- ROMs / ISOs
- BIOS / Firmware
- Runtime-Binaries
- Core-Binaries
- normale RetroArch-Saves
- rebuildbare Media Assets

Save States sind optional.

---

## 35. App-Daten und Lifecycle

Ein zentraler `AppPaths`-Service bestimmt Plattformpfade.

macOS:

```text
~/Library/Application Support/BitArchive/
├── database/
├── components/
├── media/
├── generated/
├── sessions/
├── logs/
└── state/
```

Caches:

```text
~/Library/Caches/BitArchive/
├── downloads/
├── staging/
├── extracted/
└── images/
```

### 35.1 Artifact Lifetimes

```text
Persistent
Rebuildable
SessionScoped
Temporary
```

Jedes interne Artefakt erhält einen definierten Lifecycle.

Startup Cleanup entfernt ausschließlich eindeutig BitArchive-eigene temporäre oder abgelaufene Daten.

---

## 36. Search, Filter und Library Queries

SQLite führt Suche, Filterung und Sortierung aus.

Die Query-Schicht ist **view-unabhängig**. Cover-Rail und ein möglicher späterer Grid View verwenden denselben Vertrag.

### 36.1 Typed Query

```rust
pub struct LibraryQuery {
    pub search: Option<String>,
    pub system: Option<SystemId>,
    pub favorite: Option<bool>,
    pub release_type: Option<ReleaseType>,
    pub sort: LibrarySort,
}
```

MVP:

```rust
pub enum LibrarySort {
    Title,
    RecentlyPlayed,
    Playtime,
    ReleaseDate,
}
```

Weitere Filter und Sortierungen werden später ergänzt.

### 36.2 View-independent List Contract

Library Views benötigen mindestens Operationen mit folgender Semantik:

```text
count(query)
window(query, anchor_game_id, before, after)
index_of(query, game_id)
sections(query)
```

`window(...)` liefert ein stabiles Fenster um einen fachlichen `GameId`.

`index_of(...)` lokalisiert ein Game unter aktueller Sortierung und Filterung.

`sections(...)` liefert Jump-Ziele passend zur Sortierung, z. B.:

- Buchstaben bei `Title`
- Jahre bei `ReleaseDate`
- Zeitgruppen bei `RecentlyPlayed`

PageNext/PagePrevious werden von der View semantisch auf diesem Vertrag aufgebaut und sind keine Datenbank-Offset-Identität.

### 36.3 Stable Ordering

Jede Sortierung besitzt einen deterministischen Tie-Breaker über stabile IDs.

Fokus wird immer als `GameId` gespeichert, niemals als Listenindex.

Damit bleiben Fokus und Navigation robust bei:

- Scan-Reconciliation
- Scraping-bedingten Titeländerungen
- Hide / Unhide
- Favoritenänderungen
- Wechsel zwischen Rail und zukünftigem Grid

### 36.4 FTS5

Titelsuche verwendet SQLite FTS5.

Der Index verwendet den effektiven Titel inklusive manueller Overrides.

Search-Ergebnisse liefern stabile `GameId`s und die zugehörige Systemidentität.

### 36.5 Query DTOs

Library-Ansichten laden schlanke DTOs statt vollständiger Domain-Aggregate.

```text
GameSummary
├── id
├── title
├── system
├── cover
├── favorite
└── lastPlayedAt
```

Game Details und Launch Readiness werden bedarfsgerecht geladen, insbesondere für den fokussierten Eintrag.

### 36.6 Views sind Queries

- All Games
- Favorites
- Recently Played
- System Views

sind Query-Definitionen und keine separat synchronisierten Collections.

## 37. UI und Presentation

Slint ist reine Presentation.

Slint-Komponenten greifen nie direkt auf:

- SQLite
- Repositories
- HTTP
- Scanner
- RetroArch
- Platform APIs

zu.

### 37.1 ViewModels

Domain-Entitäten werden in dedizierte ViewModels übersetzt.

### 37.2 Routes

```rust
pub enum LibraryScope {
    System(SystemId),
    AllGames,
    Favorites,
    RecentlyPlayed,
}

pub enum Route {
    Home,
    Browse(LibraryScope),
    GameInfo(GameId),
    ManageLibrary(ManageLibrarySection),
    Settings(SettingsSection),
}
```

`Home` ist die Root-Einstiegsebene mit Built-in-Libraries und Systemen.

### 37.3 Navigation Stack

Navigation wird als Stack von Einträgen mit Route und View-State modelliert.

Konzeptionell:

```rust
pub struct NavigationEntry {
    pub route: Route,
    pub state: ViewStateSnapshot,
}
```

Ein Browse-Snapshot enthält mindestens:

- LibraryScope
- fokussierten `GameId`
- Query / Sort / Filter
- optionalen View-spezifischen Anchor State

Regeln:

- `Back` im Frontend folgt History
- Lateral Switching zwischen Systemen ersetzt den aktuellen Browse-Eintrag statt einen neuen History-Eintrag zu pushen
- Deep Links aus dem Frontend nach Manage Library tragen einen `ReturnContext`
- Back von der Management-Einstiegsseite restauriert diesen Kontext
- globale Wechsel zwischen Management-Zielen erzeugen keine rekursiven Stack-Schleifen

### 37.4 Per-Library Remembered State

Für System- und Built-in-Library-Scopes wird der letzte gültige Browse-State separat gespeichert.

Fokusidentität ist `GameId`, nie Index.

### 37.5 Overlay State

Modale / transient globale Zustände werden zentral modelliert:

- Global Menu
- Search
- Context Options
- Save States
- Confirmation
- Error Details
- Release Picker
- Scrape Match Picker
- Running Session
- Activity

Overlays speichern den vorherigen Fokus und stellen ihn beim Schließen wieder her.

### 37.6 UI Thread

Slint bleibt auf seinem UI-Thread.

Background Tasks mutieren niemals direkt Slint-State.

Application Events werden kontrolliert auf den Slint Event Loop gemarshallt.

## 38. Controller und Input

App-Navigation und Emulator-Input sind strikt getrennt.

### 38.1 NavigationIntent

```rust
pub enum NavigationIntent {
    Up,
    Down,
    Left,
    Right,
    Primary,
    Back,
    PageNext,
    PagePrevious,
    SectionNext,
    SectionPrevious,
}
```

Keyboard, D-Pad und Analog-Stick werden auf dieselben Navigation Intents abgebildet.

### 38.2 AppAction

Kontextuelle und globale Aktionen werden getrennt von Directional Navigation abstrahiert.

```rust
pub enum AppAction {
    Options,
    GlobalMenu,
    Search,

    Continue,
    Favorite,
    Info,
    SaveStates,
}
```

Nicht jede `AppAction` benötigt auf jedem Gerät eine eigene physische Taste.

Gerätespezifische Mapper übersetzen Controller, Keyboard und Mouse in `NavigationIntent` und `AppAction`.

### 38.3 Controller Service

```rust
pub trait ControllerService {
    fn devices(&self) -> Vec<ControllerDevice>;
    fn subscribe(&self) -> ControllerEventStream;
}
```

Controller Hot-Plug wird unterstützt.

### 38.4 Foreground Input Ownership

BitArchive konsumiert App-Navigationsinput nur, wenn:

- das BitArchive-Fenster den aktiven/key Fokus besitzt, und
- keine foreground RetroArch-Session den Input-Kontext besitzt

Während RetroArch im Vordergrund läuft, dürfen Controller-Ereignisse nicht gleichzeitig Navigation oder Aktionen in BitArchive auslösen.

### 38.5 RetroArch Input

BitArchive ist kein Input-Proxy.

RetroArch erhält Emulationsinput direkt vom Betriebssystem.

Button Remapping und Emulations-Hotkeys bleiben RetroArch-Aufgabe.

### 38.6 Controller Preferences

Globale und systemspezifische Präferenzen speichern eine persistierbare Geräte-Matching-Signatur, keine flüchtige Controller-Nummer.

## 39. Internationalisierung

UI-Texte werden nicht direkt im Slint-Markup fest codiert.

Verwendet werden stabile Message IDs.

Beispiel:

```text
home.continue
game.play
settings.language
```

Empfohlenes Nachrichtenformat:

```text
Fluent
```

MVP-Sprachen:

- Deutsch
- Englisch

### 39.1 Getrennte Spracheinstellungen

Getrennt bleiben:

- UI Language
- Metadata Preferred Language
- Metadata Fallback Language
- Preferred Game Languages

Datums-, Zeit- und Zahlenformatierung erfolgt im Presentation Layer.

---

## 40. Accessibility und Designsystem

Accessibility ist Teil des Designsystems.

### 40.1 Design Tokens

```text
Typography
Spacing
Sizes
Radius
Motion
Focus
Semantic Colors
```

Light/Dark Mode verwendet dieselben semantischen Tokens.

### 40.2 Focus

Jede interaktive Komponente besitzt einen klar sichtbaren Focus State.

Keyboard- und Controller-Fokus teilen dieselbe semantische Fokuslogik.

Hover ist nicht gleich Focus.

### 40.3 Reduced Motion

Systemeinstellung wird respektiert.

Animationen werden bei Reduced Motion reduziert oder deaktiviert.

### 40.4 Semantische Labels

Icon-only Controls erhalten Accessibility Labels.

Zustände werden nie ausschließlich über Farbe kommuniziert.

---

## 41. Notifications

Systembenachrichtigungen werden über einen `NotificationService` abstrahiert.

Sie sind:

- sparsam
- abschaltbar
- ausschließlich für ausdrücklich geeignete Ereignisse

Keine Notifications für normale Alltagsaktionen.

---

## 42. App Updates

App Updates bleiben vollständig getrennt von Runtime-/Core-Updates.

### 42.1 Update Manifest

App-Update-Manifeste werden signiert.

Artefakte werden per SHA-256 verifiziert.

Updates werden angezeigt, aber nicht still installiert.

### 42.2 AppInstaller

Plattformabhängige Installation liegt hinter:

```rust
pub trait AppInstaller {
    fn install(&self, artifact: VerifiedAppArtifact) -> Result<()>;
}
```

MVP:

```text
MacOsAppInstaller
```

später:

```text
LinuxAppInstaller
```

---

## 43. macOS Packaging und Security Model

Distribution erfolgt außerhalb des Mac App Store.

MVP:

- Developer ID Signing
- Hardened Runtime
- Apple Notarization
- stapled Notarization Ticket
- keine App Sandbox

Mutable Daten liegen nie im `.app` Bundle.

Die gebündelte RetroArch-Runtime wird korrekt signiert.

Notwendige Hardened-Runtime-Entitlements werden minimal gehalten.

Signing und Notarisierung erfolgen ausschließlich in der CI-/Release-Pipeline.

---

## 44. Bootstrap Runtime

BitArchive kann eine getestete RetroArch-Version mit dem App-Paket ausliefern.

Diese dient nur als Bootstrap.

Beim ersten Start wird sie in den normalen Component Store übernommen bzw. als installierte Runtime registriert.

Danach existiert kein Sonderpfad zwischen:

- bundled Runtime
- downloaded Runtime

Beide werden durch denselben RuntimeManager verwaltet.

---

## 45. Testing

### 45.1 Unit Tests

Für:

- Default Release Resolution
- Metadata Resolution
- Config Inheritance
- Core Option Inheritance
- Firmware Readiness
- Save-State Compatibility
- Launch Readiness
- Content Matching

### 45.2 Integration Tests

Mit temporären:

- Libraries
- Firmware-Verzeichnissen
- App-Datenverzeichnissen
- SQLite-Datenbanken

### 45.3 Fixtures / Golden Tests

```text
fixtures/
├── cue/
├── m3u/
├── zip/
├── manifests/
├── configs/
└── save-state-layouts/
```

### 45.4 Network Tests

Normale CI-Tests verwenden:

- Fake Provider
- Mock HTTP Server
- Fixtures

Keine echte ScreenScraper-Abhängigkeit.

### 45.5 Process Tests

Prozesssteuerung wird standardmäßig mit Fake Process Adapters getestet.

Echte RetroArch Smoke Tests laufen separat in Release Validation.

### 45.6 Migration Tests

Ältere Schema-Versionen werden gegen den aktuellen MigrationRunner getestet.

### 45.7 Backup Roundtrip

```text
State
 → Backup
 → neue leere Umgebung
 → Restore
 → Assertions
```

### 45.8 Property Tests

Geeignet für:

- Pfadnormalisierung
- Config Merge
- Release Resolver
- Parser
- Matching
- Invarianten

---

## 46. CI / Release Pipeline

CI übernimmt mindestens:

1. Formatierung
2. Clippy
3. Unit Tests
4. Integration Tests
5. Catalog Validation
6. Migration Tests
7. Build
8. RetroArch Runtime Smoke Test
9. Packaging
10. Nested Binary Signing
11. App Signing
12. Notarization
13. Stapling
14. Artifact Hashing
15. Manifest Generation
16. Manifest Signing

Signing-Secrets liegen ausschließlich in der Release-Infrastruktur.

---

## 47. Eventing

Application Services kommunizieren UI-relevante Änderungen über strukturierte Events.

Kein globaler untypisierter Event Bus.

Beispiele:

```text
LibraryScanProgress
ScrapeProgress
SessionStarted
SessionEnded
SourceAvailabilityChanged
ComponentInstalled
ControllerConnected
```

Events werden nur dort eingesetzt, wo lose Kopplung sinnvoll ist.

Direkte Use-Case-Aufrufe bleiben bevorzugt für Request/Response-Operationen.

---

## 48. Concurrency Regeln

- Slint UI Thread blockiert niemals auf I/O.
- SQLite-Schreibvorgänge besitzen kontrollierte Transaktionsgrenzen.
- Langlaufende Blocking-I/O-Aufgaben werden nicht direkt auf Tokio-Core-Threads ausgeführt.
- Netzwerkzugriff ist async.
- Hashing und Dateiverarbeitung sind begrenzt parallel.
- Shared mutable state wird minimiert.
- `Arc<Mutex<T>>` ist keine Standardlösung.
- Services besitzen klare Ownership.

---

## 49. Reset

### 49.1 Library Rebuild

Zurückgesetzt werden:

- Library Index
- gescrapte Zuordnungen gemäß Produktverhalten
- abhängige rebuildbare Library-Daten

Danach werden Sources erneut gescannt.

Originaldateien bleiben unangetastet.

### 49.2 Full Reset

Entfernt BitArchive-eigene lokale Daten entsprechend definierter Lifecycle-Regeln.

Nie entfernt werden:

- ROMs
- ISOs
- BIOS/Firmware aus externen Sources

---

## 50. Architekturentscheidungen – Kurzfassung

| # | Entscheidung |
|---|---|
| 1 | Rust + Slint |
| 2 | Cargo Workspace mit wenigen größeren Crates |
| 3 | SQLite + rusqlite + Repository Layer |
| 4 | UUIDv7 für Domain-Identitäten |
| 5 | SHA-256 als kanonischer Content-Hash |
| 6 | Library Sources + relative Content Locations + Platform FS |
| 7 | mehrstufige cancellable Scan Pipeline |
| 8 | provider-neutrale feldweise Metadaten + Overrides |
| 9 | backend-neutrale Emulation + RetroArch MVP Backend |
| 10 | immutable versioniertes Runtime/Core Component Management |
| 11 | zentraler Firmware Index, Requirements pro Core |
| 12 | strukturierte Config Engine, generierte cfg nur Output |
| 13 | Save-State Index über RetroArch-Dateien |
| 14 | Tokio + Application JobManager |
| 15 | zentraler Network Layer + SecretStore |
| 16 | versioniertes lokales Backupformat |
| 17 | forward-only Database Migrations |
| 18 | typisierte Fehler + tracing |
| 19 | Slint Presentation + ViewModels + NavigationIntent |
| 20 | signiertes/notarisiertes macOS Packaging |
| 21 | mehrstufige Teststrategie |
| 22 | versionierter Curated Catalog |
| 23 | explizite constructor-basierte DI |
| 24 | App Input getrennt von Emulator Input |
| 25 | Content und LaunchContent getrennt |
| 26 | macOS Hardened Runtime ohne App Sandbox |
| 27 | zentraler AppPaths-/Artifact-Lifecycle |
| 28 | SQLite FTS5 + datenbankseitige Library Queries |
| 29 | Managed Media Store + transienter Image Cache |
| 30 | zentrale I18n-/Accessibility-Architektur |
| 31 | versionsbezogene CoreOptionSchema-Introspection |
| 32 | Core Resolution: System → Game → optional Release |
| 33 | NavigationStack + ViewStateSnapshots + ReturnContext |
| 34 | view-unabhängiger Library-List-Contract mit GameId-Ankern |
| 35 | NavigationIntent + AppAction als semantische Input-Schicht |

---

## 51. Bewusst nicht im MVP

Die Architektur soll diese Punkte später ermöglichen, implementiert sie aber nicht im MVP:

- Linux
- Android
- Windows
- Standalone-Emulator-Backends
- Custom Systems
- Cloud Sync
- Benutzerkonten
- RetroAchievements
- Big-Picture-/TV-Modus
- frei austauschbare Themes
- vollständiges Controller-Remapping
- SRAM-/Memory-Card-Verwaltung
- ROM-Reorganisation
- ROM-Konvertierung
- 7z
- Telemetrie

---

## 52. Erweiterbarkeit nach dem MVP

### 52.1 Linux

Erfordert primär neue Implementierungen für:

- AppPaths
- SecretStore
- ControllerService
- ProcessController
- AppInstaller
- Notifications
- Packaging

Domain und Application bleiben unverändert.

### 52.2 Android

Eine spätere Android-Portierung ist konzeptionell möglich.

Voraussichtlich plattformspezifisch:

- Storage Access
- Lifecycle
- Emulator Integration
- Controller Integration
- App Distribution
- Background Execution

Domain-, Library-, Metadata-, Config- und große Teile der Application-Logik sollen wiederverwendbar bleiben.

### 52.3 Weitere Emulations-Backends

Neue Backends implementieren `EmulationBackend`.

Library, Game/Release-Modell und Metadatenarchitektur bleiben unverändert.

---

## 53. Architektur-Invarianten

Folgende Regeln dürfen bei zukünftiger Entwicklung nicht stillschweigend gebrochen werden:

1. `Game != Release != Content`.
2. Pfade sind keine fachlichen IDs.
3. Hashes sind keine Primärschlüssel.
4. Original-ROMs und BIOS-Dateien sind externe Ressourcen.
5. Slint ruft keine Infrastructure direkt auf.
6. Domain kennt keine SQLite-, Slint- oder OS-Typen.
7. Generated Config ist niemals Source of Truth.
8. Emulation bleibt backend-neutral.
9. Firmware Requirements hängen am Core.
10. Runtime und Core sind getrennt versionierte Komponenten.
11. App-Updates aktualisieren Runtime/Cores niemals implizit.
12. Save States gehören zu Release + Core + Core-Version.
13. Normale RetroArch-Saves bleiben außerhalb des BitArchive-Modells.
14. Scraping ist nicht Teil des Library-Scans.
15. Netzwerkzugriffe gehören zu expliziten Subsystemen.
16. Keine automatische Telemetrie oder Crash-Übertragung.
17. Background Tasks blockieren niemals den UI-Thread.
18. Destruktive Reconciliation erfolgt nur nach erfolgreichen Scans.
19. Ungültige User-Overrides werden nicht stillschweigend gelöscht.
20. Externe Nutzerdateien werden nie im Rahmen interner Cleanup-Routinen gelöscht.
21. Core Resolution folgt `System → Game → optional Release`; es gibt keinen globalen Core-Default.
22. Frontend-Fokus wird über stabile fachliche IDs und nicht über Listenindizes erhalten.
23. BitArchive konsumiert App-Navigationsinput nicht parallel zu einer foreground RetroArch-Session.

---

## 54. Nächster technischer Schritt

Nach dieser Architektur sollte als nächstes das konkrete Datenmodell abgeleitet werden.

Empfohlene Reihenfolge:

```text
ARCHITECTURE.md
      ↓
DATA_MODEL.md
      ↓
SQLite Schema v1
      ↓
Domain Types / Repository Ports
      ↓
MVP Implementation Plan
```

Das Datenmodell sollte insbesondere konkretisieren:

- Tabellen
- Foreign Keys
- Unique Constraints
- Game-/Release-/Content-Beziehungen
- Provider Metadata
- Manual Overrides
- FTS5
- Scan Runs
- Scrape Runs
- Firmware Index
- Components
- Config Overrides
- Core Option Schemas
- Save States
- Sessions
- Statistics
- Media Assets
- Schema-Versionierung

---

**Status:** MVP-Architektur definiert.  
**Nächster Schritt:** `DATA_MODEL.md` und daraus das initiale SQLite-Schema ableiten.
