# BitArchive – Product Specification v0.1

## 1. Produktvision

BitArchive ist eine moderne, lokale Verwaltungsbibliothek und ein Emulation-Frontend für Retrospiele. ROMs, ISOs und andere unterstützte Spielinhalte werden in einer zentralen Oberfläche indexiert, mit Metadaten angereichert und über eine von BitArchive verwaltete RetroArch-Runtime gestartet.

BitArchive soll sich nicht nur wie ein Launcher verhalten, sondern wie eine hochwertige Spielebibliothek mit klarer Trennung zwischen Spiel, Release, Content-Dateien, Emulationsprofilen und Save States.

Ein besonderer Fokus liegt auf einem schnellen Wiedereinstieg in laufende Spiele. Neben einem normalen `Play`-Flow werden vorhandene Save States als chronologische Continue-Historie dargestellt und können direkt geladen werden.

## 2. Produktprinzipien

### 2.1 Local-first

BitArchive funktioniert grundsätzlich lokal. Die Spielebibliothek, Metadaten, Einstellungen, Statistiken und Save-State-Verwaltung liegen lokal auf dem Gerät.

Netzwerkzugriffe erfolgen nur für klar definierte Funktionen wie:

- ScreenScraper
- BitArchive-Updates
- RetroArch-Runtime-Updates
- Core-Downloads und Core-Updates

Es gibt im MVP kein Benutzerkonto, kein Cloud-Backend und keine Telemetrie.

### 2.2 ROMs und ISOs bleiben Eigentum des Nutzers

BitArchive indexiert Spielinhalte ausschließlich. Es reorganisiert, verschiebt, benennt oder konvertiert ROMs und ISOs nicht automatisch.

BitArchive löscht niemals ROM-, ISO- oder BIOS-Dateien.

### 2.3 RetroArch ist Backend, nicht Benutzeroberfläche

RetroArch wird als Emulations-Runtime verwendet und von BitArchive verwaltet. Der Nutzer soll für den normalen Betrieb nicht selbst eine RetroArch-Installation pflegen müssen.

BitArchive abstrahiert RetroArch so weit wie sinnvoll und stellt zentrale Einstellungen, Cores, Firmware-Readiness und Save States direkt in der eigenen UI dar.

### 2.4 One-click Play und Continue

Der normale Launch-Flow soll möglichst einfach bleiben:

- `Play` startet das konfigurierte Release mit dem hinterlegten Core.
- `Continue` lädt den letzten kompatiblen Save State.

Es gibt keinen zusätzlichen „Starten mit …“-Dialog im normalen Game-UI.

### 2.5 Kontrollierte Kompatibilität

BitArchive verwendet getestete, freigegebene RetroArch- und Core-Versionen. Updates werden nicht automatisch installiert, sondern vom Nutzer bestätigt.

Downloads werden verifiziert. Runtime- und Core-Manifeste werden digital signiert und heruntergeladene Artefakte per SHA-256 geprüft.

## 3. Zielplattform

### MVP

- macOS first

### Später

- Linux
- Windows nachrangig

Die Architektur soll plattformneutral genug bleiben, um Linux später ergänzen zu können.

Portable Installationen sind nicht Bestandteil des MVP. Unter macOS verwendet BitArchive reguläre App-Datenpfade, beispielsweise unter `~/Library/Application Support/BitArchive/`.

## 4. Zielgruppe und Nutzungsszenario

BitArchive richtet sich an Nutzer mit eigener Retrospiele-Sammlung, die eine moderne, übersichtliche Oberfläche für Verwaltung und Emulation wünschen.

Typische Ziele:

- bestehende ROM-/ISO-Sammlungen indexieren
- Spiele nach System durchsuchen
- Metadaten ergänzen
- RetroArch und Cores nicht separat verwalten müssen
- BIOS-/Firmware-Probleme verständlich erkennen
- Spiele mit einem Klick starten
- schnell in vorhandene Save States zurückspringen
- RetroArch-Einstellungen auf globaler, System- und Spielebene konfigurieren

## 5. MVP-Scope

Der MVP soll einen Nutzer zuverlässig von einer leeren Installation zu einer vollständig nutzbaren Retro-Bibliothek führen.

Der MVP umfasst:

- macOS-App
- First-Run-Onboarding
- mehrere Library Sources
- Bibliothek nach Systemen
- Alle Spiele
- Favoriten
- Zuletzt gespielt
- zentrale Home-/Library-Einstiegsebene
- Game-Detail-Experience
- Game-/Release-Modell
- Multi-Disc-Support
- ZIP-Support
- ScreenScraper-Integration
- manuelle Metadatenbearbeitung
- lokal gecachte Cover und Screenshots
- verwaltete RetroArch-Runtime
- verwaltete Cores
- Core-Katalog
- BIOS-/Firmware-Readiness
- RetroArch-Konfiguration
- Core Options
- grundlegende Controller-Verwaltung
- Save-State-Verwaltung
- Save-State-Thumbnails
- Continue-Flow
- Spielzeit und Session-Tracking
- Launch-Diagnose
- App-/Runtime-/Core-Updates
- Rollback
- lokales Backup/Restore
- Light-/Dark-Mode
- Deutsch und Englisch
- Accessibility-Grundlagen

## 6. First-Run-Onboarding

Beim ersten Start führt BitArchive durch einen kurzen Einrichtungsassistenten.

### Schritte

