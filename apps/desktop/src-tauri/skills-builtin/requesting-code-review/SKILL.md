---
name: requesting-code-review
description: Prepara i demana revisions de codi efectives, i gestiona el feedback rebut punt per punt. Ús abans de fusionar canvis rellevants o quan una revisió externa ho qüestiona tot.
---

# Demanar revisions de codi

## Auto-revisió abans de demanar
1. Llegeix el diff complet (`git diff`) com si fos d'una altra persona.
2. Checklist:
   - Fa el que diu el títol del PR, res més i res menys?
   - Noms clars, sense comentari obvi, sense codi mort?
   - Hi ha proves del comportament nou o corregit?
   - Fitxers generats, secrets o depuradors oblidats?
3. Divideix: si el diff toca >400 línies o barreja refactor+i canvii de
   comportament, separa-ho en dos canvis.

## Com redactar la petició
- **Context**: què s'intenta aconseguir i per què (enllaç al pla o disseny).
- **Tipus de revisió demanada**: correcció, disseny, llegibilitat o seguretat.
- **Punt d'atenció explícits**: on tens dubtes, què has descartat i per què.
- **Nota d'abast**: el que NO toca aquest canvi, perquè el revisor no hi
  gasti temps.

## Gestió del feedback
1. Respon CADA comentari, fins i tot per dir "no ho veig així, perquè…".
2. Classifica abans d'actuar: blocant / millora / preferència / error del
   revisor. No apliquis res automàticament.
3. Si un comentari revel·la un problema de disseny més gran, atura't i
   reenfoca el canvi sencer, no parchegis.
4. Empènyer correccions sense discutir-hi està bé; discutir sense
   correccions no.

## Criteri de fet
Aprovat + CI en verd + cada conversa resolta o responduda. Mai "LGTM"
com a primera línia d'un PR gran: llegeix-lo sencer.
