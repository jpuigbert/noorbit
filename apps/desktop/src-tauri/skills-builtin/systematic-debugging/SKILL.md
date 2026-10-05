---
name: systematic-debugging
description: Depuració sistemàtica en 4 fases centrada en la causa arrel, mai per prova-error aleatòria. Ús davant qualsevol error, test que falla o comportament unexpected.
---

# Depuració sistemàtica

## Fase 1 — Reproduir i observar
1. Reprodueix el fallo de forma consistent i mínima. Si no es pot
   reproduir, afegeix registres i torna-hi quan hi hagi dades.
2. Llegeix el missatge d'error SENCER i el traçat de pila complet. Anota:
   què fallava, què s'esperava, quan va començar.

## Fase 2 — Aïllar
1. Redueix a la superfície d'error més petita: una funció, una entrada,
   un commit (`git bisect` si cal).
2. Compara amb un cas que funciona bé. Les diferències són la pista.
3. Revisa els canvis recents de la zona afectada.

## Fase 3 — Hipòtesis verificables
1. Escriu la hipòtesi de causa arrel: "X falla perquè Y, i ho veurem en Z".
2. Verifica-la amb una prova concreta (log, punt d'interrupció, consulta)
   ABANS de canviar cap codi.
3. Hipòtesi falsada: formula'n una de nova. No enmascarris mai el símptoma.

## Fase 4 — Correcció
1. Arregla la causa arrel, no el símptoma (null-checks dispersos,
   try/catch buits, esperars arbitraris = senyals que vas tard).
2. Afegeix una prova de regressió que fallaria sense la correcció.
3. Verifica: prova nova passa, suite completa passa, cas original resolt.

## Regles d'or
- Mai "provaré una cosa ràpida" sense hipòtesi: és disparar primer i
  apuntar després. Dos intents fallits = atura't i qüestiona l'arquitectura.
- Un canvi cada vegada; si en fas dos i funciona, no saps quin va curar.
- Si després d'això no entens la causa, informa amb tota l'evidència
  recollida en lloc de tancar el bug a cegues.