1. UI-Sprache auswählen
2. bevorzugte Spielsprache auswählen
3. bevorzugte Release-Region auswählen
4. eine oder mehrere Library Sources hinzufügen
5. optional ein festes System pro Library Source zuweisen
6. zentralen BIOS-/Firmware-Ordner auswählen
7. gebündelte RetroArch-Runtime prüfen
8. empfohlene Cores für erkannte Systeme anbieten bzw. installieren
9. ersten Library-Scan ausführen
10. Home-/Library-Einstiegsebene anzeigen

ScreenScraper-Zugangsdaten und Scraping sind nicht verpflichtender Bestandteil des Onboardings.

## 7. Bibliothek und Library Sources

### 7.1 Mehrere Quellen

BitArchive unterstützt mehrere Library Sources gleichzeitig, auch über verschiedene Volumes hinweg.

Beispiele:

- `/Volumes/RetroSSD/Nintendo/`
- `/Volumes/RetroSSD/Sony/`
- `~/Games/Handhelds/`

### 7.2 Externe Volumes

Ist eine Library Source oder ein Volume vorübergehend nicht verfügbar, werden zugehörige Spiele nicht aus der Bibliothek gelöscht. Die Quelle wird als offline bzw. nicht verfügbar markiert.

### 7.3 Entfernen einer Library Source

Wird eine Library Source entfernt:

- bleiben Quelldateien unangetastet
- verschwinden Spiele, die ausschließlich aus dieser Source stammen, aus der aktiven Bibliothek
- Metadaten, Favoriten und Spielstatistiken bleiben zunächst erhalten
- bei späterem erneuten Hinzufügen können Inhalte wiedererkannt werden

### 7.4 Scanning

Im MVP gibt es:

- initialen Scan beim Hinzufügen einer Source
- manuellen Re-Scan
- keinen permanenten File Watcher

### 7.5 Systemerkennung

BitArchive nutzt eine hybride Systemerkennung aus:

- Dateiendung und Content-Format
- Ordnerstruktur
- bekannten Zuordnungen
- optional festem System pro Library Source

Unsichere Dateien werden nicht stillschweigend zugeordnet.

### 7.6 Verschobene oder umbenannte Dateien

Beim Re-Scan versucht BitArchive bekannte Inhalte über Hashes wiederzuerkennen. Wird derselbe Inhalt unter einem neuen Pfad gefunden, bleiben bestehende Metadaten, Favoriten und Spielstatistiken erhalten.

### 7.7 Duplikate

Identische Dateien werden über Hashes erkannt.

Mehrere physische Dateien dürfen auf dasselbe Release verweisen. In der normalen Bibliothek erscheint das Spiel nur einmal.

## 8. Game-, Release- und Content-Modell

BitArchive unterscheidet zwischen einem logischen Spiel und konkreten Releases.

### 8.1 Game

Ein `Game` repräsentiert das logische Werk.

Typische Game-Metadaten:

- Titel
- Beschreibung
- Genre
- Entwickler
- Publisher

### 8.2 Release

Ein `Release` repräsentiert eine konkrete Veröffentlichung bzw. Variante eines Spiels.

Typische Release-Metadaten:

- Release-Titel
- Region oder Regionen
- Sprache oder Sprachen
- Release-Datum
- Revision
- Release-Typ
- Content-Dateien

Ein Game kann mehrere Releases besitzen.

### 8.3 Release-Typen

Im MVP können Releases optional klassifiziert werden als:

- Official
- ROM Hack
- Fan Translation
- Homebrew
- Prototype
- Unlicensed

Ein komplexes Parent-/Patch-System ist nicht Bestandteil des MVP.

### 8.4 Regionen

Ein Release kann mehrere Regions-Tags besitzen.

Beispiele:

- Europe
- USA
- Japan
- World
- Asia
- Unknown

### 8.5 Sprachen

Ein Release kann mehrere Sprach-Tags besitzen, unabhängig von seiner Region.

### 8.6 Default-Release

BitArchive bestimmt automatisch ein bevorzugtes Release.

Priorität:

1. expliziter Default-Release des Spiels
2. bevorzugte Spielsprache
3. bevorzugte Region
4. definierte Fallback-Regel

Der Nutzer kann pro Spiel einen Default-Release überschreiben.

## 9. Multi-Disc-Spiele

Ein Release kann mehrere Content-Dateien besitzen.

Beispiel:

```text
Final Fantasy VII
└── USA Release
    ├── Disc 1.chd
    ├── Disc 2.chd
    ├── Disc 3.chd
    └── Final Fantasy VII.m3u
```

Vorhandene `.m3u`-Playlists werden bevorzugt als Launch-Content verwendet.

Ist keine `.m3u` vorhanden, darf BitArchive intern eine eigene verwaltete `.m3u` erzeugen. Die Originaldateien werden dabei nicht verändert.

Der Disc-Wechsel während einer laufenden Emulation bleibt im MVP vollständig RetroArch überlassen.

## 10. ZIP-Unterstützung

Im MVP wird ausschließlich `.zip` als Archivformat unterstützt.

Nicht im MVP:

- `.7z`
- andere Archivformate

Eine ZIP-Datei wird nur unterstützt, wenn sie genau einen eindeutig spielbaren Content enthält.

Archive mit mehreren ROMs gelten als mehrdeutig und werden nicht regulär importiert.

Für die Release-Identität zählt der Hash der enthaltenen ROM-Datei. Das ZIP selbst ist nur der Container.

