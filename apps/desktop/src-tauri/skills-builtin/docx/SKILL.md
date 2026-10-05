---
name: docx
description: Crear, llegir i editar documents Word .docx (estils, taules, imatges, canvis rastrejats) amb python-docx o edició directa d'OOXML. Ús quan la tasca implica fitxers .docx.
---

# Treballar amb documents Word (.docx)

## Lectura
- `python-docx`: paràgrafs, estils, taules, encapçalaments.
- Si cal XML cru (comentaris, canvis rastrejats): descompon amb
  `unzip -d carpeta/`, edita `word/document.xml` i recompon amb zip
  (mantén `[Content_Types].xml` a l'arrel del paquet).

## Creació
1. Defineix estils primer (títol, normal, encapçalaments) i no donis
   format directe al text: un document amb estils es pot reformatjar.
2. Estructura amb `add_heading`, `add_paragraph(style=...)`,
   `add_table` amb amplades explícites.
3. Imatges: `doc.add_picture(ruta, width=Inches(6))`; comprimeix-les abans.

## Edició de documents existents
1. NO reconstrueixis el document des de zero: carrega'l amb
   `Document("fitxer.docx")` i modifica'l in situ.
2. Localitza el contingut per text, no per índexos fixes (els índexs
   canvien quan edites).
3. Canvis rastrejats: per defecte deixa el document net; afegeix
   `<w:ins>`/`<w:del>` a l'XML només si l'usuari demana seguiment.

## Verificació final
- Reobre el fitxer generat amb `python-docx` i comprova: nombre de
  paràgrafs/seccions, taules amb files correctes, sense `None` als estils.
- Converteix a PDF (`libreoffice --headless --convert-to pdf`) i revisa la
  primera pàgina si el disseny és crític.

## Errors freqüents
- "Package not found": el fitxer és .doc antic o el zip no és vàlid.
- Text que falta: és en camps, notes al peu o caixes de text → explora les
  parts del paquet OPC.
