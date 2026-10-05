---
name: brainstorming
description: Transforma una idea en un disseny aprovat abans d'escriure codi. Ús quan l'usuari descriu una funcionalitat nova, un canvi de disseny o una solució poc definida.
---

# Brainstorming

## Clau
NO escriviu codi d'implementació fins que el disseny hagi estat aprovat.
Codi escrit sense disseny revisat gairebé sempre s'ha de reescriure.

## Procés
1. **Explora el context**: llegeix fitxers rellevants, docs i commits recents
   del projecte abans de preguntar res.
2. **Una pregunta cada vegada**: aclareix abast, restriccions i criteris
   d'èxit amb preguntes curtes, mai una llista de preguntes.
3. **Proposa 2-3 enfocaments**: per a cadascun, resumeix l'arquitectura,
   els punts febles i el cost estimat. Recomana'n un i explica per què.
4. **Presenta el disseny per seccions**: arquitectura, components, flux de
   dades, tractament d'errors i proves. Demana aprovació de cada secció.
5. **Escriu l'especificació**: desa-la a `docs/designs/` amb data i nom de
   la funcionalitat. Auto-revisa: ambigüitats, contradiccions, buits.
6. **Revisió de l'usuari**: mostra l'especificació i demana confirmació
   abans de continuar.

## Després del disseny
Quan el disseny estigui aprovat, continua amb la skill `writing-plans` per
convertir l'especificació en un pla d'implementació pas a pas.

## Quan tallar
Si la petició és trivial (canviar un botó, corregir un error evident),
implementa directament sense aquest procés.
