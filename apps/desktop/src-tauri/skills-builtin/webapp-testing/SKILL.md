---
name: webapp-testing
description: Provar aplicacions web amb Playwright: arrencar el servidor, simular usuaris, captures i verificacions E2E. Ús quan cal validar una interfície web o depurar fluxos de navegador.
---

# Proves d'aplicacions web

## Configuració
1. Fes servir Playwright (Python o Node). Un sol navegador headless per
   defecte; `--headed` i `slow_mo` només per depurar.
2. Arrenca el servidor de desenvolupament i ESPERA que estigui llest
   (sondeig del port amb reintent) abans de qualsevol navegació.
3. Fes servir `localhost`, mai la teva IP de xarxa (certificats i CORS).

## Flux de prova
1. Navega i després espera contingut concret:
   `page.get_by_role("button", name="Desa").wait_for()` — mai
   `sleep()` arbitraris.
2. Interactua amb selectors semàntics (`get_by_role`, `get_by_label`,
   `get_by_text`); els CSS/XPath fràgils només si no hi ha alternativa.
3. Verifica estat visible (text, comptadors, URL), no només que "no hi ha
   errors a la consola".
4. Fes captures (`page.screenshot`) davant de qualsevol fallada i anomena-les
   amb el nom de la prova; en cas d'error, mira la imatge abans de canviar
   res.

## Consola i xarxa
- Afegeix oients d'errors de pàgina i de `console.error` a cada prova: un
  sol error de JS pot invalidar tota la prova.
- Per a peticions: `page.wait_for_response` amb filtre d'URL; comprova
  codis d'estat, no només que arribi alguna cosa.

## Casos pràctics útils
- Formularis: prova camí feliç + validació en buit + camp invàlid.
- Resposta responsive: captures a 1280px i 375px i mesura el temps de
  càrrega amb l'API `performance`.
- Regressió visual: captures de referència versionades; compara amb diff
  de píxels i llindar explícit.

## Certificació
Una prova E2E només compta si (a) passa dues vegades seguides sense canvis
i (b) falla quan trenques intencionadament el comportament provat.
