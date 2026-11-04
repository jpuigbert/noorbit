---
name: Web (HTML/CSS/JS)
description: Pàgines web estàtiques i dinàmiques — HTML5, CSS modern, JavaScript, acumulació responsive i vista prèvia en viu de NoOrbit.
---

# Com programar web amb NoOrbit

1. **Estructura recomanada** (escriu-la amb «@file:»):
   - `index.html` (única si és SPA; diverses si és estàtica multi-pàgina)
   - `css/styles.css` · `js/main.js` (o més fitxers per mòduls)
   - `assets/` per imatges, fonts i desplegament
2. **HTML: semàntica primer**: `<header> <nav> <main> <section> <article>
   <footer>`, `alt` a les imatges, `<label for>` als formularis i atribut
   `lang="ca"` (o l'idioma usat). Tota etiqueta contenedor hauria tenir
   intenció, no divs buits.
3. **CSS modern sense complicacions**: variables `var(--color)`;
   `display: grid/flex` (mai floats per layouts?); media queries amb
   `clamp()` per tipografia fluida; `prefers-reduced-motion` per acessibilitat.
4. **JavaScript**: `<script type="module" src="js/main.js" defer>`; `async/
   await` amb `fetch`; Orientació a esdeveniments (mai `onclick` inline);
   no toquis el DOM en bucle: agrupa els canvis en un `DocumentFragment` si
   en tens molts de cop.
5. **Compte del navegador**: sense dependències no necessàries. Si serveix
   vanilla, NO afegeixis frameworks. Si cal React/Vue, ho muntes amb esbuild
   o tregres-lo per CDN només si l'usuari ho demana.
6. **Prova-ho IMMEDIATAMENT** amb la Vista prèvia de NoOrbit (pestanya
   «Vista prèvia» del panell «Treball»): obre `index.html` commençant des
   de la carpeta del projecte; els canvis es reflecteixen en recarregant
   (Ctrl/Cmd-R a la vista prèvia).

La resta és regular: escriu el codi COMPLET al final, sense fragments a mana.
