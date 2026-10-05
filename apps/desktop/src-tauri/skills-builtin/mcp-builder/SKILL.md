---
name: mcp-builder
description: Disseny i implementació de servidors MCP (Model Context Protocol) amb eines ben descrites, maneig d'errors i avaluació d'ús real. Ús en crear o millorar un servidor MCP.
---

# Constructor de servidors MCP

## Abans de codar
1. Decideix l'abast: 5-10 eines cohesives són millors que 30 disperses.
2. Per a cada eina, respon: quina tasca real resol, quines entrades necessita
   un model (no un humà), i què retorna en cas d'error.

## Disseny d'eines
- **Noms**: verb-o-substantiu clar (`list_files`, no `mgr_opt`).
- **Descriptions**: què fan, quan usar-les, què NO fan. Són la documentació
  que llegeix el model per triar.
- **Paràmetres**: tipus simples i validables; enums per a opcions tancades;
  valors per defecte per a tot l'optatiu.
- **Sortides**: estructura previsible; missatges d'error que diuen com
  corregir-ho ("la ruta X no existeix; les disponibles són…").
- Evita eines que requereixin encadenar crides curts amb format fràgil;
  millor una eina que faci la feina sencera.

## Implementació
1. Fes servir l'SDK oficial del llenguatge (TypeScript/Python) i el transport
   adequat (stdio per a local, HTTP/SSE per a remot).
2. Valida totes les entrades; mai confiïs en el format del client.
3. Operacions llargues: progrés corrent o paginació, no bloquejos.
4. Seguretat: sense secrets a les descriptions, saneja rutes i consultes,
  principi de mínim privilegi.

## Avaluació
1. Escriu 5-10 tasques realistes i mesura si el model tria les eines
   correctes i les crida bé.
2. Si falla, primer ajusta descriptions i esquemes; després, redissenya
   les eines.
3. Prova casos d'error: recursos inexistents, permisos, entrades brutes.

## Checklist final
- [ ] El client (config de l'agent) el reconeix i arrenca
- [ ] Les eines apareixen i es poden cridar des d'un client real
- [ ] Errors llegibles i recuperables
- [ ] Cap secret commtat al repositori
