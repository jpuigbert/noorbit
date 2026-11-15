 NoOrbit · Direcció d'art del spot (53 s)

Aquest document explica **per què** el vídeo es veu com es veu. La font de veritat
del disseny és `promo/build/story.html`; aquí hi ha la intenció.

---

 1. Les tres idees gràfiques

 1.1 L'ÒRBITA (heretada del logotip)
El logotip de NoOrbit ja és una òrbita: un nucli lluminós dins d'un anell, amb un
satèlit. El vídeo **no inventa una llengua gràfica nova: aprofita la de la marca**
i li dona un significat narratiu:

| Element | A què correspon |
|---|---|
| El **nucli** blau | La idea de la persona. És el que encén el vídeo i el que torna al final. |
| L'**anell** violeta | L'eina: l'IDE, l'IA, les integracions. |
| El **satèlit** | El client que compra. Al final és el tema publicat. |

I, sobretot: **Orbi és el logotip fet personatge.** La mateixa esfera, el mateix
anell orbital, el mateix degradat — però amb ulls, galets i mans. La marca i el
personatge són el mateix objecte, i la pel·lícula demostra que un logo es pot
convertir en algú que parla.

 1.2 L'ANIME (el llenguatge de la narració)
L'estil de les bombolles, de les línies de velocitat, del focus i dels brillants
no és decoratiu: és el codi visual que fa que el missatge es vegi com una conversa i no com una llista de funcions.

- **La bombolla escriu el text sincronitzat amb la veu.** A l'animació real el
  text apareix sencer, però al dibuix el text s'escriu: és molt més viu.
- **Qui parla avança i creix** (×1,13 i 74 px cap al centre); l'altre es queda
  enrere (×0,93, 46 px en contra i una mica esvaïda). És la convenció de l'animació
  perquè es vegi què és important.
- **Línies de velocitat radials** des del cap del qui parla, amb màscara radial:
  l'efecte «focus» de manga.
- **Brillants de quatre puntes** (amb halo) que apareixen i desapareixen al voltant
  del qui parla: el *kirakira*.
- **Parpelleig i boca animats** de tots dos, deterministes, a temps del compàs.

 1.3 LA CAIXA DE PANDORA
L'escena 2 posa una carpeta `tema-ceramica` amb sis fitxers `.md` i **cap línia de
codi**. L'escena 10 obre aquesta mateixa carpeta i en treu catorze fitxers reals,
un a un, amb el ✓ verd. La caixa es tanca: el problema i la solució són el mateix
objecte.

---

 2. Paleta

Tots els colors són **els de l'app real**, extrets de `apps/desktop/src/theme.css`:

| Token | HEX | Ús |
|---|---|---|
| `--bg-0` | `09090d` | fons de l'app i decorat del diàleg |
| `--panel` | `0f0f15` | barra de menú, barra d'estat |
| `--panel-2` | `14141c` | bombolles de xat |
| `--border-2` | `2b2b3a` | línies i vores |
| `--accent` | `7c5cff` | **violet** — la marca, el botó «Pujar tema…» |
| `--accent-2` | `5ea2ef` | **blau** — el codi, la segona òrbita, el llapis de Pip |
| `--cyan` | `22d3ee` | cian — l'anell d'Orbi, la barra de context |
| `--gold` | `e8c465` | **daurat** — reservat al CTA final i a l'URL |
| `--green` | `8fbf6a` | verd — el marketplace (heretat del site real) |
| `--ok` | `4ade80` | verd — els ✓, el «context compactat» |
| `--amber` | `fbbf24` | ambre — l'alerta de memòria, el segell «0 línies» |

**Regla de color:** el violet i el blau són de l'**eina**; el daurat és del
**resultat**; el verd és de l'**èxit**. El daurat no apareix fins als darrers
4 segons i crida exactament on ha de cridar: a l'URL.

**Colors dels personatges:** Pip fa servir el daurat del tema (pana), el marró
càlid del pell i el rosa dels galets (`ff7f9e`); Orbi fa servir el degradat
violeta-blau de la marca i el cian de l'anell. Cap color nou que no tingui origen
a la marca o a la gamma del tema «ceràmica» del vídeo.

---

 3. Tipografia

- **Display:** sistema (SF Pro). Pes 800, `letter-spacing: -0.035em`. La tracking
  negatiu és el que fa que un text en negre es llegeix «de disseny».
- **Mono:** `SF Mono` per al codi i les etiquetes tècniques. El mono diu
  «això és codi real».
- **Bombolles:** 24 px, semibold, paper crema (Pip) i blau gelat (Orbi), amb cua
  triangular cap al personatge.
- **Subtítols:** 25 px en pastilla fosca amb `backdrop-filter`. Són el 100 % del
  guió de veu: la gent veu els vídeos en silenci.

---

 4. La interfície, quan apareix