BitArchive entpackt Archive nicht dauerhaft.

## 11. Dateivalidierung

Beim Library-Scan führt BitArchive grundlegende technische Plausibilitätsprüfungen durch.

Beispiele:

- Datei nicht lesbar
- ZIP leer oder beschädigt
- ungültige ZIP-Struktur
- fehlende zusammengehörige Dateien bei `BIN/CUE`

Eine vollständige ROM-Integritätsprüfung gegen Referenzdatenbanken ist nicht Bestandteil des MVP.

BitArchive führt keine Formatkonvertierung durch.

Insbesondere keine automatische Konvertierung wie:

- `BIN/CUE → CHD`

## 12. Nicht erkannte oder nicht unterstützte Dateien

Nicht erkannte oder nicht unterstützte Dateien werden im Scan-Ergebnis separat angezeigt.

Sie werden nicht als reguläre Spiele in die Bibliothek aufgenommen.

Beispiel:

```text
Scan abgeschlossen
842 Spiele importiert
12 Dateien nicht erkannt
4 Dateien nicht unterstützt
```

## 13. Offiziell unterstützte Systeme im MVP

BitArchive unterstützt im MVP eine bewusst begrenzte, kuratierte Systemliste.

Geplant:

- NES
- SNES
- Game Boy
- Game Boy Color
- Game Boy Advance
- Nintendo 64
- Sega Master System
- Mega Drive / Genesis
- Game Gear
- PlayStation
- PC Engine / TurboGrafx-16

Für diese Systeme werden kuratiert:

- System-Metadaten
- Artworks
- unterstützte Formate
- empfohlener Core
- alternative Cores
- Firmware-Anforderungen
- Capabilities
- Launch-Konfigurationen

Custom Systems sind nicht Bestandteil des MVP.

## 14. System-Metadaten

System-Metadaten werden von BitArchive kuratiert und mit der Anwendung ausgeliefert.

Mögliche Inhalte:

- Name
- Hersteller
- Erscheinungsjahr
- Kategorie
- Logo
- Systembild

System-Metadaten werden nicht über ScreenScraper bezogen.

## 15. Metadaten und Scraping

### 15.1 Provider-Modell

Die Architektur ist provider-neutral und Multi-Provider-fähig.

Im MVP wird ausschließlich ScreenScraper integriert.

Spätere Provider können ergänzt werden, ohne das Game-/Release-Modell neu aufzubauen.

### 15.2 Scraping wird manuell gestartet

Beim Import erfolgt kein automatisches Scraping.

Der Nutzer startet einen Scraping-Durchlauf selbst über die Library-Verwaltung.

### 15.3 Scraping-Umfang

Vor einem Lauf kann der Nutzer wählen:

- nur Spiele ohne Metadaten
- fehlende Felder ergänzen
- alle Spiele erneut scrapen
- optional auf ein System begrenzen

### 15.4 Scraping-Modi

#### Automatisch

- eindeutige Treffer werden übernommen
- bei mehreren Treffern wird der beste Match verwendet
- bei keinem Treffer wird das Spiel übersprungen

#### Interaktiv

- eindeutige Treffer können automatisch akzeptiert werden
- bei mehreren oder keinen Treffern hält BitArchive an
- der Nutzer kann auswählen oder die Suche anpassen

Zusätzlich ist ein Single-Game-Scrape aus der Spiel-/Metadatenansicht vorgesehen.

### 15.5 Unterbrechen und Fortsetzen

Scraping-Läufe können unterbrochen und später fortgesetzt werden.

BitArchive merkt sich den Fortschritt pro Spiel.

### 15.6 Scraping-Ergebnis

Nach einem Lauf zeigt BitArchive eine kompakte Zusammenfassung, beispielsweise:

- erfolgreich aktualisiert
- kein Treffer
- mehrere Treffer
- Netzwerk-/API-Fehler
- übersprungen
- manuelle Overrides geschützt

Problematische Spiele können direkt geöffnet werden.

### 15.7 API-Limits

BitArchive erkennt die erlaubte ScreenScraper-Parallelität automatisch und respektiert API-Limits.

Eine frei konfigurierbare Thread-Zahl ist nicht Bestandteil des MVP.

### 15.8 Credentials

BitArchive verfügt über eigene ScreenScraper Developer API Credentials.

App-Credentials und User-Credentials werden getrennt behandelt.

ScreenScraper-User-Credentials werden lokal sicher gespeichert, unter macOS im Schlüsselbund.

Credentials dürfen nicht in Logs oder Fehlermeldungen ausgegeben werden.

### 15.9 Gescrapte Inhalte im MVP

Unterstützt:

- Cover / Box Art
- Screenshots
- Beschreibung
- Entwickler
- Publisher
- Release-Datum
- Genre
- Spieleranzahl
- Region
- weitere textuelle Metadaten im definierten Modell

Nicht im MVP:

- Videos
- Logos / Wheel Art
- Fan Art

### 15.10 Medien-Cache

Cover und Screenshots werden lokal gespeichert bzw. gecacht.

Die Bibliothek soll nach einem Scrape vollständig offline darstellbar sein.

Der Media-Cache liegt getrennt von den ROMs und kann neu aufgebaut werden.

### 15.11 Manuelle Metadaten

Metadaten können im MVP manuell bearbeitet werden.

Manuelle Overrides haben Vorrang vor gescrapten Werten und dürfen bei späteren Scraping-Läufen nicht automatisch überschrieben werden.

