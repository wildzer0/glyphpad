# Progetto: GlyphPad — motore di gesture personalizzabili per touchpad

> Specifica di progetto, riferimento vincolante per tutto lo sviluppo. Leggila tutta prima di scrivere codice.
> Se qualcosa è ambiguo, contraddittorio o tecnicamente sbagliato, chiedimelo invece di decidere da solo.

## 1. Obiettivo

Un'applicazione in **Rust** che legge i dati grezzi multi-touch di un touchpad (posizione e stato di ogni singolo dito) e riconosce gesture definite dall'utente. Le gesture vanno da quelle classiche (swipe, tap, pinch) a quelle complesse, come **simboli disegnati** sul touchpad. A ogni gesture riconosciuta il programma esegue un'azione configurabile.

Casi d'uso di riferimento:
- disegno una **L**, il PC si blocca (equivalente di Win+L);
- disegno una **q** o una **X**, l'applicazione in primo piano si chiude;
- tap con 4 dita, apre un'applicazione;
- swipe a 5 dita verso l'alto, esegue uno script.

Il mio hardware è un touchpad Amazon Basics riconosciuto da Windows come **Windows Precision Touchpad (PTP)**.

## 2. Requisiti non funzionali

- **Linguaggio:** Rust, edizione stabile più recente, nessun `unsafe` fuori dai moduli FFI di piattaforma.
- **Portable:** un singolo eseguibile, nessun installer, **nessun privilegio di amministratore** su Windows.
  - Su Windows il CRT va linkato staticamente (`+crt-static`).
  - **Modalità portable:** se accanto all'eseguibile esiste `glyphpad.toml`, si usa quello (insieme a template e log nella stessa cartella). Altrimenti si usa la cartella di config standard dell'OS (crate `directories`).
- **Multipiattaforma: Windows e Linux**, con priorità a **Windows** (MVP, completo). **macOS è esplicitamente fuori scope**: niente codice, dipendenze o `cfg` per macOS. L'architettura a trait deve comunque permettere di aggiungerlo in futuro senza toccare il core.
- **Leggero:** idle sotto l'1% di CPU, poche decine di MB di RAM, latenza di riconoscimento sotto i 50 ms dalla fine della gesture.
- **Privacy:** l'unica connessione di rete ammessa è il controllo degli aggiornamenti verso GitHub (§12), disattivabile. Nessuna telemetria. Non vengono mai registrati input da tastiera, solo i dati del touchpad.
- **Robustezza:** se il touchpad viene scollegato e ricollegato (anche via Bluetooth), il programma si riaggancia da solo senza crashare.

## 3. Architettura richiesta

Workspace Cargo con separazione netta tra il core, indipendente dalla piattaforma, e i backend:

```
glyphpad/
├─ crates/
│  ├─ core/        # modello dati, gesture engine, recognizer, config — ZERO dipendenze OS
│  ├─ platform/    # trait + backend: windows / linux (cfg-gated)
│  ├─ actions/     # esecuzione azioni, per piattaforma
│  ├─ gui/         # finestra egui: gesture, rivedi gesture, impostazioni, debug live
│  ├─ updater/     # controllo release, download, verifica, sostituzione eseguibile (§12)
│  └─ app/         # binario: CLI, tray icon, loop principale, hot reload
├─ xtask/         # automazioni di progetto (es. `cargo xtask release`)
├─ .github/workflows/  # ci.yml, release.yml (§11)
└─ tests/fixtures/ # sessioni touch registrate (vedi §8)
```

### 3.1 Modello dati normalizzato (core)

Ogni backend produce un flusso di `TouchFrame`:

```rust
struct TouchFrame {
    timestamp_us: u64,          // monotono; usa lo Scan Time HID se disponibile
    contacts: Vec<Contact>,     // supportare fino a 10 contatti
    button_pressed: bool,       // click fisico del touchpad
}
struct Contact {
    id: u32,                    // ID stabile del dito finché resta appoggiato
    x_mm: f32, y_mm: f32,       // coordinate in millimetri (non in unità del dispositivo)
    x_norm: f32, y_norm: f32,   // 0..1
    tip: bool,                  // dito appoggiato
    confident: bool,            // false = palmo o contatto accidentale
}
struct DeviceInfo { width_mm, height_mm, max_contacts, name, vendor_id, product_id }
```

**Tutte le soglie** (distanze, velocità) vanno espresse in **millimetri**, così la configurazione resta valida su touchpad di dimensioni diverse.

### 3.2 Trait di piattaforma

