[English](README.md) | [日本語](README.ja.md) | [简体中文](README.zh-cn.md) | [繁體中文](README.zh-tw.md) | [한국어](README.ko.md) | [Español](README.es.md) | [Português (Brasil)](README.pt-br.md) | [Français](README.fr.md) | **Deutsch** | [Русский](README.ru.md)

# sim, das Vim der neuen Generation

**sim** (auch *sim editor* genannt, Kurzform von *Secure vIM*) ist das Vim der neuen Generation: von Grund auf in Rust geschrieben, behält es Vims modales Editieren und die zentralen Tasten. Es liest keine Modelines, lädt keine Plugins und öffnet keine Netzwerkverbindung, und es bringt auf den Bildschirm, was sonst verborgen bleibt: unsichtbare Zeichen, Schreibzugriffe von außerhalb des Editors, Repositories, die Code ausführen würden.

**Keine Plugins. Keine Modelines. Kein Netzwerk.**

Website: <https://sim.efg-technologies.com/de/> · Linux und macOS · MIT OR Apache-2.0 · [Changelog](CHANGELOG.md) · [Sicherheitsrichtlinie](SECURITY.md)

Alles, was sim verspricht, verspricht es für jede darin geöffnete Datei: **ohne ausdrückliche Handlung geht niemandes Text verloren**, weder Ihrer noch der eines anderen Prozesses.

## Installation

Die Binärdatei heißt `sim`. Der Paketname lautet `sim-editor` im AUR und auf crates.io und `efg-technologies/tap/sim` bei Homebrew; die Bibliothek, auf der es aufbaut, heißt `sim-editor-core` auf crates.io.

sim 0.1.0 ist veröffentlicht. Mit Homebrew installieren Sie es so:

```sh
brew install efg-technologies/tap/sim
```