Einzelne Felder können wieder auf den Provider-Wert zurückgesetzt werden.

### 15.12 Metadaten-Sprache

Im MVP gibt es:

- eine globale bevorzugte Metadaten-Sprache
- eine globale Fallback-Sprache

Beispiel:

- bevorzugt: Deutsch
- Fallback: Englisch

### 15.13 Import vorhandener Metadaten

Der Import aus anderen Frontends wie ES-DE oder EmulationStation ist nicht Bestandteil des MVP.

## 16. RetroArch Runtime Management

### 16.1 Verwaltete Runtime

BitArchive verwaltet eine eigene RetroArch-Installation.

Eine bestehende RetroArch-Installation des Nutzers ist für den normalen Betrieb nicht erforderlich.

### 16.2 Distribution

Eine getestete RetroArch-Version wird mit BitArchive ausgeliefert.

Die Runtime wird anschließend als separat verwaltete Komponente behandelt, sodass sie unabhängig von der BitArchive-App aktualisiert werden kann.

### 16.3 Runtime Manager

BitArchive besitzt ein eigenes Runtime-Management für:

- RetroArch-Version
- Installationsstatus
- Update-Verfügbarkeit
- Integrität
- Rollback

## 17. Core Management

BitArchive verwaltet Cores aktiv über einen eigenen kontrollierten Core-Katalog.

### 17.1 System-Defaults

Für jedes offiziell unterstützte System gibt es:

- einen empfohlenen Standard-Core
- optional alternative kompatible Cores

### 17.2 Core Overrides

Core-Auswahl erfolgt hierarchisch:

```text
System-Default
  ↓
Game Override
  ↓
Release Override (optional)
```

Regeln:

- jedes unterstützte System besitzt einen System-Default
- ein Spiel kann diesen Default optional überschreiben
- ein konkreter Release kann den Game-Override optional nochmals überschreiben
- Release Override gewinnt vor Game Override, Game Override gewinnt vor System-Default
- besitzt ein Spiel nur einen relevanten Release, muss die UI diese zusätzliche Ebene nicht sichtbar machen

RetroArch-Einstellungen und Core Options erhalten im MVP **keine** zusätzliche Release-Ebene; deren Vererbung bleibt `Global → System → Game` bzw. `Core Defaults → System → Game`.

Im normalen Game-UI gibt es keine zusätzliche „Starten mit …“-Auswahl.

### 17.3 Installation

Cores werden bedarfsgerecht installiert.

BitArchive soll nicht pauschal alle verfügbaren Cores mitliefern.

### 17.4 Core-Katalog

Der Katalog kann Informationen enthalten wie:

- Name
- Version
- Upstream
- Lizenz
- Plattform
- Architektur
- unterstützte Systeme
- unterstützte Content-Formate
- Capabilities
- Firmware-Anforderungen
- Kompatibilitätsstatus

### 17.5 Updates

Core-Updates werden angezeigt, aber nie automatisch installiert.

Die Installation erfolgt nur nach ausdrücklicher Nutzerbestätigung.

## 18. Runtime- und Core-Sicherheit

### 18.1 Versionierte Manifeste

BitArchive nutzt ein Runtime-/Core-Manifest, das freigegebene Versionen beschreibt.

### 18.2 Signierte Manifeste

Runtime-/Core-Manifeste werden digital signiert.

BitArchive prüft die Signatur vor der Verwendung.

### 18.3 Artefaktintegrität

Jede heruntergeladene Runtime und jeder Core wird per SHA-256 gegen den erwarteten Wert aus dem Manifest geprüft.

Bei Abweichung:

- Installation wird blockiert
- Artefakt wird verworfen
- Nutzer erhält eine klare Fehlermeldung

## 19. Updates und Rollback

### 19.1 Getrennte Update-Kanäle

BitArchive behandelt getrennt:

- BitArchive-App
- RetroArch-Runtime
- Cores

Ein App-Update darf nicht ungefragt Runtime oder Cores aktualisieren.

### 19.2 BitArchive-Updater

Der MVP unterstützt:

- Prüfung auf Updates
- Release Notes
- Download und Installation
- optional automatische Prüfung beim Start

Die eigentliche Installation wird vom Nutzer ausgelöst.

### 19.3 RetroArch-Updates

RetroArch-Runtime-Updates werden angezeigt, aber nur nach Nutzerbestätigung installiert.

### 19.4 Core-Updates

Core-Updates werden ebenfalls nur nach Nutzerbestätigung installiert.

Vor der Bestätigung prüft BitArchive die Auswirkungen des Updates auf:

- vorhandene Save States des betroffenen Cores
- Core-Option-Overrides, die mit der neuen Core-Version ungültig werden könnten

Wenn Save States nach dem Update nur noch als `VersionMismatch` gelten würden, wird dies vor der Installation verständlich angezeigt.

Der Nutzer wird darauf hingewiesen, dass für den Core ein Ein-Schritt-Rollback auf die vorherige Version verfügbar ist.

### 19.5 Rollback

Im MVP kann jeweils auf die zuletzt zuvor installierte Version zurückgewechselt werden.

Unterstützt für:

- RetroArch-Runtime
- Cores

Eine vollständige Versionshistorie ist nicht notwendig.

## 20. BIOS- und Firmware-Management

### 20.1 Zentraler Firmware-Ordner

BitArchive verwendet im MVP genau einen zentralen, vom Nutzer definierten BIOS-/Firmware-Ordner.

