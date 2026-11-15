# NoOrbit · Spot publicitari

Material complet del vídeo de promoció de NoOrbit per a
**themes.thepuigberts.com**. 53 s, amb dos personatges originals en estil
kawaii/anime que expliquen el projecte.

---

## Què hi ha

| Fitxer | Què és |
|---|---|
| **`video/noorbit-spot-16x9.mp4`** | **Mestre.** 1920×1080, 53 s, H.264 + AAC. Per a la web i YouTube. |
| **`video/noorbit-spot-9x16.mp4`** | Retall vertical. 1080×1920, 53 s. Per a Reels, TikTok i Shorts. |
| `video/poster-16x9.jpg` | Cartell per al `<video>` (1280×720). |
| `video/poster-9x16.jpg` | Cartell vertical (1080×1920). |
| **`GUIO.md`** | El guió: estratègia, personatges, taula de temps, text a pantalla i locució. |
| **`DISSENY.md`** | La direcció d'art: idea gràfica, paleta, tipografia, moviment i disseny sonor. |
| `build/` | El codi que genera tot això (reproduïble i versionable). |

## Els personatges

**PIP** — la dissenyadora que té la idea. Veu Montse to 50, 150 ppm.
**ORBI** — la companya IA de NoOrbit. Veu Montse to 64, 160 ppm.

Són personatges **originals** dibuixats en SVG per a aquest anunci, no cap
personatge existent de cap marca. Orbi és el mateix logotip de NoOrbit fet
personatge: la mateixa esfera, el mateix anell orbital, amb ulls i galets.

## Com es va fer

**Un sol guió, dos formats.** Tota l'animació és una funció del temps: `render(t)`
calcula l'estat de cada element a cada instant. No hi ha cap animació que «passi»
per casualitat, de manera que el vídeo es pot re-renderitzar, versionar o retallar
sense que res es desajusti, i la veu i la imatge queden sempre sincronitzades.

**La interfície és la de veritat.** La finestra de NoOrbit del vídeo està
reconstruïda amb els tokens, les mides i els textos reals de l'app
(`apps/desktop/src/theme.css` i `i18n/locales/ca.json`). La captura del
marketplace és autèntica. Cap claim tècnic que NoOrbit no faci.

**El so és 100 % sintetitzat.** Veu en català (Montse, veu del sistema macOS) i
una banda sonora generada amb síntesis pura — caixa de música, pad de corda, baix,
maller, shaker i campanes, a 96 BPM en Do major. Cap mostra, cap llicència, cap
problema de drets. La música baixa automàticament quan parla la veu.

## Reproduir

```bash
cd promo
npm install                # playwright + ffmpeg-static
npx playwright install chromium
./build/build.sh           # genera els dos formats
./build/build.sh wide      # només el 16:9
```

Pipeline: `make-vo.mjs` (veu) → `audio.py` (música + mescla + ducking) →
`render.mjs` (fotogrames) → `encode.sh` (H.264 + graó de pel·lícula).

### Estructura de `build/`

| Fitxer | Paper |
|---|---|
| `story.html` | **La font de veritat del vídeo.** Guió visual, disseny, personatges i director. |
| `vo.json` | Els 13 segments de locució: text, temps, veu, ritme i to. |
| `make-vo.mjs` | Genera la veu amb `say` (amb `[[pbas]]` per als dos tons) i neteja durades. |
| `audio.py` | Música procedural + reverberació + mescla + ducking → `build/audio/mix.wav`. |
| `render.mjs` | Fotogrames amb Playwright (`preview` per revisar, `full` per produir). |
| `encode.sh` | Codificació H.264 amb graó de pel·lícula afegit en la codificació. |
| `build.sh` | El pipeline sencer. |
| `assets/` | Icona de l'app, captura del marketplace, graó. |

### Retocar el guió

1. Edita `build/vo.json` (text, temps, veu, to).
2. Edita la part corresponent del `render()` i la llista `TALK` de `build/story.html`.
3. `./build/build.sh`.

El guió de veu i el guió visual estan separats perquè la veu es pugui reescriure
sense tocar el disseny, i al revés. Per a un retall de 30 s, és suficient
comprimir els temps d'ambdós.

## Notes

- Els fotogrames intermedis (`frames-wide/`, `frames-v/`) i els fitxers de so
  generats no es versionen: es reconstrueixen amb `build.sh`.
- Dependències: Node 18+, Python 3 (sense biblioteques externes) i `ffmpeg-static`
  (via npm). La veu requereix macOS (`say`).
