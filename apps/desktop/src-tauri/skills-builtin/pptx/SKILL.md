---
name: pptx
description: Crear i editar presentacions PowerPoint .pptx (mestres, diapositives, layouts, exportació) amb python-pptx o edició d'XML. Ús quan la tasca implica fitxers .pptx.
---

# Treballar amb presentacions

## Creació amb python-pptx
1. Parteix d'una plantilla si n'hi ha: `Presentation("plantilla.pptx")` i
   fes servir els seus layouts (`prs.slide_layouts[1]`…), mai caixes de text
   flotants si el layout ja les ofereix.
2. Estructura primer, decoració després: genera totes les diapositives amb
   títol i esbossos de contingut, després afegeix formes i imatges.
3. Text: màxim 6 línies per diapositiva; el detall va a les notes
   (`slide.notes_slide`), no al cos.
4. Mides: calcula posicions amb `Emu`/`Inches` i no encimis elements;
   fes servir els placeholders del layout sempre que existeixin.

## Edició d'arxius existents
1. Inventaria: recorre `prs.slides` i les seves formes abans de tocar res.
2. Modifica in situ (text, imatges, ordre); no reconstrueixis la
   presentació des de zero perquè perds mestres i animacions.
3. Edicions fins (transicions, tema): deszipa i edita
   `ppt/slides/slideN.xml` + `ppt/slideMasters/`; tornar a zipar mantenint
   l'estructura del paquet.

## Exportació i verificació
- `libreoffice --headless --convert-to pdf presentacio.pptx` i revisa el
  PDF pàgina a pàgina (o convertit a imatges): text tallat, elements
  fora de marge, contrast.
- Confirma el recompte de diapositives i que cap placeholder ha quedat
  buit ("Clic para…", "Haz clic para…").

## Errors freqüents
- Fonts no disponibles: declara les del sistema o insereix el text com
  a forma amb fallback.
- Imatges enormes: redimensiona-les abans d'insertar-les (manté la proporció).
