# NoOrbit — Historial de versions

Totes les releases amb els seus binaris: <https://github.com/jpuigbert/noorbit/releases>.
Les notes completes de cada release són a la pàgina del release corresponent.

## 0.71.19-alpha (en curs)

**La Jam construïa «una altra cosa»: el brief original no arribava a la IA**

«Li dic que faci un joc i no fa un joc» — i com més s'explicava el joc, pitjor:
sortia un projecte diferent del demanat o, pitjor encara, un grapat de fitxers
soltos sense res a obrir ni a executar. El problema no era la IA: eren tres
forats del NoOrbit, tots el mateix efecte.

- *el brief original no arribava al worker* (`jam/mod.rs`): el prompt de cada
  tasca només portava la TASCA — un resum del planificador. Tota l'explicació
  del joc que havia escrit l'usuari es quedava al planner i es perdia pel camí,
  i la IA construïa el que se li demanava a la tasca, no el joc demanat. Ara el
  brief i el motor arriben al prompt de cada tasca, amb l'objectiu marcat com a
  tal («OBJECTIU DEL JOC») i amb la regla de no escriure res que no connecti amb
  aquest objectiu.
- *el motor triat no arribava al worker* (`jam/game.rs`): el worker detectava el
  motor PEL DISCO (`.uproject`, `project.godot`, `Cargo.toml`) i en un projecte
  nou no hi ha res a detectar: la IA escrivia en el llenguatge que li sembla
  (GDScript? Rust? C#?). El motor que tria l'usuari arriba ara decidit, amb els
  fitxers que ja existeixen al projecte i els que no s'han de tocar.
- *no es creava el projecte de joc* (`jam/game.rs`, `jam/commands.rs`): la Jam
  escrivia fitxers solts dins el workspace obert. Ara, si el workspace no és un
  projecte del motor triat, el joc es crea a `prototypes/<nom>/` amb l'estructura
  executable (Godot: `project.godot` + `main.tscn` + `main.gd`; Bevy:
  `Cargo.toml` + `src/main.rs`; Unity: `Assets/`), i la Jam hi treballa amb el
  seu PROPI repo git: amb el repo del pare, el `git add -A` i el `git clean -fd`
  del checkpointer tocaven el projecte de l'usuari sencer.
- *el pla demanava «un joc»* (`noorbit-planner`): el prompt del planner insisteix
  que les tasques cobreixin el brief sencer (ni una mecànica inventada, ni una
  de callada) i que la PRIMERA tasca deixi el projecte en un estat executable:
  entrada del jugador, bucle de joc i condició de guanya o perdre.
- *proves*: sis tests de regressió nous (el worker dona el brief i el motor a la
  IA; sense brief no n'inventa cap; el nom de la carpeta del joc; l'estructura de
  Godot i de Bevy; la carpeta del joc té repo propi) i un al planner (el prompt
  demana fidelitat al brief i un joc jugable); `cargo test` de l'app (427) i dels
  crates en verd.

**En un disc extern la Jam no acabava mai: el git no arribava a temps i la feina es perdia**

Treballar en un projecte de `/Volumes/...` (un disc exFAT sobre USB) donava sempre el mateix error cru:
«No s'ha pogut llegir /Volumes/Toshi/programes/forestin the sky/REPORT.md: No such file or directory
(os error 2)». El REPORT.md no era el problema: era la **consequència**.

- *el diagnòstic*: el volum és patològicament lent (un `stat` trigava 0,1-0,3 s i un `git status
  --porcelain` trigava 34,7 s). El timeout del checkpointer eren 30 s FIXES: tots els `add -A`,
  `status` i `commit` de la Jam es quedaven penjats i fallaven. Cada tasca acabava amb error i el
  `reset_hard` de rollback **esborrava el que l'agent acabava de fer**; la Jam cremava el pressupost
  en reintents que fracassaven igual i no arribava mai a escriure l'informe. El xat patia el mateix
  problema: el snapshot «shadow git» capturava l'arbre sencer (`add -A`) sense cap sostre de temps i
  cada torn de xat es quedava penjat.
- *el timeout adaptatiu* (`noorbit-git`): el timeout següent és `max(30 s, 3 × el que va trigar
  l'ordre anterior)`, amb un sostre de 10 min. En un disc ràpid no canvia res; en un disc extern
  creix sol, sense que l'usuari hagi de configurar res.
- *la feina ja no es perd* (`noorbit-jam`): si el commit falla, la Jam **no fa rollback**: els
  fitxers es queden a terra, la tasca es compta com a feta, es desactiva el checkpoint per al resta
  de l'execució i la Jam continua escrivint. «No hi ha canvis a commitar» (un error de la tasca, no
  del git) sí que es reintenta com abans. L'informe acaba amb un avís de quantes tasques s'han
  fet sense commit.
- *el xat no es queda penjat* (`agent/snapshot.rs`): les operacions del shadow git tenen un sostre
  de 120 s; passat el sostre, el snapshot es dona per perdut i l'escriptura continua (el snapshot
  és una xarxa de seguretat, no un requisit).
- *el missatge és honest* (`jam_read_report`, `commands/fs.rs`): el panell de la Jam llegeix
  l'informe a través d'un comandament nou que explica què està passant de veritat (Jam fallida,
  encara en marxa, volum lent) en lloc de l'error cru de Rust. I `read_file` tradueix l'error d'E/S
  al català («el fitxer no existeix» en lloc de «No such file or directory (os error 2)»).
- *sempre hi ha un informe* (`jam/commands.rs`): si el fill de la Jam mor sense deixar-ne cap, el
  pare escriu un REPORT.md mínim que explica per què. I si l'informe del bucle falla, es reporta en
  lloc d'escapar-se en silenci.
- *higiene d'exFAT* (`workspace.rs`, `agent/snapshot.rs`, `jam/commands.rs`): els sidecars
  AppleDouble (`._*`) que macOS crea en discs exFAT/FAT s'exclouen del `.gitignore` per defecte, del
  `info/exclude` del shadow i de l'exclusió de la Jam: no tenen cap valor i en un volum extern
  multiplicaven el nombre de fitxers que git havia d'indexar a cada torn.
