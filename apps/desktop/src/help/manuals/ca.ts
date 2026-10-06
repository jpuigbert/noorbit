import type { Manual } from "..";

export const manual: Manual = {
  language: "Català",
  title: "Manual d'ús de NoOrbit",
  intro:
    "NoOrbit és un editor creatiu d'escriptori amb IA integrada. Reuneix, en una " +
    "sola finestra, un editor de codi, una vista prèvia del teu lloc o joc, un agent " +
    "d'IA multimodal, connexió amb Blender i una integració completa amb Unreal Engine 5. " +
    "Funciona amb models locals (Ollama) i, si vols, amb models al núvol (Ollama Cloud, " +
    "OpenAI i Claude). Tot és teu: pots instal·lar-lo al Mac o portar-lo en un USB.",

  sections: [
    {
      title: "1. Primers passos",
      intro: "Quan obres NoOrbit per primera vegada:",
      items: [
        "Tria un projecte recent a la pantalla de benvinguda, o prem «Obre una carpeta…» per començar. La drecera ⌘⌥R reobre l'últim projecte.",
        "Obre una carpeta de treball. Aquesta carpeta contindrà els teus projectes (lloc web, joc, scripts…).",
        "L'explorador de l'esquerra mostrarà els fitxers; fes clic a un fitxer per obrir-lo a l'editor.",
        "A la dreta tens el panell amb les pestanyes Vista prèvia, Agent, Especialistes, Ordinador, Blender i Unreal.",
        "La barra inferior indica l'estat: si Ollama, Blender i Unreal estan connectats (punts verds = connectats).",
        "Connecta un model d'IA (local o al núvol) abans de parlar amb l'agent: vegeu la secció 6.",
      ],
    },
    {
      title: "2. La interfície",
      intro:
        "NoOrbit reuseix l'estructura de menús del VS Code / Qoder perquè totes les " +
        "funcionalitats siguin on les esperes. Cada element del menú obre un desplegable " +
        "amb la seva drecera a la dreta.",
      items: [
        "Barra de menú (a dalt): Fitxer, Edita, Selecció, Visualitza, Ves, Executa, Terminal, IA i Ajuda — els mateixos menús que VS Code, més un de propi d'IA.",
        "Fitxer: nou fitxer de text, obre fitxer…, obre carpeta…, desa, desa com a…, desa-ho tot, tanca l'editor i surt.",
        "Edita: desfés/refés, retalla/copia/enganxa, cerca i substitueix, selecciona-ho tot i formata el document.",
        "Selecció: selecciona-ho tot, selecciona la següent coincidència i alterna el mode de selecció per columnes.",
        "Visualitza: paleta de comandes, ves al fitxer…, barra lateral, panell dret, ajust de línia, minimapa, zoom, i configuració.",
        "Ves: ves al fitxer…, a la línia… i al símbol…",
        "Executa: executa/atura la vista prèvia. Terminal: terminal nou del sistema.",
        "IA (menú propi): gestor de models, proveïdors (OpenAI/Claude/Ollama Cloud), connectors i skills, i dreceres als panells Agent, Ordinador i Unreal.",
        "Barra lateral esquerra: explorador d'arxius del projecte.",
        "Centre: editor de codi (Monaco, el mateix motor que VS Code).",
        "Panell dret: vista prèvia del projecte, agent d'IA, especialistes, control de l'ordinador i connectors Blender/Unreal.",
        "Barra d'estat (a baix): connexions actives (Ollama, Blender, Unreal) i, quan la IA treballa, un indicador animat «La IA està treballant…».",
      ],
    },
    {
      title: "3. Fitxers i editor de codi",
      items: [
        "Fitxer ▸ Nou fitxer de text (⌘N) crea un fitxer sense títol; en desar, te'l demanarà on (⌘⇧S / Desa com a…).",
        "Fitxer ▸ Obre fitxer… (⌘O) obre un fitxer concret; Fitxer ▸ Obre carpeta… (⌘K ⌘O) canvia la carpeta de treball.",
        "Fes clic a un fitxer de l'explorador per editar-lo. Una pestanya amb un punt indica que hi ha canvis sense desar.",
        "Desament automàtic: activat per defecte, desa el fitxer poc després d'escriure-hi, i també en canviar de fitxer o perdre el focus. Configura'l a Configuració ▸ General (retard 500–5000 ms).",
        "Per desar manualment: ⌘S, o el menú Fitxer ▸ Desa. Desa-ho tot amb ⌥⌘S.",
        "Ves al fitxer… (⌘P) obre un cercador ràpid de tots els fitxers; la paleta de comandes (⇧⌘P o F1) deixa buscar qualsevol comandament pel nom.",
        "L'editor reconeix automàticament el llenguatge segons l'extensió (.ts, .py, .rs, .html, .css, .json, .md…).",
      ],
    },
    {
      title: "4. Vista prèvia (web i jocs HTML)",
      intro:
        "Si el teu projecte és una pàgina web o un joc HTML5, pots veure'l en directe:",
      items: [
        "Menú Executa › Executa la vista prèvia (⌘R). NoOrbit arrenca un servidor local amb la carpeta oberta.",
        "La pestanya Vista prèvia del panell dret mostra el resultat.",
        "Menú Executa › Atura la vista prèvia per aturar el servidor.",
        "Si tens un fitxer index.html, aquest es carregarà com a pàgina inicial.",
      ],
    },
    {
      title: "5. L'agent d'IA (el xat)",
      intro:
        "L'agent converteix les teves idees en text, codi, imatges o escena 3D, i pot aplicar " +
        "els canvis directament a Blender o Unreal. Tot des d'un únic xat.",
      items: [
        "Obre la pestanya Agent al panell dret. Escriu la teva instrucció i prem Enter per enviar (⇧Enter fa salt de línia).",
        "Tria el model d'IA al desplegable del capdamunt: el que triïs l'usaran tots els xats, l'agent i el control de l'ordinador. La tria es recorda.",
        "Baixa models nous des del mateix xat: prem la icona de descàrrega, escriu el nom exacte (p. ex. llama3.2:3b) i prem Baixa; en acabar quedarà seleccionat.",
        "Executa: genera el resultat. Planifica: proposa un pla de passos sense executar-lo. Resposta ràpida: consulta de text immediata.",
        "Mentre la IA treballa veuràs el progrés en temps real (cada pas s'imprimeix al cronograma «Progrés») i un cronòmetre amb els segons que porta. Quan acaba, el resultat indica quant ha trigat.",
        "Procés de pensament: si el model el proporciona, apareix un bloc desplegable «Procés de pensament» amb el raonament de la IA abans de la resposta final.",
        "Aplica a Blender / Unreal: escript el que vols fer al xat i prem la barra «Aplica a» per executar-ho dins l'aplicació (seccions 9 i 10).",
        "Atura: prem «Atura» per avortar una generació en curs.",
        "Pensament profund: si una pàgina necessita JavaScript, la IA obre el navegador web intern i la llegeix ja renderitzada. Si el lloc demana iniciar sessió o un captcha, la finestra apareix en pantalla perquè la completes TU, amb el teu compte: la IA mai supla identitats.",
        "Navegador intern per a TOTES les IAs — sense cap botó: totes les IAs del xat (local, en línia o expert) poden navegar soles. La IA escriu una directriu «NB|OBRIR|url», «NB|LLEGIR|» o «NB|PREGUNTA_IA|deepseek|pregunta» al xat, NoOrbit l'executa i li'n torna el resultat perquè acabe la resposta. Si no tens cap clau d'API, la IA pot preguntar a una IA web (DeepSeek, DeepSeek Harness local amb «dsh web» en marxa, ChatGPT, Claude, Gemini, Perplexity o Grok): s'obre el navegador intern, TU hi inicies sessió amb el teu compte, i NoOrbit escriu la pregunta al xat web (ho veus tot) i en llegeix la resposta visible en pantalla, guardant-la amb l'origen etiquetat. Totalment legal: mai galetes, tokens ni sessions alienes. I si la IA web dona CODI, NoOrbit l'extreu en blocs (amb el llenguatge) i la IA del xat l'integra: l'adapta i l'escriu com a fitxers reals del projecte (format @file), sense cap clic teu.",
        "Generació d'imatges: amb ComfyUI en marxa i prou RAM lliure, es genera en local; si no, amb els proveïdors en línia que hagis registrat amb clau teva (p. ex. OpenAI «dall-e-3»). Les imatges es desen a la carpeta generated de NoOrbitData.",
        "Imatges al xat: qualsevol imatge generada o esmentada amb la seva ruta apareix previsualitzada dins la conversa, amb botons «Copiar» (al porta-retalls, com a PNG) i «Mostra» (al Finder/explorador).",
        "Revisió dels canvis de la IA (com a Qoder o VS Code): quan la IA escriu o modifica un fitxer, l'editor mostra el codi NOU en verd i el VELL en roig al mateix editor. Prem «Accepta» per deixar el nou i esborrar el vell, o «Rebutja» per tornar exactament al que hi havia abans. Si no fas res, al cap de 35 segons el canvi s'accepta sol i la barra de revisió desapareix.",
        "Mentre la IA encara està escrivint, l'editor en va mostrant el codi en directe com a previsualització; el compte enrere de 35 s arrenca quan la IA acaba aquell fitxer. Si la IA ha tocat varis fitxers que no tens oberts, una pastilla a la cantonada de l'editor els llista (amb el temps que queda) i permet «Accepta tot», «Rebutja tot» o saltar a cada fitxer.",
        "Desar un fitxer a mà (⌘S) mentre es revisa un canvi el dona per acceptat: mana la teva edició, igual que fa VS Code.",
        "Enganxar imatges al xat: prem ⌘V sobre l'entrada de text o usa el botó «Adjunta imatges» per triar fitxers. Es desen a NoOrbitData i els models amb visió (llava, gemma3, llama3.2-vision…) les reben com a entrada.",
      ],
    },
    {
      title: "6. Connectar la IA: Ollama, núvol, OpenAI i Claude",
      intro:
        "NoOrbit pot parlar amb models locals (Ollama) o amb serveis en línia. Obre el menú IA › " +
        "Gestor de models / Proveïdors per configurar-ho.",
      items: [
        "Ollama (local, gratuït): instal·la'l des de https://ollama.com i engega'l (escolta a localhost:11434). Els models es baixen al teu disc.",
        "Catàleg d'IAs (menú IA › Catàleg d'IAs): abans de descarregar res, consulta la mida aproximada de cada model, si és sense censura, si accepta imatges (visió), si en genera (imatges o vídeo) i si ja el tens instal·lat. Els botons «Baixa» descàrreguen directament.",
        "Models gratuïts al núvol d'Ollama (Ollama Cloud): a la secció de models veuràs una llista de models allotjats (gpt-oss, qwen3, deepseek-r1…) i un camp per escriure qualsevol altre nom del catàleg. Cal una clau gratuïta de https://ollama.com/settings/keys; no cal baixar res i l'ordinador no treballa: es resol remotament.",
        "Compte amb els noms semblants: la secció «Biblioteca d'Ollama (models per BAIXAR al disc)» descarrega el model i l'executa a la teva màquina; només la secció «Models gratuïts al núvol» treballa remotament. Si un model local triga minuts a respondre, el visor del procés t'ho explica (mida del model, RAM de l'equip i si encara el carrega o ja està generant).",
        "OpenAI: menú IA › Proveïdors › premeu el preset «OpenAI», enganxa la teva clau (sk-…) i tria el model (p. ex. gpt-4o-mini).",
        "Claude (Anthropic): premeu el preset «Claude», posa la teva clau (sk-ant-…) i el model (p. ex. claude-sonnet). El tipus (kind) ja ve com a «Anthropic».",
        "DeepSeek i Perplexity: presets d'un sol clic al mateix menú; només cal enganxar la clau del teu compte (api.deepseek.com / perplexity.ai). Sense clau registrada, NoOrbit mai consulta aquests serveis.",
        "Venice AI: premeu el preset «Venice AI», obre la teva compte a venice.ai (menú Setup › API Keys), crea una clau (comença per «ven-») i enganxa-la sencera al camp de la clau. La URL base és https://api.venice.ai/api/v1 i el model de la documentació és «venice-uncensored» (imatges: «venice-sd15»). NoOrbit neteja i normalitza tot això sol: espais de més, la barra final, la URL sense «/api/v1» o fins i tot l'endpoint «…/chat/completions» sencer. Si la clau falla, el missatge t'indicarà exactament per què (401 = clau no acceptada o compte sense crèdit; 404 = URL o model incorrectes; sense clau = t'ho diu abans d'enviar res).",
        "Cada proveïdor té un «Tipus» (OpenAI-compatible o Anthropic); deixa'l com ve ve donat pels presets. Pots provar la connexió amb el botó de test.",
        "Selecciona el proveïdor actiu: l'agent, els especialistes i el control de l'ordinador l'usaran a partir d'ara.",
        "Els models en USB extern: si tens Ollama amb models en un volum extern, NoOrbit el detecta (vegeu secció 14).",
        "DeepSeek Harness (dsh): el harness d'agents de codi obert de DeepSeek (github.com/deepseek-ai/deepseek-harness) NO és una API: és un agent complet, i per això té un menú APART al Gestor de proveïdors (secció «IAs harness»). Premeu «Instal·la DeepSeek Harness» i NoOrbit instal·la automàticament el Node.js LTS oficial (si no en tens) i el paquet «dsh» dins de les seves dades (o de NoOrbitData/ en mode portàtil/USB): sense permisos d'administrador, sense tocar res del sistema. Des d'ací també podeu «Arrenca la seva interfície» (la UI web de dsh, al port 3080) i «Obre la interfície» al navegador intern. Un cop instal·lat, NoOrbit el descobreix com a IA local externa, li delega tasques en mode headless quan la IA principal no les resol, i la IA li pot preguntar amb la directriu «NB|PREGUNTA_IA|dsh|pregunta». Cal tenir-hi configurat un proveïdor de model (a la seva interfície: Settings ▸ Models accepta DeepSeek o qualsevol API compatible amb OpenAI). Per xatejar amb la API de DeepSeek, en canvi, usa el preset «DeepSeek» dels proveïdors.",
      ],
    },
    {
      title: "7. Control de l'ordinador per la IA",
      intro:
        "Si li dones permís, la IA pot executar accions al teu ordinador (obrir programes, llistar o moure fitxers, executar comandes…). Tot passa per una cascada de seguretat i res s'executa sense el teu vist-i-plau.",
      items: [
        "Obre la pestanya Ordinador del panell dret.",
        "Activa l'interruptor «Permet que la IA controli l'ordinador». Mentre estigui desactivat, cap comanda es pot executar.",
        "Marca «Demana confirmació per cada comanda» perquè la IA t'ho demani sempre abans d'executar res.",
        "Quan la IA (o tu) sol·liciti una comanda, apareixerà un diàleg amb la comanda exacta: pots Permetre o Rebutjar, i marcar «Recorda'm sempre aquest programa».",
        "Els programes permesos es mostren com a etiquetes; pots revocar-ne el permís en qualsevol moment.",
        "Executa una comanda tu mateix: escriu-ne una (p. ex. ls -la) i prem Executa per provar el permís directament.",
        "L'historial recull tot el que s'ha executat (comanda, codi de sortida i sortida). Es pot netejar.",
        "Per seguretat, certes comandes destructives (sudo, esborrats massius, apagada) estan blocades i no s'executen mai.",
      ],
    },
    {
      title: "8. Especialistes i treball en equip",
      intro:
        "Els especialistes són agents d'IA que crees tu (o la mateixa IA), amb un rol, unes instruccions i (opcionalment) un model propi. Pots treballar sempre amb un especialista o fer que diversos experts treballin a la vegada sobre un mateix objectiu.",
      items: [
"La IA crea agents sola quan la tasca ho mereix: si li demanes alguna cosa complexa que barreja dominis (p. ex. una app amb base de dades i proves), la IA pot emetre la directriu «@agent: Nom | Rol | Instruccions» i NoOrbit crearà a l'instant aquest especialista al panell, com un expert real. Després l'usuari el pot triar, llançar en solitari o posar-lo en equip. No es dupliquen mai: si el nom ja existeix, es reutilitza.",
        "Obre la pestanya Especialistes al panell dret i prem Nou especialista.",
        "Dona-li un nom, un rol curt (p. ex. Desenvolupador front-end, Artista 3D, Redactor) i unes instruccions de sistema que defineixen com ha de treballar.",
        "Tria el model que usarà, o deixa «Usa el model actiu» perquè sigui el que hi hagi seleccionat al xat.",
        "Pots editar o eliminar qualsevol especialista amb les icones de llapis i paperera.",
        "Per treballar sempre amb un agent: a la pestanya Agent, al selector «Agent multimodal (per defecte)» tria un especialista; d'ara endavant Executa i Resposta ràpida els resoldrà ell. La tria es recorda.",
        "Treball múltiple en equip: a la pestanya Especialistes escriu un objectiu, marca quins hi participaran (o cap, per usar-los tots) i prem Treballa en equip.",
        "L'equip repartirà l'objectiu en subtasques, les executarà en paral·lel i després consolidarà una resposta final única.",
      ],
    },
    {
      title: "9. Blender (3D)",
      intro:
        "Connecta NoOrbit amb Blender per generar, editar i veure l'escena en directe, també des del xat.",
      items: [
        "Obre Blender al teu ordinador. La connexió es fa per un connector (add-on) que escolta al port 9876.",
        "NoOrbit accepta DOS add-on diferents al mateix port i detecta automàticament quin tens instal·lat: (a) el pont propi «NoOrbit Bridge» (menú IA o panell Blender: «Instal·la el connector»), o (b) l'add-on comunitari blender-mcp (github.com/ahujasid/mcp-for-blender), sense instal·lar res de NoOrbit.",
        "Si tries blender-mcp: instal·la'l des del repositori (requisits: Blender 4.2+ i Python 3.10+), activa l'add-on a Edit › Preferències › Add-ons i prem «Start MCP Server» al panell N de la vista 3D. NoOrbit li parla directament: no cal servidor MCP ni client extern, NoOrbit ja fa de client.",
        "Si no el tens: al panell Blender prem «Arrenca Blender» (ho fa amb el pont integrat) o «Instal·la el connector» perquè s'arrencui sol cada vegada.",
        "Quan està connectat, el punt Blender de la barra d'estat es posa verd.",
        "Vista en directe: activa-la per rebre fotogrames de l'escena de Blender dins NoOrbit.",
        "Executa script: envia codi Python (bpy) a Blender manualment.",
        "Edita des del text (nou): escriu al xat de l'Agent què vols fer i prem «Aplica a › Blender». La IA genera l'script bpy, l'executa a l'escena i, si dona error, ho torna a intentar corregint-lo. Veuràs cada pas en temps real.",
        "Exemples: «Crea un cub vermell i posa una llum a dalt», «Afegeix el text «HOLA» flotan a l'escena», «Mou l'objecte seleccionat 2 unitats a la dreta».",
      ],
    },
    {
      title: "10. Unreal Engine 5",
      intro:
        "La integració amb Unreal es fa amb la Remote Control API i el connector Python de l'editor.",
      items: [
        "Obre la pestanya Unreal del panell dret i selecciona el projecte (.uproject) que vols obrir.",
        "Connecta amb Unreal: NoOrbit detecta les instal·lacions, arrenca l'editor amb la Remote Control API activada i s'hi connecta.",
        "Requisits a l'editor: activa els connectors «Python Editor Script Plugin» i «Remote Control». El port per defecte és 30010.",
        "Accions ràpides: crea un BluePrint, genera un actor a l'escena, compila la il·luminació, captura el viewport o empaqueta el joc (RunUAT/BuildCookRun) amb la sortida en directe.",
        "Edita des del text (nou): escriu al xat de l'Agent què vols fer i prem «Aplica a › Unreal». La IA genera un script Python d'Editor Scripting, l'executa a l'editor i corregeix errors en un segon intent.",
        "Exemples: «Engendra un cub a l'origen i posa'l vermell», «Crea un actor de llum a (0,0,500)», «Importa malla.sfb com a asset».",
      ],
    },
    {
      title: "11. Connectors i Skills",
      items: [
        "Menú IA › Connectors: gestiona els connectors i skills disponibles.",
        "Els skills són paquets de capacitats (instruccions i scripts) que amplien el que pot fer l'agent.",
        "Es poden instal·lar des de GitHub, una URL, un fitxer .zip o una carpeta local.",
      ],
    },
    {
      title: "12. Aparença, temes i idiomes",
      intro: "NoOrbit està en català per defecte, permet canviar els colors i està preparat per traduir-se.",
      items: [
        "Menú Visualitza › Configuració › General: tria el tema entre Fosc, Blanc, Gris, Verd o Groc; el canvi és immediat i es recorda.",
        "El tema afecta tota la interfície i també l'editor de codi (Monaco).",
        "Menú Visualitza › Configuració › Idioma: baixa el catàleg en català (ca.json), tradueix els valors (mantén les claus) i puja'l amb «Afegeix un idioma des d'un fitxer».",
        "Tria l'idioma al desplegable «Idioma actual» per canviar-hi a l'instant.",
        "El Manual d'ús també es pot traduir afegint un fitxer nou a la carpeta src/help/manuals/.",
      ],
    },
    {
      title: "13. Instal·lació i portabilitat",
      intro: "Pots instal·lar NoOrbit al Mac o executar-lo des d'un USB sense deixar res al sistema.",
      items: [
        "Instal·lació normal: obre el .dmg i arrossega NoOrbit a la carpeta Aplicacions.",
        "Mode portàtil en USB: copia l'aplicació NoOrbit.app dins un volum extern (USB). En executar-la des de l'USB, la configuració, els proveïdors i les dades es guarden en una carpeta NoOrbitData al costat de l'app, no al teu Mac.",
        "Drecera: a la pantalla de benvinguda, ⌘⌥R reobre l'últim projecte.",
        "Els models d'Ollama poden viure en un USB: a Configura els models externs, NoOrbit detecta volums muntats i pot usar un directori de models extern.",
        "A la pantalla d'inici consta l'autoria: «Creat per Joan Puigbert — programari lliure, pots distribuir-lo i modificar-lo lliurement».",
      ],
    },
    {
      title: "14. Dreceres de teclat",
      items: [
        "⌘N: fitxer nou de text. ⌘O: obre fitxer. ⌘K ⌘O: obre carpeta.",
        "⌘S: desa. ⌘⇧S: desa com a…. ⌥⌘S: desa-ho tot. ⌘W: tanca l'editor.",
        "⌘Z / ⌘⇧Z: desfés / refés. ⌘X / ⌘C / ⌘V: retalla / copia / enganxa. ⌘A: selecciona-ho tot.",
        "⌘F: cerca. ⌥⌘F: substitueix. ⇧⌥F: formata el document. ⇧⌘D: selecciona la següent coincidència.",
        "⇧⌘P o F1: paleta de comandes. ⌘P: ves al fitxer. ⌃⌘G: ves a la línia. ⇧⌘O: ves al símbol.",
        "⌘B: barra lateral. ⌘J: panell dret. ⌥Z: ajust de línia.",
        "⌘= / ⌘- / ⌘0: amplia / redueix / restableix el zoom. ⌘,: Configuració.",
        "⌃⌘`: terminal nou. ⌘R: executa la vista prèvia. ⌘⌥R: reobre l'últim projecte.",
        "⌘↵: accepta el canvi de la IA que tens obert. ⌘⌫: el rebutja i restaura el codi anterior.",
      ],
    },
    {
      title: "15. Resolució de problemes",
      items: [
        "Ollama no apareix connectat: comprova que Ollama estigui engegat i que escolti a localhost:11434.",
        "La IA no respon amb un proveïdor en línia: revisa la clau (sk-… o sk-ant-…), el «Tipus» del proveïdor (OpenAI/Anthropic) i fa servir el botó de test.",
        "Blender no connecta: verifica que l'add-on estigui actiu i que el port 9876 no estigui ocupat. Prova «Arrenca Blender» des del panell.",
        "L'«Aplica a Blender/Unreal» falla: assegura't que l'aplicació és oberta i connectada; la IA fa un segon intent sol, però si l'escena no existeix no podrà editar-la.",
        "Unreal no connecta: assegura't que els connectors Python i Remote Control són actius i que el port 30010 és lliure. La primera connexió triga perquè arrenca l'editor.",
        "Memòria insuficient: el model és massa gran per al teu ordinador; tria'n un de més lleuger (p. ex. :3b o quantitzat q4).",
        "La vista prèvia no carrega: comprova que la carpeta oberta contingui un index.html o fitxers web.",
      ],
    },
    {
      title: "16. Exemples de prompts i coses a fer",
      intro:
        "Copya i enganxa qualsevol d'aquests textos al xat de l'Agent. Segons el que demanis, " +
        "prem «Executa», «Resposta ràpida» o, per a canvis reals, «Aplica a Blender/Unreal».",
      items: [
        "— Lloc web —",
        "«Fes una pàgina d'aterratge per a una cafeteria, amb hero, menú de productes i peu, en HTML i CSS».",
        "«Crea un formulari de contacte amb validació en JavaScript».",
        "«Afegeix un mode fosc amb un botó a aquesta pàgina».",
        "«Fes un rellotge digital en directe amb HTML+JS».",
        "— Joc HTML5 —",
        "«Fes un joc de Snake en un sol fitxer HTML amb canvas».",
        "«Afegeix puntuació i vides al meu joc de plataformes».",
        "«Fes un bot que segueixi el cursor amb suavitat».",
        "— Codi i scripts —",
        "«Escriu un script Python que reorganitzi els fitxers d'una carpeta per extensió».",
        "«Explica aquest fitxer i proposa millores de rendiment».",
        "«Genera proves unitàries per a aquesta funció».",
        "«Converteix aquest codi de JavaScript a TypeScript amb tipats».",
        "— Text i contingut —",
        "«Redacta un text de presentació de 150 paraules per al meu projecte».",
        "«Tradueix aquest paràgraf al català natural».",
        "«Resumeix aquest fitxer en 5 punts clau».",
        "— Blender (prem «Aplica a › Blender») —",
        "«Crea un cub vermell i posa una llum a sobre».",
        "«Afegeix el text «HOLA» flotant al centre de l'escena».",
        "«Mou l'objecte actiu 2 unitats cap a la dreta».",
        "«Fes una piràmide de 4 cares i aplica-li un material daurat».",
        "— Unreal (prem «Aplica a › Unreal») —",
        "«Engendra un cub a l'origen i pinta'l de vermell».",
        "«Crea un actor de llum directional a (0,0,500)».",
        "«Compta quants actors hi ha a l'escena i mostra-ho».",
        "— Equip d'especialistes —",
        "«Dissenya una botiga online: l'equip ha de fer l'estructura, els estils i el carret».",
        "«Creeu una landing page: un redactor fa els textos, un dissenyador els estils i un dev el formulari».",
      ],
    },
  ],
};
