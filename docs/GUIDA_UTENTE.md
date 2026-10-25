# Guida utente di GlyphPad

Questa guida ti porta da «ho appena scaricato il programma» a «il mio touchpad fa quello che
voglio». Non presuppone niente: se sai usare un mouse e una casella di testo, sai usare
GlyphPad. Tutto ciò che si fa nella finestra si può fare anche modificando `glyphpad.toml`; il
[riferimento della configurazione](CONFIGURATION.md) (in inglese) elenca ogni opzione.

*(English version: [User guide](USER_GUIDE.md).)*

**Indice**

1. [Come funziona](#1-come-funziona)
2. [Installazione e avvio](#2-installazione-e-avvio)
3. [Prima, sistema Windows](#3-prima-sistema-windows)
4. [Un giro della finestra](#4-un-giro-della-finestra)
5. [La tua prima gesture](#5-la-tua-prima-gesture)
6. [I tipi di gesture](#6-i-tipi-di-gesture)
7. [Disegnare simboli](#7-disegnare-simboli)
8. [Le azioni](#8-le-azioni)
9. [Dove e quando funziona una gesture](#9-dove-e-quando-funziona-una-gesture)
10. [I profili](#10-i-profili)
11. [Aiuti: modalità prova, ripeti, promemoria, riquadro, pausa](#11-aiuti)
12. [Rivedi: modelli, cronologia, statistiche](#12-rivedi-modelli-cronologia-statistiche)
13. [Impostazioni](#13-impostazioni)
14. [Condividere le gesture](#14-condividere-le-gesture)
15. [Salvataggio, backup e modifica del file](#15-salvataggio-backup-e-modifica-del-file)
16. [Aggiornamenti](#16-aggiornamenti)
17. [Riga di comando](#17-riga-di-comando)
18. [Se qualcosa non va](#18-se-qualcosa-non-va)
19. [Limiti da conoscere](#19-limiti-da-conoscere)

---

## 1. Come funziona

Il touchpad comunica, molte volte al secondo, dove si trova ogni dito. Windows lo trasforma in
movimento del puntatore, scorrimento e gesture sue. GlyphPad **ascolta gli stessi dati** (Windows
li chiama *Raw Input*) e cerca le gesture che hai definito. Quando ne riconosce una, esegue
l'azione che le hai associato.

Due conseguenze da tenere a mente:

- GlyphPad **non può impedire** a Windows di reagire allo stesso tocco. Uno scorrimento con tre
  dita fa ciò che hai configurato **e** ciò che farebbe la gesture a tre dita di Windows, a meno che
  tu non disattivi quella di Windows ([sezione 3](#3-prima-sistema-windows)).
- Un dito solo muove il puntatore. Per questo i simboli si disegnano di default con più dita, o in
  un altro modo che Windows ignora ([sezione 7](#7-disegnare-simboli)).

Serve un **Precision Touchpad**, lo standard usato dalla maggior parte dei portatili dal 2015
circa. Con `glyphpad dump-descriptor` in un terminale vedi i touchpad trovati (nome, dimensioni,
quante dita riportano).

## 2. Installazione e avvio

1. Scarica lo `.zip` dalla pagina [Releases](https://github.com/wildzer0/glyphpad/releases) e
   decomprimilo dove vuoi (Documenti, una chiavetta…). Contiene `glyphpad.exe`, una configurazione
   di esempio, il README e la licenza.
2. Fai doppio clic su `glyphpad.exe`.

**SmartScreen.** La prima volta Windows può mostrare «Il PC è stato protetto», perché il programma
non è ancora firmato. Scegli *Ulteriori informazioni* e poi *Esegui comunque*. Per essere certo che
il file sia quello pubblicato, confronta il suo SHA-256 con il file `SHA256SUMS` della release
(`certutil -hashfile glyphpad.exe SHA256` lo calcola).

**Dove stanno i file.** GlyphPad cerca `glyphpad.toml` accanto a `glyphpad.exe` (*modalità
portatile*: copi la cartella su un altro PC e porti tutto con te). Se non c'è, usa
`%APPDATA%\GlyphPad\config\` e al primo avvio crea lì il file dall'esempio. Accanto alla
configurazione tiene anche `templates.json` (i simboli che hai registrato), `stats.json` (i
contatori d'uso), `update-state.json` e la cartella `backups`. La barra in basso nella finestra
mostra dov'è il file di configurazione.

**L'area di notifica.** GlyphPad vive nell'area di notifica (la `^` accanto all'orologio). Clic
sull'icona per aprire la finestra; clic destro per il menu: *Apri GlyphPad*, *Gesture attive*,
*Profilo*, *Ricarica la configurazione*, *Apri la cartella della configurazione*, *Controlla gli
aggiornamenti*, *Esci*. L'icona diventa grigia quando le gesture sono in pausa. Chiudere la finestra
con ✕ la nasconde e le gesture continuano a funzionare; in Impostazioni puoi fare in modo che ✕
chiuda il programma.

**Ne gira una sola copia.** Riavviare GlyphPad porta solo in primo piano la finestra già aperta.

**Avvio con Windows:** Impostazioni → *Avvia con Windows*. **Avvio nascosto:** *Avvia nell'area di
notifica*.

**Rimozione:** disattiva *Avvia con Windows*, esci dall'area di notifica, cancella la cartella (e
`%APPDATA%\GlyphPad` se non usavi la modalità portatile).

## 3. Prima, sistema Windows

Windows ha gesture proprie a tre e quattro dita. Se Windows e GlyphPad reagiscono allo stesso
movimento, succedono entrambe le cose. In **Impostazioni → Bluetooth e dispositivi → Touchpad**
imposta su *Nessuna azione* le gesture che vuoi usare con GlyphPad:

- **Gesture a tre dita** (scorrimenti e tocchi): *Nessuna azione* — necessario per disegnare
  simboli con tre dita, che è l'impostazione predefinita.
- **Gesture a quattro dita**: *Nessuna azione*, se usi gesture a quattro dita.
- *Tocca per fare clic* e lo scorrimento a due dita possono restare come sono.

GlyphPad legge queste impostazioni e all'avvio ti avvisa se il numero di dita con cui disegni è
ancora occupato da Windows.

## 4. Un giro della finestra

La **barra laterale** c'è sempre:

- **Gesture**, **Rivedi**, **Impostazioni**, **Debug live**: le quattro pagine.
- **La scheda di stato** (in basso a sinistra) dice se le gesture sono attive, in pausa, in
  registrazione o in modalità prova. I suoi interruttori mettono in pausa tutte le gesture
  (*Gesture*) e accendono o spengono la *Modalità prova*. Con più di un profilo permette anche di
  sceglierlo.

La **barra in basso** mostra dov'è il file di configurazione, se le modifiche sono salvate e i
pulsanti **annulla / ripeti** (anche Ctrl+Z e Ctrl+Y). Non c'è un pulsante Salva: **le modifiche si
salvano da sole** poco dopo che ti fermi, purché siano valide. Se qualcosa impedisce il salvataggio
(un campo non valido, il file cambiato altrove), un messaggio in alto lo dice e non si perde nulla.

Un **avviso** in alto compare quando c'è una nuova versione.

### Gesture

L'elenco a sinistra mostra le gesture in **ordine di priorità**: quando due possono scattare per lo
stesso movimento, vince quella **più in alto** (prima però vince quella più specifica, per esempio
con una condizione). Trascina la maniglia puntinata per riordinare. Ogni riga ha un interruttore per
attivarla o spegnerla, l'icona del tipo, il nome, una breve descrizione e ciò che fa. Un triangolo
d'avviso indica un possibile conflitto con un'altra gesture; selezionala per leggere il motivo.

Sopra l'elenco: una casella di ricerca, il filtro *Tutte / Attive / Spente* e il filtro per tipo.
Sotto: il **profilo attivo** e il pulsante per gestire i profili.

Selezionando una gesture a destra compare il suo editor in tre parti:

- **Quando**: cosa fai sul touchpad.
- **Allora**: cosa fa GlyphPad.
- **Solo quando**: dove funziona (app, titolo della finestra, profilo).

Il menu `…` duplica o elimina la gesture (eliminare chiede conferma e Ctrl+Z la riporta).
**Registra una gesture** avvia la registrazione guidata; **Aggiungi a mano** crea una gesture da
compilare; **Condividi** esporta e importa ([sezione 14](#14-condividere-le-gesture)).

## 5. La tua prima gesture

### Un simbolo disegnato

1. **Gesture → Registra una gesture → Un simbolo.** Scrivi il nome del simbolo (per esempio `S` o
   `stella`). Scegli quanti esempi disegnerai (5 vanno bene). La scheda ti dice *come* disegnerai: di
   default con 3 dita.
2. **Inizia a registrare.** Disegna il simbolo sul touchpad, poi fermati: la pausa (400 ms di
   default) chiude un esempio. Un simbolo con più tratti (come una X) va bene: i tratti disegnati
   ravvicinati contano come un solo esempio. L'area in diretta mostra le dita e il tratto; il
   contatore dice quanti esempi hai. Durante la registrazione nessuna gesture viene eseguita, quindi
   non può scattare nulla per sbaglio. Un esempio non ti piace? Premi il cestino sulla sua anteprima
   e disegnane un altro.
3. **Verifica.** GlyphPad confronta gli esempi tra loro e con i simboli che esistono già. Una barra
   mostra quanto ogni esempio somiglia agli altri; uno diverso dal resto viene segnalato, così puoi
   scartarlo. Se il nuovo simbolo somiglia troppo a uno esistente li vedi affiancati. Scarta ciò che
   non va, oppure *Registra di nuovo*.
4. **Nome e azione.** Dai un nome alla gesture e scegli cosa fa ([sezione 8](#8-le-azioni)). È lo
   stesso modulo dell'editor, con gli stessi controlli: una gesture non valida non si salva.
5. **Provala.** *Provala in modalità prova* ti fa disegnare il simbolo qualche volta e vedere cosa
   riconosce GlyphPad e con che punteggio, senza eseguire l'azione.

### Un movimento (tocco, scorrimento, pizzico…)

**Registra una gesture → Un movimento.** Fai il movimento un paio di volte (3 circa). GlyphPad
capisce cos'era: il tipo, quante dita, la direzione, quanti tocchi, o se altre dita erano
appoggiate. Ti compila la proposta (che puoi correggere) e ti avvisa se una gesture esistente scatta
già per lo stesso movimento.

Puoi anche creare una gesture senza registrare: **Aggiungi a mano**, poi scegli il tipo con le
tessere in *Quando*.

## 6. I tipi di gesture

Il tipo si sceglie con le tessere in cima a *Quando*.

| Tipo | Cosa fai | Opzioni |
|---|---|---|
| **Tocco** | Tocchi brevemente con N dita | dita (1–10), tocchi (singolo, doppio, triplo), *accordo* (vedi sotto) |
| **Pressione** | Appoggi N dita senza muoverle | dita |
| **Scorri** | Fai scorrere N dita in una direzione | dita, direzione (8), accordo |
| **Dal bordo** | Parti da un bordo del touchpad e scorri verso l'interno | dita, quale bordo, direzione |
| **Pizzico** | Due o più dita si avvicinano o si allontanano | dita (2+), avvicinano / allontanano |
| **Rotazione** | Due o più dita ruotano intorno al loro centro | dita (2+), oraria / antioraria |
| **Clic** | Premi il touchpad, volendo trascinando | dita, direzione del trascinamento |
| **Simbolo** | Disegni una lettera o una forma | quali simboli (uno o più), punteggio minimo |
| **Sequenza** | Da due a quattro gesture una dopo l'altra, entro un tempo | i passi, il tempo tra l'uno e l'altro |

**Accordo.** Per tocco e scorrimento puoi richiedere che *altre* dita restino appoggiate nel
frattempo: «un dito fermo, due dita scorrono a sinistra». Attiva *Con dita appoggiate* e scegli
quante.

**Priorità e attese.** Se hai configurato un doppio tocco con tre dita, un tocco *singolo* con tre
dita deve aspettare un attimo per vedere se ne segue un secondo. Se non usi doppi tocchi con quel
numero di dita, i tocchi singoli scattano subito. Il primo passo di una sequenza aspetta allo stesso
modo il passo successivo.

**Soglie.** Quanto può durare un tocco, quanto possono spostarsi le dita, quanto dura una
pressione, quanto deve percorrere uno scorrimento ecc. sono globali e in **millimetri**, quindi
valgono su qualsiasi touchpad. Stanno in *Impostazioni → Avanzate*. I valori predefiniti vengono da
registrazioni reali: cambiali solo se le gesture scattano troppo facilmente o troppo poco.

## 7. Disegnare simboli

### Perché i simboli richiedono un modo speciale

Un dito sul touchpad muove il puntatore e un tocco rapido fa clic. GlyphPad non può impedirlo a
Windows. Quindi un simbolo si disegna in un modo che Windows ignora o che GlyphPad sa distinguere
dal puntare. Scegli in **Impostazioni → Disegnare simboli → Disegna con**:

| Modo | Come disegni | Pro | Attenzione |
|---|---|---|---|
| **Più dita** (predefinito, 3) | Appoggi 3 dita e disegni: il tratto è il centro delle dita | Il puntatore non si muove; il più affidabile | Le gesture a 3 dita di Windows devono essere spente |
| **Da un bordo** | Parti con un dito in una zona lungo un bordo (sinistro di default, profonda 10 %) | Un dito | Il puntatore si muove mentre disegni, salvo *Blocca il puntatore* |
| **Dopo una pausa** | Appoggi un dito 400 ms, poi disegni | Un dito, ovunque | Stesso problema del puntatore |
| **Dopo una gesture** | Fai una gesture con azione *Disegna il prossimo simbolo con un dito*, poi disegni | Un dito, nessuna zona | Serve quella gesture in più |

*Blocca il puntatore mentre disegni* (sperimentale) tiene fermo il puntatore appena un tratto viene
rilevato. I tocchi possono comunque fare clic. Se GlyphPad si chiude male mentre è bloccato, al
prossimo avvio il puntatore viene liberato.

### Modelli e riconoscimento

GlyphPad riconosce un simbolo confrontando ciò che hai disegnato con dei **modelli**: esempi di
quel simbolo. Usa il riconoscitore a nuvola di punti $Q: non importa dove disegni, quanto è grande,
né in che ordine o direzione fai i tratti; conta la forma. **Conta anche la rotazione**, di default:
una L inclinata non è una L (*Impostazioni → Tolleranza di rotazione* permette un'inclinazione se la
vuoi).

- Modelli inclusi: **L, q, X, C, V, Z, O**, freccia su e freccia giù.
- I tuoi esempi (dalla registrazione, da *Rivedi → Registra altri esempi* o dalla cronologia) si
  **aggiungono** a quelli inclusi. Se quelli inclusi non ti vanno, disattiva *Esempi inclusi* per quel
  simbolo in Rivedi.
- Più simboli possono far scattare una sola gesture (`q` oppure `X` chiude la finestra).
- Un disegno deve avere un **punteggio** (da 0 a 1) sopra il *Punteggio minimo* della gesture (0,80
  di default; ogni gesture può avere il suo). Sotto, non succede niente e il riquadro mostra la
  migliore ipotesi con il suo punteggio: così capisci se registrare altri esempi o abbassare il
  punteggio.
- Un disegno che non somiglia a nessun simbolo ma è un semplice tocco o scorrimento viene trattato
  come tocco o scorrimento, quindi gli scorrimenti a tre dita funzionano anche accanto al disegno a
  tre dita (scattano un attimo dopo, dopo la pausa che chiuderebbe un simbolo).

### Per riconoscere bene

- Registra **5–8 esempi**, vari come disegni davvero (veloce, lento, grande, piccolo).
- Guarda il passo di verifica: scarta un esempio segnalato come diverso dagli altri.
- Se due simboli si confondono (una `C` e una `O`), registra più esempi di quello che perde, oppure
  dai un punteggio minimo più alto a quello importante.
- *Rivedi → Cronologia → un disegno → Aggiungilo come esempio* trasforma un disegno fallito in un
  nuovo esempio, o lo aggiunge a un altro simbolo se era stato riconosciuto male.
- *Debug live* mostra i punteggi dell'ultimo disegno.

## 8. Le azioni

L'azione si sceglie in *Allora*. **Provala** la esegue subito, per controllare.

| Azione | Cosa fa |
|---|---|
| **Premi una scorciatoia** | Preme una combinazione come `Ctrl+Maiusc+T`, o più di seguito. Premi **Registra** e poi i tasti. *Win+L* e *Ctrl+Alt+Canc* non si possono simulare; il tasto Windows e qualche scorciatoia di sistema le prende Windows prima che GlyphPad le veda: scrivile invece di registrarle. |
| **Blocca il PC** | Blocca la postazione (con l'API giusta; premere Win+L non può funzionare). |
| **Chiudi la finestra in primo piano** | Chiede alla finestra in primo piano di chiudersi, come la sua ✕. Con *Termina il programma* ne chiude il processo (il lavoro non salvato va perso). Non tocca mai il desktop né la barra delle applicazioni. |
| **Riduci / Ingrandisci** | La finestra in primo piano; ingrandisci alterna con la dimensione normale. |
| **Avvia un programma** | Avvia un programma con argomenti e cartella di lavoro; *Senza finestra della console* nasconde la console di uno strumento da riga di comando. |
| **Tasto multimediale** | Riproduci/pausa, successiva, precedente, stop, volume su/giù, silenzia. |
| **Più azioni di seguito** | Una *catena*: scorciatoie, programmi, pause, messaggi… eseguiti in ordine, con passi *Pausa* tra l'uno e l'altro. |
| **Mostra un messaggio** | Mostra un messaggio nel riquadro di GlyphPad (utile per vedere che una gesture è scattata). |
| **Pausa / riprendi le gesture** | Alterna tutte le gesture. Funziona anche in pausa. |
| **Ripeti l'ultima gesture** | Esegue di nuovo l'azione dell'ultima gesture ([sezione 11](#11-aiuti)). |
| **Mostra la finestra di GlyphPad** | Apre la finestra. |
| **Disegna il prossimo simbolo con un dito** | Per il modo *Dopo una gesture*. |
| **Apri un file, una cartella o un indirizzo** | Lo apre con il programma che Windows associa: un documento, una cartella in Esplora file, una pagina web nel browser. |
| **Digita un testo** | Digita un testo (fino a 2000 caratteri) nella finestra con il focus; un'andata a capo preme Invio. Funziona con qualsiasi lingua e simbolo. |
| **Disponi la finestra in primo piano** | Occupa la metà sinistra o destra dello schermo, la sposta sul monitor successivo, oppure attiva/disattiva «sempre in primo piano». |
| **Cambia desktop virtuale** | Precedente, successivo, o ne crea uno nuovo (con le scorciatoie di Windows). |
| **Fai uno screenshot** | *Scegli un'area* apre lo strumento di cattura di Windows; *tutto lo schermo* salva un'immagine in *Immagini → Screenshot*. |
| **Passa a un profilo / al profilo successivo** | Cambia il profilo attivo ([sezione 10](#10-i-profili)). |
| **Mostra l'elenco delle gesture** | Il promemoria ([sezione 11](#11-aiuti)). |

Windows non permette a un programma normale di inviare tasti a una finestra che gira **come
amministratore**, né di disporla. Le gesture rivolte a una finestra così non fanno nulla; se serve
davvero, avvia anche GlyphPad come amministratore.

## 9. Dove e quando funziona una gesture

In *Solo quando*:

- **Solo in queste app**: scegli tra i programmi con una finestra aperta ora, oppure scrivi il nome
  di un eseguibile (`chrome.exe`). Vuoto significa ovunque. Una gesture per «Chrome o Firefox» può
  inviare `Ctrl+Tab`; altrove lo stesso scorrimento può fare altro.
- **Titolo della finestra**: un'espressione regolare confrontata col titolo (`^Posta`), per gesture
  che devono funzionare solo in un documento o sito.
- **Profilo**: la gesture funziona solo mentre quel profilo è in uso; *Tutti i profili* significa
  sempre.
- **Ripetibile**: se *Ripeti l'ultima gesture* può eseguirla di nuovo (predefinito: sì, tranne
  blocco, pausa e azioni sulle finestre).

## 10. I profili

Un profilo è un insieme di gesture con un nome. Una gesture senza profilo funziona sempre; una con un
profilo funziona solo mentre quel profilo è quello attivo. Servono, per esempio, per avere un
insieme *lavoro* e uno *media*.

- **Crea, rinomina, elimina**: *Gesture → Gestisci* accanto al profilo attivo. Eliminare un profilo
  fa funzionare ovunque le sue gesture.
- **Cambia**: nella scheda della barra laterale, dall'area di notifica (*Profilo*), in Impostazioni,
  o con una gesture con azione *Passa a un profilo* o *Passa al profilo successivo*. La scelta resta
  salvata.
- **Cambio automatico**: *Impostazioni → Profili → Cambia da solo*: «quando `code.exe` è in primo
  piano usa *lavoro*». Vince la prima regola che corrisponde; se nessuna corrisponde vale la tua
  scelta. Questo non riscrive mai il tuo file.

## 11. Aiuti

- **Modalità prova** (barra laterale). Le gesture vengono riconosciute e mostrate (nel riquadro, in
  *Cronologia* e in *Debug live*) ma le loro azioni non vengono eseguite. Usala per provare gesture
  nuove. Un disegno con punteggio basso viene registrato con la migliore ipotesi.
- **Ripeti l'ultima gesture.** Una gesture come il doppio tocco a tre dita esegue di nuovo
  direttamente l'azione dell'ultima gesture. Non ripete mai se stessa, ignora le gesture segnate come
  non ripetibili e di default non fa nulla se ora è in primo piano una *finestra diversa* (così un
  «chiudi finestra» ripetuto non può chiudere il programma sbagliato). Nelle impostazioni puoi
  aggiungere un limite di tempo.
- **Promemoria.** Dai a una gesture (per esempio una pressione con quattro dita) l'azione *Mostra
  l'elenco delle gesture*. Al centro dello schermo compare un pannello con le gesture che funzionano
  ora (nel profilo attivo e per il programma in primo piano), ciascuna con un piccolo disegno del
  movimento e ciò che fa. Sparisce da solo (8 secondi di default).
- **Riquadro di riscontro.** Un piccolo pannello in un angolo dello schermo mostra il tratto mentre
  disegni e poi la gesture riconosciuta, oppure «Non riconosciuto» con la migliore ipotesi. Ci
  compaiono anche i messaggi di *Mostra un messaggio*. Posizione, dimensione, opacità e durata sono
  nelle Impostazioni.
- **Pausa automatica.** Le gesture si fanno da parte mentre è in primo piano un programma a schermo
  intero (un gioco, una presentazione) e mentre lo è uno di quelli che elenchi. La barra laterale ne
  dice il motivo e l'icona nell'area di notifica diventa grigia.
- **Conflitti.** GlyphPad avvisa quando due gesture potrebbero scattare per lo stesso movimento,
  quando due simboli si somigliano e quando una pressione potrebbe anticipare un accordo.

## 12. Rivedi: modelli, cronologia, statistiche

- **Modelli.** Ogni simbolo con tutti i suoi esempi, ciascuno disegnato su una sagoma con le
  proporzioni del tuo touchpad. **Riproduci** ripassa i tratti con i tempi reali (velocità ¼×–2×,
  ciclo, un cursore per scorrere). *Registra altri esempi* ne aggiunge; il cestino elimina uno dei
  tuoi. *Esempi inclusi* attiva o disattiva quelli inclusi per un simbolo.
- **Cronologia.** Le ultime gesture riconosciute (20 di default; si possono salvare su file nelle
  Impostazioni): ora, cosa è stato riconosciuto, il punteggio, l'azione, il programma in primo piano.
  Selezionandone una vedi il suo tracciato sovrapposto al modello che ha corrisposto, per capire
  *perché* ha funzionato o no, e puoi aggiungerlo come nuovo esempio di quel simbolo o di un altro.
- **Statistiche.** Quante volte è stata eseguita ogni gesture, quanti disegni non sono stati
  riconosciuti e quanti simboli sono stati riconosciuti senza una gesture da eseguire. Le gesture
  *mai usate* si notano, e molti disegni non riconosciuti dicono di registrare più esempi. Il
  conteggio resta sul tuo computer; si spegne nelle Impostazioni; *Azzera i conteggi* lo cancella.

## 13. Impostazioni

Qui c'è tutto `[settings]` e `[update]`, raggruppato per ciò che vuoi fare; le soglie tecniche sono
in **Avanzate** (chiuse all'inizio). Accanto a un valore diverso dal predefinito c'è il pulsante ↺
per ripristinarlo. L'elenco completo con valori e intervalli è nel
[riferimento della configurazione](CONFIGURATION.md#settings).

- **Aspetto**: tema (*Sistema* segue Windows, *Chiaro*, *Scuro*) e lingua (*Sistema*, *English*,
  *Italiano*).
- **Finestra e area di notifica**, **Disegnare simboli**, **Profili**, **Ripeti l'ultima gesture**,
  **Pausa automatica**, **Riquadro di riscontro**, **Promemoria**, **Aggiornamenti**, **Backup**,
  **Avanzate**.

## 14. Condividere le gesture

**Gesture → Condividi**:

- **Esporta le gesture…** Scegli quali gesture includere e un nome, poi *Salva come…*. Ottieni un
  file `.glyphpad` con le gesture e gli esempi dei simboli che usano.
- **Importa le gesture…** Scegli un file `.glyphpad`. Un'anteprima elenca le sue gesture. Per ognuna
  il cui nome esiste già scegli *Tieni entrambe* (la nuova viene rinominata), *Sostituisci la mia* o
  *Salta*; compare un avviso quando una gesture importata scatta per lo stesso movimento di una tua.
  Gli esempi dei simboli si possono escludere uno a uno. Nulla cambia finché non premi *Importa*, e
  l'importazione si annulla con Ctrl+Z.

Un pacchetto è un semplice file zip (`manifest.json`, `gestures.toml`, `templates.json`); non
contiene programmi, solo dati, e le sue gesture vengono controllate come una qualsiasi
configurazione. Però guarda le azioni di un pacchetto altrui prima di usarlo: una gesture può
avviare un programma.

## 15. Salvataggio, backup e modifica del file

**Salvataggio automatico.** Ogni modifica si salva circa un secondo dopo che ti fermi, se è valida.
La barra in basso dice *Tutte le modifiche sono salvate*.

**Backup.** Prima che il file cambi, una copia va nella cartella `backups` (se ne tengono 10; le
modifiche ravvicinate condividono una copia). *Impostazioni → Backup → Ripristina…* ne riporta una
(prima viene salvato il file attuale).

**Modificare `glyphpad.toml` a mano.** Con un qualsiasi editor di testo. GlyphPad se ne accorge e lo
carica (*ricaricamento a caldo*). Se il file ha un errore continua con l'**ultima configurazione
valida** e ti dice riga e campo; `glyphpad check-config` controlla un file senza avviare nulla. Se
cambi il file mentre la finestra ha modifiche non salvate, un messaggio offre *Carica il file*,
*Tieni le mie modifiche* o *Mostra le differenze*. I commenti e la formattazione del tuo file vengono
conservati quando salva la finestra.

## 16. Aggiornamenti

GlyphPad può controllare su GitHub se c'è una versione nuova (*Impostazioni → Aggiornamenti*):

- **Controlla gli aggiornamenti**: *Mai* (solo quando premi *Controlla ora*), *Avvisami*
  (predefinito: un avviso propone la versione con le sue note) o *Installa in automatico*.
- **Canale**: *Stabile* o *Pre-release*. **Controlla ogni**: 24 ore di default; il primo controllo
  avviene pochi secondi dopo l'avvio.
- **Installa e riavvia** scarica il pacchetto, lo verifica con il `SHA256SUMS` della release, sostituisce
  l'eseguibile e avvia la nuova versione. Se la nuova versione non parte entro 15 secondi, GlyphPad
  **torna alla vecchia** e te lo dice. Non installa mai nel mezzo di una gesture, di un disegno o
  di una registrazione, e non torna mai a una versione più vecchia. *Ignora questa versione* nasconde
  una release che non vuoi.
- Se la cartella del programma non è scrivibile (per esempio `Program Files`), GlyphPad non può
  sostituirsi e ti dà il link per scaricare la release.

Le release non sono ancora firmate: il controllo protegge da un download danneggiato ma non da un
account GitHub manomesso. Da riga di comando: `glyphpad update --check` / `glyphpad update`.

## 17. Riga di comando

`glyphpad <comando>`; senza comando si avvia. I più utili:

| Comando | Cosa fa |
|---|---|
| `glyphpad` / `glyphpad run` | Avvia GlyphPad |
| `glyphpad check-config [file]` | Valida una configurazione e stampa gli errori con la riga |
| `glyphpad record <nome> [--samples N] [--replace]` | Registra esempi di un simbolo dal terminale |
| `glyphpad debug` | Apre Debug live in modalità prova e stampa ciò che riconosce, con i punteggi |
| `glyphpad dump-descriptor` | Stampa ciò che GlyphPad sa dei tuoi touchpad |
| `glyphpad capture <file.jsonl> [--seconds N]` | Registra i tocchi grezzi su file (per segnalare un problema o per provare) |
| `glyphpad replay <file.jsonl> [--all] [--frames]` | Rilegge una registrazione e stampa cosa riconosce |
| `glyphpad update [--check]` | Cerca una versione più nuova e la installa |
| `glyphpad --version` | Stampa la versione |

Opzioni globali: `--config <file>` per usare un'altra configurazione, `--toplevel-window` come
ripiego se l'input grezzo non arriva in background. Con `RUST_LOG=debug` nell'ambiente ottieni un
log dettagliato sulla console.

## 18. Se qualcosa non va

**Non succede nulla quando faccio una gesture.**
Apri *Debug live* e tocca il touchpad: dovresti vedere le dita. Se no, GlyphPad non vede il tuo
touchpad: lancia `glyphpad dump-descriptor`; un touchpad che non è un Precision Touchpad non è
supportato. Se le dita si vedono, controlla la scheda di stato (*In pausa*? un programma a schermo
intero in primo piano? *Modalità prova*, che non esegue azioni?).

**Scatta due volte (la mia azione e qualcos'altro).**
La gesture di Windows è ancora attiva: vedi la [sezione 3](#3-prima-sistema-windows).

**Il mio disegno non viene riconosciuto.**
Accendi la *Modalità prova*, disegna e leggi il punteggio in *Debug live* o nel riquadro. Sotto il
punteggio minimo della gesture? Registra altri esempi (vari) o abbassa un poco il *Punteggio minimo*
(0,75). Somiglia a un altro simbolo? Guarda in *Rivedi → Cronologia* cosa ha riconosciuto. Tratti
troppo corti (sotto 10 mm) contano come un tocco.

**Il puntatore salta quando disegno con un dito.**
È normale nei modi a un dito ([sezione 7](#7-disegnare-simboli)): usa più dita oppure prova *Blocca
il puntatore mentre disegni*.

**Una gesture non fa nulla in un solo programma.**
Quel programma potrebbe girare come amministratore, o disegnare la finestra in modo che le
scorciatoie non arrivino. Prova *Provala* nell'editor con quel programma in primo piano… oppure
avvia GlyphPad come amministratore.

**In basso c'è «Non salvato: serve attenzione».**
Un campo non è valido (un nome vuoto, una scorciatoia sbagliata, un'espressione regolare che non si
compila). Il messaggio in alto dice quale; la tua modifica resta e si salva appena è valida.

**Manca l'icona nell'area di notifica.**
Potrebbe essere tra le icone nascoste (`^`). Se Windows la nasconde, trascinala nell'area visibile.
Anche senza, la finestra funziona.

**SmartScreen o l'antivirus protestano.**
Il programma non è firmato e legge il touchpad e invia tasti, che è proprio ciò che certi programmi
di sicurezza cercano. Verifica il download con `SHA256SUMS` e aggiungi un'eccezione se ti fidi.

**Ho rovinato la configurazione.**
Impostazioni → Backup → *Ripristina…*, oppure cancella `glyphpad.toml` e riavvia per riavere
l'esempio.

**Segnalare un problema.** Indica la versione di Windows, quella di GlyphPad e cosa hai fatto. Se
riguarda il riconoscimento, una registrazione (`glyphpad capture problema.jsonl`, poi fai la
gesture) permette di rigiocare esattamente quello che è successo. Le registrazioni contengono solo
le posizioni delle dita, nessun dato della tastiera.

## 19. Limiti da conoscere

- Per ora solo Windows; Linux è in programma.
- Una gesture di clic non può impedire al clic di arrivare a Windows.
- Il touchpad segnala un palmo come un dito per pochi millisecondi prima di marcarlo; una sessione in
  cui un dito si rivela un palmo non produce alcuna gesture.
- Le gesture rivolte a finestre che girano come amministratore non funzionano se anche GlyphPad non
  gira come amministratore.
- Il riquadro e il promemoria usano l'API classica di Windows e su schermi ad alta densità appaiono
  leggermente sfocati.
- Le descrizioni dei movimenti riconosciuti nella *Cronologia* sono in inglese in ogni lingua.
- Altro in [ARCHITECTURE.md](ARCHITECTURE.md#known-limitations).
