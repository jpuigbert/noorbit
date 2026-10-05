---
name: executing-plans
description: Executa un pla d'implementació escrit pas a pas, verificant cada tasca i sense desviar-se. Ús quan existeix un pla a docs/plans o l'usuari demana executar-lo.
---

# Executar plans

## Abans de començar
1. Llegeix el pla sencer. Si algun pas és ambigu o incorrecte, arregla el
   pla abans d'executar-lo, no improvises.
2. Comprova l'estat del repo (branca, canvis sense confirmar) i crea una
   branca de treball si escau.

## Durant l'execució
1. **Una tasca cada vegada**: segueix l'ordre del pla. No endavantis passes
   ni agrups tasques per estalviar temps.
2. **Verificació obligatòria**: executat cada pas, corre la comanda de
   verificació indicada al pla i confirma'n el resultat abans de marcar la
   tasca com a feta.
3. **Commit petit**: després de cada tasca completada i verificada, fes un
   commit amb missatge descriptiu de la tasca.
4. **Cap canvi fora del pla**: si detectes un problema no previst, atura't,
   afegeix-lo al pla com a tasca nova i després continua.

## Quan alguna cosa falla
- Un pas falla repetidament (2-3 intents): atura l'execució i informa amb
  el context complet (tasca, error, intents). No endevines solucions.
- El pla revela una premissa falsa: atura't i torna a la skill
  `writing-plans` per revisar el pla. Mai executis un pla trencat.

## En acabar
1. Corre la suite completa de proves del projecte.
2. Revisa la llista de tasques: tot fet? res marcat com a pendent?
3. Resum final: tasques completades, desviacions detectades i següents
   passos suggerits (normalment `requesting-code-review`).
