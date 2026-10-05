---
name: skill-creator
description: Guia per crear skills efectives en format SKILL.md amb frontmatter, nom i descripció que maximitzen la probabilitat d'ús correcte. Ús quan l'usuari vol crear, millorar o empaquetar una skill.
---

# Creador de skills

## Estructura
Una skill és una carpeta amb nom en minúscules i guions que conté
`SKILL.md` (nom de carpeta == camp `name`):

```
skills/<nom-skill>/
├── SKILL.md          # obligatori: frontmatter + instruccions
├── scripts/          # opcional: codi executable
└── references/       # opcional: documentació de suport
```

## Frontmatter (els dos camps decideix si s'usarà)
```
---
name: nom-en-guions
description: Què fa i QUAND usar-lo, amb paraules-clau del llenguatge de
l'usuari. Màxim 1024 caràcters. Tercera persona.
---
```
- La `description` és l'únic que es llegeix abans d'activar la skill:
  inclou-hi els símptomes i contextos que l'han de disparar.
- Mai descripcions buides ("skill útil per a documents"): sigues concret.

## Cos de la skill
1. **Disclosió progressiva**: instruccions essencials a SKILL.md; detall a
   `references/*.md` enllaçats; codi estable a `scripts/`.
2. **Imperatius i verificables**: "Executa X, comprova Y", no "es podria…".
3. **Pas a pas amb punts de control**: què ha de ser cert abans de passar
   al següent pas.
4. **Anti-patrons explícits**: què no fer i senyals d'alerta.
5. Menys de 500 línies; si en necessites més, divideix amb referències.

## Avaluació
1. Escriu 3-5 escenaris realistes d'ús i 2 de contra (no s'haurien
   d'activar).
2. Executa'ls i compara amb/sense la skill: la skill ha de millorar
   resultats mesurables (errors, temps, línies).
3. Si no dispara als escenaris positius: reescriu la `description` amb el
   vocabulari real de l'usuari.

## Empaquetat
- Prohibit: secrets, rutes absolutes privades, dependències no declarades.
- Versiona els canvis rellevants amb data al final del fitxer.
