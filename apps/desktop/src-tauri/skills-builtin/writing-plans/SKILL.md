---
name: writing-plans
description: Converteix un disseny aprovat en un pla d'implementació amb passos mínims, exactes i verificables. Ús quan existeix una especificació o decisió de disseny clara.
---

# Esciure plans d'implementació

## Estructura del pla
Desa'l a `docs/plans/` amb aquest format per a cada tasca:

```
### Tasca N: <nom curt>
- Fitxers: <rutes exactes a crear/modificar>
- Passos: <acció concreta, 2-5 minuts cadascun>
- Prova: <comanda exacta i resultat esperat>
- Criteri de fet: <condició objectiva i verificable>
```

## Regles
1. **Passos mínims**: cada pas ha de ser una acció d'uns pocs minuts amb
   verificació immediata. Mai "implementar el mòdul X" a seces.
2. **Zero marcadors de posició**: prohibit "TODO", "implementar més tard",
   "omplir ací". Si un detall encara no es pot concretar, el pla no està
   acabat: afegeix una tasca prèvia d'investigació.
3. **Ordre TDD**: per a cada funció, primer el pas que escriu la prova que
   falla, després el codi mínim que la fa passar.
4. **Rutes completes**: escriu sempre la ruta sencera de cada fitxer i el
   contingut exacte dels canvis rellevants (signatures, funcions noves).
5. **Comandes de verificació**: cada tasca acaba amb la comanda exacta
   (`cargo test`, `pnpm tsc --noEmit`…) i eix esperat.
6. **Encadenament**: indica quina tasca habilita la següent i quines es
   poden paral·lelitzar.

## Auto-revisió final
Llegeix el pla com si l'executara un autòmat sense context: pot fer cada
pas sense prendre decisions de disseny? Si la resposta és no per a algun
pas, concreta'l més.

## Després del pla
Executa'l amb la skill `executing-plans`, tasca per tasca.
