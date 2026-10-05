---
name: pdf
description: Extreure, crear i manipular fitxers PDF (text, taules, fusionar, dividir, formularis, marques d'aigua). Ús quan la tasca implica fitxers .pdf.
---

# Treballar amb PDF

## Eines (Python)
- `pdfplumber`: extracció de text i taules (primera opció per llegir).
- `pypdf`: fusionar, dividir, rotar, metadades, xifratge senzill.
- `reportlab`: creació de PDF nous des de zero.
- `pdftotext` (poppler) per a bolcats ràpids de text.

## Llegir
1. Primer inventaria: nombre de pàgines, si té capa de text
   (`pdfplumber` → si surt buit, és un escaneig: cal OCR amb `pytesseract`
   sobre `pdf2image`).
2. Taules: `page.extract_tables()`; verifica l'alineació de columnes amb
   les dades reals abans de confiar-hi.
3. Mai transcriguis de memòria: extreu el text i treballa-hi a sobre.

## Crear
1. Per a documents amb disseny: `reportlab` (Canvas o Platypus per a
   flux de text). Defineix marges, fonts i estil en un sol lloc.
2. Per a documents amb plantilla: omple la plantilla i exporta; no
   reconstrueixis el disseny a mà.
3. Verifica sempre el resultat: reobre el PDF generat i comprova el nombre
   de pàgines, el text extret i les fonts encastades.

## Manipular
- Fusionar: `pypdf.PdfWriter` + `append` de cada lector.
- Dividir: un `PdfWriter` per interval de pàgines.
- Formularis omplibles: llegeix camps amb les anottacions de la pàgina i
  escriu-los amb `update_page_form_field_values`.

## Errors freqüents
- Text desordenat en llegir: fes servir `extract_text(layout=True)`.
- Fitxer corrupt: prova `pypdf` amb `strict=False` i informa del dany.
- Mides enormes: comprimeix les imatges abans d'insertar-les.
