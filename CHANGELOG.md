# NoOrbit — Historial de versions

Totes les releases amb els seus binaris: <https://github.com/jpuigbert/noorbit/releases>.
Les notes completes de cada release són a la pàgina del release corresponent.

## 0.71.11-alpha (2026-11-16)

**Panells redimensionables i separables a finestres (multi-pantalla)**

- *panells de mida ajustable*: la barra lateral, el panell dret i el terminal tenen **nanses d'arrossegament** (estil VS Code) per canviar-ne la mida. La mida es recorda entre sessions (localStorage) i el doble clic sobre la nansa restableix el valor per defecte.
- *xat i terminal en FINESTRA SEPARADA*: el botó de separar (o els menús «Vista» i «Terminal») obre el xat de l'Agent o el terminal en una **finestra pròpia** que es pot posar a **qualsevol pantalla** (monitor extern, segona pantalla del portàtil…). La finestra separada té títol i botó «Torna al programa»; la seva geometria (posició i mida) es recorda, i si la pantalla ja no hi és es recol·loca dins l'àrea visible.
- *reenganxament net*: tancar la finestra separada (creu X, «Torna al programa», ⌘J o el menú) torna el panell al programa i recarrega els xats del calaix del projecte. Mentre el xat és separat, el panell dret mostra un marc informat (un sol lloc edita els xats: no es barallen calaixos).
- *arrencada multi-finestra*: el frontend decideix què pintar segons l'etiqueta de la finestra («main», «noorbit-chat», «noorbit-terminal»); les finestres separades comparteixen capacitats amb la principal i el projecte del xat viatja pel localStorage compartit de l'app.

**Finestres separades: reparacions de fiabilitat**

- *tancar una finestra separada matava la IA local*: el tancament de QUALSEVOL finestra (també les dels panells separats) aturava tots els servidors locals de `llama.cpp` — separar el xat i tancar-lo podia tallar una generació en marxa. Ara només s'aturen en tancar la finestra principal.
- *panell «destacat» sense finestra*: si la creació de la finestra separada fallava, el programa marcava igualment el panell com a separat i l'usuari es quedava amb el marc «torna al programa» i cap finestra per tornar-hi. Ara el flag només es marca si la finestra s'ha pogut crear (o ja existia) i es desfà si la creació falla a mitges.
- *xarxa de seguretat al tancament*: si una finestra separada mor sense passar pel seu tancament JS (crash del webview, tancament extern), el programa principal se n'assabenta igualment (event `panel://closed` emès pel backend en destruir-la) i el panell es reenganxa. El listener és idempotent: si el flag ja és net, no fa res.
- *doble clic sobre «separar»*: dues crides simultànies podien crear dues finestres amb la mateixa etiqueta (la segona fallava amb «label already exists»). Ara les crides en curs es comparteixen.
- *geometria perduda en reenganxar*: tornar el panell al programa DES DEL PROGRAMA tancava la finestra sense recordar on era — la següent separació sortia amb la mida per defecte. Ara també es desa la geometria en aquest camí, i les geometries corruptes del localStorage (mides negatives o NaN) es descarten en lloc de crear una finestra invisible.
- *geometria doblada a les pantalles Retina*: la posició i la mida es desaven en píxels FÍSICS però es restauraven com a LÒGICS — a cada cicle de separar/tancar la finestra es feia el doble de gran. Ara tot es desa i es compara en píxels lògics (conversió amb `scaleFactor`).
- *tema desincronitzat*: canviar el tema amb una finestra separada oberta no l'afectava fins a reobrir-la. Ara les finestres separades segueixen el canvi de tema en directe.

**Temps de treball multi-dia amb progrés i ETA + millora autopilotada de WordPress i de qualsevol projecte**