- *proves*: dos tests de regressió nous a `noorbit-jam` (el commit que falla no esborra la feina i
  la Jam continua; «no hi ha canvis a commitar» es reintenta) i dos a `noorbit-git` (el timeout
  s'adapta al volum i el timeout configurat és el mínim); `cargo test` dels crates, `cargo check
  --all-targets` del backend, `tsc --noEmit`, ESLint i Vitest (359) en verd.

**El menú de l'editor en català, la pantalla de feina que s'obre sola i la xarxa altruista P2P**

Tres peces que feien falta per treballar hores sense por: el menú del botó dret de l'editor ara és
en català; quan la Jam treballa i no es toca res, en lloc d'apagar-se la pantalla s'obre l'estalvi
propi de NoOrbit; i qui vulgui pot donar inferència de la seva IA a projectes d'altri — i rebre'n —
sense cap servidor central.

- *el menú contextual en català* (`Editor/EditorPane.tsx` + nou `Editor/EditorContextMenu.tsx`):
  Monaco 0.52 no porta localització catalana i el seu mecanisme intern no és estable per injectar-ne
  una de parcial, així que el menú natiu es desactiva i se'n munta un de propi amb el patró que ja
  fa servir l'app: Desfés, Refés, Retalla, Copia, Enganxa, Selecciona-ho tot, Cerca, Substitueix,
  Vés a la definició, Reanomena el símbol, Formata el document i «Edita la selecció amb la IA».
- *la pantalla de feina s'obre sola* (nou `screensaver/autoSaver.ts`): quan la Jam o el treball
  autopilotat corre i la inactivitat supera el límit (5 minuts per defecte, configurable als
  Ajustos), s'obre l'estalvi de NoOrbit en lloc de parar-se l'ordinador: es veu de lluny què fa la
  IA i en quin projecte, i un clic o qualsevol tecla torna a la feina. I `keep_awake` afegeix `-d`
  al `caffeinate` de macOS perquè la pantalla no s'apagui mentre la Jam corre i l'estalvi es mostra.
- *la xarxa altruista P2P* (mòdul nou `src-tauri/src/grid/` + secció «Xarxa altruista» als Ajustos):
  sense servidor central — el propietari del projecte escolta amb QUIC (certificat autofirmat
  generat un cop i desat a l'appdata) i els voluntaris s'hi connecten amb una invitació
  (`noorbit-grid://…`). El voluntari només aporta INFERÈNCIA: respon els torns de pensament de la
  Jam amb el seu model local (mai executa codi), i només quan passa la porta triple — inactivitat
  real, estalvi obert i cap generació local en curs —. Si cap voluntari respon abans de la data
  límit, la IA local del propietari respon el torn: res no es perd.
- *la distribució estil torrent* (`grid/` + botó «Distribueix als participants» al panell de la
  Jam): quan la Jam acaba, el desenvolupador principal pot — explícitament, mai automàtic — oferir
  una còpia del projecte acabat als voluntaris que hi han donat almenys un torn. El projecte es
  parteix en peces de 256 KB amb hashes SHA-256 (mateixes exclusions que els snapshots), es
  descarrega amb represa per bitfield si l'estalvi es tanca i es torna a obrir, es verifica peça a
  peça i es munta a la carpeta triada; cada còpia queda verificada pel manifest.
- *fora d'abast (MVP honest)*: el P2P funciona en xarxa local o amb el port del propietari obert
  (el voluntari inicia la connexió, així que només el propietari ha de ser accessible): sense
  NAT traversal ni descobriment, sense reputació, sense tasques de CPU ni execució de codi remot.
- *proves*: `cargo check --all-targets` i `cargo test --lib` del backend (416 tests: protocol,
  codis d'invitació, caducitat de la cua, manifest i peces amb verificació SHA-256 i represa,
  edicions de renombrament LSP),
  `cargo check --workspace` dels crates, `tsc --noEmit`, ESLint i Vitest (360) en verd.

## 0.71.18-alpha (2026-11-23)

**Godot es moria sol al Mac: era la «translocació» de macOS, no el projecte**

Godot 4 es va instal·lar, es va obrir i, al cap d'una hora, va petar amb `EXC_BAD_ACCESS
(SIGBUS)`. No era un error del projecte ni de la GPU: era macOS.

- *el diagnòstic* (`godot/mod.rs`): l'informe de fallada deia, al final, «backing vnode was force
  unmounted», i el procés que havia petat corria des de `/private/var/folders/…/Godot.app/…`, no
  des de `/Applications`. Això és la **translocació**: una app baixada del navegador (amb la marca
  `com.apple.quarantine`) no s'executa on és, sinó des d'una còpia temporal de només lectura. Quan
  macOS desmunta aquella còpia —cosa que fa quan el sistema netja la carpeta temporal— el procés
  que hi ha corrent es queda sense les pàgines de codi i mor amb SIGBUS. En aquest cas el motor
  s'havia baixat amb l'Opera, que és el que posa la marca.
- *el problema real*: NoOrbit deia «Godot no està instal·lat» tot i que el motor era obert al
  costat de la finestra, perquè la detecció només mirava `/Applications`, Homebrew i el `PATH`.
- *detecció de debò* (`detect_install`, `apps_in`, `app_binary`): es busquen les apps `Godot*.app`
  de `/Applications`, `~/Applications` i `/Applications/Godot`, el ZIP descomprimit de les
  Descàrregues i, finalment, la **translocació** (el lloc on macOS executa de debò l'app baixada).
  L'ordre és a propòsit: la instal·lació estable sempre guanya a la còpia temporal, perquè les dues
  últimes es poden desmuntar sota els peus del motor.
- *s'ha de poder arreglar* (`is_quarantined`, `remove_quarantine`, `godot_fix_quarantine`): la interfície
  detecta la marca de quarantina i ofereix un botó **«Arregla»** que la treu, de manera que macOS
  deixi de transplantar l'app. Un clic i el motor ja s'executa des de la seva ruta real.
- *l'estat de la instal·lació* (`GodotStatus`): ara diu on és el motor, si viu en un lloc volàtil
  (temporal o Descàrregues) i si té la marca de quarantina, en lloc de dir només «instal·lat».
- *proves*: tests Rust nous de la detecció (rutes temporals = volàtils, app instal·lada = estable,
  binari dins de l'app, només es filtren les `Godot*.app`); `cargo test --lib` (399), `tsc --noEmit`,
  ESLint i Vitest (359) en verd.

**Godot: el projecte es crea sol. I la plantilla de Bevy, de fet, compilava?**

Treballar en Godot quan el projecte encara no existia era un carreró sense sortida: NoOrbit
deia «no hi ha cap projecte de Godot (project.godot) dins el projecte» i deixava a l'usuari
la feina de muntar l'escena a mà. Ara el projecte **neix sol**, i el mateix passa amb Bevy.

- *el projecte de Godot es crea automàticament* (`godot/setup.rs`, `ensure_project`): quan
  l'usuari demana treballar en Godot i no hi ha cap `project.godot`, NoOrbit crea el
  `project.godot`, el `main.tscn` i el `main.gd`, i afegeix `.godot/` al `.gitignore`.
  L'error ja no apareix: el que hi ha abans és un projecte **que s'obre**.
- *una sola font de veritat* (`godot/setup.rs` + `ai/codegen.rs`): les constants
  `GODOT_PROJECT_GODOT`, `GODOT_MAIN_TSCN` i `GODOT_MAIN_GD` són les que fa servir també la IA
  nativa, de manera que un projecte creat automàticament i un projecte generat per l'IA són
  **idèntics** i no pod divergir mai. El `project.godot` nou va amb `config_version=5`,
  `run/main_scene="res://main.tscn"` i el renderer «Compatibility» (OpenGL 3.3), que és el
  que funciona amb la GPU integrada dels Mac Intel.
- *tres punts d'entrada, el mateix comportament* (`engine_open.rs`, `engine_tools.rs`,
  `engine_bridge.rs`): «Obre al motor», les directrius `@godot:` i `@bevy:` i el pont d'actius
  `@import:`. Tots tres busquen el projecte i, si no hi ha, el creen abans de continuar.
- *mai no es pisa res* (`godot/setup.rs`, `bevy/mod.rs`, `fsutil.rs`): la creació és
  **idempotent** (crear dues vegades no fa res), un `project.godot` existent no s'overescriu,
  i si a la carpeta ja hi ha `main.tscn`/`main.gd` (o un `Cargo.toml` que no és de Bevy) el
  projecte nou va a una **subcarpeta** en lloc d'esborrar el treball de l'usuari. El
  `.gitignore` s'hi **afegeix**, mai no se substitueix.
- *la detecció continua sent innocent*: `EngineContext::detect()` no crea res (només mira el
  disc); la creació només passa per les rutes d'acció, que és on l'usuari ha demanat un motor.
- *la plantilla de Bevy NO compilava* (`bevy/setup.rs`): el `src/main.rs` que es generava feia
  servir `Assets<Material>` —a la 0.14 `Material` és un *trait*, no el tipus de l'actiu— i
  `Cube`, que a la 0.14 no existeix. El projecte que creava NoOrbit no es podia compilar.
  Ara la plantilla és l'exemple oficial `3d_scene` de Bevy 0.14: `Assets<Mesh>`,
  `Assets<StandardMaterial>`, `Cuboid::new(1.0, 1.0, 1.0)` i `Color::srgb(…)`. **Verificat
  compilant de debò** el projecte generat (`cargo check`: 0 errors).
- *proves*: tests Rust nous de l'auto-creació (projecte de Godot creat i detectat, coherència
  entre manifest i escena, idempotència, `.gitignore` de l'usuari respectat, projecte Bevy
  garantit, `Cargo.toml` alien no pisat) i un test que **vigila l'API de Bevy 0.14** perquè
  la plantilla no torni a quedar obsoleta; `cargo test --lib` (395), `tsc --noEmit`, ESLint i
  Vitest (359) en verd.

**Bevy a macOS: es detecta de veritat, i l'assistent fa les preguntes (on instal·lar Rust i on treballar)**

Treballar en Bevy al Mac era mig cremador: el `cargo` es cercava només pel `PATH` i
per `~/.cargo`, el `Cargo.toml` es llegia a text lliure (així que `bevy_ecs` feia
creure que un projecte era de Bevy) i, quan el Terminal no s'obria, el botó no donava
cap error. Ara el flux és complet i verificable:

- *detecció real del «cargo»* (`bevy/mod.rs`, `detect_cargo`): es proven la carpeta de l'entorn (`CARGO_HOME`), la que l'usuari ha triat a l'assistent, `~/.cargo/bin`, les rutes de Homebrew i, al final, el `PATH`. L'app llançada des del Finder (que no té el `PATH` de la shell) troba el `cargo` igual. La `Jam` ja no crida `cargo` a dit (`jam/mod.rs`): feia el control de qualitat fallar sense motiu en un Mac.
- *el manifest es parseja, no es busca* (`manifest_has_bevy` / `manifest_bevy_version`): només compta la dependència `bevy` exacte, es detecten `[dependencies.bevy]`, `[dev-dependencies]` i `[target.'…'.dependencies]`, i `bevy_ecs`/`bevy_render` ja no fan false positius.
- *errors que arriben a l'usuari* (`launch_project`): l'«osascript» s'espera i, si macOS denega el permís d'automatització, el motiu real torna a la interfície en català en lloc de deixar un botó que no fa res.
- *assistent de Bevy* (`bevy/setup.rs` + `BevySetupModal.tsx`): l'instal·lador de Rust deixa de ser una pàgina de descàrrega i es converteix en **preguntes**: on viu el `cargo` (per defecte o en una carpeta triada, fins i tot en un disc extern) i on treballar el projecte (nom + carpeta). La instal·lació és l'oficial de rustup en un Terminal **transparent**, i la carpeta triada es desa a la configuració perquè la detecció la trobi sempre.
- *projecte nou verificat* (`bevy_create_project`): es crea el `Cargo.toml`, el `src/main.rs` (API de Bevy 0.14) i el `.gitignore`, i tot seguit es **detecta** amb el mateix detector que fa servir l'IA: si no es reconeix, es crea igual però l'assistent ho diu.
- *proves*: tests Rust nous de la detecció (`bevy_ecs` no compta, taules de dependències, versió llegida, el `cargo` detectat existeix al disc, noms de projecte vàlids) i de l'assistent (projecte creat i detectat, nom invàlid rebutjat, `Cargo.toml` sense `bevy` no detectat); `tsc --noEmit` i ESLint nets.

**El motor llama.cpp dins del bundle universal: les dues arquitectures, i res en silenci**

Els instal·ladors de la 0.71.17 portaven el model GGUF (396 MB) però **no el motor**
a macOS: Windows i Linux sí que l'empaquetaven. Cap test ho deia: el pas de CI és
`continue-on-error` i l'API de GitHub havia respost un error de quota que el script
iterava com si fos la llista de releases (`rels is not iterable`), matant el procés
DESPRÉS d'haver baixat el model. Només es va veure llistant el contingut del `.app`
ja publicat.

- *API validada, amb reintent i token* (`scripts/fetch-native-model.mjs`, `releasesLlamaCpp`): es comprova l'HTTP i que el payload sigui REALMENT una llista, tres intents amb espera creixent, i el `GITHUB_TOKEN` del runner (de 60 a 5000 peticions/h). El motor és opcional: **avisa i continua**, no mata el build i no calla (`avisa` emet `::warning::` dins Actions).
- *bundle universal de veritat* (`assetsAEmpaquetar` i `engine_src_for`, `ai/native_llm.rs`): a macOS es baixen **LES DUES slices** (`bin-macos-arm64` + `bin-macos-x64`) a `native-engine/macos-arm64|macos-x64`, i l'app copia **només la de l'arquitectura que s'executa**. Abans es copiava tot el directori i `llamacpp::binary()` agafava la primera carpeta que trobava: un Mac Intel amb un binari arm64 rep «bad CPU type in executable», que és pitjor que no portar-ne cap.
- *què porta realment l'instal·lador, escrit al log* (`.github/workflows/release.yml`, pas `Verify bundled model + engine`): comprova el `.gguf` i cada slice del motor i ho converteix en anotació visible, perquè un `continue-on-error` no torni a amagar res.
- *docs que no caduquen* (`README.md`, `INSTALL.md`): els noms d'instal·lador no porten una versió escrita a mà (el README deia 0.71.13 i l'INSTALL 0.71.6); ara `<VERSIÓ>`, globs o `releases/latest`.
- *proves*: 3 tests Rust nous de la selecció per arquitectura (bundle amb les dues slices, slice absent, motor pla i bundle sense motor); verificat de punta a punta amb la descàrrega real dels dos binaris: la slice x64 arrenca (`Darwin x86_64`) i l'arm64 dona «bad CPU type» en aquest mateix Mac — per això escollir la bona no és un detall cosmètic. `cargo test --lib` (368), `cargo check --all-targets`, `tsc --noEmit`, ESLint, Vitest (359) i `vite build` en verd.

**Els snippets d'Epic Games, a la IA local d'Unreal**

La comunitat d'Epic Games publica els millors exemples d'Editor Scripting d'UE5, i eren justament el que li faltava a la IA local de NoOrbit: sense referència, els models locals escrivien `EditorLevelLibrary` i `EditorAssetLibrary`, deprecades des d'UE 5.0/5.1, que executades pel pont Remote Control **no fallen** — l'usuari veu «✅ canvis aplicats» i l'escena no ha canviat. Ara els patrons enters, amb les signatures reals, arriben al prompt per quatre vies.

- *skill ECC* (`skills-builtin/unreal/SKILL.md`): 12 patrons numerats (subsistemes, actor amb malla, moure/girar/escalar, llums, text 3D, importació FBX/GLB/OBJ, Blueprint, material amb color, reixilla procedural, lightmaps, llistar i esborrar) amb les regles d'or i les fonts. La carpeta es diu `unreal` i no `unreal-engine` perquè `ai/ecc_native.rs::llista_skills` tokenitza el nom de la carpeta: amb «unreal» cal un sol token perquè la skill aparegui.
- *catàleg de codi* (`ai/codegen.rs`): tres plantilles noves — «Il·luminació Unreal (Python)», «Importació d'assets Unreal (Python)» i «Escena Unreal (Python)» — que guanyen el patró genèric quan la petició és específica (el desempatge del catàleg resol cap al patró més específic i l'ordre del `vec!` el decideix). Cada títol té el seu braç a `default_per_titol` perquè el fitxer es digui `iluminacio_…`, `importa_…` i `escena_…` en lloc de `app`.
- *prompt de l'agent* (`agent/mod.rs::apply_unreal`): el block `PATRONS_PYTHON` s'afegeix al final del missatge de sistema, de manera que el model veu les signatures exactes (`build_light_maps`, `import_assets_automated`, `unreal.Rotator`…) abans de decidir què escriure.
- *mòdul nou* (`unreal/snippets.rs`): consulta l'API GraphQL de la comunitat d'Epic (8 s d'espera, agent de navegador i extracció tolerant del JSON) i, si no hi ha resposta, torna el catàleg local de 10 patrons: l'agent mai es queda sense referència. Un circuit obert 5 minuts després d'un fracàs evita que cada torn pagui l'espera quan l'usuari no té xarxa. La comanda `unreal_search_snippets` l'exposa al frontend (`core/unreal/client.ts`) i el bucle MCP (`unreal/agent.rs`) injecta els snippets a cada torn, amb un esdeveniment «context» que en deixa el rastre a la interfície.
- *les APIs deprecades, fora* (`ai/codegen.rs`, `unreal/snippets.rs`): ni el codi generat ni el block de patrons en porten el nom — un model local copia el que llegeix, i un nom de classe dins d'un comentari és una invitació a cridar-la. Els avisos de «subsistema obsolet» s'han reescrit sense el nom de les classes.
- *de pas, un error de la creació de projectes Bevy* (`bevy/setup.rs::bevy_create_project`): el nom es validava DESPRÉS de crear la carpeta pare, de manera que una petició amb un nom invàlid deixava una carpeta buida al sistema de fitxers. Ara el nom es valida primer i el disc no s toca.
- *proves*: 8 tests Rust nous (els patrons no contenen APIs deprecades, el circuit obert després d'un fracàs, el catàleg local té codi útil, el filtre per consulta, el parseo d'una resposta GraphQL, la resposta sense snippets, el prompt de context i els patrons d'Unreal guanyant el genèric al catàleg); `cargo test --lib` (395), `cargo check --all-targets`, `tsc --noEmit`, ESLint, Vitest (359) i `vite build` en verd.

## 0.71.17-alpha (2026-11-22)

**GitHub Copilot com a motor del xat: el runtime en Rust, parlat per ACP**

GitHub ha reescrit el runtime de Copilot en Rust — el mateix que alimenta el CLI, el SDK, l'extensió de VS Code i M365 Copilot. NoOrbit hi pot parlar **directament**, sense cap API de compatibilitat: el CLI de Copilot exposa un servidor **ACP** (Agent Client Protocol: JSON-RPC 2.0 sobre NDJSON) i NoOrbit hi actua de client.

- *client ACP propi* (`src/acp/mod.rs`): obre la sessió (`initialize` + `session/new`), mana el prompt (`session/prompt`) i atén les peticions de l'agent: `fs/read_text_file` i `fs/write_text_file` — tots dos **dins de la carpeta del projecte**, amb normalització de `..` i comprovació dels enllaços simbòlics — i `session/request_permission`.
- *streaming de veritat* (`ai/mod.rs`): cada fragment que arriba es retransmet com a «ai://chunk» i el raonament va per separat, igual que la resta de motors. El fil del diàleg **es conserva entre torns** (la sessió ACP viu per xat i per projecte) i el botó «Atura» envia `session/cancel` a l'agent.
- *permisos a la interfície* (`AcpPermissionModal.tsx`): quan l'agent vol fer alguna cosa, apareix un diàleg amb l'operació i les opcions que ofereix l'agent. **Mai no s'aprova en silenci**: si l'usuari no contesta en 3 minuts, l'operació es rebutja.
- *instal·lació sense administrador* (`external_orchestrator/copilot.rs`): si el CLI ja és al sistema (npm global, Homebrew…) es fa servir tal qual; si no, el botó l'instal·la dins les dades de NoOrbit amb el mateix patró que el DeepSeek Harness (baixa el Node.js LTS oficial si al sistema no n'hi ha i fa `npm install @github/copilot`), sense tocar res del sistema i conservant-se en mode portàtil/USB.
- *al xat i al Gestor de proveïdors* (`AgentPanel.tsx`, `HarnessSection.tsx`): «GitHub Copilot (ACP)» apareix com a IA del xat, i el Gestor de proveïdors mostra l'estat (instal·lat o no, amb versió), el botó d'instal·lació i l'avís de fer «copilot login» una vegada.
- *proves*: 8 tests Rust nous del client ACP (parseo dels tres tipus de missatge, soroll ignorat, text i raonament separats, crida d'eina, camins fora de l'espai rebutjats, lectura d'una porció de fitxer i permís sense resposta); `cargo test --lib` (319), `tsc --noEmit`, ESLint, Vitest (275) i `vite build` en verd.

**Rutes absolutes, bucles d'edicions i `RUN|` enganxats**

Un projecte obert des d'un volum extern (`/Volumes/Toshi/programes/forestin the sky`) acabava amb un xat que reemetia **15 blocs idèntics** i una carpeta falsa `Volumes/Toshi/…` dins del propi projecte. Són quatre causes reals, i les quatre estan arreglades:

- *una sola manera de resoldre rutes* (`agent/runtools.rs`, `write_rel` + `fileBlocks.ts`, `relForWrite`): si la ruta del model **reprodueix l'arrel** (amb o sense barra inicial, repetida dues o tres vegades, amb symlink o majúscules diferents) se'n treu l'arrel i queda RELATIVA; si és realment FORA, no s'escriu mai. El backend i el frontend deixen de divergir (abans l'un rebutjava la ruta en silenci i l'altre hi creava la carpeta falsa).
- *fi de l'error silenciós* (`apply_edit_blocks_at`, `rejected_write_paths`, `applyEditBlocks`, `autoMaterialize`): una ruta rebutjada —tant a les «@edit:» com a les «@file:», que abans es descartaven soles— **s'escriu al report**: el model rep la ruta bruta, l'arrel esperada i com ho ha de reemetre, i l'usuari veu un «⛔ … FORA del projecte obert» al resum del torn (`agentFlow.pathsOutsideProject`). Abans era un `continue` mut: el model no sabia res i la reemetia igual torn rere torn.
- *guard anti-bucle* (`commands/ai.rs`, `action_fingerprint`): es compara NOMÉS el pla d'accions (`RUN|`, `@edit:`, `@file:`, `NB|`) —mai la prosa—. Primer torn idèntic: no es torna a executar res i s'envia al model una nota amb les quatre correccions concretes; segon: el torn s'ATURA i ho fa visible a la UI. `MAX_ROUNDS` ja no es crema repetint una comanda que no pot canviar res.
- *`RUN|` enganxats* (`split_glued`): «`RUN|pip install -r requirements.txtRUN|python3 main.py`» era **una sola** comanda innombrable; ara es tallen en tantes com cal, respectant les cometes (un `run|` dins d'un grep no es parteix).
- *prompt* (`RUN_PROMPT`): rutes SEMPRE relatives a l'arrel, rutes amb espais **entrecomillades** dins `RUN|`, cada directriu a la seva línia i prohibició de reemetre una acció que no canvia res.
- *proves*: 8 tests Rust nous (`write_rel` amb volum extern, matriu de rutes rebutjades, `RUN|` enganxats, segell d'accions, report no mut tant d'«@edit:» com d'«@file:») i 11 de Vitest (`relForWrite`, `@edit:`/`@file:` absoluts dins i fora del projecte); `cargo test --lib` (353), `tsc --noEmit`, ESLint i Vitest (338) en verd.

**L'app real en marxa: els quatre defectes que només es veuen fent clics**

El pla es va validar amb tests, però mai s'havia executat el binari. Fer de veritat els fluxos estrella (tema de WordPress des del xat, «Desfés el pas», terminal PTY, mode professional) en va treure quatre que cap prova de unitat veia:

- *«fes un joc» amb IA local feia Pygame* en lloc de Godot: amb un LLM disponible (Ollama) el torn es menjava la branca neuronal i **bypassava el determinista**. Ara `scaffold_joc_generic` (`ai/codegen.rs`) resol el joc genèric amb el projecte Godot 4 canònic (`project.godot` + `main.tscn` + `main.gd`) abans que el model respongui, i la consigna del proveïdor local (`ai/sysprompt.rs`, `provider_layer`) fixa Godot com a motor per defecte. Els motors **explícits** («fes un joc en pygame/Unity/Bevy…») no s'intercepten mai.
- *la cadena greu «PNG fals → pantalla de pànic → pèrdua del projecte»*: la IA escrivia text dins d'un `.png`, el visor el servia sense validar-lo, la webview no el decodificava, l'error arribava al vigilà d'arrencada i **segrestava tota la UI**; en reiniciar, el projecte es perdia. Tallada als quatre nivells: `magic_ok` (`commands/image.rs`) rebutja el fitxer si la signatura no correspon a l'extensió, el vigilà (`index.html`) cobreix **només la finestra d'arrencada** i es retira quan React ha pintat, l'editor i el xat degraden a un placeholder «imatge no vàlida» (`onError` en lloc de petar) i `restoreLastRoot` reobre el projecte després de qualsevol reinici.
- *Esc no tancava els modals*: Configuració, Projecte nou i Compte tenien cadascun la seva lògica (o cap). Ara comparteixen `utils/useEscapeToClose.ts`.
- *lastRoot esborrat per un permís pendent*: després d'un rebuild amb firma ad-hoc, macOS torna a demanar accés (TCC) a l'Escriptori o als volums i `openWorkspace` falla **temporalment**; el codi esborrava la clau `noorbit.lastRoot` per a qualsevol error, així que els reinicios següents ja no restauraven res. Ara només l'esborra si el camí **realment no existeix** (`isMissingRootError`, amb el backend reservant «… no existeix» per a `ErrorKind::NotFound`) i la conserva en fallades de permís.
- *proves*: tests Rust nous de l'enrutament del joc (`joc_generic_amb_llm_local_resol_el_scaffold_godot`, `joc_explicit_no_s_intercepta_mai`, `joc_generic_nomes_la_pregunta_real`, `joc_generic_godot_i_endevinament_nomes_explicit`, `la_consigna_local_fixa_godot_com_a_motor_per_defecte`) i de la signatura d'imatge; Vitest per al vigilà d'arrencada (`bootGuard.test.ts`, sobre l'`index.html` real), el visor d'imatges, la imatge del xat, l'Esc dels tres modals i la restauració del projecte. Total: `cargo test --lib` (365), `cargo test --workspace` a `crates`, `tsc --noEmit`, ESLint, Vitest (359) i `vite build` en verd.

## 0.71.16-alpha (2026-11-21)

**El cost real dels tokens: què s'ha consumit i què s'ha pagat**

«Es pot tornar la informació a l'API dient que realment no s'han gastat tokens?» La resposta curta: el compte del proveïdor de núvol és del servidor — el client no hi pot tocar, i enganyar-lo seria exportar-ho malament. El que sí que es pot fer, i el que fa aquesta versió, és **calcular el cost real**: els tokens servits des de la memòria cau del proveïdor es consumeixen igual, però es paguen a una fracció del preu, i fins ara NoOrbit els tractava com si es paguessin sencers.

- *la memòria cau, per proveïdor* (`api/mod.rs`, `parse_usage`): OpenAI-compatible (`prompt_tokens_details.cached_tokens`), DeepSeek (`prompt_cache_hit_tokens`) i Anthropic (`cache_read_input_tokens` + `cache_creation_input_tokens`) queden separats dels tokens d'entrada; Anthropic els exclou de `input_tokens` i això es té en compte per no comptar-los dos cops.
- *preus reals* (`ai/pricing.rs`): taula de preus en USD per milió de tokens (entrada, sortida, lectura i escriptura de la memòria cau) per a 21 models d'OpenAI, Anthropic, DeepSeek i Gemini; el cost es calcula amb els tokens **pagats** (entrada menys els de la memòria cau), no amb els consumits. Model desconegut → **cap xifra inventada**: el cost queda buit i la interfície no hi mostra res.
- *els agents també hi compten* (`ai/mod.rs`): el camí sense streaming (planificadors, agents, equips) **no emetia cap comptabilització** de tokens; ara emet el mateix esdeveniment que el xat.
- *proveïdors locals a cost zero* (`is_free_provider`): Ollama i la IA integrada mostren «gratis» en lloc d'un preu estimat.
- *al xat* (`AgentPanel.tsx`, `agentStore.ts`): cada resposta mostra els tokens de memòria cau i el **cost estimat del torn** en dòlars, amb el tooltip que explica quins tokens es paguen al preu sencer.
- *proves*: 8 tests Rust nous (4 de la taula de preus, 4 del parseo de la memòria cau per proveïdor); `cargo test --lib` (283), `tsc --noEmit`, ESLint, Vitest (160) i `vite build` en verd.

## 0.71.15-alpha (2026-11-21)

**La Jam ara es veu treballar, es pot canviar d'IA en marxa i allibera la memòria**

Donar una tasca a l'Autonomous Jam era un salt al buit: el registre existia però no avançava sol, no hi havia manera de triar amb quina IA treballava, i els gigabytes dels models locals quedaven ocupats sense fer serves.

- *visibilitat en viu* (`jam/mod.rs`, `JamPanel.tsx`): el worker emet un **cor batent** cada 20 s («la IA genera el codi… (fa 40 s)») perquè el model local no sembli encallat; el registre fa **autoscroll**, marca l'hora de cada línia, i la capçalera mostra «en marxa» amb pols i «fa X s» del darrer esdeveniment.
- **tria d'IA** (`JamRequest`, `resolve_jam_ai`, `AiPicker`): el desplegable «IA que farà la Jam» ofereix la IA integrada, els models locals d'Ollama i els proveïdors en línia; la tria s'aplica al **pla** i a **cada tasca**, i el pla es fa en una sessió aïllada (`jam-plan`) per no canviar la IA del xat de l'usuari sota els seus peus.
- **canvi d'IA EN MARCA** (`jam_set_ai`, `jam/agent.rs`): el backend desa la tria a `ai.json` i el procés fill la rellegeix cada 2 s — la Jam continua amb la IA nova sense aturar-se ni perdre la feina feta.
- **«Allibera la memòria»** (`ai_free_ram`): un sol botó descarrega els models d'Ollama residents i atura els motors locals (llama.cpp), mesura la RAM abans i després i diu quant s'ha recuperat. Es desactiva mentre la Jam treballa (ara la memòria sí que s'està fent servir).

## 0.71.14-alpha (2026-11-21)

**Les edicions buides «@edit:» ja no passen per feina feta**

Un model consultat per afegir una dependència al `requirements.txt` va emetre un bloc CERCA/NOU amb el **mateix text als dos bàndols** — no canviava res — i NoOrbit ho va anunciar com a «📁 Aplicats automàticament (1 fitxers)». Mentida a la UI, diff verd/roig fals i un bucle que no aprèn res.

- *rebuig no-op al disc* (`agent/runtools.rs`, `apply_edit_blocks_at`): si CERCA == NOU la secció **no compta com a aplicada i no s'escriu res**; si tot el bloc era no-op, l'informe que torna al model diu explícitament que les seccions són IDÈNTIQUES i que no emet res si el fitxer ja és correcte (abans el model no rebia cap senyal).
- *mateix criteri al xat* (`agentStore.ts`, `applyEditBlocks`): les seccions idèntiques es listen a part («⏹️ Edicions inútils rebutjades») en lloc d'obrir un diff que no canvia res.
- *la Jam no es deixa enganyar* (`jam/mod.rs`): la segona via d'escriptura només comptava qualsevol informe com a feina feta; ara exigeix com a mínim una edició realment «aplicada».
- *prompts reforçats* (`RUN_PROMPT`, `prompts.ts`, regles del worker de la Jam): el text NOU ha de ser DISTINT del CERCA, mai edicions buides, i escriu NOMÉS els fitxers que la petició demana.
- *proves*: test Rust nou (`noop_editions_are_not_applied`: el fitxer queda intacte i l'informe ho declara) i test Vitest nou (el parser conserva les seccions no-op perquè l'aplicador les rebutge); `cargo check --lib`, `cargo test edit_tests`, `pnpm typecheck` i Vitest (23 a `fileBlocks`) nets.

**Experts: gestió d'estil «treballar amb professionals», menys RAM/CPU i temps estimat**

El panell d'Especialistes passa de «llista amb caselles» a una gestió completa, i el treball en equip deixa de malgastar minuts de CPU recarregant el mateix model a cada torn.

- *selecció i filtratge* (`ExpertsPanel.tsx`): botó **Tots/Cap** per seleccionar tot l'equip de cop (actua sobre els visibles quan hi ha filtre), recompte «{n} de {total} seleccionats», cerca per nom/rol quan n'hi ha més de 5, i **duplicar** un especialista per partir d'un de semblant i afinar-lo.
- *claritat del flux*: la línia d'equip sempre visible explica què passarà («Treballaran: …» o «sense seleccionar-ne cap, treballaran tots N») i el botó d'iniciar-ho ho recorda al tooltip.
- *RAM/CPU al treball en equip* (`commands/experts.rs`): abans es descarregava el model local **després de cada especialista**, obligant a recarregar-lo (minuts de CPU a Mac amb CPU) tot i que els següents feien servir el mateix. Ara el model es **manté mentre el torn consecutiu utilitzi el mateix model** i es descarrega només quan canvia, quan la fusió no el necessita, quan l'usuari atura l'equip i en acabar; el protocol de fitxers de cada especialista (`FILE_PROTOCOL`) afegeix regles d'estalvi: edita amb «@edit:» sense reemetre fitxers sencers i escriu NOMÉS el que la tasca demana (menys tokens = menys RAM i menys temps).
- *temps estimat (ETA)*: els events de progrés de l'equip inclouen `eta_ms` calculat amb el **ritme real** (temps mitjà per subtasca feta, amb priors locals 150 s / núvol 40 s) i es mostra a la UI («queda 4 min 30 s»). Al xat (`AgentPanel.tsx`), mentre la IA treballa es mostra un ETA deduït de la **mediana de longitud i velocitat de les respostes anteriors** d'aquell xat i, quan ja està escrivint, del cabal de caràcters en viu (barrejat amb l'historial per no distortar-se amb el primer token).
- *proves*: `tsc --noEmit`, Vitest (107 proves), `cargo check --lib` i `cargo test edit_tests` en verd.

**Experts declaratives i jerarquia cap/obrer (després d'estudiar Qoder, OpenCode i VS Code)**

S'han revisat els agents instal·lats al sistema (subagents `.md` de Qoder amb capçalera `name/description/model`, el model d'agents d'OpenCode amb `mode` primari/subagent i permisos, i el registre d'accions/paleta de VS Code) i se'n traslladen els patrons que milloren com es gestiona i es treballa amb els especialistes:

- *especialistes com a fitxers `.md`* (`utils/expertMarkdown.ts` + botons al panell): cada expert pot **exportar-se** a un Markdown declaratiu (capçalera `name/description/model/mode` + instruccions al cos, com els subagents de Qoder/d'OpenCode) i **importar-ne** diversos de cop: si el nom ja existeix, el fitxer MANA i sincronitza. Així els equips es poden versionar amb Git dins del projecte i compartir entre màquines.
- *mode cap/obrer* (nous valors `mode` a `Expert`/`ExpertInput`, Rust i TS, backward compatible per defecte «all»): **primary** = cap que dona ordres al xat (no apareix al roster de delegació «@use:», no fa d'obrer a l'equip), **subagent** = obrer (delegable i d'equip, no es pot vincular al xat), **all** = ambdós. S'aplica a totes bandes: formulari (selector) + xips a la targeta, filtre del selector d'expert del xat, `rosterContext()` (tots 5 punts d'injecció coberts d'una vegada), delegació `@use:` i equip per defecte (sense selecció, treballen només els obrers).
- *ECC adaptat*: els 72 agents `.md` que importa el botó ECC es marquen **subagents** per definició (a la pràctica ja ho són: els crida un orquestrador), alliberant el selector del xat sense perdre'ls com a obrers.
- *paleta de comandes* (⇧⌘P, l'infraestructura ja existia estil VS Code): noves entrades «Especialistes: nou especialista» i «Especialistes: importar fitxers .md…» — accés des de qualsevol pantalla.
- *proves*: 7 tests Vitest nous (`expertMarkdown`: serialització, anada i volta, fitxers sense capçalera, alias en català, noms d'arxiu); `tsc`, Vitest (114), `cargo check --lib` i `cargo test --lib` en verd.

## 0.71.13-alpha (2026-11-20)

**Portada, wiki i release multiplaforma: tot al dia**

- *README nou* (`docs/hero.svg`, `docs/architecture.svg`, `docs/asset-bridge.svg`): la portada de GitHub canvia de «llista de versions» a **explicació del producte** — què és NoOrbit, les tres idees que el sostenen, què hi ha dins (agent autònom, Jam, motors, vídeo, veu, extensions) i com instal·lar‑lo. Els diagrames són **SVG fets a mà** (escalables, text llegible i poc pes), no captures.
- *Wiki actualitzada a la 0.71.13*: pàgina nova [[Integració-Godot-i-Bevy]] (detecció, directrius, pont d'actius, obertura del motor, instal·lació opcional) i revisions de Home, Funcionalitats, Arquitectura, Instal·lació, Integració-Blender, Integració-Unreal, Treball-autopilotat, Contribució i IA-en-NoOrbit (que a més es neteja d'un text mal enganxat).
- *Release multiplaataforma*: el workflow de release ara **publica també en un «Run workflow» manual** (abans construïa els artefactes però no publicava res) i hi afegeix les **portes del frontend** (typecheck + lint + tests) abans de construir els instal·ladors de macOS, Windows i Linux.
- *Git*: el clon local de la wiki deixa de ser un *gitlink* trencat al repo pare (ara és una carpeta ignorada amb instruccions de clonatge) i la wiki autogenerada de Qoder (`.qoder/`) s'ignora: és sortida de l'eina, no codi.
- *Instal·lació local*: `pnpm app:build:mac:intel` genera el DMG de la versió nova, que s'instal·la a `/Applications`.

**Godot 4 i Bevy: motors de primera classe — i el PONT D'ASSETS (Blender → motor, automàtic)**

Godot i Bevy ja no són «modes»: es detecten, s'instal·len, s'editen i la IA els pilota com a Blender i Unreal. I, si la IA crea un model en Blender, el pot deixar dins el joc sense que l'usuari faci res: el que exporta a `EngineWork/Exports/` s'importa al motor detectat quan la IA ho decideix.

- *directrius `@godot:` i `@bevy:`* (`engine_tools.rs`): `@godot: import | run [escena] | script res://fitxer.gd | version` executa el motor DE VERITAT en headless (importa els assets, arrenca l'escena i torna els errors de GDScript); `@bevy: check | build | run | test | clippy` compila i executa amb `cargo`. El resultat de cada ordre torna al model, que hi pot corregir.
- *detecció de projecte* (`godot/mod.rs`, `bevy/mod.rs`): un `project.godot` (a l'arrel o un nivell) i un `Cargo.toml` amb la dependència `bevy` es detecten automàticament; el binari de Godot es busca a `/Applications`, Homebrew i al PATH, i el `cargo` a `~/.cargo/bin` (les apps llançades des del Finder no tenen el PATH de la shell).
- *PONT D'ASSETS — `@import:`* (`engine_bridge.rs`): la IA exporta el model des de Blender a `EngineWork/Exports/` i emet `@import: model.glb` (opcionalment `@import: godot model.glb` per forçar el destí). A **Godot** l'asset es copia a `assets/`, es crea l'escena `scenes/<nom>.tscn` que l'instancia i es reimporta en headless; a **Bevy** es copia a `assets/` (on el codi Rust el llegeix amb `asset_server.load`); a **Unreal** s'importa de debò via la Remote Control API (que verifica que l'asset s'ha creat). **No s'importa res automàticament: el moment d'importar el decideix la IA.**
- *rutes i errors clars*: la ruta pot ser absoluta, relativa al projecte, dins de `EngineWork/Exports/` o només el nom del fitxer; sense motor detectat o sense asset, el missatge diu exactament què fer.
- *obrir el projecte al motor* (`engine_open.rs`): el botó «Obre al motor» del panell Jam obre l'editor de Godot, el Terminal amb `cargo run` de Bevy o l'editor d'Unreal (amb els plugins del pont activats abans de l'arrencada).
- *instal·lació opcional per plataforma* (`commands/system.rs` + `SetupCheckModal.tsx`): Godot 4 i Rust (per a Bevy) són passos OPTIONALS de l'assistent, com Unreal: un clic obre la descàrrega oficial ja filtrada pel sistema operatiu (macOS / Windows / Linux) i NoOrbit els detecta sol quan l'usuari els instal·la.
- *Bevy a la Jam* (`JamPanel.tsx`, `GameStudiosModal.tsx`): «🦀 Bevy (Rust)» és un motor triable, i el planner el coneix com a «Rust + ECS» (no com a «bevy» a secas) perquè el model escrigui codi compatible.
- *`target/` protegit* (`jam/commands.rs`): el directori de compilació de Rust s'exclou a `.git/info/exclude`; sense això el `git add -A` de la Jam el commitejava (centenes de MB) i el `git clean -fd` esborraria la compilació sencera entre tasques.
- *proves*: 12 tests nous (importació `.glb` → `assets/` + escena `.tscn`, asset pla sense escena, còpia a `assets/` de Bevy, error d'asset inexistent, directriu `@import:` en una línia / amb motor / tancant un bloc de codi, error clar sense motor, detecció de motor a `engine_open`); `cargo test --lib` (248), `pnpm typecheck`, Vitest (106) i ESLint nets.

**Si el programa peta, hi ha un INFORME per enviar — i es copia d'un clic**

Abans, una caiguda de NoOrbit era un tancament sense cap rastre útil: el procés moria i l'usuari es quedava sense saber què havia passat ni on buscar-ho. Ara cada caiguda deixa paper i hi ha un informe llest per copiar i enviar.

- *registre de caigudes* (`src/crash.rs`): el hook de panic escriu a `crash.log` (dins de la carpeta de dades de NoOrbit) amb l'hora, el missatge, el lloc exacte (`fitxer:línia:columna`) i la pila de la crida. Cada arrencada deixa una marca de sessió, de manera que es distingeix el que ha passat ara del que va passar en sessions anteriors; el registre es retalla sol quan creix massa.
- *informe de fallida* (`Ajuda ▸ Informe de fallida…`): un diàleg amb la versió, el sistema, quantes caigudes hi ha hagut (aquesta sessió i les anteriors) i el text complet per copiar, desar com a fitxer o obrir el registre al Finder. El botó «Obre GitHub» COPIA l'informe i obre la pàgina d'incidents del projecte (només cal prémer ⌘V). Si l'última sessió va acabar amb una caiguda recent, el diàleg **s'obre sol** en arrencar.
- *copiar des de la caiguda* (`main.tsx`, `PanelErrorBoundary.tsx`, `DebugConsole.tsx`): quan la interfície peta, el botó «Copiar informe de fallida» deixa al porta-retalls l'informe sencer (backend + interfície + darrers errors de la consola) perquè l'enviï a l'autor sense buscar res pel sistema. El botó nou de la consola de depuració fa el mateix amb els errors que s'hi acumulen.
- *proves*: 6 tests nous a `crash.rs` (el registre es talla en blocs amb sessió, el backtrace es llegeix del fitxer, l'informe explica la caiguda i on és el registre, un registre buit igual genera informe, un registre sense marca de sessió compta com a caiguda anterior i el registre es retalla quan creix massa); `cargo test --lib` (254), `pnpm typecheck`, Vitest (106), ESLint i `vite build` nets.

**La Jam diu QUINA IA hi treballa — i «Atura» ATURA de debò**

El panell Jam de l'agent autònom no deia amb quina IA treballava, i prémer «Atura» durant la fase de pla no feia res: la Jam es quedava a «preparant el pla…» (amb un model local, fins a una hora) i el botó semblava mort.

- *IA en servei a la vista* (`AiManager::active_labels` + `JamState.provider/model`): el capçal del panell mostra el nom real del proveïdor i el model («OpenAI · gpt-4o-mini», «Ollama · llama3», «IA nativa de NoOrbit») i és EL MATEIX que rep el procés fill. Si Ollama no és en marxa hi ha la mateixa caiguda que al xat: el primer proveïdor en línia amb clau i, si no n'hi ha cap, la IA integrada de NoOrbit.
- *«Atura» atura* (`jam/commands.rs`): la fase de pla és CANCEL·LABLE (el token es mira de debò) i té un sostre de temps per damunt del pressupost del planner (perquè els seus reintents no es perdin); en aturar s'emet l'estat a l'instant, el botó passa a «Aturant…» i l'agent fill ni tan sols arrenca si l'aturada ja s'ha demanat.
- *una sola Jam a la vegada* (`jam_start`): la Jam es reserva amb un compare-and-swap ABANS de qualsevol espera, així que dos «Comença» (o dues finestres) no poden colar-se: abans hi havia una finestra de segons en què la segona Jam pisava l'estat i el token de la primera i les dues escrivien al mateix repo.
- *proves*: 6 tests nous a `jam/tests.rs` (aturar pendant el pla, el token cancel·lat amb l'estat «aturant…», aturar sense cap Jam en marxa, la serialització de proveïdor/model/stopping, la reserva que impedeix dos «Comença» i el sostre del pla per damunt del pressupost del planner); `cargo test --lib` (248), `pnpm typecheck`, Vitest (106) i ESLint nets.

**Unreal Engine en un DISC EXTERN: ara es detecta sol — i es pot triar a mà**

Quan l'Unreal no viu a la carpeta habitual, NoOrbit deia «no s'ha detectat cap instal·lació» i no hi havia manera de fer res. Ara la cerca és a TOT el sistema —inclosos els discs externs, els NAS i les unitats de xarxa— i, si el motor viu en un lloc que la màquina no veu, es pot triar la carpeta a mà.

- *detecció a tot el sistema* (`unreal/launcher.rs`): es busquen motors a `/Applications`, `/Users/Shared`, a **totes** les unitats montades (`/Volumes/*` al macOS, totes les lletres d'unitat a Windows, `/mnt`, `/media` i `/run/media` a Linux) i a la carpeta de l'usuari. Abans només es miraven tres rutes fixes i un motor en un disc extern era invisible.
- *contenidor renomenat*: es reconeix el motor sigui quin sigui el nom de la carpeta («epic games toshi», «Jocs»…) i s'accepten els layouts d'Epic Games, de Windows (`Jocs\Epic Games`) i de Steam (`steamapps/common/Unreal Engine`).
- *filtratge real*: només compta com a motor si té `Engine/Binaries` o `Engine/Build`; s'exclouen els bundles `.app` (el launcher d'Epic Games en porta un «UE» dins que es confonia amb un motor) i les carpetes buides o encara a mitges.
- *motor triat a mà* (`unreal_auto_connect` amb `engine_root`): el panell d'Unreal té un selector «Motor d'Unreal» amb les instal·lacions detectades i un botó per triar la carpeta a mà (unitat no montada, NAS amb permisos, lloc rar). La carpeta triada té prioritat sobre la detecció automàtica.
- *instal·lacions a mitges* (`unreal_detect_pending_installations`): si l'Epic Games Launcher encara està baixant el motor, es mostra com a tal i amb la seva ruta, en comptes de dir que no s'ha trobat res. Les rutes detectades es poden llegir senceres al panell.
- *proves*: 12 tests nous de detecció (disc extern, contenidor renomenat, Steam, carpetes buides, versions ordenades de la més nova a la més vella, sense duplicats, launcher d'Epic exclòs, instal·lació a mitges); `cargo test --lib` (227), `pnpm typecheck`, Vitest (106) i ESLint nets.

**L'IA local (Ollama) ja treballa DINS d'Unreal: tres fases, del setup a l'eina invocada**

El «Agent MCP» del panell d'Unreal era un tot-u que es quedava penjat si l'editor trigava a arrencar i que no deixava veure què passava. Ara és un flux de tres fases independents: primer es PREPARA l'entorn, després es DESCUBREIX quines eines hi ha i finalment l'agent EXECUTA la tasca pintant cada pas en directe.

- *FASE 1 · SETUP — un sol clic* (`UnrealSetupButton` → `unreal_find_uproject` + `unreal_agent_setup`): busca el `.uproject`, comprova que hi ha Ollama en marxa, activa els plugins «ModelContextProtocol» i «AllToolsets», arrenca el servidor MCP al port 8000 i fa la sonda. Els cinc passos apareixen en directe amb el percentatge, el pas «encara treballant» si el servidor tarda i el resum final amb el nombre d'eines detectades.
- *FASE 2 · DESCOBRIMENT — el catàleg d'eines* (`UnrealToolsDiscovery` → `unreal_mcp_tools` + `unreal_mcp_call`): les eines que ofereix l'editor, agrupades per prefixe, amb cerca (nom i descripció) que no bloqueja el render, insígnia «modificadora» a les que poden tocar l'escena i un formulari de JSON per cridar-ne una a la mà amb l'argument validat i el resultat retallat.
- *FASE 3 · EXECUCIÓ — l'agent fa la feina* (`UnrealAgentChat` → `unreal_agent_task`): s'escriu què es vol en català i l'agent cerca les eines, les invoca i respon; el xat mostra una cronologia en directe amb el model, la ronda, cada eina (amb el resultat o l'error) i la resposta final, més un cronòmetre mentre treballa.
- *esdeveniments rics* (`unreal/agent.rs`, `unreal://agent-task`): el backend emet `{phase, text, tool?, turn?}` amb les fases `model`, `ronda`, `eina`, `eina_ok`, `eina_err`, `answer` i `error` — el panell ja no rep una parella text/nivell sinó què va passar, on i en quina ronda.
- *xarxes de seguretat*: timeout de 6 min al setup i **bloqueig de reintent de 60 s** (un altre intent obriria un SEGON editor d'Unreal, dos ports MCP i el doble de RAM), límit de 100 KB als arguments de les eines, retallat a 20 KB dels resultats, validació del nom de l'eina contra el catàleg carregat i cerca diferida (`useDeferredValue`) per als catàlegs de centenars d'eines.
- *proves*: 16 tests nous a `utils/unrealAgent.test.ts` (fases, eines modificadores, cerca, agrupació, truncat, validació d'arguments); Vitest (106 tests, 9 fitxers), `pnpm typecheck`, ESLint i `cargo test --lib` (207) nets.

**Menú «Vídeo» nou: la IA munta, es critica i renderitza (fins a 40 min de treball seguit)**

Blender (o qualsevol carpeta del projecte) pot treure'n un vídeo RODAT sense obrir cap editor a mà. NoOrbit no afegeix un editor nou: aprofita l'`ffmpeg`/`melt` que ja ve dins el `.app` de Kdenlive i la IA treballa sobre un EDL en JSON —l'única font de veritat— que després genera també un `.kdenlive` editable per si un humà el vol poleix.

- *menú superior independent* («Vídeo», entre «IA» i «Zip»): panell propi amb 4 pestanyes (Inventari, Muntatge, Render, Exportar), pressupost de temps triable de 10 min a 24 h amb la xifra **40 min per defecte**, aturada en sec i progrés en directe. Cada entrada del menú (`video.panel|scan|director|auto|preview|render|export|kdenlive`) crida EL MATEIX motor que fa servir el xat.
- *director amb autocrítica* (`video/director.rs`): mira els plans (durada, mida, moviment, negres, silenci), escriu el guió, es rellegeix i corregeix, munta, renderitza i VERIFICA el resultat (durada, LUFS, plans morts, trams congelats); si el model falla o no hi és, no mor — cau a un muntatge determinista i ho diu. Tot dins el mateix pressupost de temps.
- *directriu `@video:*` al xat* (`video_directive`): `status`, `scan`, `mount <json>`, `auto <segons>`, `preview`, `render`, `export`, `verify`, `open <ruta>` i `director {brief,budgetS}`. El xat també detecta peticions en llenguatge natural («munta'm un vídeo de 40 segons», «fes un muntatge amb això») i les enllaça al motor ABANS de preguntar al model, perquè mai no quedin instruccions mortes al prompt.
- *render autoritzat amb `ffmpeg`* (`video/render.rs`): `xfade`/`acrossfade` per a fusions reals mesurables (50 % de cada pla al mig del fús), `concat` per a tall sec, `loudnorm` en dues passades cap a −16 LUFS, subtítols/títols amb `subtitles`+libass (aquest build no porta `drawtext`) i `anullsrc` per a plans sense so, perquè la línia de temps no es desencaixi. El `.kdenlive` (MLT) s'escriu només per a l'edició humana.
- *degradació honesta*: les eines es detecten al primer cop (variables `NOORBIT_*`, `PATH`, `.app` de Kdenlive, Homebrew, Flatpak, Windows) i el que no hi és NO trenca res — p. ex. el `ffmpeg` del paquet de Kdenlive no porta `vidstab`, així que l'estabilització s'omet i la nota del pla (`graf: … estabilització omesa`) apareix al registre del director i al panell.
- *proves*: `cargo test --lib` (205) amb l'aritmètica del muntatge (solapaments, velocitats, durades, LUFS, ASS) i una prova real d'extrem a extrem `#[ignore]` que genera 3 vídeos (un vertical i un sense àudio), els escaneja, els munta, renderitza, verifica durada/sonoritat/negres, escriu el `.kdenlive`, fa la previsualització, degrada l'estabilització i comprova que un render cancel·lat no deixa fitxer a mitges: `cargo test --lib video::render::tests::muntatge_real -- --ignored`. Al front, `pnpm typecheck` i `vite build` nets.

**Ordres al xat per VEU: un botó de micròfon al composer**

Fins ara, per demanar una cosa a la IA calia escriure-la. Ara hi ha un botó de micròfon al costat del clip: es parla, es torna a prémer (o s'espera que s'ature sol als 2 minuts) i el que s'ha dit apareix ESCRIT al composer, a punt de revisar i enviar amb ⏎. No envia res sol: una paraula mal reconeguda sempre es pot corregir abans.

- *tres motors, el primer que estiga llest* (`voice/`): **macOS Speech** (`SFSpeechRecognizer`, sense clau; al dispositiu si el sistema té el model de la llengua i, si no, amb el servei d'Apple), **whisper.cpp local** (`whisper-cli` + un `ggml-*.bin`, cent per cent fora de línia) i qualsevol **proveïdor compatible OpenAI** amb la clau ja configurada (`POST {base}/audio/transcriptions`, p. ex. OpenAI o Groq). Si cap està llest, el missatge diu QUÈ falta, no un error genèric.
- *la gravació no queda enlloc*: el WebView grava amb `MediaRecorder` (mp4/webm/ogg), l'àudio viatja en base64 a la comanda `voice_transcribe`, es desa un instant en un fitxer temporal dins de NoOrbitData i s'esborra abans de tornar el text. `Esc` cancel·la el dictat sense transcriure res i el micròfon es tanca sempre (també en canviar de pestanya).
- *idioma correcte*: el dictat agafa l'idioma de la interfície (català → `ca-ES`) i els motors el reben com a codi (`ca`); el botó mostra el cronòmetre mentre grava i el motor triat al tooltip.
- *permisos de macOS*: `Info.plist` amb `NSMicrophoneUsageDescription` i `NSSpeechRecognitionUsageDescription`, incrustat TAMBÉ al binari des de `build.rs` perquè `tauri dev` no mora en obrir el micròfon.
- *proves*: `voice` porta tests d'extensió segons MIME, codi d'idioma i tria del model d'àudio; `cargo check` net.

**ECC ve DE SÈRIE: 328 skills, 72 especialistes i regles dins l'instal·lador — i la IA nativa els fa servir**

La compatibilitat amb [ECC](https://github.com/affaan-m/ECC) (MIT) ja no depèn de la xarxa: el coneixement viatja amb l'instal·lador i es sembra sol al primer arrencada.

- *vendoritzat al bundle* (`src-tauri/ecc-builtin/`, recurs de Tauri): les 328 skills (amb `references/` i `scripts/`), els 72 agents i les regles comunes, amb LICENSE i ORIGEN.md. Només el coneixement: els hooks, MCP i AgentShield d'ECC NO s'executen (són d'altres harnessos).
- *sembra a l'arrencada* (`seed_ecc_builtin`): sense xarxa i idempotent — skills a `ecc/`, regles a `ecc-rules/`, agents com a Especialistes «ECC · …» deduplicats pel nom. Reinstal·lar o prémer el botó actualitza sense duplicar.
- *ECC natiu* (`ai/ecc_native.rs`): el motor simbòlic de la IA nativa consulta les skills ECC instal·lades quan la petició hi encaixa (cerca per paraules clau sobre nom i descripció, i torna el contingut del SKILL.md) — sense Ollama, sense token, sense baixar res.
- *proves*: tests de sembra/importació i de consulta nativa; `cargo test --lib` (175), Vitest (90), `pnpm typecheck` i ESLint nets.

**La IA nativa dissenya en Blender i programa jocs Unreal lleugers — sense baixar cap model**

- *6 plantilles noves compilades al binari* (`ai/codegen.rs`): escena Blender (bpy, EEVEE amb fallback de versió, terra + figura + material + llum + càmera, desa .blend i renderitza PNG), animació per keyframes (òrbita), material procedural (metall + soroll), exportació glTF per a motors de joc (Godot/Unreal), projecte Unreal lleuger (`.uproject` amb PythonScriptPlugin + RemoteControl i `DefaultEngine.ini` sense Lumen/Nanite/ombres virtuals) i script Unreal Python (EditorActorSubsystem, sense compilar C++). «fes una escena en blender», «fes un joc en unreal» o «exporta de blender a godot» creen fitxers REALS al projecte via `@file:` + materialització, sense Ollama ni GPU.
- *recepta Soup exportable* (`soup_export_recipe`, botó al gestor de models): exporta `soup.yaml` + dataset alpaca generat del coneixement de sèrie (plantilles, receptes bpy, regles ECC) per afinar Qwen amb [Soup](https://github.com/MakazhanAlpamys/Soup) en una màquina AMB GPU (CUDA/MPS) i tornar el GGUF per Hugging Bay. En aquest Mac Intel no s'entrena: la recepta ho diu honestament.
- *proves*: plantilles verificades (marcadors `{nom}`/`{Nom}` omplerts, codi no buit, puntuació sense segrestar Godot); `cargo test --lib` (175) en verd.

**Les preguntes sobre el projecte ja no disparen plantilles: la IA respon el que demanes, no crea fitxers**

Abans, en preguntar «què fa aquest projecte» la nativa podia contestar amb la plantilla d'una escena Blender i crear fitxers pel motiu més absurd: el context del projecte que el xat anteposa a cada missatge (README, noms de carpetes) conté paraules com «blender», «render» o «NoOrbit», i una arrel d'intenció («fa») encaixava amb la primera paraula de «què fa…». Ara el motor extreu NOMÉS la pregunta real (després de «PREGUNTA / TASCA:») per a les matemàtiques, les plantilles, les skills i l'informe après, i la detecció d'intenció exigeix un verb inequívoc («crea», «fes»…) o una petició educada («com puc fer…») quan el missatge comença com una pregunta. Les ordres reals («fes una escena en blender») segueixen funcionant igual.
- *proves*: dos tests de regressió (`preguntes_no_disparen_plantilles` a codegen i `el_context_no_segresta_la_pregunta` a native); `cargo test --lib` (207) en verd.

**L'IA ara REALMENT treballa amb agents: veu l'equip, delega i els especialistes també es delegen**

La IA podia crear especialistes amb «@agent:», però gairebé mai treballava amb ells: les instruccions li deien «delega amb el nom exacte» però el model NO havia vist mai la llista de noms reals, qualsevol diferència mínima al nom («el Tester» vs «Tester de QA») matava la delegació en silenci, i quan un especialista parlava pel xat no tenia accés a res del sistema d'agents.

- *llista real d'especialistes al prompt*: abans de cada torn, NoOrbit carrega els especialistes del disc i injjecta DAMUNT del system prompt el bloc «ESPECIALISTES DISPONIBLES» amb els noms exactes i els rols, just amb l'ordre de delegar-hi les subtasques amb «@use:» — sense dades no hi ha delegació possible. S'aplica al xat normal, al mode ràpid i a la consolidació.
- *delegació tolerant al nom*: nova funció `matchExpert` — ignora majúscules, accents, articles, cometes i puntuació, accepta noms parcials i emparella per sobreposició de paraules. Si encara així no troba l'agent, el missatge d'error ara LLISTA els especialistes que hi ha perquè el model encerti-hi al següent intent.
- *els especialistes també treballen en equip*: quan un especialista és qui parla pel xat, ara rep la llista dels seus COMPANYS + les instruccions de creació/delegació al seu system prompt (nou paràmetre `system_extra` a `expert_run`): pot delegar subtasques a altres agents i crear-ne de nous. Els agents delegats també veuen els seus companys: la feina encadena com una xarxa d'experts.
- *proves*: Vitest (16 tests de directrius, amb 4 nous de `matchExpert`), `pnpm typecheck` i `cargo check` nets.

**Instal·lar Ollama, Blender i Unreal amb UN SOL CLIC, ja segons el sistema operatiu**

- *assistent de l'entorn ampliat*: la finestra «Posant NoOrbit en marxa» ara té 6 passos: a més d'Ollama, servei, model i ComfyUI, Blender i Unreal Engine tenen el seu botó «Instal·la».
- *Blender en un clic*: amb Homebrew (macOS) l'instal·la sol des del Terminal; sense brew o a Linux/Windows obre la descàrrega oficial del teu SO.
- *Unreal en un clic*: nou pas (opcional, no obre la finestra ell sol) que obre directament la pàgina de descàrrega correcta: Epic Games Launcher a macOS/Windows, documentació oficial d'instal·lació a Linux. Detecta les instal·lacions existents i les marca com a fetes.
- *Ollama ja segons el SO*: nou comandament `open_tool_download` que resol la pàgina per SO (ollama.com/download/mac \| /windows \| /linux) per quan la instal·lació automàtica no és possible.

**Els skills cercats a GitHub per fi S'INSTAL·LEN: l'URL de clonatge només portava l'usuari**

El cercador de skills del panell «Skills i connectors» trobava repositoris en viu, però en prémer «Instal·la» no passava res o només sortia un «No s'ha pogut instal·lar el skill» inútil. Cap skill extern s'havia pogut instal·lar mai des d'aquesta via.

- *URL de clonatge correcta* (`commands/skills_store.rs`): es construïa `https://github.com/<usuari>.git` — sense el nom del repositori — així que `git clone` sempre deia «Not Found» (comprovat: `github.com/anthropics.git` no existeix). Nova funció `parse_github_ref` que desxifra `usuari/repo[/subcarpeta...]@branca` i munta l'URL completa; l'usen els DOS camins (instal·lació i previsualització), que tenien el mateix bug duplicat.
- *subcarpeta on viu el SKILL.md*: abans el que venia després de l'usuari es prenia com a subcarpeta, de manera que el nom del repo es perdia i es buscava `<clon>/<repo>` dins el propi clonatge. Ara owner/repo van a l'URL i la resta és la subcarpeta real; el nom de la skill és l'última component (o el repo).
- *validar abans de copiar*: si el repositori no conté cap SKILL.md, ara dona un error concret («El repositori X no conté cap SKILL.md. Indica la subcarpeta que en té: usuari/repo/carpeta») en lloc de deixar una carpeta buida al disc. Segments amb `..`, `/` o que comencen per `-` es rebutgen (no poden alterar la comanda `git` ni escapar de la carpeta temporal).
- *skills aninades detectades*: `scan_skills` explorava fins a profunditat 3, insuficient per a un repo copiat sencer (`<skills>/skills/pdf/SKILL.md` n'estava a 4) — pujada a 4, i ni `.git` ni `node_modules`/`target`/`dist` s'exploren. La còpia del clonatge tampoc arrossega `.git`.
- *l'error real a la pantalla*: `skillStore` llençava el text del backend i només guardava el codi de la font; nou camp `lastErrorDetail` que ensenya exactament què ha fallat (repo inexistent, sense connexió, sense SKILL.md). Textos del panell actualitzats amb el format `usuari/repo/carpeta`.
- *proves*: test nou `una_font_github_desxifra_usuari_repo_i_carpeta` (URL, subcarpeta, branca i formats invàlids), `cargo test --lib skills` (4), Vitest (90) i `pnpm typecheck` nets; clonatge real de `anthropics/skills` per confirmar on són els SKILL.md.

**Claude (Anthropic) i DeepSeek als apartats d'APIs en línia: ara funcionen DE VERITAT amb tot**

Posar la clau de Claude no bastava: NoOrbit li enviava `Authorization: Bearer` (Anthropic la rebutja), un `max_tokens` fix de 8192 i un `thinking` amb pressupost que els models nous contesten amb **400**. A més, els presets duien noms de model antics (`claude-3-5-sonnet`, `deepseek-chat`) i la base de DeepSeek portava un `/v1` que ja no existeix. El resultat era «la clau no va» sempre, tant al xat com als agents, la Jam o el treball autopilotat.

- *format Anthropic correcte* (`api/mod.rs`): peticions reescrites sobre `POST {base}/v1/messages` amb capçalera `x-api-key` i `anthropic-version: 2023-06-01`, i detecció automàtica pel nom de la URL (`anthropic.com` o `/anthropic` al final) — encara que el «Tipus» del proveïdor digui una altra cosa. El raonament es tria segons el model: `adaptive` als 4.6/4.7/4.8 i 5.x, `enabled` + pressupost als més vells, i res si el model no l'admet; els sostres de sortida passen a 64k/32k/16k per generació en lloc dels 8192 antics.
- *reintentar amb seny*: un sol camí (`build_request` + `open_chat_response`) per al xat normal i per al streaming, amb degradació progressiva quan el servidor respon 400 per format (primer treu `thinking`/`reasoning_effort`, després abaixa `max_tokens`) i alternança de ruta en un 404 (amb i sense `/v1`). Els 400 que no són de format (model inexistent, quota, clau dolenta) es deixen passar amb el missatge real del servei.
- *imatges amb els proveïdors remots*: la visió ja no és cosa local. Les imatges del xat van en base64 (png/jpg/gif/webp fins a 4 MB) com a blocs `image` a Anthropic o `image_url` a OpenAI-compatible, així que capturar la pantalla i preguntar què hi ha funciona igual amb Claude, DeepSeek, Ollama Cloud o Venice.
- *avis de tall*: quan la resposta s'atura al límit de tokens (`stop_reason: max_tokens` / `finish_reason: length`), el visor del procés ho diu en lloc de deixar un codi tallat en silenci. Mai s'afegeix al text de la resposta, perquè les directrius `@file:`/`NB|`/`RUN|` de l'agent es trencarien.
- *presets i models vàlids*: el preset «Claude (Anthropic)» ve creat per defecte (base `/v1`, `x-api-key`, model actual) i el de DeepSeek usa `https://api.deepseek.com` amb `deepseek-flash`; nou preset «DeepSeek (format Claude)» per a l'entrada `…/anthropic` del mateix token. El camp de model té llista desplegable amb els noms que el servei reconeix avui (també a WordPress ▸ IA) i les configuracions velles es migren soles en obrir.
- *autocompletat de l'editor en mode curt* (`chat_quick`): les peticions de continuació de codi van SENSE raonament i amb 1.024 tokens de marge, perquè Claude contesti dins del temps límit de l'editor (amb el `thinking` per defecte se'n menjava els 8 segons i el suggeriment no arribava mai, cosa que semblava «no funciona amb l'editor»). El mateix mode fa servir el botó de prova dels proveïdors («Respon només: OK»). L'editor ara espera 8,5 s en lloc de 6 s perquè no sigui ell qui talli el remot abans que el backend; i cap reintent del `degrade_request` apuja ja el sostre de sortida, només el rebaixa.
- *context i mesures*: `context_window_for` dona 1M al Claude 4.6+ i a DeepSeek (abans 200k), de manera que el medidor de context i la compactació deixen de tallar converses que cabien.
- *diagnòstic «Comprova»*: si el servei no publica `/models` (com fa Anthropic), es fa una petició mínima real a `/v1/messages` per saber si la clau va bé; si el model no és al servidor, el missatge enumera els noms vàlids d'aquell servei.
- *catàleg d'IAs i manual*: Claude i DeepSeek quaden amb el que fan de debò (visió d'entrada, 1M de context, 128k de sortida; DeepSeek només text) i l'apartat 7 del manual i la skill de DeepInfer expliquen els presets nous.
- *proves*: `cargo check --lib` i `pnpm typecheck` nets; comprovat amb peticions reals que Claude 5.x/4.8 responen amb `adaptive` i que un `thinking` clàssic dona 400 (d'aquí ve el reintent degradat).

**La IA local aprèn del que TU aproves: encaix de RAM abans de baixar, vots 👍/👎, dades per a Soup i una porta que desfà el que empitjora**

[Soup](https://github.com/MakazhanAlpamys/Soup) (Apache-2.0) és una CLI de Python per afinar models amb QLoRA des d'un sol YAML. Exigeix CUDA o Apple Silicon (MLX) i Python 3.11-3.13: en un Mac Intel amb 8 GB de RAM i Python 3.9 **no s'hi pot entrenar**, i NoOrbit no l'instal·la ni l'executa. El que sí que es pot aprofitar són les seves quatre idees de disciplina — comprovar el maquinari ABANS, saber quines respostes van bé, no acceptar un aprenentatge que empitjora, i no inventar-se cap número. Aquestes ja funcionen aquí, i les dades es deixen escrites als formats de Soup perquè qui tingui una GPU pugui entrenar-les i tornar el GGUF per Hugging Bay.

- *encaix de maquinari abans de gastar la descàrrega* (`ai/fit.rs`, mòdul nou; idea del `soup doctor`): es compara el pes real del GGUF + la cua de claus del context (8k tokens) contra la RAM total i lliure d'aquest equip, amb un sostre per trams de memòria. Quatre verdicts — «hi cap», «just d'RAM», «NO hi cap», «encaix sense comprovar» — i quan falta una dada es diu QUÈ falta en lloc de fingir un sí. Xip d'encaix a cada fila del catàleg de models i als detalls de Hugging Bay (amb els números i què fer al títol). `ollama_pull_model`, `hf_bay_download` i `hf_bay_install_native` REFUSEN el que no hi cap tret que l'usuari doni un segon clic conscient (`force`); nou comandament `ai_fit_check` per preguntar-ho abans.
- *vots 👍/👎 al xat i al registre d'aprenentatge* (`learning_vote`, component `ChatVote`): el que converteix una conversa en dada. El 👍 val com a prova i com a fila de trenament; el 👎 exclou el torn de les lliçons i del dataset. El vot s'escriu de seguida a `.soup-loops/feedback.jsonl` amb el format de Soup (`kind: thumb`, `value: 1/-1/null`, UUID per resposta, temps ISO 8601 UTC): els registres NOMÉS s'afegeixen i l'última línia per resposta guanya, així que retirar el vot no esborra res. Si el torn no és al registre (aprenentatge desactivat o torn fora del límit) s'avisa: no es pinta un vot que no s'ha desat. La marca es conserva en reobrir el xat.
- *paquet de dades que Soup entén* (`learning_soup_export` → carpeta `.soup-loops/`): `traces/*.jsonl` i `feedback.jsonl` del bucle continu, `train.jsonl` ChatML pur, `preference.jsonl` amb parelles `prompt/chosen/rejected` per a `task: dpo`, `provenance.jsonl` apart (qui va respondre cada fila, sense claus que Soup no entén), `soup.yaml` + `soup-preference.yaml` amb NOMÉS claus documentades (una clau inventada refusa el fitxer sencer), `val_split` que reserva les ÚLTIMES files, i un `LLEGEIX-ME.md` amb les comandes exactes. El panell Aprenentatge en mostra la qualitat, les files, els parells, els vots, els duplicats i la fuita a la validació, i `soup_pot_entrenar_aqui()` comprova DE VERITAT si aquest equip pot entrenar (soup instal·lat + Python 3.11-3.13) en lloc de suposar-ho. Complementa la recepta que ja existia (`soup_export_recipe`, que exporta el coneixement de sèrie): aquesta exporta les DADES de l'usuari.
- *porta d'avaluació i rollback del cervell natiu* (`ai/brain_gate.rs`, mòdul nou; idea de `training.eval_gate`): abans de fusionar coneixement nou — tant el que arriba de fora com el que la nativa aprèn vaguejant — es puntua contra les respostes que l'usuari ja va aprovar (solapament de tokens). Si la nota cau més d'un 5%, la fusió es DESFA i el cervell es queda com estava. Sense proves no hi ha nota: diu «no he pogut avaluar» (`null`), mai un 100% inventat. El veredicte (acceptat o desfet, peces, proves, nota abans/després i motiu) es veu al panell Aprenentatge (`learning_brain_verdict`), a la nota del vagueig i a l'esdeveniment `native://learned`.
- *cap número inventat*: el que no s'ha mesurat és `null`, no un zero; el que no es pot comprovar es DIU que no s'ha pogut comprovar; i els fitxers de registre mai es reescriuen. És el criteri de Soup aplicat a l'aprenentatge local.
- *proves*: 20 tests nous — 8 de l'encaix (`ai/fit.rs`), 6 de la porta (`ai/brain_gate.rs`) i 6 del paquet de dades i els vots (`learning/mod.rs`) — tots en verd a `cargo test --lib` (190 tests en verd, cap warning); `pnpm typecheck`, `vite build`, ESLint i Vitest (90) nets.

*Aquest bloc es va preparar per a una 0.71.12 que mai es va publicar com a release: el primer binari que el conté és el de la 0.71.13.*

**La Jam i el treball autopilotat ara dirigeixen Unreal i Blender: la IA decideix sola quin motor fa servir**

Fins ara la feina autònoma només sabia escriure fitxers. Ara, quan el projecte és d'Unreal (té un `.uproject`) o hi ha Blender a l'abast, cada tasca pot dirigir els motors reals: crear actors, escenes i blueprints a Unreal, modelar i animar a Blender, renderitzar vídeos i importar-ho tot al projecte — amb la IA triant sola quin motor li convé a cada pas i encadenant-los (Blender crea i exporta, Unreal importa).

- *directrius de motor al text del model* (`engine_tools.rs`, mòdul nou): «@unreal: {"tool":…,"args":{…}}» (eina MCP d'UE 5.8+), «@unreal-py: <codi>» (Python dins l'editor via Remote Control, funciona des d'UE 5.0), «@blender: <codi bpy>» (Blender connectat), «@blender-headless: <codi>» (Blender en segon pla: exports FBX/OBJ/glTF, renders) i «@blender-video: <ruta .blend>» (render de vídeo automàtic). Cada directriu s'executa DE VERITAT i el resultat torna al model, que pot corregir-se o demanar-ne més (fins a 4 rondes per tasca).
- *el vídeo es renderitza sol*: si el model anima una escena i imprimeix «NOORBIT_VIDEO_OK:», NoOrbit llança el render en segon pla (temps límit, sense penjar el bucle) i deixa el .mp4 a `EngineWork/Exports/` del projecte — llest perquè Unreal l'importi.
- *artefactes visibles i commitejables*: cada operació queda registrada a `EngineWork/noorbit-engine.json` dins el projecte; el verificador de la Jam accepta tasques que són només de motor (sense fitxers de codi) perquè la feina feta queda al repo.
- *autodetecció a cada tasca i passada*: el `.uproject` es busca a l'arrel o un nivell, i Blender pel pont obert (port configurat) o el binari instal·lat; es detecta a cada tasca, així que obrir Blender/Unreal a mitja feina activa les eines sense reiniciar res. Els errors de motor tornen al model amb missatges que l'ajuden (llista d'eines disponibles, alternatiu «@unreal-py:»).
- *la Jam i el treball autopilotat ho comparteixen*: el worker de la Jam (subprocés) i cada passada del treball autopilotat (`project_work`) fan el mateix bucle d'eines; el panell Jam mostra cada execució al registre («executant l'eina de motor…») i el treball autopilotat l'anuncia a la línia d'activitat.
- *proves*: `cargo test --lib` (148, amb 11 tests nous de parseig de directrius i autodetecció), `cargo check --all-targets`, `pnpm typecheck`, Vitest (86), ESLint i `vite build` nets.

**Agent Unreal amb MCP: un sol clic prepara tot l'entorn i la IA local treballa DINS de l'editor**

Fins ara connectar una IA local a Unreal Engine demanava un manual: instal·lar Ollama, activar plugins a mà i arrencar un servidor. Ara la pestanya **Unreal ▸ Agent MCP** ho automatitza tot amb un botó.

- *botó «Activar Agent Unreal»*: comprova i prepara l'entorn en 5 passos idempotents (es pot prémer tantes vegades com es vulgui) amb el registre de cada pas a la interfície: (1) Ollama en marxa (l'engega si cal), (2) plugins **ModelContextProtocol** i **AllToolsets** activats al `.uproject` (JSON, amb còpia de seguretat; avisa si cal reiniciar l'editor), (3) arrencada automàtica del servidor MCP escrita a `EditorPerProjectUserSettings.ini`, (4) servidor comprovat al port 8000 — amb l'opció «Obre Unreal si cal», que llança l'editor amb `-ModelContextProtocolStartServer` i espera fins a 5 minuts, i (5) connexió real amb recompte de les eines MCP disponibles.
- *client MCP Streamable HTTP* (`unreal/mcp.rs`): handshake `initialize` + `notifications/initialized`, descobriment amb `tools/list` i crides amb `tools/call`, amb sessió (`mcp-session-id`) i respostes SSE — els pings del servidor no es confonen amb respostes.
- *bucle d'agent amb «capability search»*: la tasca no envia TOT el catàleg d'eines al model (els models locals petits s'hi queden en bucle); primer tria les **5 eines més rellevants** per paraules clau i només aquestes van al prompt. Fins a **8 rondes** de raonament; cada `tool_call` s'executa de veritat contra Unreal i el resultat torna al model, amb validació estricta (una ronda sense crida a eina no es dona per feta).
- *proves*: `cargo test --lib` (127, amb 6 tests del parser SSE i 4 del capability search), `cargo check --all-targets`, `pnpm typecheck`, Vitest (86), ESLint i `vite build` nets.

**Compatibilitat amb ECC (affaan-m/ECC): instal·lació d'un clic i skills que la IA realment fa servir**

NoOrbit ara és compatible amb [ECC](https://github.com/affaan-m/ECC) (MIT), el sistema d'optimització per a agents de codi: 328 skills, 72 agents i regles.

- *botó «Instal·la ECC (affaan-m)»* a **IA ▸ Skills i connectors**: clona la font OFICIAL (`github.com/affaan-m/ECC`, depth 1) i importa les skills (`skills/` + `.agents/skills/`, deduplicades) a la carpeta `ecc/`, amb els seus `references/` i `scripts/`. Consentiment previ amb resum del que s'importa.
- *els 72 agents d'ECC esdevenen Especialistes de NoOrbit* (nom «ECC · …», rol i prompt de sistema); es dedueixen pel nom: reinstal·lar ECC actualitza les skills sense duplicar especialistes. Ids d'especialista únics fins i tot en creacions en ràfega (abans dos agents creats el mateix mil·lisegon compartien id).
- *les regles comunes* (`rules/common/`) s'importen com a skills a `ecc-rules/`. Limitació honesta i visible al consentiment: els **hooks, MCP i AgentShield d'ECC NO s'executen** (són d'altres harnessos); només s'importa el coneixement.
- *les skills ara arriben a la IA*: fins ara les skills instal·lades no s'injectaven mai al prompt — estaven al panell però el xat no les veia. Ara cada torn porta un **índex compacte** (nom + descripció + ruta del SKILL.md, sostre de 6.000 caràcters) i la IA llegeix la skill completa quan la tasca hi encaixi («progressive disclosure»). Els models locals petits no porten l'índex (context curt) i els reintents per OOM el descarten.
- *proves*: `cargo test --lib` (127, amb tests nous d'importació ECC i deduplicació), Vitest (86, amb tests de l'índex de skills), `pnpm typecheck` i ESLint nets.

**Jam: el panell ja no sembla «penjat» — cronòmetre i compte enrera des del primer segon**

A l'apartat **Agent ▸ Jam**, mentre preparava el pla la IA no deia res i el panell semblava congelat, a més de no deixar fer res.

- *el pla ja no bloqueja l'arrencada*: `jam_start` esperava que la IA fes el pla (amb models locals pot durar minuts) ABANS de marcar-se «en marxa»: la UI no veia res i «Comença» es podia polsar un altre cop (cua de plans duplicats). Ara l'estat `running` + el cronòmetre s'emeten a l'instant i el pla es fa en segon pla; si falla, la Jam es marca com a fallida amb explicació (error visible al panell + registre).
- *compte enrera real des del primer segon*: mentre no hi ha ritme (cap tasca acabada) el xip «Restant» deia «calculant…» minuts sencers; ara cau en el compte enrera de la durada triada (el sostre) i «Fin estimada» és l'hora prevista de acabament.
- *«Està treballant…»*: la línia d'activitat, que només mostrava «…» fins la primera tasca, ara ho diu amb totes les lletres.
- *el registre no es perd en canviar de pestanya*: la sub-pestanya desmunta el panell i el registre d'esdeveniments, que era dins del component, tornava buit (semblava que «no fa res»). Ara és un buffer compartit de nivell de mòdul (últims 200 esdeveniments) que es reinicia només quan arrenca una Jam nova.
- *proves*: `cargo test --lib` (114), Vitest (81), `pnpm typecheck` i ESLint nets.

**La Jam autònoma ARA sí que escriu el codi al final + no deixa adormir el Mac/PC + pantalla de feina**

La Jam i el treball autopilotat acabaven sovint **sense haver escrit res** al projecte, i mentre treballaven sols el Mac/PC **s'adormia per inactivitat** a mitja generació. Ara la feina autònoma arriba fins al final, no es penja i es pot veure de lluny què està fent.

- *la causa arrel: una cascada de timeouts* incompatible amb models locals en CPU (Macs Intel). El xat remot tallava a **120 s**, Ollama a **240 s**, el worker de la Jam a **5 min**, el planner a **120 s** i cada passada del treball autopilotat a **300 s**: cap tasca acabava → la Jam declarava la tasca «bloquejada» i el seu `reset_hard` **esborrava tot el working tree**. Resultat: la Jam acabava sense haver canviat res.
- *`long_run` (nou flag a `ChatOpts`)*: les crides llargues de la Jam i del treball autopilotat fan servir un client remot amb sostre d'**1 hora** i, a Ollama, el canal **sense timeout** (el límit real el posa el bucle). El xat interactiu i la cerca de proves mantenen els timeouts curts d'abans.
- *`JamConfig::worker_timeout`* (nou, **45 min** per defecte) i `LLM_TIMEOUT` del planner a **20 min**; el timeout per passada del treball autopilotat també puja a **45 min**.
- *segona via d'escriptura `@edit:`*: els models petits no reeixen a reemetre fitxers sencers (els tallen o els inventen); ara, si un `@file:` no apareix, s'accepten les **edicions quirúrgiques CERCA/NOU** (`extract_edit_blocks` + `apply_edit_blocks_at` amb arrel explícita) i la tasca COMPTA com a escrita. Nou `apply_edit_blocks_at` per a el subprocés de la Jam, que no depèn del workspace «corrent».
- *verificador tolerant*: abans exigia que **TOTS** els camins dels criteris d'acceptació existissin; com que el pla barreja fitxers nous amb altres que **ja existeixen**, bloquejava tasques ben fetes. ara n'hi prou que en aparegui **un** (per sufixo), i només falla si no en troba cap.
- *el Mac/PC ja no s'adorm* (`keep_awake.rs`, mòdul nou): mentre la Jam o el treball autopilotat són vius es bloqueja el son del sistema amb el mecanisme natiu de cada SO — macOS `caffeinate -i -s -w <pid>` (mor amb el pare, ni per SIGKILL), Linux `systemd-inhibit`, Windows `SetThreadExecutionState` reafirmat cada 50 s. Patró RAII: l'últim guard en morir allibera el son. Si el mecanisme no hi és, la feina continua igualment.
- *pantalla de feina (estalvi propi)*: botó **«Pantalla de feina»** al panell Jam que obre una finestra a **pantalla completa** (label `noorbit-screensaver`, `screensaver/`) amb el títol, un cronòmetre gegant, la barra de progrés + fi estimat, la tasca actual i **en quin projecte** s'està treballant. Es tanca amb un clic o qualsevol tecla i la feina continua al darrere.
- *es veu el projecte i es pot reobrir la pestanya*: el `JamState` ara porta `project_path`; el panell Jam, la franja «feina en viu» i el rellotge de la barra d'estat mostren el **nom de la carpeta** del projecte (amb botó «Mostra al Finder»). A més, els accessos al panell Jam (rellotge, franja viva, menú i ⌘P, Game Studios) ara **tornen a obrir el panell dret** si era plegat — abans només canviaven de pestanya i, si el panell era tancat, no es podia «tornar a obrir la pestanya».
- *la IA mostra QUÈ està fent i el seu PROCÉS DE PENSAMENT*: abans el worker de la Jam corre en un **subprocés** i el text del model **no arribava mai** a la pantalla — l'usuari només veia el títol de la tasca. Ara el `NoOrbitWorker` rep un `ProgressSink` i emet dos esdeveniments nous (`Progress::Note` i `Progress::Thinking`): **notes** de l'activitat en viu («cercant informació a internet…», «generant codi per «tasca»…») i el **raonament del model** (prosa + línia RESUM, **sense blocs de codi**, retallat a ~1200 caràcters). Viatgen per stdout (`JAM|{kind:"note"/"thinking"}`), el pare les re-emet com a `jam://event` i es mostren **al panell Jam** (registre amb icona de cervell, text multlínia) i **a la pantalla de feina**, que ara porta un **feed en viu amb autoscroll** («Què està fent la IA ara mateix»). La `note` també alimenta el `current_detail`, així que la línia d'activitat del rellotge i de la franja viva canvia en temps real.
- *proves i portes*: **Tot verd** — `cargo check --lib`, `cargo test --lib` (114), crates `noorbit-jam`/`noorbit-planner`, `pnpm typecheck`, Vitest (81) i `vite build`.

**La IA nativa ja PROGRAMA: plantilles de codi, motors de joc, edició de fitxers i generacions llargues**

La IA nativa (el motor integrat al binari, sense descàrregues) deixa de ser només calculadora i conversa: ara **crea fitxers reals al projecte, els edita i els esborra**, i per a les generacions que el seu motor simbòlic no pot fer **encarrega la feina a una IA disponible** i la torna com a resposta pròpia.

- *plantilles de codi cablejades*: el catàleg de plantilles (`ai/codegen.rs`) existia però **no el cridava ningú** (codi mort). Ara «crea un component React», «fes una pàgina HTML», «genera un servidor Python»… responen amb el fitxer en format «@file:» + tanca de codi, que es materialitza al projecte amb la revisió verd/roig de sempre. «fes un backend en golang» (llenguatge reconegut sense plantilla) respon amb el catàleg honest en lloc de la reserva genèrica.
- *motors de joc per a Mac Intel*: 4 plantilles noves de projectes complets multi-fitxer: **Godot 4** (renderer Compatibility, el més equilibrat per a Intel Iris), **Defold** (el més lleuger), **Bevy** (Rust) i **Panda3D** (Python). Cada plantilla emet la configuració del motor MÉS l'escena i el guió d'exemple («fes un joc en godot que s'anomeni Salt» → `project.godot` + `main.tscn` + `main.gd`).
- *edició simbòlica de fitxers* (`ai/edit.rs`, mòdul nou): «esborra src/vell.py» emet «@delete:» (verificant que la ruta existeix al projecte); «canvia "A" per "B" a app.py» emet «@edit:» amb CERCA/NOU, llegint el fitxer REAL i verificant-hi el text a canviar (insensible a majúscules, retorna el text exacte del fitxer); «afegeix X al final de app.py» encaixa la darrera línia real; «crea hola.py amb "..."» (o amb una tanca de codi enganxada) emet «@file:» amb el contingut. Camins de Windows («C:/coses/x.tsx»), carpetes («esborra la carpeta dist») i accents s'analitzen amb seguretat de bytes.
- *generacions llargues per delegació silenciosa*: quan la petició no és ni matemàtiques, ni coneixement, ni plantilla, ni edició («escriu un article de 600 paraules», «fes-me un joc complet de tres en ratlla»), la nativa l'encarrega a una IA open-source JA disponible — el model LOCAL d'Ollama si el motor està en marxa, si no un proveïdor en línia — en una sessió AÏLLADA («native_task») que no toca el xat ni delata cap «teacher»; el system prompt hi viatja sencer, així que el que generi es materialitza com qualsevol resposta. Sense cap IA disponible, respon la reserva honesta. MAI baixa cap model.
- *ordre de la resposta nativa*: millora del cervell (quan es demana) → edició simbòlica → plantilles → cervell/lliçons → xerrameca → delegació de generació llarga → reserva honesta. El cervell «codi» i «ajuda» anuncien les capacitats noves i hi ha una entrada nova «motors de joc».
- *reparacions menors*: el paràmetre «adreces» de `wander_profunda_set` es deia «adreses» (no compilava); el fitxer del component React ara es genera en CamelCase («src/Cart.tsx»), com mana la convenció.
- *proves*: 15 tests nous a `ai/edit.rs` (detecció de tokens i carpetes, segments entre cometes, tanques de codi, cerca insensible amb índexos segurs, esborrat/creació honestos, converses que no són edicions) i tests de motors de joc a `ai/codegen.rs`. **Tot verd**: `cargo test --lib` (111), `cargo check --all-targets`.

**Aprenentatge autònom en repòs: la IA nativa navega sola i aprèn sola**

Mentre el programa no el toca ningú, la IA nativa **surt a Internet, llegeix i n'extreu conceptes**; si li preguntes «què has après?», et respon amb el que ha llegit.

- *vagueig en repòs* (`ai/wander.rs`, mòdul nou): només comença després de **150 s sense activitat**, amb **cap resposta de la IA en curs** (comptador global RAII de torns) i amb **la RAM gairebé tota lliure** (procés <6% i sistema >15% lliure, mesurat amb `sysctl`/`vm_stat` a macOS i `/proc` a Linux). Sessions de **5 pàgines** amb **10 minuts de refrigeri**; el diari es desa a `native_wander.json` amb escriptura atòmica.
- *ordre de les fonts, tot LLIURE*: **StackOverflow** (API oficial amb llicència CC BY-SA) → **Viquipèdia en català** (extractes) → **cercadors lliures** per buscar **els seus propis dubtes** → **Internet profunda** (adreces .onion configurables) només si hi ha un **proxy local** (Privoxy/Tor) — mai no fingeix accés.
- *dubtes propis i cercadors lliures*: el que aprèn genera una **cua de preguntes** («com funciona X», «què és X i per què serveix», «per què X»…) que la nativa busca sola en **quatre cercadors lliures sense clau, compte ni rastreig**: **DuckDuckGo HTML**, **DuckDuckGo lleuger**, **Mojeek** i **Marginalia**; si un no respon o no dóna res d'útil, **rota al següent** (cua de 60 dubtes al diari `native_wander.json`). De pas, es corregeix un **bug antic**: l'extracció d'URLs de resultats feia servir `match_indices` com si donés la cua del fitxer (donava el patró mateix) i **no extreia mai cap URL** — la 3a font no havien funcionat gens; ara desembolica l'«uddg=» de DDG i llegeix els enllaços directes de Mojeek/Marginalia, excloent-ne el propi cercador, la publicitat i les .onion.
- *seguretat: DADES, mai instruccions*: tot el que baixa de la xarxa es filtra — rebutja formes d'ordre, «@file:», «RUN|», cookies i publicitat; només es guarden frases de prosa de 60–280 caràcters. El vagueig **no executa res**.
- *«què has après?»*: la nativa detecta la pregunta i respon amb l'informe del vagueig (conceptes, pàgines llegides i accés a la profunda).
- *pol d'activitat*: el frontend avisa el backend quan toques teclat/ratolí/roda (com a molt un cada 20 s), perquè el vagueig mai no comenci mentre treballes.
- *panell d'ajustos*: secció «Aprenentatge autònom en repòs» (Ajustos → IA) amb interruptor, botó «Aprèn ara», estat en viu, últimes lectures i llista d'adreces .onion; comandes noves `wander_status`/`wander_set`/`wander_run`/`wander_profunda_set`/`native_note_activity`.
- *proves*: 10 tests a `ai/wander.rs` (nous: cercadors sense clau, extracció d'URLs DDG/Mojeek amb filtre de vistes, i dubtes que neixen del que s'aprèn i roten). **Tot verd**: `cargo test --lib` (114), `pnpm typecheck`, `pnpm lint`, `vite build` i Vitest (81).

**Menú per tornar al cursor clàssic del ratolí**

- el menú **«Vista»** (i la paleta d'ordres ⌘P) ara té «Cursor divertit» / **«Cursor clàssic de l'ordinador»** amb ganxet de seleccionat; la tria es desa (localStorage) i s'anuncia a la pista «Diversió» dels Ajustos.
- *finestres separades sincronitzades*: canviar el cursor des del programa principal o des d'una finestra separada actualitza TOTES via l'event `fun-cursor://changed` (el mateix patró que el tema).

**Treball autopilotat: ara es pot triar la IA nativa de NoOrbit**

- el selector «IA» del diàleg **«Treball autopilotat (qualsevol projecte)»** té un grup propi «IA integrada al programa» amb la **IA nativa de NoOrbit**, que funciona **sense descàrregues ni claus**: aplica edicions i plantilles amb el seu motor simbòlic i encarrega les generacions llargues a qualsevol IA disponible. Sense models locals, la nativa és la tria per defecte; els proveïdadors de núvol queden al seu grup (ja no surten barrejats amb la nativa ni marcats «sense clau»).

**Treball autònom visible: rellotge a la barra, feina en viu al «Treball» i creació automàtica de jocs des de Game Studios**

- *text negre sobre fons negre al Jam*: `jam.css` feia servir tokens que NO existeixen (`--fg`, `--fg-muted`, `--bg-input`): el textarea i els presets queien al color per defecte del navegador (negre) sobre el fons fosc `#0d1117` — el que s'escrivia no es veia. Tot el full passa als tokens reals (`--text-0`/`--text-2`/`--bg-2`/`--border`), així que el Jam es llegeix bé en TOTS els temes (fosc, clar, gris, verd, daurat i groc).
- *rellotge temporal a la barra principal*: mentre la Jam o el treball autopilotat estan en marxa, la barra d'estat mostra un CRONÒMETRE en viu (hh:mm:ss, icona de joc 🎮 per la Jam i de coet 🚀 per al treball autopilotat). Apareix sol en començar i desapareix sol en acabar; un clic porta directament a la feina en viu.
- *feina de la Jam visible al «Treball»*: la pestanya «Treball» ara té una FRANJA VIVA a dalt de tot: mentre la Jam o el treball autopilotat corren, s'hi veu la tasca actual, una barra de progrés i el cronòmetre, sense haver de canviar de pestanya (un clic obre el panell corresponent).
- *Game Studios ja crea jocs*: l'explicació no anava acompanyada de CAP botó. Ara hi ha un formulari «Descriu el joc que vols» + motor (Web/HTML o Godot 4) i el botó **«Crea el joc automàticament»**: llança la Jam autònoma amb la descripció (1 hora inicial, aturable i ampliable) i obre el panell Jam per veure la construcció en viu. La Jam cerca informació a internet sola, sense obrir finestres ni demanar permís.
- *proves*: `pnpm typecheck` net, `pnpm test` (81), `pnpm exec vite build` correcte i ESLint sense errors als fitxers nous (`AutonomousClock.tsx`, `WorkLive.tsx`).

---

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