La finestra de NoOrbit del vídeo **no és una maqueta inventada**: està
reconstruïda amb els tokens, les mides, els menús, les pestanyes i els textos
reals de l'app (`Fitxer · Edició · Selecció · Vista · Ves · Executa · Terminal ·
IA · Ajuda`, la barra violeta, el xip «IA local», les pestanyes `Xat · Equip ·
Ordinador · Unreal · Blender · Mòbil`). El codi PHP i CSS que s'escriu és un
tema WordPress real i funcional. La captura del marketplace és **autèntica**.

**El que fa la diferència és el moviment:**
- el codi s'escriu **caràcter a caràcter**;
- l'arbre de fitxers **es pobla sol**, un fitxer cada 0,62 s, amb el ✓ que entra
  0,3 s més tard;
- la llista de tasques es marca sola, amb el spinner a la tasca que s'està fent;
- la barra de context **arriba al 100 %, flixeja en verd i cau al 20 %**: es veu
  la compactació, no només se'n parla.

---

 5. Moviment i ritme

**Regla:** tota l'animació és una funció del temps. `render(t)` calcula l'estat de
cada element a cada instant; no hi ha cap animació que «passi» per casualitat.
Això vol dir que el vídeo es pot re-renderitzar, versionar o retallar sense que
res es desajusti, i que la veu i la imatge quedin sempre sincronitzades.

**Corbes:** `easeOut` (entrades), `easeInOut` (transicions d'estat),
`easeOutBack` (el logotip, el modal i Orbi saltant de la icona).

| Temps | Esdeveniment |
|---|---|
| 0:00 | Neix el nucli. La primera imatge és la promesa, no el producte. |
| 0:05.3 | Canvi a fricció: de la introspecció al soroll. |
| 0:10.6 | **Flàs** violeta i el logotip. La fi del problema. |
| 0:11.25 | **Orbi salta de la icona.** El logo es presenta com un company. |
| 0:13.5 | Pip i Orbi entren en escena. Aquí el relat passa a la persona. |
| 0:17.0 | **Canvi de.focus:** el diàleg ja funciona i el guió el porta la veu. |
| 0:23.4 | Orbi explica la compactació. Els dos celebren. |
| 0:35.4 | La finestra de l'app puja: del relat a la prova. |
| 0:41.2 | **Compactació** — el moment que diferencia NoOrbit de la resta. |
| 0:43.6 | «Pujar tema…»: tot plega cap a la venda. |
| 0:46.9 | **Impacte** i el marketplace: el tema ja és a la venda. |
| 0:49.3 | Cartell final, repòs, URL, i els dos saluden. |

---

 6. Disseny sonor

 Veu
Montse (català, veu del sistema macOS). **Dos personatges amb el mateix timbre,
to diferent**, per la comanda de veu `[[pbas]]` de macOS:

| | To | Ritme | Per què |
|---|---|---|---|
| **Pip** | `pbas 50` (≈186 Hz) | 150 ppm | Veu natural, alla mateixa, sense accelerar. |
| **Orbi** | `pbas 64` (≈364 Hz) | 160 ppm | Un graó més alt i més ràpid: el robot lleuger i content. |

Cap dels dos és «llegit massa ràpid»: la locució ocupa 51,5 s de 53 i deixa
entre 0,4 i 2,8 s de silenci entre frases. Les bombolles s'escriuen exactament
al ritme de la veu.

 Música
100 % sintetitzada, cap mostra, cap llicència. **96 BPM, Do major, progressió
| C | Am | F | G |.**

- **Caixa de música** (celesta): la melodia. Atac de 3 ms, cua llarga, un pèl de vibrato.
- **Pad de corda**: serrellos d'ona suaus, atac lent, un acord per temps.
- **Baix** a l'arrel, **maller** als temps 2 i 4, **shaker** molt suau.
- **Campanes FM** a la revel·lació (0:11.35) i a la publicació (0:44.05).
- **Brillants**: un ping just abans de cada frase de veu — el ritme parla.

**L'arranjament segueix la pel·lícula:**

| Secció | Temps | Instruments |
|---|---|---|
| Ganxo | 0:00–0:05.6 | pad + melodia apagada i baixa |
| Fricció | 0:05.6–0:10.9 | pad + melodia al 22 %, baix apagat |
| Revel·lació | 0:10.9–0:13.6 | **tot** + campanes |
| Diàleg | 0:13.6–0:35.4 | **llit prim**: pad + brillants. Molt espai per a la veu |
| Demo | 0:35.4–0:43.8 | construcció: melodia, baix, maller, shaker |
| Publicació | 0:43.8–0:47.9 | **màxima** + campanes i arpegi |
| Cartell | 0:47.9–0:53.0 | resolució, decreixement |

**Ducking:** quan parla la veu, la música baixa al 30 % amb rampes de 340 ms.
Sense això la locució no s'entén; amb això, la música continua viva. La música
és **coherent amb el tema** perquè sona com el tema que es ven: farg, ceràmica,
calidesa i una miqueta de llum.

---

 7. Formats i accessibilitat

- **16:9 (1920×1080)** — mestre, per a la web i YouTube.
- **9:16 (1080×1920)** — retall per a Reels/TikTok/Shorts, amb la mateixa
  storyboard i una maqueta vertical pròpia.
- **Subtítols cremats** en tots els formats: es veu completament en silenci.
- **Contraste:** tot el text sobre fosc està per sobre de WCAG AA; el daurat
  `e8c465` sobre `04060e` dona 10,4:1.
- **Sense dependències:** H.264 (`yuv420p`) + AAC, compatible amb tots els
  reproductors i amb l'`<video>` de qualsevol web.