```rust
trait TouchSource { fn devices(&self) -> Vec<DeviceInfo>; fn run(self, tx: Sender<DeviceEvent>); }
trait ActionExecutor { fn execute(&self, action: &Action) -> Result<()>; }
trait ForegroundInfo { fn foreground_app(&self) -> Option<AppInfo>; } // per le condizioni per-app
```

## 4. Backend Windows (MVP) — dettagli tecnici

- **Raw Input:** `RegisterRawInputDevices` su Usage Page `0x0D` (Digitizer), Usage `0x05` (Touch Pad), con `RIDEV_INPUTSINK | RIDEV_DEVNOTIFY` su una finestra nascosta. Verifica che con una message-only window (`HWND_MESSAGE`) l'input arrivi davvero in background; se non arriva, usa una finestra top-level invisibile.
- Usa il crate **`windows`** (windows-rs), non `winapi`.
- **Parsing dei report** con le API HID (`HidP_GetCaps`, `HidP_GetValueCaps`, `HidP_GetUsageValue`, `HidP_GetUsages`), partendo dai *preparsed data* (`GetRawInputDeviceInfo` con `RIDI_PREPARSEDDATA`). **Non** fare hardcoding degli offset dei byte.
  - Ogni dito è una *link collection* separata (Finger, `0x0D/0x22`).
  - Usage da leggere: Contact Count `0x54`, Contact ID `0x51`, Tip Switch `0x42`, Confidence `0x47`, X/Y (Generic Desktop `0x30`/`0x31`), Scan Time `0x56`, Button 1.
  - Normalizza le coordinate con logical min/max; ricava i millimetri da physical min/max, unit ed exponent.
  - Leggi **Contact Count Maximum** (`0x55`) per sapere quante dita supporta il dispositivo.
- **Hybrid reporting mode:** molti PTP inviano i contatti di un frame su **più report**. Solo il primo report porta il Contact Count reale, i successivi riportano 0. L'assemblatore deve accumulare i contatti fino a completare il frame. Gestisci anche `dwCount > 1` in `RAWHID`, cioè più report nello stesso messaggio.
- **Hot-plug:** gestisci `WM_INPUT_DEVICE_CHANGE`.
- **Azioni:**
  - `LockWorkStation()` per il blocco;
  - chiusura app: `GetForegroundWindow` + `PostMessage(WM_CLOSE)` di default, con opzione `force` per terminare il processo;
  - tasti e scorciatoie via `SendInput`.
  - Nota: Win+L **non** è iniettabile via SendInput, quindi per il blocco usa l'API dedicata.
- **App in primo piano:** nome dell'eseguibile e titolo della finestra, per le condizioni per-app.

## 5. Il problema centrale: disegnare senza muovere il cursore

Raw Input **legge** i dati ma **non li blocca**. Windows continua a muovere il cursore, fare scroll e interpretare le sue gesture native. Un simbolo disegnato con un dito sposterebbe il puntatore e un tap farebbe clic.

Implementa **più strategie di attivazione del "draw mode"**, configurabili, e valutale sul campo:

1. **Disegno a N dita** (default: 3). Il tratto è il centroide delle dita appoggiate. Le gesture native a 3 dita vanno disattivate in Impostazioni → Touchpad, e il programma deve avvisare l'utente se non lo sono.
2. **Avvio da bordo o angolo:** il tratto inizia in una zona configurabile (es. il 10% sinistro del touchpad).
3. **Hold-to-draw:** dito fermo per X ms, poi disegno.
4. **Toggle:** una gesture (es. tap a 4 dita) attiva la modalità disegno per il simbolo successivo.
5. **Sperimentale, solo Windows:** durante il draw mode, congela il cursore con `ClipCursor` su un rettangolo 1×1 nella posizione corrente e ripristinalo alla fine. Valuta gli effetti collaterali (tap involontari, ripristino in caso di crash: usa un guard RAII e un ripristino all'avvio).

Il **filter driver** in kernel mode è fuori scope.

## 6. Gesture engine (core)

### 6.1 Tipi di gesture da supportare
- **Tap** e **multi-tap** con N dita (1–10), con soglie di durata e movimento.
- **Hold** con N dita.
- **Swipe** con N dita in 4 o 8 direzioni, con distanza e velocità minime.
- **Pinch** e **rotate** (2+ dita).
- **Edge swipe**: swipe che parte da un bordo.
- **Simboli disegnati** (§6.2), sia **unistroke** che **multistroke**.
- **Sequenze e combo:** gesture B entro T ms dopo la gesture A, oppure un dito fermo come "modificatore" mentre le altre dita fanno una gesture (chord).
- **Click fisico + gesture.**

Implementa ogni riconoscitore come **macchina a stati esplicita**, testabile in isolamento. Più riconoscitori girano in parallelo sullo stesso flusso; serve una **politica di disambiguazione** chiara (priorità, "la gesture più specifica vince", attesa di un timeout prima di confermare un tap singolo se esiste un doppio tap configurato).

### 6.2 Riconoscimento dei simboli
- Usa la famiglia **$-recognizer**: **$P** o **$Q** (point-cloud, multistroke, indipendenti dall'ordine e dalla direzione dei tratti). Valuta anche **$1** come opzione per unistroke dove la direzione conta.
- **Multistroke:** una X sono due tratti. I tratti si raggruppano in un unico simbolo se la pausa tra loro è sotto una soglia (default 400 ms).
- **Invarianza alla rotazione disattivata** di default (una L ruotata non è una L), con tolleranza configurabile.
- **Più template per simbolo** e **soglia di confidenza** per gesture: sotto la soglia non succede nulla e nel log compare il miglior candidato con il suo punteggio.
- **Modalità registrazione:** `glyphpad record <nome>` registra N esempi di un simbolo e li salva come template. Includi template di default per L, Q, X, C, V, Z, O, freccia su/giù.
- Prevedi rumore e smoothing: ricampionamento, filtro sui micro-movimenti, scarto dei contatti con `confident = false`.

## 7. Altri backend

### Linux
- **evdev** (crate `evdev`), Multi-Touch protocol B: `ABS_MT_SLOT`, `ABS_MT_TRACKING_ID`, `ABS_MT_POSITION_X/Y`, risoluzione da `input_absinfo` per i millimetri.
- Lettura **senza** `EVIOCGRAB` di default. Il grab, opzionale, sopprime l'input verso libinput: utile per il draw mode, ma blocca il cursore.
- Azioni via **uinput**, che funziona sia su X11 che su Wayland. Blocco schermo con `loginctl lock-session`.
- Permessi: documenta la regola udev e il gruppo `input`.
- Tray icon su Linux: `tray-icon` richiede GTK e AppIndicator/StatusNotifier. Documenta le dipendenze e fai in modo che, se la tray non è disponibile (alcuni ambienti GNOME senza estensione), l'app funzioni comunque con la sola finestra.

## 8. Testabilità (fondamentale)

Io sviluppo su Windows con il touchpad fisico, ma tu potresti non avere accesso all'hardware.

- `glyphpad capture <file.jsonl>`: registra i `TouchFrame` normalizzati su file.
- Il core deve poter **riprodurre** una cattura (`ReplaySource`) in modo deterministico.
- **Test di regressione** su catture reali in `tests/fixtures/`, una per gesture, con il risultato atteso.
- Unit test per ogni macchina a stati, test property-based (`proptest`) per il recognizer (traslazione e scala non devono cambiare il risultato).
- `glyphpad dump-descriptor`: stampa caps e value caps HID del dispositivo, per il debug su hardware nuovo.
- `glyphpad debug`: apre direttamente la scheda Debug live della GUI (§10.1) e stampa nel terminale le gesture candidate con i punteggi.

Quando un passo richiede il mio hardware, **fermati e dimmi esattamente quale comando lanciare** e cosa rimandarti.

## 9. Configurazione

Il TOML è la **fonte di verità**, ma l'utente non deve mai essere costretto a modificarlo a mano: tutto ciò che si configura nel file si configura anche dalla GUI (§10.2).

TOML con **hot reload** (crate `notify`) e validazione con errori leggibili: riga, campo, motivo. Un errore nella config non deve mai far crashare il programma; resta attiva l'ultima config valida.

Esempio indicativo, da migliorare:

```toml
[settings]
draw_activation = { mode = "fingers", count = 3 }   # fingers | edge | hold | toggle
stroke_gap_ms = 400
default_confidence = 0.80

[[gesture]]
name = "lock"
type = "symbol"
symbol = "L"
action = { type = "lock_screen" }

[[gesture]]
name = "close-app"
type = "symbol"
symbol = ["q", "X"]
confidence = 0.85
action = { type = "close_window", force = false }

[[gesture]]
name = "terminal"
type = "tap"
fingers = 4
action = { type = "run", command = "wt.exe" }

[[gesture]]
name = "next-tab-in-browser"
type = "swipe"
fingers = 3
direction = "right"
when = { app = ["chrome.exe", "firefox.exe"] }
action = { type = "keys", combo = "Ctrl+Tab" }
```

Azioni minime:
- `keys` (combo, sequenze);
- `lock_screen`;
- `close_window`, `minimize`, `maximize`;
- `run` (comando e argomenti, opzionalmente nascosto);
- `media` (play/pausa, volume);
- `chain` (sequenza di azioni con delay);
- `notify` (toast, utile per il debug);
- `toggle_enabled` (mette in pausa tutte le gesture);
- `repeat_last` (ripete l'ultima azione eseguita, vedi §9.1);
- `show_window` (apre o porta in primo piano la GUI).

Condizioni: app in primo piano (exe o regex sul titolo), profilo attivo.

### 9.1 Ripeti ultima gesture

Una gesture del touchpad configurabile (esempio: doppio tap a 3 dita) esegue l'azione `repeat_last`, che **riesegue l'azione dell'ultima gesture riconosciuta**. L'azione viene rieseguita direttamente, senza ripetere il riconoscimento.

Regole:
- `repeat_last` non registra mai sé stessa come "ultima azione": ripeterla più volte di fila ripete sempre la stessa azione originale.
- **Esclusioni configurabili:** di default `lock_screen`, `toggle_enabled` e `show_window` non sono ripetibili. Ogni gesture può dichiarare `repeatable = false`.
- **Scadenza opzionale** (`repeat_timeout_s`): oltre questo tempo la ripetizione non fa nulla e viene registrata nel log.
- **Contesto app** (`repeat_same_app_only`, default `true`): se l'app in primo piano è diversa da quella in cui è stata eseguita la gesture originale, non ripetere e mostra una notifica. Serve a evitare, per esempio, di chiudere per sbaglio un'altra applicazione con una `close_window` ripetuta.
- L'ultima gesture con la sua traccia resta in memoria ed è visibile nella GUI (§10.1).

```toml
[settings]
repeat_timeout_s = 60
repeat_same_app_only = true

[[gesture]]
name = "repeat"
type = "tap"
fingers = 3
taps = 2
action = { type = "repeat_last" }
```

## 10. Applicazione

### 10.1 GUI

Una GUI piccola e leggera con **egui/eframe** (backend `glow` o `wgpu`; scegli quello che pesa meno sul binario e motiva la scelta). Requisiti:

**Comportamento finestra e tray**
- **Tray icon** (crate `tray-icon`) con menu: Apri, Abilita/Disabilita gesture, Ricarica config, Apri cartella config, Controlla aggiornamenti, Esci.
- `close_to_tray = true | false` in config, modificabile anche dalla GUI.
  - Con `true`, la X della finestra **nasconde** la finestra e l'app resta attiva nella tray. Con `false`, la X chiude l'applicazione.
  - Click (o doppio click) sull'icona della tray riapre la finestra.
- `start_minimized = true | false`: avvio direttamente in tray.
- **Il motore delle gesture non dipende dalla finestra:** gira sempre in un thread separato e continua a funzionare con la finestra nascosta. Quando è nascosta, la GUI non deve consumare CPU (niente repaint continui).
- Attenzione all'integrazione tra il loop di eframe/winit e la tray. Su Windows `tray-icon` ha bisogno di un message loop sullo stesso thread. Nascondere la finestra invece di chiuderla va fatto intercettando la richiesta di chiusura (`ViewportCommand::CancelClose` + `Visible(false)`). Verifica che la finestra si riapra correttamente dopo essere stata nascosta: è un punto noto di fragilità.
- **Seconda istanza:** se si avvia l'app mentre è già in esecuzione, la nuova istanza segnala a quella esistente di mostrare la finestra e poi termina.

**Schede**
1. **Gesture:** l'editor completo delle gesture e dei profili (§10.2), da cui parte anche la registrazione di nuove gesture (§10.3).
2. **Rivedi gesture:** visualizzazione grafica delle gesture registrate.
   - Per ogni simbolo mostra **tutti i template salvati**, ognuno disegnato su una sagoma del touchpad con le proporzioni reali del dispositivo.
   - **Riproduzione animata** del tratto con le tempistiche reali: punto di partenza evidenziato, direzione con frecce o gradiente di colore, tratti multipli in colori diversi con il numero d'ordine.
   - Controlli: play/pausa, velocità (0.25×–2×), loop, scorrimento manuale con slider.
   - Per le gesture non a simbolo (swipe, pinch, tap…) mostra una ricostruzione dei percorsi di **ogni dito**, con colore distinto per contatto.
   - **Cronologia** delle ultime N gesture eseguite (default 20): ora, gesture riconosciuta, punteggio di confidenza, azione eseguita, app in primo piano. Selezionando una voce si rivede la traccia eseguita **sovrapposta al template** che l'ha fatta riconoscere. È utile per capire perché una gesture viene riconosciuta male.
   - Dalla cronologia si può aggiungere una traccia come **nuovo template** del simbolo, oppure assegnarla a un simbolo diverso se il riconoscimento era sbagliato.
   - Si possono eliminare i template e aggiungerne di nuovi con il flusso di registrazione (§10.3).
3. **Impostazioni:** tutte le opzioni di `[settings]` e `[update]`, raggruppate per area, con le più tecniche in una sezione "Avanzate" chiusa di default. Tra queste: strategia di draw mode, soglie principali, `close_to_tray`, `start_minimized`, avvio automatico, opzioni di `repeat_last`, aggiornamenti (modalità, canale, pulsante "Controlla ora", versione installata e data dell'ultimo controllo). Quando c'è un aggiornamento disponibile, la finestra mostra un banner con versione, note di rilascio e i pulsanti Installa / Ignora questa versione.
4. **Debug live:** dita in tempo reale con ID e coordinate, tratto corrente, candidati del recognizer con punteggio, informazioni sul dispositivo (dimensioni in mm, numero massimo di contatti).

Il formato su disco dei template e della cronologia deve contenere i timestamp di ogni punto, così la riproduzione animata è fedele. La cronologia è solo in memoria di default; il salvataggio su disco è attivabile da config.

### 10.2 Gestione della configurazione dalla GUI

La GUI è un **editor completo** della configurazione: non esistono opzioni modificabili solo dal file.

- **Elenco gesture:**
  - ricerca e filtri per tipo, profilo e stato;
  - abilitazione e disabilitazione rapida;
  - crea, modifica, duplica, elimina (con conferma);
  - **priorità** modificabile con drag & drop: è l'ordine usato dalla disambiguazione del §6.1.
- **Editor della gesture:** un form che si adatta al tipo (dita, direzione, soglie, simbolo con anteprima del template). Ogni soglia mostra il valore di default e ha un pulsante "Ripristina default".
- **Editor delle azioni:**
  - per `keys`, un campo che **cattura la scorciatoia** premuta. Alcune combinazioni, come Win+L, vengono intercettate dal sistema prima dell'app: va spiegato all'utente;
  - per `run`, selettore del file, argomenti e opzioni;
  - per `chain`, una lista ordinabile di azioni con i ritardi;
  - per ogni azione, un pulsante **"Esegui ora"** per provarla.
- **Condizioni:** selezione dell'app dall'elenco dei processi con finestre aperte, oppure inserimento manuale (nome exe o regex sul titolo).
- **Profili:** crea, rinomina, elimina, scegli quello attivo.
- **Validazione in tempo reale** nei form, con gli stessi controlli di `check-config`. Una configurazione non valida non si può salvare.
- **Salvataggio:**
  - le modifiche restano in bozza, con un indicatore "modifiche non salvate" e i comandi Salva e Annulla modifiche;
  - Ctrl+Z / Ctrl+Y nella sessione di modifica;
  - scrittura **atomica** (file temporaneo + rename) con `toml_edit`, **preservando commenti e formattazione**;
  - **backup automatico** delle ultime N versioni (default 10) in `backups/`, ripristinabili dalla GUI.
- **Modifiche esterne:** l'hot reload aggiorna la GUI. Se nella GUI ci sono modifiche non salvate e intanto il file cambia sul disco, chiedi all'utente se ricaricare dal file, sovrascriverlo o vedere le differenze.

### 10.3 Registrazione delle gesture dalla GUI

Una procedura guidata, avviabile da Gesture ("Nuova gesture") e da Rivedi gesture ("Aggiungi template"):

1. **Tipo:** simbolo disegnato, oppure gesture "per dimostrazione" (tap, swipe, pinch…).
2. **Registrazione:**
   - un'area con le proporzioni reali del touchpad mostra dita e tratto in tempo reale;
   - riconoscimento e azioni sono **sospesi** durante la registrazione, così non parte nulla per errore;
   - deve essere chiaro quando un campione inizia e finisce (fine = pausa oltre `stroke_gap_ms`);
   - si registra con la strategia di draw mode configurata, cioè nello stesso modo in cui si userà la gesture.
3. **Campioni:** per i simboli da 3 a 5 esempi (configurabile). Ogni campione si può rivedere animato, scartare o rifare.
4. **Controllo qualità** prima di salvare:
   - coerenza tra i campioni, con segnalazione di quello anomalo;
   - **somiglianza con i simboli esistenti:** se il nuovo simbolo rischia di essere confuso con uno già presente, avvisa e mostra il confronto affiancato;
   - per le gesture per dimostrazione, l'app **deduce** tipo, numero di dita, direzione e soglie e precompila il form (correggibile), segnalando i conflitti con le gesture già configurate.
5. **Nome, azione e condizioni** con lo stesso editor del §10.2. Il salvataggio scrive template e gesture nel TOML in un'unica operazione.
6. **Prova:** subito dopo il salvataggio, propone di eseguire la gesture qualche volta in modalità prova (§13) per verificare che venga riconosciuta.

Il comando `glyphpad record` resta disponibile come alternativa da CLI.

### 10.4 Altro
- Una sola istanza alla volta (vedi sopra per l'attivazione della finestra).
- Avvio automatico opzionale, attivabile da config: chiave `Run` in HKCU su Windows, `.desktop` in autostart su Linux.
- Logging con `tracing`, su file ruotato in modalità portable.
- CLI con `clap`: `run` (default), `record`, `capture`, `replay`, `dump-descriptor`, `debug`, `check-config`, `update` (con `--check` per la sola verifica), `--version`.
- Su Windows niente console nel binario di release (`windows_subsystem = "windows"`), ma i sottocomandi CLI devono comunque poter stampare (attach alla console del parent).

## 11. Repository, CI e release

### 11.1 Git
- Il progetto è un repository git dal primo commit. Il remote GitHub te lo indico all'avvio; finché non lo conosci, usa il segnaposto `<OWNER>/glyphpad` in un unico punto del codice.
- `.gitignore` per Rust, che escluda anche config, log e catture personali (ma non `tests/fixtures/`). `.gitattributes` con `* text=auto` e fine riga LF per `.jsonl`, `.toml` e `.md`.
- Commit piccoli e atomici con messaggi **Conventional Commits** (`feat:`, `fix:`, `refactor:`, `test:`, `ci:`, `docs:`, `chore:`).
- **Si lavora solo sul branch `main`**: mai creare branch, worktree o pull request.
- Mai push forzati su `main`, mai riscrivere storia già pubblicata.
- **Versioning SemVer.** La versione vive in un solo posto, `[workspace.package] version` nel `Cargo.toml` radice, ereditata da tutti i crate e mostrata da `--version` e nella GUI.
- `CHANGELOG.md` in formato *Keep a Changelog*, con la sezione `Unreleased` aggiornata a ogni modifica rilevante.
- **Procedura di release** automatizzata con `cargo xtask release <versione>`, che:
  1. verifica che il working tree sia pulito e che fmt, clippy e test passino;
  2. aggiorna la versione e sposta `Unreleased` nella nuova sezione del changelog;
  3. crea il commit `chore(release): vX.Y.Z` e il **tag annotato** `vX.Y.Z`.

  Il push lo faccio io, oppure tu dopo una mia conferma esplicita.

### 11.2 Pipeline CI (`.github/workflows/ci.yml`)
- Si attiva a ogni push su `main`. Esegue `cargo fmt --check`, `clippy -D warnings` e i test, su `windows-latest` e `ubuntu-latest`.
- Cache delle build con `Swatinem/rust-cache`. Versioni delle action fissate, preferibilmente allo SHA del commit.
- Su Linux installa le dipendenze di sistema per GUI e tray (GTK, AppIndicator).

### 11.3 Pipeline di release (`.github/workflows/release.yml`)
Si attiva **solo** con il push di un tag `v*.*.*` (anche con suffisso, es. `v0.3.0-beta.1`).

1. **Controllo di coerenza:** il tag deve coincidere con la versione nel `Cargo.toml`, altrimenti la pipeline fallisce.
2. **Build release** (LTO, `strip`, `codegen-units = 1`) per:
   - `x86_64-pc-windows-msvc` (obbligatorio);
   - `x86_64-unknown-linux-gnu`, compilato sulla Ubuntu LTS più vecchia disponibile tra i runner, per massimizzare la compatibilità con glibc;
   - `aarch64-pc-windows-msvc` come target opzionale, da valutare.
3. **Pacchetti** con nome fisso e prevedibile, perché l'updater li cerca per nome: `glyphpad-<versione>-<target>.zip` per Windows e `.tar.gz` per Linux. Contengono l'eseguibile, `glyphpad.example.toml`, `README.md` e `LICENSE`.
4. **`SHA256SUMS`** con l'hash di tutti gli asset, più la sua firma `SHA256SUMS.minisig` (§12.3).
5. **GitHub Release** creata con `GITHUB_TOKEN` (`permissions: contents: write`). Le note vengono dalla sezione del `CHANGELOG.md` relativa alla versione. I tag con suffisso (`-alpha`, `-beta`, `-rc`) vengono pubblicati come **pre-release**.

Firma del codice (Authenticode): per ora non c'è. Documenta nel README che SmartScreen può mostrare un avviso al primo avvio. Struttura la pipeline in modo che aggiungere la firma in futuro richieda un solo step.

## 12. Aggiornamento automatico

### 12.1 Comportamento
- **Fonte:** le GitHub Releases del repository del progetto. Owner e nome del repo sono **fissati a compile time** e non modificabili da config, così nessuno può far puntare l'updater a un altro repository.
- **Modalità** (`[update] mode` in config):
  - `off`: nessuna connessione automatica; il controllo manuale resta disponibile;
  - `notify` (default): controlla e avvisa (notifica e banner nella GUI), decide l'utente;
  - `auto`: scarica, installa e riavvia da solo, poi notifica "Aggiornato alla vX.Y.Z".
- **Quando:** all'avvio, con qualche secondo di ritardo per non rallentarlo, e poi ogni `check_interval_h` (default 24).
- **Controllo manuale:** pulsante nelle Impostazioni, voce nel menu della tray e comando `glyphpad update`.
- **Canale** `stable` (default) o `prerelease`.
- Confronto delle versioni con il crate `semver`. **Mai downgrade.**
- In modalità `notify` mostra le note di rilascio prima di installare, e permette "Ignora questa versione".

```toml
[update]
mode = "notify"          # off | notify | auto
channel = "stable"       # stable | prerelease
check_interval_h = 24
```

### 12.2 Download e sostituzione dell'eseguibile
- Client HTTP con **rustls**, niente OpenSSL, per restare portable. Valuta `self_update` (backend GitHub) insieme a `self-replace`. Se `self_update` non permette la verifica della firma come richiesto al §12.3, usa `ureq` + `self-replace` con logica propria.
- Seleziona l'asset del target corrente in base al nome (§11.3).
- Scarica in una cartella temporanea **accanto all'eseguibile** (stesso volume, così la rinomina è atomica). Verifica firma e SHA256, poi estrai.
- **Mai aggiornare durante una gesture** o con il draw mode attivo; prima dell'aggiornamento rilascia l'eventuale `ClipCursor`.
- **Sostituzione:** su Windows un eseguibile in esecuzione non si può sovrascrivere, ma si può rinominare. Rinomina il corrente in `glyphpad.old.exe` e metti il nuovo al suo posto (`self-replace` gestisce questi dettagli).
- **Riavvio con rollback:**
  1. il vecchio processo rilascia il lock di single-instance e avvia il nuovo eseguibile con gli stessi argomenti più `--post-update`;
  2. il nuovo processo, dopo un'inizializzazione riuscita (config caricata, motore avviato), lo segnala al vecchio, per esempio con un file marker o tramite il canale IPC della single-instance;
  3. il vecchio processo attende fino a 15 secondi. Se riceve il segnale, esce. Se non lo riceve (il nuovo processo crasha o non risponde), lo termina, **ripristina il vecchio eseguibile**, riprende il lock e continua a funzionare, notificando il fallimento;
  4. il nuovo processo elimina `glyphpad.old*` all'avvio successivo andato a buon fine.
- **Cartella non scrivibile** (es. `Program Files`): niente fallimenti silenziosi. Mostra una notifica con il link alla release per l'aggiornamento manuale.
- **Linux:** stesso meccanismo sul binario. Se è in una directory di sistema non scrivibile, solo notifica.
- **Compatibilità della config:** il TOML contiene `config_version`. Quando il formato cambia tra versioni, la migrazione è automatica, previo backup del file.

### 12.3 Sicurezza
- L'hash in `SHA256SUMS` protegge dai download corrotti, ma non da un account GitHub compromesso. Per questo `SHA256SUMS` viene **firmato con minisign**:
  - la chiave privata sta nei secret di GitHub Actions;
  - la chiave pubblica è incorporata nell'eseguibile;
  - la verifica usa il crate `minisign-verify`;
  - firma non valida significa aggiornamento rifiutato e registrato nel log.
- **Non generare tu le chiavi** e non metterle mai nel repository. Spiegami come generarle e quali secret creare.
- Rispetta il rate limit dell'API GitHub non autenticata (60 richieste/ora): usa `ETag`/`If-None-Match` e un backoff su 403/429. Nessun token incorporato nel binario.
- User-Agent: `glyphpad/<versione>`.

### 12.4 Test
- La sorgente degli aggiornamenti sta dietro un trait `UpdateSource`, con un'implementazione GitHub e una locale (cartella con asset finti) per i test.
- Flag nascosto `--update-source <dir>`, presente solo nelle build di debug.
- **Test end-to-end su Windows in CI:**
  - aggiornamento da una versione fittizia alla successiva tramite sorgente locale, con verifica della sostituzione;
  - **rollback** con un binario volutamente rotto;
  - rifiuto di un pacchetto con firma non valida.

## 13. Funzionalità aggiuntive

Funzionalità non richieste esplicitamente ma che rendono l'app molto più usabile. Sono in ordine di priorità.

### Alta priorità (dentro M3–M5)
- **Overlay di feedback:**
  - una finestra trasparente, sempre in primo piano, che non riceve né clic né focus (`WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW`);
  - disegna la scia del tratto durante il draw mode;
  - mostra per un attimo il nome della gesture riconosciuta, oppure "non riconosciuta" con il candidato migliore;
  - posizione, durata e opacità configurabili, disattivabile.
- **Modalità prova:** dalla GUI esegui gesture e vedi cosa verrebbe riconosciuto e con quale punteggio, senza eseguire le azioni.
- **Rilevamento conflitti:** avvisa quando due gesture possono scattare con lo stesso movimento (stesso tipo, dita, direzione e condizioni che si sovrappongono), o quando due simboli sono troppo simili.
- **Pausa automatica:**
  - gesture sospese quando è in primo piano un'app in una lista configurabile, o un'app a schermo intero (giochi, presentazioni);
  - l'icona della tray cambia quando le gesture sono in pausa.

### Media priorità (M8)
- **Cheat sheet:** una gesture configurabile (es. hold a 5 dita) mostra un overlay con tutte le gesture attive nel profilo corrente e le loro anteprime.
- **Profili:** commutabili dalla tray o con una gesture, con cambio automatico in base all'app in primo piano.
- **Import/export** di pacchetti di gesture (archivio con frammento TOML e template), per backup o per condividerli. All'import: anteprima e risoluzione dei conflitti.
- **Azioni aggiuntive:**
  - `open`: file, cartella o URL con l'applicazione predefinita;
  - `type_text`: digita un testo;
  - `window`: affianca a sinistra o a destra, sposta su un altro monitor, sempre in primo piano;
  - `virtual_desktop`: precedente, successivo, nuovo;
  - `screenshot`.
- **Statistiche locali:** quante volte viene usata ogni gesture e quante non vengono riconosciute, per capire cosa migliorare. I dati restano solo sulla macchina.
- **Lingua e tema:** interfaccia in italiano e in inglese (di default la lingua del sistema, modificabile); tema chiaro o scuro che segue il sistema.

## 14. Milestone suggerite

1. **M0:** repository git, workspace, `CLAUDE.md`, `CHANGELOG.md`, README, `cargo xtask release`, CI (§11.2) e workflow di release (§11.3), per ora senza firma minisign. Collaudo della pipeline con il tag `v0.0.1-alpha.1`: deve produrre una pre-release con gli asset di un binario minimale.
2. **M1:** backend Windows, `dump-descriptor` + `capture` + `debug`. Verifico io sul mio hardware che i contatti arrivino corretti, incluso l'hybrid mode e il numero massimo di dita.
3. **M2:** gesture engine con tap, swipe e hold, più azioni base e config TOML. Replay test.
4. **M3:** riconoscimento simboli ($P/$Q), `record`, template di default, strategie di draw mode. Obiettivo: i casi L, q e X funzionano in modo affidabile.
5. **M4:** GUI base. Tray con `close_to_tray`, scheda Debug live, scheda Rivedi gesture con riproduzione animata dei template e cronologia. Azione `repeat_last`. Registrazione delle gesture dalla GUI (§10.3), overlay di feedback e modalità prova (§13).
6. **M5:** pinch, rotate, edge, sequenze e chord, condizioni per-app, hot reload. Editor completo della configurazione dalla GUI (§10.2), rilevamento conflitti e pausa automatica (§13).
7. **M6:** aggiornamento automatico (§12), firma minisign nella pipeline, test di sostituzione e rollback.
8. **M7:** backend Linux, comprese GUI e tray.
9. **M8:** funzionalità di media priorità del §13.

## 15. Linee guida di lavoro

- Prima di ogni milestone proponi il piano. Dopo ogni milestone: test verdi, `clippy` pulito, breve changelog.
- Preferisci crate maturi e mantenuti; giustifica ogni dipendenza non ovvia.
- Commenta il *perché* nelle parti FFI e nelle soglie del recognizer.
- Non inventare API di sistema: se non sei sicuro di una firma o di un comportamento, verificalo nella documentazione o scrivi un piccolo test.
- Tieni un `docs/` con l'architettura e con i limiti noti, in particolare quelli del §5 e quello sul numero massimo di dita.
- Non aggiungere supporto a macOS, nemmeno parziale o "predisposto" con `cfg`.
- **Git:** lavora solo su `main`, senza mai creare branch. Committa a ogni passo logico completato. **Chiedimi conferma prima di ogni push e prima di creare o pushare un tag.** Non committare mai segreti, chiavi o file personali.