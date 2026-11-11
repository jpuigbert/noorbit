# NoOrbit — Historial de versions

Totes les releases amb els seus binaris: <https://github.com/jpuigbert/noorbit/releases>.
Les notes completes de cada release són a la pàgina del release corresponent.

## 0.71.4-alpha (2026-11-11)

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

