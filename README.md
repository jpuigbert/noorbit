# NoOrbit

IDE nativa per a escriptori amb **agent IA local**, integracions reals amb **Blender** i **Unreal Engine 5**, suport per a **Android/iOS natius** (Kotlin+Compose, Swift+SwiftUI) amb emuladors, i un sistema obert d'**extensions**, **skills** i **MCP**.

Construïda amb [Tauri 2](https://tauri.app) + [React](https://react.dev) + [TypeScript](https://www.typescriptlang.org) + [Monaco Editor](https://microsoft.github.io/monaco-editor/). Lleugera, sense Electron.

<p align="center">
  <img src="apps/desktop/src-tauri/icons/128x128@2x.png" alt="NoOrbit" width="128" />
</p>

---

## Contents

- [Què és NoOrbit](#què-és-noorbit)
- [Novetats de la 0.71.5](#novetats-de-la-0715)
- [Novetats de la 0.71.4](#novetats-de-la-0714)
- [Novetats de la 0.71.3](#novetats-de-la-0713)
- [Versions anteriors](#versions-anteriors)
- [Seguretat i claus API (important)](#seguretat-i-claus-api-important)
- [Novetats de la 0.6.0 (historial)](#novetats-de-la-060-historial)
- [Instal·lació (usuari final)](#instal·lació-usuari-final)
- [Compilar des del codi font](#compilar-des-del-codi-font)
- [On són els instal·ladors un cop compilats](#on-són-els-instal·ladors-un-cop-compilats)
- [Wiki](#wiki)
- [Arquitectura](#arquitectura)
- [Llicència](#llicència)

---

## Què és NoOrbit

| Àmbit | Què inclou |
|---|---|
| **Editor** | Monaco amb multi-cursor, Quick Open, Cerca global, Command Palette, pestanes, git diff integrat. |
| **Terminal** | PTY real multi-sessió connectat al backend Rust. |
| **IA** | Xat multi-sessió amb streaming en viu, **materialització automàtica** de fitxers mentre la IA encara està escrivint, i **reintents automàtics** si l'equip es queda sense memòria/context. |
| **Mòbil natiu** | Selector de plataforma per xat (escriptori / Android / iOS). Android: Kotlin + Jetpack Compose + Material 3. iOS: Swift + SwiftUI. Gestió d'AVDs i simuladors, *live view* amb captures periòdiques. |
| **Blender** | Add-on Python (`blender-scripts/noorbit_bridge.py`) + client socket. Executa scripts dins la sessió de Blender i llegeix l'escena. |
| **Unreal** | Remote Control API + bootstrap Python (`unreal-scripts/bootstrap.py`). Build, launcher i execució de scripts. |
| **Extensions** | Skills (carpetes amb `SKILL.md`), plugins natius i servidors **MCP**. |
| **Especialistes** | Rols amb model, temperatura i prompt propis per tasques concretes. |
| **Aprenentatge local** | La IA **aprèn de les altres IAs**: registra les teues converses (pregunta + resposta, amb l'origen), en destil·la **lliçons** amb el model local i les torna al context dels xats futures. Tot a la teva màquina; el dataset es pot exportar en JSONL per entrenar un model local (LoRA). |
| **i18n** | **Català com a idioma base**. Extensible amb fitxers JSON a `apps/desktop/src/i18n/locales/`. |

Tota la documentació ampliada és a la **wiki**: <https://github.com/jpuigbert/noorbit/wiki>

---

## Novetats de la 0.71.5

- **Aprenentatge local (en segon pla)**: una pestanya nova a l'Agent («Aprenentatge») on NoOrbit observa les converses amb **totes** les IAs — la local d'Ollama, les remotes amb token, els especialistes i les IAs web — i en treu **lliçons** (preferències, decisions, fets del projecte) que s'injecten al context dels xats futures: la IA local ja sap el que li has ensenyat i no torna a preguntar-ho. La destil·lació la fa **el model local** (Ollama) i, si no n'hi ha cap, una heurística que detecta les preferències del text. Tot és local: res surt de l'ordinador tret que ho exportis.
- **Dataset per entrenar el teu model**: «Exporta el dataset» genera un **JSONL** amb totes les converses en format de xat (el material per a un LoRA amb LLaMA-Factory, Unsloth o Axolotl) — el pas següent perquè la IA local sigui «teva» de debò.
- **En segon pla de veritat**: la IA aprén sola en una tasca de fons (sessió aïllada: ni la cancel·lació ni el raonament del xat es barrejen) i el xat **no espera**; si treballes amb un proveïdor en línia, la destil·lació automàtica espera (no carrega un model local a la RAM de cop).
- **Checklist amagable**, `@doing:` (el que la IA té entre mans, en directe), adjunt automàtic de fitxers al xat, `@edit:` (CERCA/NOU) i errors d'escriptura explicats (permís, disc ple, volum de només lectura).

## Novetats de la 0.71.4

- **Entorn auto-instal·lable**: finestra de comprovació de l'entorn a l'obrir (Ollama/Blender/ComfyUI) que instal·la SOLA el que falta (zip oficial d'ollama.com a macOS, `install.sh` a Linux, `OllamaSetup.exe` silenciós a Windows) i després engega el servidor — i amb la ruta REAL del binari ollama (el CLI viu dins el bundle de la app).
- **Unreal Engine sempre connectable**: el `.uproject` es parcheja sol amb els plugins `PythonScriptPlugin`+`RemoteControl` (còpia `.bak`), arrencada fiable del bootstrap (`-ExecutePythonScript`, admet rutes amb espais), watchdog de connexió cada 5 s que detecta l'editor sol, cerca automàtica del `.uproject` al projecte obert, captura del viewport robusta i bug de qualitat d'il·luminació corregit.
- **Usuari nou sense 404**: si Ollama no té cap model baixat, NoOrbit baixa'n un d'inicial (mida segons la RAM) amb progrés al gestor de models — al camí amb streaming i al que no, i fins i tot si Ollama ja estava en marxa.
- **Blender amb so**: música generada amb numpy a l'editor de seqüències, veu amb TTS del sistema (say/espeak-ng/SAPI), boca animada amb keyframes i àudio AAC al render MP4.
- **Remats**: el `.uproject` mai no queda tocat sense còpia de seguretat; `cargo clippy` passa amb 0 errors.

## Novetats de la 0.71.3

- **Especialistes en directe**: mentre l'equip treballa, cada expert té la seva mini-card amb el que escriu EN TEMPS REAL (mateix format del xat) i botó «Obre al xat» per continuar-hi.
- **Execució per torns + estalvi de RAM**: amb model local, l'equip treballa d'un en un («Torn N/M») i allibera el model després de cada torn; amb proveïdor al núvol, fins a 3 en paral·lel.
- **Rodet d'imatges en CPU** (stable-diffusion.cpp) per a Mac Intel, on ComfyUI no existeix: txt2img i img2img locals amb models `.gguf`/`.safetensors`.
- **CivitAI** com a font de models (sense token) i IAs locals instal·lables amb un clic (LM Studio, Jan, GPT4All, llama.cpp); editors de vídeo (Kdenlive, OpenShot, DaVinci Resolve, Shotcut).
- **Nudge d'imatges**: si demanes una imatge i el model respon prosa, NoOrbit li torna l'ordre una vegada amb la regla «IMG|» al manual.
- Confirmació de tancament, botons **Desfés/Refés** a la barra, checklist de tasques fixat, **GLM-5.3-Flash i GLM-OCR** al catàleg.

## Versions anteriors

- **0.71.2 — Núvol d'equip**: projecte connectat a Git (connecta 1 clic, ↑↓ pendents, Sincronitza + Auto cada 90 s) per editar-lo des de diversos dispositius.
- **0.71.0**: les integracions DINS de l'Agent (sub-pestanyes Xat/Equip/Ordinador/Unreal/Blender/Mòbil), franja viva «equip treballant», raonament de la IA en català automàtic.
- **0.70.0**: Monaco a demanda (arrencada sense 4 MB+8 MB de workers), −40% tokens, tema «Verd i daurat», servidor d'IA remot amb tutorial pas a pas.
- **0.62 → 0.68**: servidor d'IA remot (SFTP + API OpenAI-compatible), DeepInfer, diagnòstic pas a pas de serveis, gestió de RAM dels models, arrencada robusta amb `crash.log`.
- **0.6.0**: navegador intern controlat per les IAs (`NB|…`), imatges al xat, catàleg d'IAs amb mides, orquestrador d'IA externes.
- L'historial COMPLET, versió per versió, és a [`CHANGELOG.md`](CHANGELOG.md) — i cada release a GitHub porta les seves pròpies notes a <https://github.com/jpuigbert/noorbit/releases>.

---

## Seguretat i claus API (important)

**Aquest repositori NO conté cap clau API, token ni credencial personal — i no n'hi ha hagut mai cap fuita**: la comanda d'auditoria de sota és una comprovació PREVENTIVA que qualsevol pot executar per verificar-ho, no un indicador d'accidents anteriors.

- Els tokens dels proveïdors que enganxes a l'app es desen a la **carpeta de dades del sistema** (`~/Library/Application Support/no-orbit/…`, o `NoOrbitData/` en mode portàtil/USB) — **mai dins el codi del projecte**, i aquesta carpeta no forma part del repo mai.
- No hi ha fitxers `.env` reals (gitignored) ni claus escrites al codi font. NoOrbit no apuja mai res del disc de l'usuari a cap servidor sense la teva acció explícita.
- Si algú clona aquest repo en una altra màquina (o un altre Mac), l'app **funciona igual sense cap clau**: models locals amb Ollama i tota la resta. Les claus API es queden només a la màquina on es van guardar i no se sincronitzen mai amb el repo ni amb cap servei remot.
- Auditoria preventiva ràpida:
  ```bash
  git grep -lI -E "sk-[a-zA-Z0-9]{20,}|gho_[a-zA-Z0-9]{20,}|ghp_[a-zA-Z0-9]{20,}|AKIA[A-Z0-9]{16}"
  ```
  (ha de no tornar RES; si en tornés, seria un accident — obre un *issue*, si us plau).

---

## Novetats de la 0.6.0 (historial)

- **Navegador intern controlat per totes les IAs, sense botons**: qualsevol IA del xat (local, remota o experta) pot obrir pàgines, llegir-ne el contingut i consultar IAs web (DeepSeek, ChatGPT, Claude, Gemini, Perplexity, Grok…) escrivint directrius de text `NB|OBRIR|`, `NB|LLEGIR|`, `NB|PREGUNTA_IA|` que NoOrbit executa. Abans de preguntar a una IA web es demana **permís a l'usuari** i la finestra sempre és visible (el login el fa sempre la persona).
- **El codi dels xats web s'aprofita**: quan una IA web o una pàgina retorna codi, NoOrbit l'extreu en blocs estructurats (amb el llenguatge) i la IA local l'adapta i l'escriu al projecte com a fitxers reals (format `@file:` + automaterialització).
- **Imatges al xat**: es mostren en línia, es poden copiar al porta-retalls i adjuntar/enganxar (⌘V) com a entrada dels models amb visió.
- **Catàleg d'IAs** (menú IA): abans de descarregar veus la mida aproximada de cada model, si és sense censura, si té visió i si pot generar imatges o vídeo.
- **Generació i manipulació d'imatges**: amb ComfyUI local (si la RAM ho permet) o via API en línia amb la teua clau.
- **Orquestrador d'IAs externes legit**: descobriment d'instal·lacions locals (Ollama, LM Studio, llama.cpp…) i fallback en línia **només** amb tokens propis de l'usuari, etiquetant sempre l'origen de cada resposta.
- **Proveïdors d'API robustos**: claus normalitzades (espais, barras, URLs amb `/v1`…), Venice amb el model correcte (`venice-uncensored`) i migració automàtica de la configuració vella, errors 401/404/429 explicats en català.

- **Navegador intern controlat per totes les IAs, sense botons**: qualsevol IA del xat (local, remota o experta) pot obrir pàgines, llegir-ne el contingut i consultar IAs web (DeepSeek, ChatGPT, Claude, Gemini, Perplexity, Grok…) escrivint directrius de text `NB|OBRIR|`, `NB|LLEGIR|`, `NB|PREGUNTA_IA|` que NoOrbit executa. Abans de preguntar a una IA web es demana **permís a l'usuari** i la finestra sempre és visible (el login el fa sempre la persona).
- **El codi dels xats web s'aprofita**: quan una IA web o una pàgina retorna codi, NoOrbit l'extreu en blocs estructurats (amb el llenguatge) i la IA local l'adapta i l'escriu al projecte com a fitxers reals (format `@file:` + automaterialització).
- **Imatges al xat**: es mostren en línia, es poden copiar al porta-retalls i adjuntar/enganxar (⌘V) com a entrada dels models amb visió.
- **Catàleg d'IAs** (menú IA): abans de descarregar veus la mida aproximada de cada model, si és sense censura, si té visió i si pot generar imatges o vídeo.
- **Generació i manipulació d'imatges**: amb ComfyUI local (si la RAM ho permet) o via API en línia amb la teua clau.
- **Orquestrador d'IAs externes legit**: descobriment d'instal·lacions locals (Ollama, LM Studio, llama.cpp…) i fallback en línia **només** amb tokens propis de l'usuari, etiquetant sempre l'origen de cada resposta.
- **Proveïdors d'API robustos**: claus normalitzades (espais, barras, URLs amb `/v1`…), Venice amb el model correcte (`venice-uncensored`) i migració automàtica de la configuració vella, errors 401/404/429 explicats en català.

---

## Instal·lació (usuari final)

### Descàrrega de binaris

Els instal·ladors publicats es troben a la secció **Releases** del repositori:

**👉 <https://github.com/jpuigbert/noorbit/releases>**

Cada release inclou, per a cada sistema, l'arxiu corresponent (substitueix la versió per l'última publicada; els noms d'exemple són els de la **0.71.4-alpha**):

| Sistema | Fitxer que has de descarregar |
|---|---|
| **macOS** (universal, Intel + Apple Silicon) | `NoOrbit_0.71.4-alpha_universal.dmg` |
| **Windows** (10 / 11) | `NoOrbit_0.71.4-alpha_x64-setup.exe` |
| **Linux** (Debian / Ubuntu) | `NoOrbit_0.71.4-alpha_amd64.deb` |
| **Linux** (Fedora / RHEL) | `NoOrbit-0.71.4-alpha-1.x86_64.rpm` |
| **Linux** (universal) | `NoOrbit_0.71.4-alpha_amd64.AppImage` |

L'índex automàtic de totes les versions i els seus enllaços de descàrrega directa és l'asset **`versions.json`** de l'últim release.

> **Versions anteriors**: cada tag és un release independent a GitHub i **no esborra mai** els de versions velles (p. ex. la 0.5.0 completa és a <https://github.com/jpuigbert/noorbit/releases/tag/v0.5.0>). L'índex automàtic de totes les versions i els seus enllaços és l'asset `versions.json` de l'últim release.

### Instruccions per sistema

**macOS — primera arrencada (app no signada)**

NoOrbit **no està signada amb Developer ID ni notaritzada** per Apple (això requereix un compte de desenvolupador de 99 $/any). macOS aplica *Gatekeeper* i bloqueja l'arrencada el primer cop. **No és un error de la app**: és el comportament esperat de qualsevol app descarregada d'Internet sense signar. Tens 3 manes d'activar-la, de més suau a més directa:

**Mètode 1 — Clic dret → Obre (el més recomanable)**
1. Obre el `.dmg` i arrossega **NoOrbit** a la carpeta **Aplicacions**.
2. Al **Finder**, ves a **Aplicacions**, fes **clic dret** (o `Control`+clic) sobre **NoOrbit.app** → **Obre**.
3. Apareixerà el diàleg *«No es pot verificar el desenvolupador»* — prem **Obre**.
4. **Només cal fer-ho un cop.** A partir d'ara, l'obriràs amb doble clic normal.

**Mètode 2 — Configuració del Sistema (si el clic dret no mostra el botó Obre)**
1. Fes doble clic a NoOrbit (refusarà obrir-la; és normal).
2. Obre **Configuració del Sistema → Privadesa i seguretat**.
3. Baixa fins a l'apartat **Seguretat**; hi apareixerà:
   *«S'ha bloquejat NoOrbit.app perquè prové d'un desenvolupador no identificat»* → prem **Obre de totes maneres**.
4. Escriu la contrasenya del Mac i confirma. Torna a obrir la app.

**Mètode 3 — Terminal (elimina l'atribut de quarantena)**
Si els dos mètodes anteriors no funcionen o vols automatitzar-ho:
```bash
xattr -dr com.apple.quarantine /Applications/NoOrbit.app
```
(No cal `sudo` si la app és a Aplicacions i la vas moure tu. Després obre NoOrbit normalment amb doble clic.)

> ⚠️ **Missatge «NoOrbit està malmès i no es pot obrir»**? Sembla alarmant però gairebé sempre significa una d'aquestes dues coses:
> - La descàrrega s'ha tallat a mitges → torna-la a descarregar i compara el SHA-256 amb l'asset del Release.
> - El `.dmg` s'ha obert des de la còpia temporal de Safari/Chrome sense moure la app a Aplicacions → mou **NoOrbit.app** a **Aplicacions** primer, i després aplica el Mètode 1 o el 3.

**Nota per a Apple Silicon (M1–M4)**: no cal cap pas addicional. El bloqueig és el mateix Gatekeeper que a Intel; l'arquitectura universal ja inclou el codi natiu arm64.

**Per què no es firma?** La signatura i notarització exigeixen un compte de desenvolupador d'Apple de pagament. Si vols una versió signada (sense bloquejos ni passos manuals), col·labora amb el projecte o crea un issue per prioritzar-ho: el workflow de CI ja prepara els binaris i només caldria afegir-hi els secrets `APPLE_SIGNING_IDENTITY`, `APPLE_ID` i `APPLE_APP_SPECIFIC_PASSWORD`.

**Windows**
1. Doble clic a `NoOrbit_0.71.4-alpha_x64-setup.exe`.
2. S'instal·la **per a l'usuari actual**, sense necessitat de drets d'administrador.
3. L'executable apareix al menú Inici.

**Linux (Debian/Ubuntu)**
```bash
sudo apt install ./NoOrbit_0.71.4-alpha_amd64.deb
# si el paquet ja està descarregat i vols instal·lar-lo "a la vella":
sudo dpkg -i NoOrbit_0.71.4-alpha_amd64.deb && sudo apt -f install
```
Executa'l amb `no-orbit` o des del menú d'aplicacions.

**Linux (Fedora/RHEL)**
```bash
sudo dnf install ./NoOrbit-0.71.4-alpha-1.x86_64.rpm
```

**Linux (AppImage)**
```bash
chmod +x NoOrbit_0.71.4-alpha_amd64.AppImage
./NoOrbit_0.71.4-alpha_amd64.AppImage
```

---

## Compilar des del codi font

### Requisits comuns (a totes les plataformes)

| Eina | Versió mínima | Com install·lar |
|------|----------------|-----------------|
| Node.js | 20 | <https://nodejs.org> |
| pnpm | 9 | `corepack enable` |
| Rust | 1.77 | <https://rustup.rs> |

**Dependències del sistema operatiu**:

- **macOS**: `xcode-select --install` (Command Line Tools).
- **Linux (Debian/Ubuntu)**:
  ```bash
  sudo apt update && sudo apt install -y \
    libwebkit2gtk-4.1-dev build-essential curl wget file \
    libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev
  ```
- **Windows**: Microsoft **C++ Build Tools** (workload *Desktop development with C++*) + **WebView2 Runtime** (a Windows 11 ja ve inclòs; a Windows 10 es baixa sol gràcies a `webviewInstallMode: downloadBootstrapper`).

### Passos

```bash
git clone https://github.com/jpuigbert/noorbit.git
cd noorbit
pnpm install

# Mode desenvolupament (hot reload)
pnpm app:dev

# Compilar paquets segons el sistema:
pnpm app:build:mac:universal   # macOS: DMG universal (Intel + Apple Silicon)
pnpm app:build:mac:intel       # macOS: només Intel
pnpm app:build:mac:arm         # macOS: només Apple Silicon
pnpm app:build:deb             # Linux: .deb
pnpm app:build:linux           # Linux: .deb + .rpm + AppImage
pnpm app:build:win             # Windows: .exe (NSIS) + .msi
pnpm app:build                 # El paquet natiu del SO on s'està executant
```

> ⚠️ Cada instal·lador s'ha de **construir des del seu sistema operatiu** (o en CI). Des de macOS no es poden crear `.deb` de Linux ni `.exe` de Windows de forma fiable perquè enllacen biblioteques pròpies d'aquelles plataformes. El que sí es pot fer des d'un sol Mac és produir un **DMG universal** (Intel + Apple Silicon).

---

## On són els instal·ladors un cop compilats

Tots els paquets es generen a:

```
apps/desktop/src-tauri/target/[<triple>]/release/bundle/
```

Rutes completes segons sistema:

### macOS (des de macOS)

```
apps/desktop/src-tauri/target/universal-apple-darwin/release/bundle/
├── dmg/NoOrbit_0.71.4-alpha_universal.dmg      ← compartiu això
└── macos/NoOrbit.app                     ← app "nua" (provable directament)
```

Per a només Intel (`--target x86_64-apple-darwin`) o només Apple Silicon (`--target aarch64-apple-darwin`), substitueix `universal-apple-darwin` per l'altre triple.

### Windows (des de Windows)

```
apps\desktop\src-tauri\target\release\bundle\
├── nsis\NoOrbit_0.71.4-alpha_x64-setup.exe      ← instal·lador clàssic (recomanat)
└── msi\NoOrbit_0.71.4-alpha_x64_en-US.msi        ← paquet per a desplegaments empresarials (GPO/SCCM)
```

### Linux (des de Linux)

```
apps/desktop/src-tauri/target/release/bundle/
├── deb/NoOrbit_0.71.4-alpha_amd64.deb            ← Debian / Ubuntu
├── rpm/NoOrbit-0.71.4-alpha-1.x86_64.rpm         ← Fedora / RHEL
└── appimage/NoOrbit_0.71.4-alpha_amd64.AppImage  ← universal (tots els distros)
```

En una ARM Linux (p. ex. Raspberry Pi) afegeix `--target aarch64-unknown-linux-gnu` i ajusta el nom del paquet.

---

## Wiki

La documentació detallada (arquitectura, integracions, extensions, dreceres, guia de contribució) és a:

**📚 <https://github.com/jpuigbert/noorbit/wiki>**

Pàgines disponibles:

- [[Instalació]]
- [[Funcionalitats]]
- [[IA-en-NoOrbit]]
- [[Desenvolupament-mòbil-natiu]]
- [[Integració-Blender]]
- [[Integració-Unreal]]
- [[Extensions]]
- [[Dreceres-de-teclat]]
- [[Arquitectura]]
- [[Contribució]]

---

## Arquitectura

```
noorbit/
├── apps/desktop/                 ← única app (Tauri)
│   ├── src/                      ← frontend React/TypeScript (components/, stores/, i18n/, editor/, core/, help/)
│   ├── src-tauri/                ← backend Rust (commands/, ai/, agent/, mobile/, blender/, unreal/, experts/, plugins/, skills/, lsp/, computer/, autonomous/, api/)
│   └── package.json
├── blender-scripts/              ← add-on Python per connectar amb Blender
├── unreal-scripts/              ← bootstrap Python per Unreal
├── scripts/                      ← instal·ladors (install-unreal.sh)
└── tools/                        ← eines de build (make-icons.mjs)
```

Fluxos clau:

- **Streaming IA**: el backend emet l'event `ai://chunk` etiquetat per `sessionId`; el frontend (`agentStore.appendStream`) acumula el text i **crida `scheduleLiveMaterialize`** per escriure fitxers reals a disk **en viu**, mentre el model encara està generant.
- **Reintents automàtics**: si Ollama/LM Studio fallan per `out of memory` o el stream es talla, NoOrbit torna a intentar-ho automat­icament amb context **reduït → sense codi → sense historial**, sense intervenció humana.
- **Invoke**: comandaments Rust cridats des de TS via `invoke("name", { args })` de `@tauri-apps/api/core`.

---

## Llicència

Distribuïda sota **MIT** — veure [`LICENSE`](LICENSE) i [`NOTICE`](NOTICE).

Part del disseny i alguns fragments deriven o estan inspirats en **Visual Studio Code**, i utilitzen el **Monaco Editor**, ambdós llicenciats sota MIT per Microsoft Corporation. L'atribució completa és a `NOTICE`.

---

## Estat del projecte

Versió actual: **0.71.4-alpha** (vegeu [`CHANGELOG.md`](CHANGELOG.md)). El projecte està en desenvolupament actiu. Si trobes un bug o vols una funcionalitat, obre un [issue](https://github.com/jpuigbert/noorbit/issues).
