---
name: xlsx
description: Crear i analitzar fulls de càlcul .xlsx (fórmules, gràfics, validació de dades) amb openpyxl o pandas. Ús quan la tasca implica .xlsx, .xls o CSV grans.
---

# Treballar amb fulls de càlcul

## Triar eina
- Llegir/analitzar dades: `pandas` (`read_excel`), anàlisi ràpida.
- Crear amb format, fórmules o gràfics: `openpyxl`.
- Arxius .xls antics: converteix-los primer amb LibreOffice headless.

## Lectura i anàlisi
1. Inspecciona abans d'analitzar: butxaques, capçaleres reals (poden no
   ser la fila 1), files de peu, cel·les combinades.
2. Detecta tipus: `df.dtypes`; els nombres emmagatzemats com a text són
   la font número 1 d'errors silenciosos.
3. Per a anàlisi: no escriguis valors calculats "a mà": posa la fórmula
   (`ws["E2"] = "=SUM(B2:D2)"`) perquè el fitxer segueixi viu.

## Creació
1. Capçaleres amb estil, amplades de columna explícites, congela
   les capçaleres amb `freeze_panes`.
2. Validació de dades (`DataValidation`) i formats numèrics per a
   moneda/percentatges.
3. Gràfics amb `openpyxl.chart`: BarChart/LineChart sobre rangs de cel·les,
   no sobre dades duplicades.

## Regles d'integritat
- Referències absolutes on calgui (`$B$2`) o taules estructurades.
- Després d'escriure, recalcula una cel·la coneguda i compara amb
  l'esperat; si difereix, atura't i revisa la lògica.
- CSV: gestiona la codificació (UTF-8 amb BOM per a Excel) i els separadors
  locals (`;` en molts Excel catalans).

## Verificació final
Torna a obrir el fitxer amb `openpyxl.load_workbook(data_only=False)` i
confirma: butxaques, fórmules on esperaves i files correctes.