BitArchive indexiert und validiert dessen Inhalte.

### 20.2 Keine Distribution

BitArchive lädt oder verteilt keine BIOS-/Firmware-Dateien.

### 20.3 Keine Veränderung

Firmware-Dateien werden nicht automatisch umbenannt, verschoben oder verändert.

### 20.4 Erkennung

Firmware wird primär anhand bekannter Hashes erkannt.

Der Dateiname wird zusätzlich geprüft, weil Cores bestimmte Namen erwarten können.

Mögliche Zustände:

- Inhalt korrekt, Dateiname korrekt
- Inhalt korrekt, Dateiname falsch
- falscher Hash
- fehlt

### 20.5 Readiness pro Core

Firmware-Anforderungen werden pro Core geprüft.

In der UI darf der Status system- oder spielbezogen zusammengefasst werden.

## 21. Emulationskonfiguration

BitArchive verwaltet einen kuratierten Teil der RetroArch-Konfiguration direkt in der eigenen UI.

### 21.1 Hierarchie

RetroArch-Einstellungen folgen der Vererbung:

```text
Global
  ↓
System
  ↓
Spiel
```

Eine spezifischere Ebene überschreibt nur explizit gesetzte Werte.

Einzelne Werte können wieder auf „geerbt“ zurückgesetzt werden.

### 21.2 Kuratierte Settings im MVP

Mindestens:

- Video
- Audio
- Input
- Shader
- Rewind
- Latency / Run-Ahead, sofern unterstützt
- Aspect Ratio
- Integer Scaling
- Fullscreen / Windowed
- VSync

Es gibt keinen vollständigen generischen `.cfg`-Editor im MVP.

### 21.3 Shader

BitArchive zeigt vorhandene RetroArch-Shader-Presets an und erlaubt ihre Auswahl auf:

- Global
- System
- Spiel

BitArchive installiert oder lädt im MVP keine zusätzlichen Shader-Pakete.

### 21.4 Core Options

Core-spezifische Optionen sind ebenfalls Teil des MVP.

Sie werden getrennt von normalen RetroArch-Settings verwaltet, aber konsistent in der UI dargestellt.

Vererbung:

```text
Core Defaults
  ↓
System
  ↓
Spiel
```

## 22. Controller

BitArchive bietet im MVP eine grundlegende Controller-Verwaltung.

Unterstützt:

- verbundene Controller erkennen
- Standard-Controller auswählen
- optional bevorzugten Controller pro System wählen
- Controller-Status anzeigen

Nicht im MVP:

- vollständiges Button-Remapping
- Core-spezifische Input-Remaps
- komplexe Multi-Controller-Profile

Diese tieferen Funktionen bleiben zunächst bei RetroArch.

## 23. Save States

BitArchive unterstützt im MVP ausschließlich Save States.

Normale Spielstände wie SRAM, Memory Cards oder andere Save-Dateien werden vollständig von RetroArch verwaltet und von BitArchive nicht indexiert oder dargestellt.

### 23.1 Quelle

RetroArch bleibt die technische Quelle der Save States.

BitArchive legt eine Verwaltungs- und Metadatenebene darüber.

### 23.2 Save-State-Metadaten

Ein Save State kann enthalten:

- Release
- Core
- Core-Version
- Slot
- Timestamp
- optionalen Anzeigenamen
- optionales Thumbnail

### 23.3 Zuordnung

Ein Save State gehört technisch zu:

- einem konkreten Release
- einem konkreten Core
- einer Core-Version

### 23.4 Kompatibilität

BitArchive behandelt Save-State-Kompatibilität konservativ:

- gleicher Core + gleiche Core-Version → kompatibel
- gleicher Core + andere Core-Version → Warnstatus / potenziell inkompatibel
- anderer Core → inkompatibel

### 23.5 Timeline

RetroArch-Slots bleiben die technische Grundlage.

BitArchive stellt Save States jedoch primär als chronologische Timeline dar.

Die Timeline heißt user-facing **Save States**. `Continue` bezeichnet ausschließlich die Aktion, den neuesten kompatiblen Save State zu laden.

Beispiel:

```text
Save States

[ Screenshot ]  Gestern, 21:43   Slot 2
[ Screenshot ]  10. Sep., 19:12  Slot 1
[ Screenshot ]  08. Sep., 23:01  Slot 3
```

### 23.6 Thumbnails

Save-State-Thumbnails werden im MVP unterstützt.

Ein Thumbnail ist optional und technisch vom State getrennt.

### 23.7 Verwaltung

BitArchive kann Save States:

- erkennen
- anzeigen
- laden
- benennen
- löschen
- auf Kompatibilität prüfen

Beim Löschen werden die tatsächliche RetroArch-State-Datei und das zugehörige Thumbnail entfernt.

Vor dem Löschen wird bestätigt.

Es gibt im MVP keinen Papierkorb und keine Wiederherstellung.

### 23.8 Erzeugen neuer Save States

Neue Save States werden im MVP nicht aus BitArchive heraus erzeugt.

Das Erzeugen bleibt vollständig RetroArch überlassen.

## 24. Play und Continue

### Play

`Play` startet immer einen normalen Spielstart mit dem aktuell definierten Release und Core.

### Continue

Wenn ein kompatibler Save State vorhanden ist, bietet BitArchive zusätzlich `Continue` an.

`Continue` lädt den letzten kompatiblen Save State.

## 25. Launch Readiness

