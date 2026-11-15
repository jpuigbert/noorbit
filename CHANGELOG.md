# NoOrbit — Historial de versions

Totes les releases amb els seus binaris: <https://github.com/jpuigbert/noorbit/releases>.
Les notes completes de cada release són a la pàgina del release corresponent.

## 0.71.10-alpha (2026-11-15)

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

