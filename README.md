# Rust Log-Tailer — Lern- & Nachbau-Guide

> **Ziel:** Rust lernen, indem du einen Log-Tailer zuerst als **CLI-Tool** und danach als **Browser-App (WASM)** baust.  
> Du arbeitest bewusst **manuell**: kleine, verdaubare Schritte, mit Erklärungen und zuverlässigen Quellen.

---

## Inhaltsverzeichnis
- [Voraussetzungen](#voraussetzungen)
- [Was du baust](#was-du-baut)
- [Roadmap (4–6 Wochen)](#roadmap-4–6-wochen)
  - [Woche 1 – Rust-Fundament](#woche-1--rust-fundament)
  - [Woche 2 – Mini-Projekt 1: CLI-Log-Tailer](#woche-2--mini-projekt-1-cli-log-tailer)
  - [Woche 3 – WASM-Grundlagen & Tooling](#woche-3--wasm-grundlagen--tooling)
  - [Woche 4 – Mini-Projekt 2: Log-Tailer im Browser](#woche-4--mini-projekt-2-log-tailer-im-browser)
  - [Woche 5 – UX, Performance & Worker (optional)](#woche-5--ux-performance--worker-optional)
  - [Woche 6 – Tests, Robustheit, Release](#woche-6--tests-robustheit-release)
- [Abhak-Checklisten](#abhak-checklisten)
- [Nützliche Links (kuratiert)](#nützliche-links-kuratiert)
- [Troubleshooting](#troubleshooting)
- [Lizenz](#lizenz)

---

## Voraussetzungen
- Aktuelle Rust-Toolchain via `rustup`  
  Installation: <https://rustup.rs>  
- Ein Terminal, ein Editor (z. B. VS Code), und ein moderner Browser (für die WASM-Variante).

---

## Was du baust
- **CLI-Tailer**: verhält sich ähnlich wie `tail -f`. Liest eine Datei, merkt sich ein Byte-Offset und zeigt nur neu angehängte Zeilen.
- **Browser-Tailer (WASM)**: Datei im Browser auswählen, inkrementell neue Chunks lesen, Zeilen robust zusammenbauen. Build & Live-Reload mit **Trunk**.

---

## Roadmap (4–6 Wochen)

### Woche 1 – Rust-Fundament
**Lernziele**: Ownership/Borrowing, `Result`/Fehlerbehandlung, Iteratoren, Cargo-Workflows.  
**Warum**: Saubere Basis; ohne Ownership-Verständnis wird DOM-Interop in WASM unnötig schwer.

**Aufgaben**
- [ ] Rust-Buch (The Book) lesen: Kap. 1–4 (Basics), 9–10 (Generics, Traits, Lifetimes)  
  <https://doc.rust-lang.org/book/>
- [ ] **Rustlings** bis inkl. `move_semantics`, `error_handling`, `iterators`  
  <https://github.com/rust-lang/rustlings>
- [ ] **Rust by Example** für kurze, laufbare Snippets  
  <https://doc.rust-lang.org/rust-by-example/>

**Cheat-Sheet**
- `Option`: <https://doc.rust-lang.org/std/option/>  
- `Result`: <https://doc.rust-lang.org/std/result/>  
- `Iterator`: <https://doc.rust-lang.org/std/iter/trait.Iterator.html>

---

### Woche 2 – Mini-Projekt 1: CLI-Log-Tailer
**Lernziele**: Datei-I/O, `BufRead`, `Seek`, Offsets, Zeilenpuffer, robuste Fehlerpfade.

**Start**
```bash
cargo new tailer_cli --bin
cd tailer_cli
```

**APIs, die du brauchst**
- `File`: <https://doc.rust-lang.org/std/fs/struct.File.html>  
- `BufRead`: <https://doc.rust-lang.org/std/io/trait.BufRead.html>  
- `Seek`: <https://doc.rust-lang.org/std/io/trait.Seek.html>

**Hinweise**
- Merke `last_offset: u64`. Lies nur den neuen Bereich via `SeekFrom::Start(last_offset)`.
- Puffer eine unvollständige **Restzeile**, falls ein Chunk mittig endet.
- Optional: Dateisystem-Events mit `notify` (plus Polling-Fallback):
  - Crate: <https://crates.io/crates/notify> • Doku: <https://docs.rs/notify/latest/notify/>

**Erfolgskriterium**
```bash
cargo run -- file.log
# Beim Append in file.log erscheinen neue Zeilen live.
```

*(Exkurs Performance: MMap für sehr große Dateien — nur CLI, nicht Browser)*  
`memmap2`: <https://docs.rs/memmap2/latest/memmap2/>

---

### Woche 3 – WASM-Grundlagen & Tooling
**Lernziele**: Rust→WASM, DOM/Web-APIs via `web-sys`, Promises↔Futures.

**Tooling installieren**
```bash
rustup target add wasm32-unknown-unknown
cargo install trunk
```

**Hello WASM mit Trunk**
- Trunk-Doku: <https://trunkrs.dev/>
- `index.html` mit:
  ```html
  <link data-trunk rel="rust" href="Cargo.toml">
  ```
- Starten:
  ```bash
  trunk serve --open
  ```

**Brücke Rust⇄Web**
- wasm-bindgen Guide: <https://rustwasm.github.io/wasm-bindgen/>  
- `web-sys` (DOM-APIs): <https://docs.rs/web-sys/latest/web_sys/>  
- `js-sys`: <https://docs.rs/js-sys/latest/js_sys/>  
- `wasm-bindgen-futures`: <https://docs.rs/wasm-bindgen-futures/latest/wasm_bindgen_futures/>

**Debug**
- `console_error_panic_hook`: <https://docs.rs/console_error_panic_hook/latest/console_error_panic_hook/>

---

### Woche 4 – Mini-Projekt 2: Log-Tailer im Browser
**Lernziele**: Datei im Browser auswählen, nur neue Chunks lesen (`Blob.slice`), Zeilenpuffer.

**Datei-APIs (MDN)**
- `FileReader.readAsText`: <https://developer.mozilla.org/en-US/docs/Web/API/FileReader>  
- `Blob.slice`: <https://developer.mozilla.org/en-US/docs/Web/API/Blob/slice>  
- `File`: <https://developer.mozilla.org/en-US/docs/Web/API/File>  
- **Bonus**: File System Access API (persistenter Zugriff)  
  <https://developer.mozilla.org/en-US/docs/Web/API/File_System_Access_API>

**Minimum-UI**
```html
<input id="file" type="file" />
<pre id="out"></pre>
```

**Vorgehen**
1. Beim ersten Auswählen gesamte Datei lesen und anzeigen.  
2. Anschließend im Intervall (500–1000 ms) `file.size` prüfen. Ist `size > offset`, nur `[offset..size]` per `Blob.slice` + `FileReader` lesen, anfügen, `offset` erhöhen.  
3. Unvollständige Zeile puffern (CRLF/`\n` berücksichtigen).  

**Angenehm für Timer**
- `gloo_timers` (Interval/Timeout + Cancel):  
  <https://docs.rs/gloo-timers/latest/gloo_timers/>

---

### Woche 5 – UX, Performance & Worker (optional)
**Lernziele**: Arbeit aus der UI entkoppeln, große Dateien stressfrei handhaben.

**Idee**
- Lesen/Parsen in einen **Web-Worker** auslagern, UI erhält „neue Zeilen“ per Message.
- MDN-Einstieg zu Web-Workern:  
  <https://developer.mozilla.org/en-US/docs/Web/API/Web_Workers_API/Using_web_workers>

*(Optional: Einfache UI mit Leptos)*
- <https://leptos.dev/> • Buch: <https://book.leptos.dev/>

---

### Woche 6 – Tests, Robustheit, Release
**Lernziele**: Unit-Tests, Fehlerpfade, Logging, Build/Deploy.

**Aufgaben**
- [ ] Parser-Tests (Zeilen-Split, CRLF, Restzeile).  
- [ ] Fehlerwege (Encoding, sehr lange Zeilen, Truncate/Rotation) simulieren.  
- [ ] WASM-Release-Build erstellen:
  ```bash
  trunk build --release
  ```

---

## Abhak-Checklisten

### Quickstart CLI
- [ ] `cargo new tailer_cli`  
- [ ] Lesen ab Offset (`SeekFrom::Start`)  
- [ ] Restzeilen-Puffer  
- [ ] Optional: `notify` integrieren

### Quickstart WASM
- [ ] `rustup target add wasm32-unknown-unknown`  
- [ ] `cargo install trunk`  
- [ ] `index.html` mit `<link data-trunk ...>`  
- [ ] Erstlese-Pfad (komplette Datei)  
- [ ] Tail-Pfad (Slice + Offset)  
- [ ] Timer via `gloo_timers`  
- [ ] Optional: File System Access API  
- [ ] Optional: Worker-Variante

### Lernquests
- [ ] **Ownership-Quest**: Funktion, die `String` konsumiert vs. `&str` nur leiht; Compiler-Fehler verstehen.  
- [ ] **Result-Quest**: `read_tail_chunk(path, offset) -> Result<(String, u64), io::Error>`; Fehler propagieren.  
- [ ] **Iterator-Quest**: Iterator, der Chunks in Zeilen spaltet und eine Restzeile zurückbehält.  
- [ ] **Browser-Quest**: Offset-Slicing mit `Blob.slice` + `FileReader.readAsText`; Vergleich `input[type=file]` vs. `showOpenFilePicker()`.  
- [ ] **Timer-Quest**: `window.setInterval` → `gloo_timers::callback::Interval` mit sauberem Stop.  
- [ ] **Debug-Quest**: `console_error_panic_hook` aktivieren, Panic auslösen, Stacktrace in DevTools prüfen.

---

## Nützliche Links (kuratiert)
- **The Rust Book**: <https://doc.rust-lang.org/book/>  
- **Rustlings**: <https://github.com/rust-lang/rustlings>  
- **Rust by Example**: <https://doc.rust-lang.org/rust-by-example/>  
- **Trunk** (WASM-Bundler + Dev-Server): <https://trunkrs.dev/>  
- **wasm-bindgen Guide**: <https://rustwasm.github.io/wasm-bindgen/>  
- **web-sys**: <https://docs.rs/web-sys/latest/web_sys/>  
- **js-sys**: <https://docs.rs/js-sys/latest/js_sys/>  
- **wasm-bindgen-futures**: <https://docs.rs/wasm-bindgen-futures/latest/wasm_bindgen_futures/>  
- **FileReader (MDN)**: <https://developer.mozilla.org/en-US/docs/Web/API/FileReader>  
- **Blob.slice (MDN)**: <https://developer.mozilla.org/en-US/docs/Web/API/Blob/slice>  
- **File (MDN)**: <https://developer.mozilla.org/en-US/docs/Web/API/File>  
- **File System Access API (MDN)**: <https://developer.mozilla.org/en-US/docs/Web/API/File_System_Access_API>  
- **gloo_timers**: <https://docs.rs/gloo-timers/latest/gloo_timers/>  
- **notify**: <https://docs.rs/notify/latest/notify/>  
- **memmap2** (Exkurs CLI-Performance): <https://docs.rs/memmap2/latest/memmap2/>  
- **Web-Worker (MDN)**: <https://developer.mozilla.org/en-US/docs/Web/API/Web_Workers_API/Using_web_workers>

---

## Troubleshooting
- **WASM lädt nicht / weiße Seite:**  
  Prüfe `trunk serve`-Logs und Browser-Konsole, aktiviere `console_error_panic_hook`.  
- **Nichts passiert beim Tailen:**  
  Verifiziere `offset`-Logik; ändere die Datei wirklich (Append), nicht nur speichern ohne Änderungen.  
- **Große Dateien ruckeln:**  
  Größere Intervalle wählen (z. B. 1000 ms), Worker-Variante erwägen, Zeilenpuffer begrenzen (z. B. nur letzte N Zeilen anzeigen).  
- **File System Access API funktioniert nicht:**  
  Nur über HTTPS/„secure context“ und in unterstützten Browsern verfügbar.

---

## Lizenz
Wähle eine passende Lizenz (z. B. MIT/Apache-2.0) und füge sie dem Repo hinzu.

---

**Viel Erfolg!** Wenn du magst, können wir die **Beispiel-Projektstruktur** (CLI + WASM, zwei Crates in einem Workspace) ergänzen oder eine kleine **Starter-Implementierung** für den Tail-Loop in beiden Varianten beilegen.