Vor dem Start prüft BitArchive, ob das Spiel startbereit ist.

Mögliche Probleme:

- Core fehlt
- BIOS/Firmware fehlt
- Content-Datei nicht verfügbar
- Library Source offline
- Dateiformat vom Core nicht unterstützt
- beschädigte oder unvollständige Content-Dateien

`Play` bleibt sichtbar, der Start wird bei fehlender Readiness jedoch blockiert und der Grund klar angezeigt.

Installierbare Komponenten wie Cores können direkt aus diesem Kontext nachinstalliert werden.

## 26. Launch-Diagnose

BitArchive protokolliert im MVP grundlegende Launch-Informationen.

Mindestens:

- verwendeter Core
- verwendete Content-Datei
- Startzeit
- Prozessstatus
- Exit-Code
- relevante RetroArch-Fehlermeldungen

Die UI zeigt eine verständliche Fehlermeldung und erlaubt bei Bedarf technische Details.

## 27. Emulationssession

### 27.1 Eine Session gleichzeitig

Im MVP unterstützt BitArchive genau eine aktive Emulationssession gleichzeitig.

Während RetroArch läuft:

- bleibt BitArchive geöffnet
- zeigt den laufenden Titel
- zeigt die Session-Dauer
- verhindert den Start einer zweiten Emulationssession
- bietet „Zu RetroArch wechseln“
- bietet „Sitzung beenden“

### 27.2 Session beenden

BitArchive versucht RetroArch zuerst geordnet zu schließen.

Nur wenn RetroArch nicht reagiert, wird ein erzwungenes Beenden angeboten.

### 27.3 Rückkehr zu BitArchive

Nach Ende des RetroArch-Prozesses bringt sich BitArchive automatisch wieder in den Vordergrund.

### 27.4 Spielaktivität

BitArchive erfasst:

- `lastPlayedAt`
- `totalPlayTime`
- `playCount`
- Dauer der letzten Session

### 27.5 BitArchive schließen während eines Spiels

Wenn RetroArch läuft und BitArchive beendet werden soll, entscheidet der Nutzer zwischen:

- nur BitArchive schließen, Spiel weiterlaufen lassen
- Spiel beenden und BitArchive schließen

### 27.6 Session-Recovery

Aktive Sessions werden persistent gespeichert.

Beim nächsten Start prüft BitArchive, ob der zugehörige RetroArch-Prozess noch läuft.

- läuft er noch → Session wird wieder aufgenommen
- läuft er nicht mehr → Session wird sauber abgeschlossen, soweit möglich

## 28. Fullscreen

RetroArch startet standardmäßig im Fullscreen.

Das Verhalten ist über die normale Konfigurationshierarchie anpassbar:

- Global
- System
- Spiel

## 29. Home

BitArchive besitzt im MVP eine zentrale **Home**-Einstiegsebene.

Sie dient als Root des normalen Frontends und ermöglicht schnellen Zugriff auf:

- zuletzt gespielte Titel
- Favoriten
- Alle Spiele
- Systeme

Die konkrete Darstellung und Navigationsstruktur wird in der UI/UX-Spezifikation definiert.

Eine separate, vorgeschaltete Dashboard-Seite ist nicht erforderlich. Die Home-Experience darf direkt durch die Library-/System-Navigation repräsentiert werden.

Komplexe Filter, Bearbeitung und Verwaltung bleiben außerhalb dieser Einstiegsebene.

## 30. Library Views

Im MVP gibt es:

- Systeme
- Alle Spiele
- Favoriten
- Zuletzt gespielt

Nicht im MVP:

- frei definierbare Collections

## 30.1 Zuständigkeit von Manage Library und Settings

Für die Informationsarchitektur gilt user-facing:

- **Home** = normaler Einstieg in die Spielebibliothek
- **Manage Library** = Was ist vorhanden und ist es spielbereit?
- **Settings** = Wie verhalten sich BitArchive und Emulation?
- **Context Options** = Wie verhält sich das aktuell fokussierte System, Spiel oder Release?

Zu **Manage Library** gehören insbesondere:

- Library Sources
- Systeme
- Scan
- Scraping
- Metadaten
- BIOS / Firmware
- Runtime- und Core-Status
- Library Health
- Maintenance

Zu Settings gehören insbesondere:

- General
- Appearance
- Input
- Emulation
- Sprache / Region
- Notifications
- Backup / Restore
- Advanced

Technische Pfade sollen nicht als primäre Top-Level-Kategorie auftreten, sofern sie fachlich klar einem Library- oder Settings-Bereich zugeordnet werden können.

## 31. Suche, Filter und Sortierung

### Suche

- Volltextsuche nach Titel

### Filter

- System
- Favoriten
- Release-Typ

### Sortierung

- Titel
- zuletzt gespielt
- Spielzeit
- Release-Datum

Komplexere Filter können später ergänzt werden.

## 32. Game Detail Experience

BitArchive stellt im MVP eine vollständige Game-Detail-Experience bereit.

Die konkrete UI muss nicht aus einer vorgeschalteten separaten Detailseite bestehen. Der normale Game Browser darf bereits die wichtigsten Informationen und Aktionen anzeigen; weiterführende Informationen können über eine sekundäre Info-/Detailansicht erreichbar sein.

Die Experience umfasst insgesamt:

