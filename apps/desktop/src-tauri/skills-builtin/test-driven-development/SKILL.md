---
name: test-driven-development
description: Codi guiada per proves amb el cicle RED-GREEN-REFACTOR. Ús en implementar qualsevol funció, correcció d'errors o canvi de lògica amb proves disponibles.
---

# Desenvolupament guiado per proves (TDD)

## El cicle
### 1. RED — prova que falla
- Escriu UNA prova que descriu el comportament que vols, amb nom clar
  (`test_login_rexecta_token_caducat`).
- Executa-la i **verifica que falla** amb el missatge esperat. Una prova
  que passa sense codi nou no prova res.

### 2. GREEN — el mínim per passar
- Escriu el codi més simple que faci passar la prova. Res de
  generalitzacions, res de funcions auxiliars "per si de cas".
- Executa la prova i **verifica que passa**. Després corre totes les proves
  del fitxer/mòdul per confirmar que no has trencat res.

### 3. REFACTOR — neteja amb xarxa
- Elimina duplicació, millora noms, extreu funcions.
- Torna a executar les proves: han de seguir passant sense tocar-les.

Repeteix el cicle per a la següent conducta. Un pas, una prova.

## Correcció d'errors
Primer escriu la prova que reprodueix l'error (RED), després arregla'l
(GREEN). Mai corregeixis sense prova de regressió.

## Prohibicions
- NO escriure primer el codi de producció i després "afegir proves".
- NO saltar-te la verificació visual de l'error de la prova RED.
- NO mesclar RED+GREEN d'una sola tacada sense executar res enmig.
- Si fas trampa (codi primer), esborra'l i torna a començar amb la prova.

## Excepcions vàlides
Prototips de recerca, fitxers de configuració generats i scripts
d'una sola ús poden anar sense proves; tota lògica de negoci, no.