- *Jocs en diversos dies*: la Jam autònoma ara accepta durades de fins a **7 dies** (abans el sostre era 24 h). El panell Jam mostra una **barra de progrés** amb el **temps transcorregut**, l'**ETA** (temps restant estimat segons el ritme real de tasques) i l'**hora de fin estimada**, tot en viu. L'ETA es retalla al sostre de durada triada (la Jam s'atura sola en arribar-hi). Nous presets: 8 h, 1 dia, 2 dies, 3 dies i 7 dies.
- *Millora contínua de temes WordPress*: en crear un tema, l'usuari pot triar un **temps de treball extra** (15 min fins a 1 dia). Durant eixe temps la IA **busca millores i bugs** del tema i **implementa tot el que se li acudeixi** (rendiment, accessibilitat, SEO, seguretat, noves funcionalitats, qualitat de codi), passada a passada i acumulant millores. Vista de progrés amb temps transcorregut/restant/fin estimada i botó «Atura».
- *100% autopilotat*: tant la Jam multi-dia com la millora de WordPress **cerquen informació a internet soles** (recerca *headless* per HTTP: documentació, APIs, solucions) i fan peticions extra sense obrir cap finestra ni demanar sessió, captcha, permís o confirmació. L'usuari **no ha de tocar el PC per a res**: tria el temps i la IA treballa sola fins a completar-lo.
- *Temps PERSONALITZABLE*: a més dels presets, la Jam i WordPress permeten triar un **temps exacte** (valor + unitat: minuts/hores/dies) amb l'opció «Personalitzat», retallat a 1 minut..7 dies (igual que el backend).
- *Millora autopilotada de QUALSEVOL projecte*: nou menú **«Treball autopilotat»** (IA → Treball autopilotat, o la paleta d'ordres) per millorar **qualsevol projecte existent** (joc, web, backend, eina…) durant un temps triat. L'usuari assenyala la carpeta, explica l'objectiu i tria el temps; la IA hi treballa sola amb el mateix motor autopilotat que WordPress. El backend `wp_work` es generalitza a **`project_work`** (perfil WordPress + perfil genèric amb autodetecció, event `project://work`).
- *component compartit `WorkProgress`* + utilitats de temps (`formatDuration`, càlcul d'ETA) reutilitzats per la Jam, WordPress i qualsevol projecte, amb tests.
- *registre del handle de la Jam*: `Arc<JamHandle>` es registra explícitament a Tauri (abans `jam_start`/`jam_stop`/`jam_state` no el trobaven i petaven en runtime).

**Auditoria de l'Autonomous Jam: ja no es trenca (guardian, bucle, represa i UI)**

Revisió completa del codi de la Jam autònoma (els 14 crates de `crates/` i la seva integració a l'app). S'hi havien colat fallos que feien que la Jam **refés la feina, ho perés tot o tanqués l'app sencera** just quan la memòria apretava — que és precisament quan el supervisor hi serveix.

- *pànic que tancava l'app (release)*: `prune_old` restava 1 hora a un `Instant` quan el rellotge del procés portava menys temps — una resta que no es pot fer. Amb `panic = "abort"` del profile `release` **tancava NoOrbit sencer**. Ara es compara amb `elapsed() < max_age` (mai no es resten `Instant`) i la funció queda coberta per un test que fa servir un `Instant` acabat de crear.
- *`pre_exec` que no és «async-signal-safe»*: dins el `pre_exec` del fill es cridava `warn!` (agafa un mutex i reserva memòria dins la lògica de logging). Entre `fork` i `exec` només es permeten crides *async-signal-safe*: podia **bloquejar el naixement del procés** amb un mutex ja agafat per un altre fil. Els avisos passen a un `write(2)` directe a stderr (`note()`).
- *les últimes línies del fill es perdien*: en acabar (o en matar-lo) es feia `abort()` sobre el lector de stdout sense drenar el que ja havia llegit. El fill just havia imprès l'última tasca i el resultat de la memòria: **la UI no ho arribava a veure mai**. Ara es dona un marge de 250 ms al lector perquè acabi (`finish_drain()`) a la cancel·lació, la sortida natural, el crash, l'aturada per memòria i el `give_up()`.
- *sostre de memòria invàlid acceptat*: `agent_rlimit_bytes == 0` passava la validació i significa «cap memòria»: el fill moria abans de néixer. Ara `validate()` el rebutja. A més, el `setrlimit(RLIMIT_AS)` **només s'intenta on existeix** (a macOS no hi ha aquest recurs: l'intent només embrutava stderr amb un error inútil).

**El bucle de la Jam podia mentir sobre què havia fet**

- *verifier ceg als fitxers reals*: el `Verifier` només rebia la tasca; `NoOrbitVerifier` es limitava a «el projecte no és buit» i **qualsevol tasca passava**, encara que el model no hagués escrit res. Ara `Diff` declara els fitxers tocats (`materialized_files`/`patch_files`), el bucle els passa amb el mètode nou `Verifier::run_with_files` (amb *default* cap a `run`, per no trencar cap implementació existent) i, si el worker no n'ha declarat, **els pregunta a git** (`changed_files` amb `status --porcelain -uall` i temps límit). El verifier rebutja «cap fitxer modificat» i comprova els **camins** que mencionen els criteris d'acceptació.
- *represa que tornava a un punt fals*: el `save_fn` feia el seu propi `git rev-parse HEAD` i fixava la qualitat a «COMPLETA». Si el fill moria, la represa **tornava a un commit equivocat i amb un pressupost de qualitat mentider** (reescrivia feina ja feta amb el màxim de línies per fitxer). Ara el bucle entrega un `SaveInfo` amb el SHA real de l'últim commit bo i la qualitat amb què s'ha executat la tasca.
- *estat sense validar*: `ProgressStore::load()` acceptava qualsevol JSON. Ara rebutja el que passi de **1 MiB** (abans de parsejar-lo), els plans incoherents (`task_count` ≠ nombre de tasques, ids buits, durada 0, estadístiques que no quadren) i l'estat **massa vell (>7 dies) o del futur (>1 h de desviació)** — que havien fet refer una Jam de fa setmanes sobre un projecte que ja no s'hi assembla.

**Integració amb l'app: els fitxers interns ja no viuen dins del repo**

- *la Jam esborrava la seva pròria memòria*: el brief, el pla, l'estat de represa i les lliçons s'escrivien a l'arrel del projecte. El `git add -A` del checkpointer **els comitava a l'historial de l'usuari** i el `git clean -fd` de cada intent **els esborrava**: la represa era impossible i l'històric quedava brut. Ara tot va al directori de dades (`…/jam/<id>/`), i `.git/info/exclude` protegeix els rastells de versions anteriors (`.noorbit/`). Nova funció `ensure_git_excluded` (escriptura atòmica i idempotent) + test que ho demostra fent `git clean -fd` i `git add -A` de veritat.
- *l'id de la Jam canviava a cada reinici*: es derivava del `pid` + l'epoch, i el guardian **reinicia el fill amb un pid nou** just quan hi ha pressió de memòria → `load()` rebutjava l'estat («jam_id diferent») i la Jam **començava de zero** tot el que ja havia fet. Ara l'id es deriva del brief + motor + durada + repo (estable) i el pare la passa amb `--jam-id`.
- *la Jam parlava amb la IA equivocada*: el fill construïa el seu propi `AiManager` i el proveïdor actiu i el model **no es persisteixen enlloc** → encara que l'usuari tingui OpenAI/Claude/Gemini seleccionat, la Jam sempre anava a Ollama (i si no hi havia Ollama, no feia res). Ara el pare passa `--provider` i `--model`.
- *el pla es pagava dues vegades*: el pare ja el feia per ensenyar-lo a la UI, però el fill **el tornava a demanar a la IA**: doble cost i, com que el model no és determinista, s'executava un pla **diferent del que l'usuari havia vist i aprovat**. Ara el pla es serialitza (`StoredPlan`) i es passa amb `--plan`; si el fitxer no hi és o no és vàlid, el fill el fa (mai no es queda sense).
- *un error del supervisor era invisible*: el resultat de `supervisor.run()` s'empassolava (`let _ = …`) — l'usuari veia la Jam «aturada» **sense cap explicació** (spawn del fill, protocol, E/S). Ara es reporta com a error amb el motiu, i el mateix passa amb el `reader` d'esdeveniments (abans `abort()`; ara es drena amb temps límit).
- *lliçons mai connectades*: `noorbit-lessons` existia i el worker el sabia fer servir, però ningú no li passava el `LessonsStore`: l'agent **no recordava mai els intents fallits** i repetia el mateix error. Ara les lliçons viuen fora del repo i s'injecten al prompt.

**UI de la Jam**

- *la barra de progrés no es movia mai*: es calculava amb `plan?.tasks.length ?? state.completed` i el `plan` es buida en prémer «Comença» → el denominador eren **les tasques ja fetes** i la barra començava al 100% (o a 0 si no n'havia fet cap). Ara el backend envia el `total` real del pla en execució i el percentatge compta també les tasques **bloquejades i saltades** (una tasca processada ho està, encara que no surti bé). Els comptadors ara són els **reals del bucle** (els emet el fill amb una línia `JAM|{"kind":"stats"…}` després de cada tasca) i es mostren com a «X de Y fets».
- *el resultat final desapareixia*: el panell de progrés només es pintava amb `running`, així que en acabar la Jam l'usuari es quedava **sense saber què s'havia aconseguit** (només l'enllaç a l'informe). Ara el resum es queda a pantalla.
- *registre en anglès intern*: les etiquetes del log eren els noms interns de l'event (`task_done`, `memory`, `compact`); ara diuen «fet», «memòria», «neteja de memòria».

**Projectes nous: ja hi poden fer servir la Jam**

- *«el projecte ha de ser un repositori git»*: ni «Fitxer ▸ Nou projecte» ni la plantilla Game Studios (que **esborrava el `.git`** del clone) creaven un repo, així que un projecte nou **no podia executar mai una Jam**. Ara neixen amb `git init` + primer commit (sense tocar la configuració global de l'usuari) i amb un `.gitignore` de conveni — si no, el `git add -A` de la Jam hauria comitat `node_modules`, `.godot/` o `__pycache__/`.
- *pantallada gelada clonant*: `clone_game_studios` feia `std::process::Command::output()` dins d'un `async fn` — el clone (xarxa, minuts) **es menjava un fil del runtime de Tauri** i durant l'estona no responia cap altra comanda, incloent-hi el streaming de la IA. Ara és `tokio::process` amb `kill_on_drop`.
- *missatge d'error útil*: si el projecte obert no és un repo, la Jam diu ara com arreglar-ho (crear un projecte nou o `git init`) en lloc d'una frase seca.

**Compilació i proves (era això el que realment estava trencat)**

- *la llibreria de l'app NO compilava els seus tests*: `unreal/python.rs` tenia **dos noms de funció amb espais** (`fn traceback_es converteix en error visible()`), i com que la CI només feia `cargo check` (sense `--all-targets`) ningú no ho havia vist mai — els **78 tests del backend no s'havien executat mai a la CI** i, de fet, no passaven. Corregit i amb el job de CI ara `cargo check --all-targets`.
- *cicle de futurs `async`*: `chat_local_only → chat_inner → native_compute → native_improve → chat_local_only` donava `error[E0733]: recursion in an async fn requires boxing` i la llibreria no compilava. `chat_local_only` retorna ara un futur embolsat (`Box<dyn Future>`) — es trenca el cicle pel camí fred, no pel nucli del xat.
- *error de tipus a `runtools.rs`*: `clip(&e, 70)` rebia un `anyhow::Error` (Unreal) on volia un `&str` — no compilava.
- *dos tests de la IA nativa fallaven* (i ningú no ho sabia): l'extractor matemàtic convertia **«el projecte 2024-2025» en una resta** (fals positiu que segrestava qualsevol frase amb números i un guió); dins d'una frase ara cal un operador fort (`+ * / ^ %`) o un menys separat per espais. I les **lliçons apreses de l'usuari no es servien mai**: el cervell predefinit (que també contenia «rust», «tauri») guanyava sempre; ara es comparen amb el mateix rasant (paraules de la pregunta cobertes) i **guanya la lliçó més específica**.

**Qualitat: els tests de `crates/` per fi corren a CI**

- *101 tests que mai no s'executaven*: `crates/` és un workspace **separat** (no entra al workspace de `src-tauri`) i la CI no en deia res — ni es compilava. Nou pas: `.github/workflows/ci.yml` amb una feina `crates` (`cargo check --workspace --all-targets` + `cargo test --workspace`, amb git configurat perquè els tests d'integració fan commits de veritat).
- *porta de qualitat al release*: abans de llançar la matriu de build de 3 SO, el job `gate` executa els tests dels crates de la Jam — un tag amb la Jam trencada ja no genera instal·ladors.
- *proves noves*: 3 al guardian (regressió de `prune_old`, `rlimit` zero i **les últimes línies del fill no es perden**, test d'integració amb un procés real), 3 al bucle de la Jam (el verifier rep els fitxers, la desada porta SHA + qualitat, `changed_files` veu fitxers nous), 7 a la persistència (pla incoherent, estat vell, estat del futur, fitxer gegant, JSON corrupt, exclosa que sobreviu a `git clean`, i que fora d'un repo falla **en lloc de callar**).
- *tot en verd*: `cargo test --workspace` a `crates/` (101), `cargo test --lib` a `src-tauri` (78), `pnpm typecheck`, `pnpm lint` i `pnpm test` (55).

**Unreal Engine 5: ja passa, de veritat, el que s'envia a l'editor**

L'usuari veia «canvis aplicats» (o res de res) sense que a l'escena canviés res. Auditada la integració de punta a punta (client Remote Control → execució Python → ordres Tauri → agent → panell d'Unreal), les causes eren diversos defectes que s'amagaven entre ells.

- *èxit fals per un parseig que no existia*: `run_script` mirava `ReturnValue.Success` i `LogOutput.Entries`, i si no hi eren ho donava per bo. La documentació oficial diu una altra cosa: `ExecutePythonCommand` retorna `void` (UE 5.0–5.3) o un `bool` (5.4+), i només `ExecutePythonCommandEx` retorna `CommandResult` + `LogOutput`. Per tant **qualsevol execució semblava correcta**. Ara es cobreixen les tres formes (cercant els camps sense importar la caixa) i es dóna per fracàs el que no tingui senyal explícit d'èxit.
- *ni el `print()` ni el traceback arribaven mai*: la resposta HTTP de la Remote Control no conté la sortida de l'editor. Ara un **wrapper** (`python::wrap_script`) embolica l'script (stdout/stderr duplicats, `exec(compile(…))`, `traceback.format_exc()` sobre `BaseException`) i l'escriu a `<temp>/noorbit/ue`; NoOrbit llegeix **aquest fitxer** (l'HTTP només és la reserva) i el segell `NOORBIT-TOKEN:<uuid>` evita que una crida concurrent es doni per bona amb la sortida d'una altra. El temps límit de la petició passa de 30 s a 300 s (il·luminació i imports llargs tallats a mitges).
- *camps mal escrits i funció equivocada*: els payloads anaven en `ObjectPath`/`FunctionName`/`GenerateTransaction` quan la API oficial usa camelCase; l'editor respondia «unknown function/parameter» i l'error s'empassolava. Ara el client escriu camelCase, no demana transacció per a un simple exec de Python, i **reintenta amb `ExecutePythonCommandEx`** si l'editor diu que la funció no hi és.
- *el port configurat s'ignorava*: l'editor s'arrencava amb el port de la configuració, però el client, `is_rc_open`, `wait_for_rc` i els scripts esperaven sempre 30010 — amb un port canviat, la connexió no fallava sorollosament: **no feia res**. Ara tot passa per `client::resolve_port` / `launcher::rc_port`, el bootstrap rep `NOORBIT_UNREAL_RC_PORT` i el panell d'Unreal té un camp «Port de Remote Control» (comandes noves `unreal_get_rc_port` / `unreal_set_rc_port`, que desen `rc_port` a `config.toml` i actualitzen la configuració en memòria).
- *scripts amb APIs deprecades i ordres inexistentes*: `EditorLevelLibrary`/`EditorAssetLibrary` (deprecades: no fan res i la resposta HTTP no avisa'n) i un `BuildLighting High` que no és cap comanda. Ara subsistemes (`LevelEditorSubsystem.build_light_maps` amb avís de Lumen, `EditorActorSubsystem.spawn_actor_from_class`, `EditorAssetSubsystem` i `AssetTools` amb `AutomatedAssetImportData`), i la importació **verifica** que l'asset hagi aparegut (diferència de `list_assets`) en lloc de prometre que ho ha fet.
- *«Aplica a Unreal» sense cap garantia*: no comprovava connexió ni que la resposta de la IA fos codi — una frase en prosa arribava a l'interpretador. Ara rebutja la prosa (`unreal::guard_python`, amb un segon intent que li demana l'script de veritat), afegeix `import unreal` si falta, fa `ping()` abans d'executar (si no respon, diu **el port concret** i deixa l'script generat perquè l'usuari el puga executar a mà) i, en fallar, el reintent rep el traceback real.
- *directriu `RUN|UNREAL|`*: abans executava `execute_python` i bolcava el JSON cru de la resposta a la IA. Ara fa servir `python::run_script` i torna `eixit=0 — sortida de l'editor: …` o l'error Python amb la sortida prèvia, accepta la ruta d'un `.py` del projecte (com Blender) i avisa quan l'script no imprimeix res. El `RUN_PROMPT` explica l'API actual i mana **llistar abans d'actuar** (endevinar rutes de classe és just el que donava l'error silenciós).
- *captura del viewport que sempre fallava*: buscava `NOORBIT_CAPTURE:` en una sortida que mai no arribava, l'ordre de consola anava amb `None` de context (UE5 no trobava el viewport) i el primer llistat es feia abans de res. Ara passa l'`editor_world`, mira `Saved/Screenshots` i `Saved/ScreenShots`, espera per diferència i per data de modificació, i **l'error diu el motiu** (abans sempre el mateix text genèric).
- *versió i projecte inventats*: `GET /remote/info` només llista les rutes HTTP — no hi conté ni la versió ni el projecte. Ara una sonda Python (`python::probe_info`) dona les dades reals, amb un cau de 60 s perquè el watchdog de la UI pregunta cada 5 s i no s'executi Python a cada tick; l'HTTP queda com a reserva.
- *bootstrap.py*: port fixe i APIs deprecades per a la consola; ara `editor_world()` prova primer els subsistemes i el port ve de l'entorn.
- *proves*: 8 tests nous a `unreal/python.rs` (wrapper i rutes segures, captura només bona amb el seu segell, traceback, les tres formes de resposta, sonda d'informació, cap script amb APIs deprecades i sintaxi Python vàlid validat amb `python3`) i el test de `ports.rs` continua lligant Rust, Python i l'instal·lador. **Tot verd**: `cargo test --lib` (78), `pnpm typecheck`, `pnpm lint`.
- *comprovació en viu pendent*: aquest equip no té Unreal Engine instal·lat, així que les correccions estan verificades contra la documentació oficial d'Epic i amb tests, però no contra un editor real en marxa.

---

## 0.71.10-alpha (2026-11-15)

**IA nativa integrada al 100%: NoOrbit funciona NOMÉS INSTAL·LANT-LO**

- *IA nativa de sèrie, sense descàrregues*: hi ha una **IA nativa integrada al binari** (`ai/native.rs`, motor simbòlic en Rust) que respon **només instal·lant NoOrbit**, sense baixar cap model ni motor de cap altre lloc i sense GPU: resol **matemàtiques** (avaluador amb funcions, constants i notació científica), consulta el seu **cervell de coneixement** i manté **conversa**. Funciona en aquest Mac (Intel) tal qual.
- *Fallback garantit*: si no hi ha Ollama en marxa **ni** cap proveïdor en línia amb token, el xat (tant en streaming com sense) **cau a la IA nativa** en lloc de fallar: l'editor, el terminal, les imatges i Blender/Unreal funcionen igual des del primer arrencada, sense configurar res.
- *Aprèn aprofitant l'aprenentatge local*: la nativa fa servir les **lliçons destil·lades** de les converses (el bloc `[APRENENATGE LOCAL]` que ja injecta el xat) i el seu torn es **recorda i es destil·la** com qualsevol altra IA: el coneixement après es retroalimenta.
- *Es reprograma en SILENCI amb IAs open-source*: quan l'usuari demana «**millora el context matemàtic**» (o de física, codi, història…), el cervell natiu es **reprograma en silenci** delegant la generació de coneixement en una IA open-source **JA disponible**: primer el model **local d'Ollama** (prefereix `qwen3`, sense baixar res) i, si no n'hi ha, un proveïdor **gratuït en línia** (MiniMax, Ollama Cloud…). La crida va en una **sessió aïllada** (`native_improve`) perquè no toque el xat de l'usuari ni delati el teacher: **l'usuari no veu quina IA ho genera**. Si no hi ha cap IA, aplica coneixement predefinit (millora *offline*). Mai no baixa cap model.
- *Cervell persistent i deduplicat*: el coneixement builtin (matemàtiques, codi, NoOrbit, identitat, ajuda) es fusiona amb l'après i es guarda a `native_brain.json` (només l'après, sense duplicats).
- *Proveïdor integrat al gestor*: «**IA nativa de NoOrbit**» ve sembrada de fàbrica (`kind: native`, sense token, activada): al gestor de proveïdors mostra el badge «**Integrada**» (icona `Cpu`), **no es pot esborrar ni editar** (protegit també al backend) i només es pot activar/desactivar, provar i fer servir. El frontend és **kind-safe**: activar o editar un proveïdor ja no corromp el `kind` natiu.
- *MiniMax (gratuït) com a teacher potent*: es sembra el proveïdor **MiniMax** (`https://api.minimax.io/v1`, model `MiniMax-M3.1-Flash`, context d'1M, tier gratuït) i s'afegeix un **preset** al formulari, perquè la millora silenciosa tinga una IA en línia potent quan no hi ha Ollama local.
- *Tests*: **11 tests nous** a `native.rs` (avaluació matemàtica, extracció del prompt, retrieval de coneixement i lliçons, detecció de peticions de millora, parseig del JSON del teacher, missatge que no delata cap IA, fusió/deduplicació del cervell).

**Blender: ordres complexes (figures, nans, escenes) ja no fallen amb «NO és codi Python»**

- *RUN|BLENDER| de varies línies*: fins ara la directriu només capturava la PRIMERA línia del codi — una figura complexa (nan de jardí, personatge, escena) no hi cab en una línia, i el model es veia obligat a escriure una descripció en català, que Blender rebutjava amb «NO és codi Python». Ara `parse()` recull el codi sencer a sota de la directriu o dins d'un bloc ```python (fins a la següent directriu o a la prosa), i l'executa de veritat.
- *Des de la conversa, la descripció es converteix en codi*: si el model envia una DESCRIPCIÓ en lloc de codi a «RUN|BLENDER|», NoOrbit la passa a l'agent 3D (que té les receptes bpy: nan de jardí, materials, càmera, animació, vídeo, veu) i executa el script multi-línia generat, en lloc de deixar l'usuari amb un error tècnic.
- *Guàrdia de codi millorada*: `looks_like_python` i una nova `looks_like_code_line` distingeixen una frase en català d'un script («crea un nomo de jardi» ja no es confon amb codi; «bpy.ops…», assignacions i crides sí). El missatge d'error ara explica que el codi pot ser de varies línies.
- *Prompt*: `RUN_PROMPT` diu clarament que «RUN|BLENDER|» pot ocupar varies línies i inclou un exemple; `expects_execution` també reconeix peticions de figura/escena 3D («crea un nan de jardí», «fes un model 3D d'una casa») encara que no diguin «Blender», perquè el model emeti la directriu en lloc d'explicar com fer-ho.
- *Tests*: 8 tests nous a `runtools` (multi-línia, bloc ```python, parada a la següent directriu, descripció rebutjada) i 3 a `blender` (prosa vs. codi real).

**Hugging Bay: només models descarregables + mida del disc**

- *Hugging Bay, catàleg net*: el mirror marca «downloadable» repositoris que **no tenen cap fitxer GGUF hostatjat** (p. ex. `Qwen3.8-27B-iMatrix-NVFP4-MTP-GGUF`): abans apareixien com a «Ready to download» i fallaven en baixar-los («Aquest model no té cap fitxer descargable»). Ara el catàleg **només mostra els models que realment es poden descarregar**: cada candidat es valida amb `get_model` (fitxer recomanat present) — la mateixa comprovació que fa la descàrrega — i els que no en tenen s'oculten. La comprovació es fa en paral·lel per no alentir la pàgina, i un error transitori de xarxa no amaga models.
- *Hugging Bay, mida del disc*: a més de l'espai **lliure**, el destí mostra la **mida total** del disc/USB («854 MB lliures de 500 GB»), perquè es vegi d'una ullada quant hi cap. Nou camp `total_bytes` a `hf_bay_disk_space` (Unix `statvfs.f_blocks`, Windows `GetDiskFreeSpaceExW`) i clau i18n `hfBay.diskSpace`.
- *Crash (macOS) en arrencar i en fer servir el navegador intern*: `no-orbit` moria amb `abort() called` (SIGABRT) en el fil principal quan el getter d'URL del webview (`WebviewWindow::url()` → `WKWebView.URL()` → `Option::unwrap()`) trobava un `None`: el `WKWebView` no té URL fins que no confirma una navegació. Tauri resol aquests getters via l'event loop de macOS (un **observer de `CFRunLoop`**), i amb `panic = "abort"` al profile `release` un panic en el fil principal tanca l'app sencer. Fix: `wry` actualitzat a `0.57.2` (retorna `Err` en lloc d'abortar) i el navegador intern recorda l'última URL enviada. Incident: `no-orbit` 0.71.9-alpha (2026-11-14, `~/Library/Application Support/no-orbit/crash.log`).

---

## 0.71.9-alpha (2026-11-13)

**Auditoria de codi integral — fiabilitat, qualitat i UX**

*Correccions de les auditories de codi:*
- *fitxers: cursa en cadena a `loadFile` (dues lectures ràpides podien fer que l'editor mostrés i **desés** el fitxer equivocat); avisos visibles (toasts) per a desats, proveïdors i descàrregues de models que fallaven en silenci.*
- *seguretat: el servidor de preview ja no serveix fitxers **fora del workspace** (`/../../../etc/passwd`); les skills ja no poden esborrar la carpeta de dades de NoOrbit; el record de permisos «per sempre» no s'autoamplia amb encadenaments (`npm … && curl …`); el preview demana un token (cookie) i rebutja `Origin`/`Host` aliens (CSRF i DNS-rebinding).*
- *estabilitat: el terminal ja es pot aturar (mata el grup de processos) i les comandes tenen temps límit; es fixen els bucles infinits amb enllaços simbòlics; la interfície no es congela en arrencar Ollama, clonar skills, escanejar el disc o calcular la memòria; detecció de repeticions O(n²) → O(n); el pont de Blender no mor en silenci si el codi executat fa `sys.exit()`.*
- *interfície: proveïdors de autocompletat de Monaco ja no es dupliquen (cada pulsació llançava N inferències); l'autocompletat **es cancel·la de veritat** quan l'usuari segueix escrivint (la inferència vella ja no consumeix CPU/GPU); la cua de configuració de IA, la cerca a Hugging Bay i els esdeveniments globals ja no queden penjats si el registre falla.*
- *ports: font única (`ports.rs`) per als ports de Blender i Unreal; el port **configurat** per l'usuari ara s'injecta al script Python (abans el Python sempre obria el per defecte i la configuració era ignorada).*
- *terminal: `run_command` rebia el `call_id` en camelCase — abans el terminal enviava `call_id` i Tauri 2 l'esperava com `callId`: la comanda fallava i **el terminal no executava res**.*
- *estabilitat (2a auditoria de codi): el panell d'Especialistes ja no es penja amb «Maximum update depth exceeded» (selector zustand inestable → estat cru + `useMemo`); els esdeveniments d'experts i de control de l'ordinador ja no es dupliquen amb StrictMode (registre d'esdeveniments amb promesa única compartida); el servidor de preview ja no fa panic amb URLs amb caràcters invàlids per a capçaleres HTTP (es codifiquen i, si cal, es respon 400); els zips de skills tenen límit de mida (20 MB comprimits / 50 MB descomprimits) i fora de localhost només s'accepten URLs HTTPS.*
- *integritat de les descàrregues (Hugging Bay): els GGUF es baixen a un fitxer «.part» i es verifiquen amb el **SHA-256** del mirror abans de renombrar-los al nom final — una descàrrega interrompuda ja no deixa mai un model parcial que es donava per bo (corrupte i impossible de rebaixar), i una descàrrega corrupta es descarta amb un missatge clar.*
- *integritat de les descàrregues (neteja de «.part»): si l'app es tanca o mor mentre una IA baixa un model, la descàrrega parcial «*.part» s'esborra sola en arrencar (tant els GGUF de Hugging Bay com els models d'imatge en CPU); a més, qualsevol error a mitges de la descàrrega (xarxa, disc, SHA-256 o rename) també esborra el «.part». Només es toquen fitxers «*.part»: els models complets no es toquen mai.*
- *estabilitat de la interfície: cada zona gran (editor, panell lateral, terminal, IA de web, activitat IA, Git, problemes, consola) va amb la seva pròpia `PanelErrorBoundary`: si un panell peta, només mor aquell panell i la resta de NoOrbit continua funcionant.*
- *qualitat: workflow nou de **CI** (`.github/workflows/ci.yml`) a cada push/PR — typecheck, lint (ESLint react-hooks), tests (Vitest), `cargo check` + `cargo test` i detector de selectors zustand inestables (React #185) — perquè un commit que no compila o que trenca els tests no pugui arribar a un release.*
- *qualitat interna: **55 tests Vitest** de lògica pura (i18n `t()`, directrius `@file:`/`@agent:`/`@use:`/`@todo:`, utilitats del xat; `quant_de`/`safe_name` ja es proven amb `cargo test`), **ESLint** amb react-hooks (dependències inestables als hooks corregides a l'editor i a la revisió d'edicions), i els **dos monòlits** s'han partit en **mòduls purs**: l'`agentStore` (3.843 línies → `stores/agent/`: types, chatUtils, prompts, fileBlocks, directives) i `ai/mod.rs` (4.086 → 3.655 línies → `repetition`, `reasoning`, `cloud`), amb l'API pública intacta — la mateixa lògica, ara testeable i mantenible.*
- *rendiment: els components que se subscribien al **store sencer** de zustand (Agent, Especialistes, Terminal, Ordinador, Sidebar, Editor, panells d'Unreal/Blender/Mòbil, gestors de models i proveïdors, skills, configuració…) ara usen **selectors fins** (`useShallow` / per camp): només es re-renderitzen quan canvia el que pinten, no amb qualsevol canvi de l'estat — clau amb models grans en streaming.*
- *i18n complet: tot el text visible de la interfície que encara estava «endurit» ara passa per `t()` — AgentPanel, HuggingBayModal, SettingsModal, ProblemsPanel, RightPanel (missatges d'error per panell), catàleg d'IAs, visor del procés de la IA, estat de la IA local, consola de depuració, editor, Blender, botons de copiar/desar, renderitzador Markdown i el modal de pujar temes; ~130 claus noves al catàleg `ca.json` (seccions `problems`, `console`, `process`, `localAI`, `publish` + `common`/`rightPanel`/`ai`/`blender`). L'app queda preparada per a altres idiomes.*
- *estabilitat de la interfície (React #185): la migració a `t()` del renderitzador Markdown del xat (`ChatMarkdown`) havia corromput una expressió regular (salts de línia literals dins de les classes `[^…]` → «Unterminated regular expression literal»), que trencava la compilació i la pantalla en pintar; l'expressió s'ha restaurat. Amb els selectors estables (`useShallow`), la interfície ja no cau en el bucle infinit de re-render («Maximum update depth exceeded», #185). Typecheck, lint i tests en verd.*
- *Hugging Bay, cua de descàrregues*: es poden **encuar múltiples models** (una en curs, la resta en espera) i **cancel·lar** cada descàrrega (atura el backend i esborra el «.part»); el modal mostra l'**espai lliure del disc destí** i avisa ABANS de baixar si el model no hi cap (p. ex. un GGUF de 247 GB en un USB petit). La cua sobreviu al tancament del diàleg (el modal ja no es desmunta) i el progrés en viu ara és visible per a cada model (claus d'event unificades `huggingbay:<id>`). Typecheck, lint, tests i `cargo check` en verd.*
- *Skills externes, verificació abans d'instal·lar*: una skill és codi que l'agent pot executar, així que abans d'instal·lar-la des de GitHub/URL/zip/carpeta es mostra un **resum del que conté** (blocs de codi, fitxers executables i senyals de risc: shell, sudo, ordres destructives, xarxa, `curl … | sh`, eval) i cal **confirmació explícita**. La preview llegeix la font en una carpeta temporal sense instal·lar res (mateixos límits de mida/HTTPS) i l'esborra després; nou comandament `skills_preview` + test del detector de riscos. Typecheck, lint, tests i `cargo test` (37) en verd.*

---

## 0.71.8-alpha (2026-11-12)

**Hugging Bay amb destí a triar + importació de codi web fiable**

- **Hugging Bay, destí a triar**: el destí de la descàrrega es pot triar **directament des del modal** («Tria carpeta / disc extern…»): els models GGUF es baixen a un disc extern (USB/HDD) sense passar per IA ▸ Models, amb botó per tornar al destí per defecte.
- **Importar codi del web, fiable**: l'extracció ja no depèn del títol del document — amb `eval_with_callback` de Tauri 2 el valor torna DIRECTAMENT a Rust (sense curses, sense límit de temps i la pàgina no es queda amb el títol esborrat); xarxa de seguretat per a pàgines sense `<pre><code>` (molts xats web renderitzen el codi en `<div class=language-…>` o en text entre clausilles), reintents (4 intents en 12 s) per a les pàgines que tarden a pintar, errors de l'script com a JSON (a Windows les exceptions no arriben al callback) i, quan no hi ha cap bloc, missatge útil (esperar que la IA acabi d'escriure, obrir «Veure codi» o prémer-ho de nou).

---

## 0.71.7-alpha (2026-11-12)

**IA local sense Ollama (llama.cpp) + Hugging Bay + context compactat**

- **Motor local sense Ollama**: secció nova al gestor de models per engegar els GGUF baixats amb `llama-server` (instal·lació amb binaris oficials de GitHub, vàlida a macOS, Windows i Linux); el servei es registra sol com a proveïdor local sense clau.
- **Hugging Bay**: cerca de GGUF al mirror verificat, detalls amb SHA-256 i descàrrega amb progrés que registra el model a Ollama.
- **Sense Ollama, tot funciona**: el xat cau a la IA en línia, el modal no s'obre sol i no hi ha instal·lacions forçades.
- **Context compactat**: en lloc de retallar, es condensa (objectiu, pla, esquelet del codi) i la IA continua a trossos; s'allibera RAM abans.
- **Codi, no documentació**: generació fitxer a fitxer, recordatori si la IA respon amb només `.md` i cap «bloc_N.md».
- **Blender**: recordatori automàtic i `import bpy` automàtic; figures sense dir «Blender».
- **Xat**: línia «què està fent la IA», raonament amagable i copiar en generació.

---

## 0.71.6-alpha (2026-11-11)

**APRENENATGE LOCAL — la IA que aprèn de les altres IA (en segon pla)**

- Pestanya nova a l'Agent: «Aprenentatge». Un interruptor engega l'observació de les converses i la injecció de les lliçons.
- NoOrbit registra cada torn (pregunta + resposta + proveïdor/model) de TOTES les IAs: la local d'Ollama, les remotes amb token, els especialistes i les IAs web. El registre viu a la carpeta de dades (`learning.json`), mai al repositori.
- Destil·lació en segon pla: el model LOCAL (Ollama) llegeix les converses pendents i en treu lliçons curtes (preferències, decisions, fets del projecte); si no hi ha cap model en marxa, una heurística detecta les preferències del text i l'aprenentatge funciona igual.
- Les lliçons que apliquen a la teva pregunta s'injecten al system prompt: la IA local sap el que ja li has ensenyat. Les preferències apliquen sempre; la resta, per coincidència de paraules.
- Destil·lació automàtica en segon pla (sessió «learning» aïllada: no barreja cancel·lació ni raonament amb el xat) i només quan l'usuari ja treballa amb el model local, per no carregar-lo a la RAM de cop. El xat NO espera.
- «Exporta el dataset» genera un JSONL de totes les converses en format de xat: el material per entrenar un model local amb LoRA (LLaMA-Factory, Unsloth, Axolotl).
- Tot local: cap dada surt de l'ordinador tret que l'usuari exporti el dataset. Desactivat per defecte (decisió de l'usuari) i esborrable sencer («Esborra tot»).
- Altres 0.71.6: checklist de tasques amagable, `@doing:` (el que la IA té entre mans, en directe), `write_file` que ja no descarta l'error de `create_dir_all` (explica el motiu amb `explain_io()`), recuperació del 404 amb model absent (descàrrega automàtica + reintent, a tots els camins) i correccions de tipus i de sintaxi.

---

## 0.71.5-alpha (2026-11-11)

**Entorn auto-instal·lable + Unreal sempre connectat**

- Finestra de comprovació de l'entorn a l'obrir (Ollama/Blender/ComfyUI) que instal·la SOLA el que falta i després engega el servidor; ruta REAL del binari ollama.
- Unreal: el `.uproject` es parcheja amb PythonScriptPlugin+RemoteControl (còpia .bak), `-ExecutePythonScript` en lloc d'ExecCmds (rutes amb espais), watchdog de connexió cada 5 s i cerca automàtica del `.uproject`; captura del viewport robusta; bug de qualitat d'il·luminació corregit.
- Usuari nou sense 404: si Ollama no té cap model, baixa'n un d'inicial (mida segons la RAM) amb progrés al gestor de models.
- Blender: música generada amb numpy a l'editor de seqüències, veu amb TTS del sistema (say/espeak-ng/SAPI), boca animada amb keyframes i àudio AAC al render MP4.
- L'agent desa els blocs de codi sense nom a `codi_ia/`.

*Correccions posteriors de la mateixa versió:*
- *fix: TRES BUGS que clippy va destapar — regex de neteja d'HTML amb retroreferència no suportada pel motor regex de Rust (la IA rebia `<script>`/`<style>` dins), bucle mai iterat al select de cancel·lació, atribut de lint inexistent duplicat.*
- *fix2: EL 404 AMB OLLAMA «DISPONIBLE» — a l'arrencada, si Ollama ja estava en marxa es feia return ABANS de comprovar models (ara `ensure_default_model` corre SEMPRE); el camí no-streaming (experts, equip, agent) ara també té la recuperació 404 del camí streaming.*


---

## 0.71.3-alpha (2026-11-11)

ESPECIALISTES EN DIRECTE — el que cada expert escriu en temps real (mini-chats per sessió «team:<id>», mateix format del xat, botó «Obre al xat»); execució per TORNS amb model local (Torn N/M, un acaba → comença l'altre) i alliberament del model després de cada torn (RAM); stream netejat a l'inici de cada treball en equip; rodet d'imatges en CPU per a Mac Intel (stable-diffusion.cpp, txt2img + img2img amb «-i»/«--strength»); CivitAI com a font de models (sense token); IAs locals instal·lables (LM Studio, Jan, GPT4All, llama.cpp); editors de vídeo (Kdenlive, OpenShot, DaVinci, Shotcut); nudge d'imatges (IMG_NUDGE) quan el model respon prosa; confirmació de tancament; botons Desfés/Refés a la barra; checklist de tasques fixat; GLM-5.3-Flash i GLM-OCR al catàleg


---

## 0.71.2-alpha (2026-11-10)

NÚVOL D'EQUIP — el projecte connectat a Git (connecta 1 clic, ↑↓ pendents, Sincronitza + Auto cada 90 s) per editar-lo des de diversos dispositius; Equip al xat (toggle + Crea equip ràpid Arquitecte/Programador/Revisor); convertir en fitxers rescata CRLF/~~~/blocs tallats/nom al final i demana el projecte sencer quan el model emés només un README; activació MANUAL de proveïdors (DeepInfer) amb «local: no cal token»; memòria: el nivell 3 conserva el fil mínim i el xat avisa; mida del model (GB) abans de descarregar (ollama.com + HF blobs + catàleg); permisos: «Recorda» guanya sobre confirma-cada-comanda i cd&&cat no pregunta


---

## 0.71.1-alpha (2026-11-10)

les Integracions DINS de l'Agent (sub-pestanyes: Xat/Equip/Ordinador/Unreal/Blender/Mòbil) + franja viva «equip treballant» sempre visible com fan feina; raonament de la IA sempre obert i AUTOMÀTICAMENT en català (crida crua) amb alternança «Original»; ajuda ▸ repetir l'assistent de benvinguda; paleta ⌘P amb 'benvinguda'


---

## 0.71.0-alpha (2026-11-10)

diàlegs grans amb lazy-load a demanda (main −27%); botó primari amb degradat del tema; focus ring als inputs; veta d'accent al fitxer actiu de l'arbre


---

## 0.70.0-alpha (2026-11-10)

menú «Servidor d'IA remot» + tutorial pas a pas inclos; Monaco ÀJUS (arrencada sense 4MB+8MB workers); −40% tokens (RUN_PROMPT comprimit + manual navegador contextual); tema «Verd i daurat»; dreceres amb tecles boniques, focus ring, veta d'accent al menú; il·lustracions SVG (editor/onboarding/servidor)


---

## 0.69.0-alpha (2026-11-10)

disseny — tokens de ritme/moviment + App.css dividida per domini (12 fitxers); permisos «una vegada i prou» (recorda per defecte + sessió + lectures inofensives sense preguntar); onboarding de 3 passos; autocomplete IA al editor (provider Monaco); editar la selecció amb la IA (diff + accepta/rebutja); workers de Monaco reutilitzats


---

## 0.68.0-alpha (2026-11-09)

README: novetats 0.62→0.68 (servidor remot, DeepInfer, diagnòstic, equip/RAM, arrencada robusta) + secció «Seguretat i claus API» — cap credencial al repo, tokens només a la carpeta de dades del sistema


---

## 0.61.3-alpha (2026-10-06)

release: v0.61.3-alpha — silencia l'error de Monaco en tancar diffs (pegat @monaco-editor/react 4.7 + Monaco 0.52: «TextModel got disposed…» filtrat a l'origen i a la consola)


---

## 0.61.2-alpha (2026-10-06)

fix(release): Windows nomes NSIS (setup.exe doble clic); MSI rebutja prerelease alfa no numeric


---

## 0.6.0 (2026-10-05)

release: v0.6.0 — navegador intern per a totes les IAs (NB|..), integracio de codi web, imatges al xat, catalogue d'IAs, Venice/proofed claus API, sistema de releases amb versions.json


---

## 0.5.0 (2026-10-05)

fix(windows): list_external_volumes no compilava per base fora de cfg(not(windows))


---