- Cover
- Screenshots
- Titel
- System
- Region / Release
- Beschreibung
- Entwickler
- Publisher
- Release-Datum
- Genre
- Spieleranzahl
- Favorit
- Play
- Continue, falls verfügbar
- Spielzeit
- zuletzt gespielt
- Save States inkl. Thumbnails
- verwendeter Core
- Core-Override
- relevante Emulationseinstellungen
- Readiness-Status für Core / BIOS / Content

`Play` muss ohne verpflichtenden zusätzlichen „Starten mit …“- oder Detail-Schritt erreichbar bleiben.

### Release-Auswahl

Der aktive/default Release wird kompakt angezeigt, wenn ein Spiel mehrere Releases besitzt oder die Information für eine Aktion relevant ist.

Der Release kann über die Game-Detail-/Info- bzw. Game-Options-Experience gewechselt werden.

Beim Wechsel können sich ändern:

- Region
- Sprache
- Content-Dateien
- Core-Override
- Readiness
- Continue-Verfügbarkeit
- Save-State-Zuordnung
- release-spezifische Metadaten

Besitzt ein Spiel nur einen relevanten Release, muss die UI die Release-Komplexität nicht prominent darstellen.

## 33. Desktop- und Controller-UX

BitArchive ist eine Desktop-first Anwendung mit controller-first Library-Navigation.

Die zentralen Nutzungsflows sind vollständig controllerfähig.

Mit Controller müssen mindestens funktionieren:

- Systeme wechseln
- Bibliothek durchsuchen
- suchen / filtern
- Spiel öffnen
- Spiel starten
- Continue ausführen
- Save State auswählen und laden
- grundlegende Einstellungen

Die normalen Library- und Game-Browsing-Flows sollen sich mit einem Controller besonders direkt anfühlen.

Komplexe Verwaltungsbereiche dürfen stärker auf Maus und Tastatur optimiert sein, müssen aber dort, wo sie zum grundlegenden MVP-Setup gehören, weiterhin sinnvoll per Controller bedienbar bleiben.

Im MVP gibt es keinen separaten Big-Picture- oder TV-Modus.

## 34. Tastatur und Hotkeys

BitArchive unterstützt normale Tastaturkürzel innerhalb der App.

Es gibt im MVP keine globalen Hotkeys, die während einer laufenden RetroArch-Session eingreifen.

RetroArch bleibt während der Emulation für Hotkeys zuständig.

## 35. UI und Appearance

BitArchive verwendet ein festes Designsystem mit semantischem Light-/Dark-Support.

Nicht im MVP:

- frei austauschbare Themes
- Theme-Pakete

Die aktuelle UI-Konzeption bevorzugt für den immersiven Library-/Gaming-Bereich einen schwarzen bzw. sehr dunklen Canvas. Ob dieser Bereich unabhängig vom globalen Light-/Dark-Modus dunkel bleibt, wird im Visual-Design-Schritt entschieden.

Die Produktanforderung ist daher semantischer Light-/Dark-Support; die konkrete visuelle Ausprägung einzelner Bereiche wird nicht in `PRODUCT.md` festgeschrieben.

## 36. Internationalisierung

BitArchive wird von Anfang an internationalisierbar aufgebaut.

Im MVP verfügbare UI-Sprachen:

- Deutsch
- Englisch

## 37. Accessibility

Accessibility-Grundlagen sind Bestandteil des normalen Designsystems.

Der MVP berücksichtigt:

- UI-Skalierung
- größere Schrift
- vollständige Tastaturnavigation
- klar sichtbaren Fokus für Tastatur und Controller
- ausreichende Kontraste in Light und Dark Mode
- Zustände nicht ausschließlich über Farbe
- Respektierung von macOS Reduced Motion

## 38. System-Benachrichtigungen

BitArchive darf sparsame, abschaltbare System-Benachrichtigungen verwenden.

Geeignete Fälle:

- BitArchive-Update verfügbar
- RetroArch-Update verfügbar
- Core-Update verfügbar
- Scraping abgeschlossen
- Library-Scan abgeschlossen

Keine Benachrichtigungen für normale alltägliche Aktionen.

## 39. Backup und Restore

BitArchive bietet im MVP ein manuelles lokales Backup und Restore.

### Enthalten

- Datenbank
- Einstellungen
- Favoriten
- Spielstatistiken
- manuelle Metadaten-Overrides
- Scraping-/Library-Zuordnungen
- Runtime-/Core-Konfigurationen

### Nicht enthalten

- ROMs / ISOs
- BIOS / Firmware
- RetroArch-Runtime selbst
- Cores selbst
- normale RetroArch-Saves

### Save States

Save States und Thumbnails können optional per Checkbox in das Backup aufgenommen werden.

Standardmäßig ist diese Option deaktiviert.

## 40. Reset-Funktionen

### Bibliothek neu aufbauen

- Index zurücksetzen
- gescrapte Metadaten zurücksetzen
- Sources anschließend neu scannen

### BitArchive vollständig zurücksetzen

- Datenbank
- Einstellungen
- Statistiken
- Metadaten
- lokale BitArchive-Daten

ROMs, ISOs und BIOS-/Firmware-Dateien bleiben immer unangetastet.

## 41. Entfernen und Ausblenden von Spielen

BitArchive löscht niemals zugrunde liegende ROM-/ISO-Dateien.

Im MVP sind stattdessen möglich:

- aus Bibliothek ausblenden
- ignorieren
- Indexeintrag zurücksetzen

## 42. Datenschutz und Netzwerk

BitArchive sendet im MVP keine Nutzungsdaten.

Es gibt:

