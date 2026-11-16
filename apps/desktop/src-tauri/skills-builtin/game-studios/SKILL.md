---
name: game-studios
description: Workflow «Claude Code Game Studios» (donchitos/claude-code-game-studios): idea → preguntes → brief d'una pàgina → primer build en navegador (prototypes/<nom>/), canvis comprovats, compartir, créixer (Vite+TS) i acabar (/cut). Compatible amb qualsevol IA connectada a NoOrbit (local o núvol).
---

# Game Studios (NoOrbit)

Plantilla i flux de treball per crear jocs de navegador a partir d'una idea, inspirat en
`donchitos/claude-code-game-studios`. Funciona amb **qualsevol IA** que tenguis connectada
a NoOrbit (Ollama/llama.cpp locals, OpenAI, Claude, Gemini, DeepSeek, Qwen, GLM, Grok…),
sense dependre d'un proveïdor concret.

## Començar
1. **Nou projecte → Game Studios** (menú Fitxer o paleta de comandes): NoOrbit clona la
   plantilla a una carpeta nova i l'obre.
2. Obre el panell **Agent** i tria la IA que vulguis (local o núvol).
3. Escriu, per exemple: `/prototype un joc de pesca on el peix rar només pica a la posta de sol`

## Flux
1. **Unes preguntes** (una ronda): gènere, sensació, aspecte, dificultat, objectiu.
   «Tu tries» funciona.
2. **Brief d'una pàgina** a `design/game-brief.md`: les teves paraules tal com les vas
   escriure, l'aspecte, el primer minut, el repte, les funcions i el que queda per a més
   tard. Revisa'l i digues «Construeix-lo» o canvia alguna cosa.
3. **Primer build**: joc de navegador a `prototypes/<nom>/` que s'obre amb doble clic.
   Totes les figures dibuixades, sense emoji, sense còpia d'un altre joc, i no trivialment
   fàcil. Pots demanar «construeix-lo a mitjà» (més barat i més prim).
4. **Comprovació abans d'entregar-te'l**: pulsacions de teclat en navegador sense capçalera
   per veure que obre sense errors i respon; text massa petit, solapat o fora de pantalla i
   pantalles que es congelen es corregeixen en una ronda; cada línia del brief es marca
   construïda, pendent o trencada a partir de captures i del codi. El teu joc és la
   prova real: la targeta d'entrega diu què ha passat, què segueix trencat i què s'ha
   suposat, i acaba amb «Encara no jugat per una persona».

## Després
- **Canvia-ho preguntant**: cada canvi es comprova contra el que vas dir que es manté i
  el que s'evita; cada canvi acceptat és un punt de control al qual pots tornar.
  Demana «ajusta-ho» i guanya un panell de tweaks amb un lliscador per a cada valor.
- **Comparteix-ho**: còpia neta sense el panell ni els fitxers de treball, llesta per
  itch.io o un enllaç.
- **Fes-lo créixer**: quan t'agrada, es converteix en projecte web real (Vite +
  TypeScript, valors en fitxers de dades, proves i build), i hi afegeixes una funció
  per sessió. Després de cada funció es torna a jugar la mateixa partida sembrada i
  es mostren els canvis. Històries (`/create-stories`, `/dev-story`, `/story-done`) només
  quan les demanes.
- **Acaba'l**: `/cut` proposa què tallar i una llista de poliment, i una data per
  vendre el joc.

## Ja tens un joc de navegador?
`/prototype --from <ruta>` en porta una còpia tal com està, per canviar-lo i fer-lo créixer
igual.

## Dreceres (shortcuts) per IA
- **Local (Ollama/llama.cpp)**: tria el model actiu al Gestor de models; el flux
  funciona igual, però la IA pot trigar més. Recomanat per a proves ràpides.
- **Núvol (OpenAI/Claude/Gemini/DeepSeek/Qwen/GLM/Grok)**: millor qualitat i velocitat;
  connecta el token a IA ▸ Gestor de models.
- **Especialistes**: pots assignar rols (dissenyador, programador, art) amb models i
  temperatures pròpies.
- **Web IA**: si vols consultar una IA web (DeepSeek, ChatGPT, Claude, Gemini…), usa el
  panell «IA de web»; NoOrbit extreu el codi i el materialitza al projecte.
- **Codi del xat web**: NoOrbit extreu blocs de codi estructurats i la IA local els adapta
  i escriu com a fitxers reals (`@file:` + automaterialització).
- **Imatges**: amb ComfyUI local o via API amb la teua clau; nudge «IMG|» si la IA respon
  amb prosa.
- **Blender/Unreal**: per a models 3D, usa els panells Blender/Unreal; el flux de Game
  Studios es centra en jocs de navegador, però pots portar assets.

## Compatibilitat
Qualsevol IA connectada a NoOrbit (local o núvol) pot executar aquest flux. Els resultats
depennen de la capacitat del model: els models grans (Claude, GPT-4, DeepSeek) donen millor
qualitat; els petits són útils per a proves barates.

## Referència
- Repo original: https://github.com/donchitos/claude-code-game-studios
- Instal·la/importa des de IA ▸ Skills i connectors, o menú IA ▸ Game Studios.