Der Quellcode der Version liegt auf der [Release-Seite v0.1.0 auf GitHub](https://github.com/efg-technologies/sim/releases/tag/v0.1.0) (`sim-0.1.0.tar.gz`). Aus dem Quellcode bauen:

```sh
git clone https://github.com/efg-technologies/sim.git
cd sim
cargo install --locked --path sim
```

Das AUR-Paket `sim-editor` und die Crates `sim-editor` und `sim-editor-core` auf crates.io folgen.

Danach, wenn Sie es als Ihren Editor haben möchten:

```sh
export EDITOR=sim                     # Ctrl-g von Claude Code, crontab -e, …
git config --global core.editor sim   # git commit (ZZ schließt ab, :cq bricht ab)
export SUDO_EDITOR='sim --plain'      # sudoedit (unter Linux ist sim dort von selbst plain)
```

Benötigt Rust 1.88 oder neuer. Läuft unter Linux und macOS in jedem Terminal; Farben, das kitty-Tastaturprotokoll und OSC 52 werden genutzt, wenn das Terminal sie anbietet (Ghostty, kitty, WezTerm, tmux). git und ripgrep sind optional: sim nutzt sie, wenn sie in `PATH` liegen. Icons brauchen einen Nerd Font; in Ghostty, kitty und WezTerm starten sie eingeschaltet, anderswo ausgeschaltet, mit einem einmaligen Hinweis, und `zi` schaltet sie ein und aus. Mehr auf der [Installationsseite](https://sim.efg-technologies.com/de/install/).

## Was sim kann

**Zeigt, was verborgen ist**

- Ein Rechts-nach-links-Override, ein Leerzeichen mit Breite null oder ein Steuerzeichen wird als `<U+202E>` oder `^[` auf einem magentafarbenen Block gezeichnet, in jedem Zustand, in dem sich eine Zelle befinden kann, und nie roh an das Terminal gesendet. Der wörtliche Text `<U+202E>` in einer Datei bekommt keinen Block.
- Leerzeichen am Zeilenende erscheinen als `·`, ein geschütztes Leerzeichen als `⍽`, ein Tabulator als `»`.
- Schreibt ein anderer Prozess die geöffnete Datei neu, blinken die geänderten Zeilen kurz auf und werden dann im Text markiert, der Rahmen nennt die Zahl der Änderungen auf dem Datenträger, der Tab zeigt `↯`, und der Dateibaum markiert die Datei. `]c` und `[c` springen zur nächsten und zur vorherigen Markierung.
- Änderungen gegenüber git HEAD erscheinen als Markierungen in der Randspalte, als Statusbuchstabe für jede Datei im Dateibaum (ein Ordner zeigt den Buchstaben des stärksten Status darunter) und als `+n ~n -n` in der Statuszeile.
- git wird nicht ausgeführt, wo die eigene Konfiguration des Repositorys einen Befehl ausführen könnte (`core.fsmonitor`, ein Filtertreiber und mehr): Der Rahmen sagt `git not run` auf dem Warnblock, und die Statuszeile nennt den Schlüssel und die Datei. Läuft git, nachdem eigene Einstellungen des Repositorys (`core.fsmonitor`, Hooks-Pfad, Filter, externes Diff) abgeschaltet wurden, nennt der Rahmen die Zahl (`2 git settings off`).
- Ein Markdown-Link, dessen Text eine Adresse nennt, die sein Ziel nicht ist (`[https://good.example](https://evil.example)`, auch ein Referenz-Link `[good.example][1]`, dessen Definition woandershin zeigt, ein Link über mehrere Zeilen oder mit Backslash oder Entität im Ziel, ein HTML-Link `<a href="…">`), zeigt sein Ziel, bei einer Referenz auch seinen Text, auf dem magentafarbenen Block, in der Leseansicht und auch im Calm-Look; die Leseansicht versteckt von einem Link nie Klammern oder URL.

**Lehnt ab, was abzulehnen ist, und bewahrt Ihren Text**

- Einstellungen und Befehle werden nie aus Dateiinhalten gelesen: keine Modelines, keine Konfigurationsdateien, keine Plugins, kein Netzwerk.
- Gespeichert wird atomar (temporäre Datei, fsync, rename, fsync des Verzeichnisses), und vorher wird der Hash der Datei geprüft: Hat sich die Datei auf dem Datenträger geändert, wird nichts überschrieben. Findet ein Speichern einen Konflikt, behält `:w!` Ihre Version, nachdem die fremde ins State-Verzeichnis kopiert wurde, und `:e!` übernimmt die fremde nach einem Snapshot Ihrer eigenen.
- Ungespeicherte Änderungen werden jede Sekunde als Snapshot gesichert; nach einem Absturz, `kill -9` oder einem geschlossenen Terminal bietet das erneute Öffnen der Datei sie wieder an, als einen einzigen Undo-Schritt.
- Die Suche findet ripgrep selbst, startet es mit leerer Umgebung und `--no-config` und folgt nie einem Symlink aus dem Projekt hinaus.
- `sim --plain` ist der Kern allein: eine Datei, kein Panel und kein externes Programm (kein git, kein ripgrep, kein Filter, kein Hook). Es ist der Standard als root und als Editor von sudoedit.

**Erreichbarkeit, Tempo und Lesbarkeit**

- Ein Dateibaum, der sich nie verschiebt, `Ctrl-p` zum Finden von Dateien, `Ctrl-f` zum Durchsuchen von Text mit Vorschau, wahrscheinliche Dateien mit `•` markiert, mehrere Dateien mit einer Tab-Leiste.
- Im eigenen Benchmark des Projekts braucht eine Taste bei p99 weniger als 1 ms (Datei mit 100 000 Zeilen, Release-Build); der erste Frame in einem Repository mit 100 000 Dateien wurde mit 1,8 bis 8,7 ms gemessen.
- Syntaxfarben aus der Palette Ihres Terminals, Markdown direkt lesbar (Auszeichnungen ausgeblendet, Tabellen ausgerichtet, und die Zeile, in der Sie stehen, immer der Text der Datei), drei Looks (`zl`), japanischer Kinsoku-Umbruch und vollbreite Tasten.
- Der Titel des Editors ist ein Breadcrumb, `src › sim.rs › leader_key`: der Pfad und die Überschrift oder Funktion, in der der Cursor steht. Jede Angabe steht an einer Stelle: Die Statuszeile hat den Modus, den Branch, die Zähler und die Position und nennt `CRLF`, `BOM` oder eine Datei, die kein UTF-8 ist, nur wenn es zutrifft; der Tab hat die Datei und `●`, die Bildlaufleiste die Position, und der Baum liegt nicht im Nebel.
- `NO_COLOR` und Terminals mit weniger als 8 Farben zeichnen nur mit Schriftstärke, Unterstreichung und inverser Darstellung, und Bedeutung hängt nie allein von der Farbe ab: Eine geänderte Zeile ist `┆`, Ordner zeigen Statusbuchstaben, ein Tab zeigt `↯` für eine auf dem Datenträger geänderte Datei, und der Griff der Bildlaufleiste ist eine `║`-Schiene statt eines inversen Blocks.

**Tasten, die sich finden lassen**

- `Space` ist die Leader-Taste. Eine Pause bei einem Präfix (`Space`, `g`, `z`, `Z`, `[`, `]` oder einem Operator wie `d`) öffnet ein kleines which-key-Fenster, das die Tasten auflistet, die als Nächstes kommen können; Weitertippen funktioniert genau wie ohne das Fenster.
- `:help` und `F1` öffnen eine durchsuchbare Liste aller Tasten und Befehle, mit dem, was jede tut; `:help tab` findet `Tab`, `Shift-Tab` und `<Space>bp`.
- `<Space>bp` setzt einen Buchstaben auf jeden Tab, und der Buchstabe springt zu dessen Datei; `<Space>bc` tut dasselbe zum Schließen eines Tabs (eine Datei mit Änderungen bleibt offen, wie bei `:q`). Ein Hinweis nennt `Tab` und `Shift-Tab`, sobald zum ersten Mal eine zweite Datei offen ist, bis Sie sie dreimal benutzt haben.
- Solange die Treffer einer Suche hervorgehoben sind, sagt die Statuszeile, bei welchem davon Sie sind und wie viele es gibt, `[3/12]`.
- `sim --help` gibt die Verwendung und die von sim gelesene Umgebung aus (`NO_COLOR`, `SIM_ON_NORMAL`, `XDG_STATE_HOME`); `sim --version` gibt die Version aus. Keines von beiden fasst das Terminal an.

## sim auf einem Bildschirm

| Tasten | Was sie tun |
|---|---|
| `i` | einfügen |
| `Ctrl-s` | speichern |
| `ZZ` | speichern und beenden |
| `q` | beenden |
| `ZQ` | beenden ohne Speichern |
| `u` | rückgängig machen |
| `U` | wiederherstellen (redo) |
| `Y` | bis zum Zeilenende kopieren |
| `/` | vorwärts suchen |
| `n` | nächster Treffer |
| `*` | Wort vorwärts suchen |
| `yy` | Zeilen kopieren |
| `]c` | nächste Änderung von außen |
| `[c` | vorherige Änderung von außen |
| `:s` | ersetzen |
| `:{range}!cmd` | filtern |
| `:seen` | Änderungsmarkierungen löschen |
| `Ctrl-n` | Dateibaum ein- und ausblenden |
| `Ctrl-p` | Dateien finden |
| `Ctrl-f` | Dateiinhalte durchsuchen |
| `.` | geänderte und zuletzt geöffnete Dateien (im Dateibaum) |
| `Tab` | nächste Datei |
| `Shift-Tab` | vorherige Datei |
| `zh` | Syntaxfarben an/aus |
| `zl` | nächster Look |
| `zi` | Icons an/aus |
| `<Space>` | Leader-Taste |
| `<Space>bp` | Tab per Buchstabe wählen |
| `<Space>bc` | Tab zum Schließen wählen |
| `<Space>bk` | wohin die aufgegebene Version ging (`:w!`, `:e!`) |
| `<Space>ge` | Änderungen gegenüber HEAD anzeigen |
| `<Space>gi` | was die Schutzschicht mit diesem Repository getan hat |
| `<Space>um` | Markdown-Leseansicht an/aus |
| `F1` | Hilfe |
| `:help` | Hilfe |

Alles andere ist vi: Motions, Operatoren, Textobjekte, Anzahlen, `.`, Visual-Modus, `:s`, Bereiche. `Y` kopiert bis zum Zeilenende, `U` stellt wieder her (redo), und jeder Yank geht über OSC 52 auch in die System-Zwischenablage, ebenso durch tmux hindurch. Öffnen Sie eine Datei erneut, stehen Sie dort, wo Sie aufgehört haben. Bei eingeschaltetem IME funktionieren vollbreite Tasten als Befehle; `SIM_ON_NORMAL='fcitx5-remote -c'` schaltet den IME aus, sobald Sie in den Normal-Modus zurückkehren (`SI_ON_NORMAL`, der alte Name, funktioniert weiterhin). Jede Taste steht in einer einzigen Tabelle, `sim/keys.tsv`, die `:help` und which-key lesen und die die Tests gegen das prüfen, was die Tasten tun. Die vollständige Liste steht auf der [Doku-Seite](https://sim.efg-technologies.com/de/docs/).

## Sicherheitsmodell

Sie öffnen Dateien und Repositories, die Sie nicht geschrieben haben. Der Angreifer kontrolliert deren Inhalt, nicht Ihren Rechner. sim versucht sicherzustellen, dass sich dieser Inhalt nicht vor Ihnen verstecken, den Editor nicht umkonfigurieren und den Editor nicht dazu bringen kann, Code auszuführen. Es ist **keine Sandbox**: Es läuft mit Ihren Rechten, erkennt keine ähnlich aussehenden Buchstaben aus anderen Schriftsystemen, und seine Aussagen stammen aus der eigenen Spezifikation und den Tests des Projekts, nicht aus einem unabhängigen Audit. Language Server sind noch nicht gebaut; ein Client in einer Sandbox ist geplant. Um eine Sicherheitslücke zu melden, lesen Sie die [Sicherheitsrichtlinie](SECURITY.md).

Das vollständige Bedrohungsmodell, eine Bedrohung nach der anderen, steht auf der [Sicherheitsseite](https://sim.efg-technologies.com/de/security/). Drei ausführliche Artikel gehen tiefer: [versteckte und bidirektionale Zeichen](https://sim.efg-technologies.com/blog/hidden-characters/), [Repositories, die Code ausführen](https://sim.efg-technologies.com/blog/repositories-that-run-code/) und [sicheres Speichern und Geheimnisse](https://sim.efg-technologies.com/blog/safe-saves/).

## Im Vergleich mit Vim, Neovim und Helix

Vim, Neovim und Helix sind ausgereifte Editoren, und sim ist neu. Verglichen wird nur, was eine Primärquelle sagt; die [Vergleichsseite](https://sim.efg-technologies.com/de/compare/) belegt alles (geprüft am 2026-10-17: Vim 9.2.1202, Neovim 0.12.6, Helix 25.07.1).

| | Vim | Neovim | Helix | sim |
|---|---|---|---|---|
| Erweiterungen | Vim script, Pakete | Lua, Vim script | kein Plugin-System in einem Release; der Steel-PR [#8675](https://github.com/helix-editor/helix/pull/8675) ist ein Draft | keine, mit Absicht |
| Modelines | an (für root aus); `'modelinestrict'` seit [9.2.0350](https://github.com/vim/vim/commit/4c2879471) | an (für root aus) | nicht geparst ([#729](https://github.com/helix-editor/helix/issues/729)) | nie gelesen |
| Projektlokale Konfiguration | `'exrc'`, standardmäßig aus | `'exrc'`, Vertrauensliste | `.helix/config.toml` wird in 25.07.1 geladen; die Vertrauensprüfung (trust) liegt auf dem Entwicklungszweig | gibt es nicht |
| U+202E und Verwandte | `<202e>` | `<202e>` | keine Markierung in 25.07.1 (unser Test; [#11255](https://github.com/helix-editor/helix/issues/11255)) | `<U+202E>` auf einem magentafarbenen Block |

Was die anderen vorerst noch besser können: Neovim und Helix liefern einen Language-Server-Client mit, Vim und Neovim haben Plugins und Makros. sim hat davon noch nichts.

## Roadmap (geplant, nicht ausgeliefert)

Vim-Register, Makros und Marken · Zwischenablage-Integration über OSC 52 hinaus · ein Language-Server-Client in einer Sandbox · `:tutor` · ein git-Diff-Baum · eine GUI mit echter Tiefe (ein Prototyp) · Pakete für AUR und crates.io.

## Entwicklung

sim ist ein Programm aus zwei Crates: dem **Kern** (`sim-editor-core`, in `core/`: der Puffer, die Grammatik von vi, das Speichern, die Zusammenführung mit dem, was andere Prozesse schreiben, das Terminal) und dem **Editor** (`sim-editor`, in `sim/`: die Binärdatei `sim`, mit Dateibaum, Finden, Suche, Marken, Syntax, den Looks und der Maus). Das Warum hinter jeder Entscheidung steht in [CONCEPT.md](CONCEPT.md), zusammen mit der Recherche, auf der sie beruht. Was sim leisten muss, steht in [SPEC.md](SPEC.md): Jede Anforderung hat eine ID, und jede ID hat Tests.

Die Pyramide: [CONCEPT](CONCEPT.md) (warum) → [SPEC](SPEC.md) (was, mit IDs) → `core/src` und `sim/src` (wie) → Tests (Beleg). Änderungen fließen nach unten, Belege nach oben. [CLAUDE.md](CLAUDE.md) enthält die Regeln.

```sh
sh scripts/check.sh          # fmt, clippy, Tests, SPEC↔Test-Zuordnung, beide Zeilenobergrenzen, die Abhängigkeiten des Kerns, Binärgröße
sh scripts/e2e.sh            # der Kern (als `sim --plain`) und der Editor in tmux (eigener Socket)
cargo test --release --workspace -- --ignored bench   # Leistungsanforderungen
```

Property-Tests, Fuzzer mit einem Verlustfreiheitsmodell des Textes jeder Seite, Fehlerinjektion bei jedem Schritt eines Speichervorgangs, ein differenzielles nvim-Orakel und Goldens des gerenderten Bildschirms halten die Versprechen ehrlich.

## Lizenz

MIT OR Apache-2.0, nach Ihrer Wahl: [MIT](LICENSE-MIT) oder [Apache-2.0](LICENSE-APACHE). Copyright EFG Technologies.