- keine Telemetrie
- keine automatischen Crash-Reports
- keine versteckten Hintergrundverbindungen

Netzwerkzugriffe müssen einer klaren Funktion zugeordnet sein und, soweit sinnvoll, in den Einstellungen nachvollziehbar oder deaktivierbar sein.

## 43. Nicht im MVP / später

### Emulation und Plattformen

- Linux-Support
- Windows-Support
- Standalone-Emulatoren wie Dolphin, RPCS2/PCSX2, RPCS3 etc.
- Custom Systems

Die Architektur bleibt backend-neutral, sodass später weitere Emulations-Backends ergänzt werden können.

### Library und Metadaten

- freie Collections
- Import vorhandener Metadaten aus ES-DE/EmulationStation
- Video-Scraping
- Logos / Wheel Art
- Fan Art
- komplexe ROM-Patch-/Parent-Beziehungen
- Multi-ROM-ZIP-Archive
- 7z-Unterstützung

### Online-Funktionen

- RetroAchievements
- Cloud Sync
- Cloud Backup
- Benutzerkonten

### UI

- eigener Big-Picture-/TV-Modus
- frei austauschbare Themes

### Controller

- vollständiges Button-Remapping
- komplexe Controller-Profile
- globale Hotkeys

### Save-System

- normale SRAM-/Memory-Card-Verwaltung
- eigenes Save-Format
- Save-State-Erstellung aus BitArchive
- Papierkorb oder Versionshistorie für Save States

### ROM-Management

- ROMs verschieben
- ROMs umbenennen
- ROMs reorganisieren
- Disc-Image-Konvertierung
- automatische Dateikonvertierung

### Betrieb

- Portable Mode
- Telemetrie
- automatische Crash-Reports

## 44. Architektur-relevante Produktgrenzen

Die spätere technische Architektur sollte folgende Produktgrenzen ausdrücklich widerspiegeln:

1. `Game` ist nicht dasselbe wie `Release`.
2. `Release` ist nicht dasselbe wie eine physische Content-Datei.
3. Ein Release kann mehrere Content-Dateien besitzen.
4. Mehrere physische Dateien können denselben Inhalt repräsentieren.
5. Metadaten sind provider-neutral und können manuell überschrieben werden.
6. Emulation ist backend-neutral, auch wenn im MVP nur RetroArch implementiert wird.
7. RetroArch-Runtime und Cores sind separat versionierte, verwaltete Komponenten.
8. Firmware-Anforderungen hängen am Core, nicht nur am System.
9. RetroArch-Settings folgen `Global → System → Spiel`.
10. Core Options folgen einer separaten, ähnlichen Vererbung.
11. Save States gehören zu `Release + Core + Core-Version`.
12. Normale RetroArch-Saves liegen außerhalb des BitArchive-Scopes.
13. Library Sources und Originaldateien bleiben externe, nicht von BitArchive kontrollierte Ressourcen.
14. Netzwerkfunktionen sind optionale, klar abgegrenzte Subsysteme.
15. App-, Runtime- und Core-Updates bleiben voneinander unabhängig.

## 45. Offene technische Fragen für die Architekturphase

Diese Product Specification definiert den Funktionsumfang, aber noch nicht die technische Umsetzung.

In der Architekturphase sind unter anderem zu klären:

- App-Framework und UI-Technologie für macOS-first mit späterem Linux-Support
- lokale Datenbanktechnologie
- Dateisystem- und Volume-Abstraktion
- Hashing-Strategie und Performance bei großen Libraries
- genaue RetroArch-Prozessintegration
- Runtime-Verzeichnisstruktur
- sichere Distribution der ScreenScraper Developer Credentials
- Signaturformat und Schlüsselverwaltung für Runtime-/Core-Manifeste
- Update-Infrastruktur
- genaue Config-Generierung für RetroArch
- Core-Option-Erkennung und Schema
- Save-State-Erkennung und Thumbnail-Zuordnung
- Session-Recovery über Prozessgrenzen hinweg
- Backup-Dateiformat
- Media-Cache-Strategie
- Migrationsstrategie für zukünftige Datenbankschemata
- Logging ohne sensible Pfade/Credentials in externen Reports, falls später Diagnosefunktionen ergänzt werden
- macOS Code Signing, Notarisierung und Packaging der gebündelten RetroArch-Runtime

## 46. MVP-Erfolgskriterium

Der MVP ist erfolgreich, wenn ein Nutzer auf macOS:

1. BitArchive installiert und startet,
2. seine bestehenden Spieleordner hinzufügt,
3. die Sammlung zuverlässig indexieren kann,
4. Metadaten bei Bedarf per ScreenScraper ergänzt,
5. benötigte Cores und Firmware-Readiness versteht,
6. ein Spiel mit `Play` zuverlässig startet,
7. nach Ende der Session wieder zu BitArchive zurückkehrt,
8. Spielzeit und zuletzt gespielte Titel sieht,
9. vorhandene Save States als visuelle Timeline durchsuchen kann,
10. mit `Continue` direkt in einen kompatiblen Save State springen kann,
11. RetroArch-Einstellungen über Global/System/Spiel-Hierarchien konfigurieren kann,
12. ohne Eingriffe von BitArchive an seinen Original-ROMs, ISOs und BIOS-Dateien arbeiten kann.

---

**Status:** Product Scope für MVP definiert.  
**Nächster Schritt:** Technische Architektur und anschließend Datenmodell ableiten.
