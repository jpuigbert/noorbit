import type { Manual } from "..";

export const manual: Manual = {
  language: "Català",
  title: "Manual d'ús de NoOrbit",
  intro:
    "NoOrbit és un editor creatiu d'escriptori amb IA integrada. Reuneix, en una " +
    "sola finestra, un editor de codi, una vista prèvia del teu lloc o joc, un agent " +
    "d'IA multimodal, terminal integrat, control de canvis amb Git, connexió amb " +
    "Blender, Unreal Engine 5 i dispositius mòbils. Funciona amb models locals " +
    "(Ollama o llama.cpp) i, si vols, amb models al núvol (Ollama Cloud, OpenAI, " +
    "Claude i altres). Tot és teu: pots instal·lar-lo al Mac o portar-lo en un USB.",

  sections: [
    {
      title: "1. Primers passos",
      intro: "Quan obres NoOrbit per primera vegada:",
      items: [
        "L'assistent de benvinguda (3 passos) t'acompanya la primera vegada. El pots repetir quan vulguis des d'Ajuda ▸ Assistent de benvinguda (repeteix), o escrivint «benvinguda» a la paleta de comandes.",
        "Tria un projecte recent a la pantalla de benvinguda, o prem «Obre una carpeta…» per començar. «Nou projecte…» (⇧⌘N) crea una carpeta nova; ⌘⌥R reobre l'últim projecte.",
        "Comprovació de l'entorn: si falta alguna cosa, s'obre un assistent de 4 passos (Ollama, servei en marxa, model inicial segons la RAM de l'equip, i Blender/ComfyUI) que instal·la i engega el que falta, amb progrés real. Pots silenciar-lo amb «No tornar-la a mostrar» i reobrir-lo des de la barra d'estat.",
        "L'explorador de l'esquerra mostra els fitxers; fes clic a un fitxer per obrir-lo a l'editor.",
        "A la dreta tens el panell amb quatre pestanyes: Agent, IA de web, Vista prèvia i Treball (secció 2).",
        "La barra inferior indica l'estat: connexions (Ollama, Blender, Unreal), diagnòstics del projecte i avisos d'actualització.",
        "Connecta un model d'IA (local o al núvol) abans de parlar amb l'agent: vegeu la secció 7.",
      ],
    },
    {
      title: "2. La interfície",
      intro:
        "NoOrbit reutilitza l'estructura de menús del VS Code perquè totes les " +
        "funcionalitats siguin on les esperes. Cada element del menú obre un desplegable " +
        "amb la seva drecera a la dreta.",
      items: [
        "Barra de menú (a dalt): Fitxer, Edita, Selecció, Visualitza, Ves, Executa, Terminal, IA i Ajuda — els mateixos menús que VS Code, més un de propi d'IA. Al costat, botons Desfés/Refés sempre a mà i, quan la IA treballa, un indicador animat amb el que està fent.",
        "Fitxer: nou projecte, nou fitxer de text, obre fitxer…, obre carpeta…, projectes recents, desa, desa com a…, desa-ho tot, tanca l'editor i surt.",
        "Edita: desfés/refés, retalla/copia/enganxa, cerca i substitueix, selecciona-ho tot i formateja el document. Selecció: selecciona-ho tot, següent coincidència i mode de selecció per columnes.",
        "Visualitza: paleta de comandes, ves al fitxer…, barra lateral, panell dret, ajust de línia, minimapa, zoom, els sis temes de color i la configuració.",
        "Ves: ves al fitxer…, a la línia…, al símbol… i a la definició (F12). Executa: executa/atura la vista prèvia.",
        "Terminal: terminal integrat (⌘J), terminal nou del sistema i Control de canvis (Git).",
        "IA (menú propi): Gestor de models, Catàleg d'IAs, Hugging Bay, Pujar tema, Servidor d'IA remot, Skills i connectors, i dreceres als panells Agent, Ordinador, Especialistes, Unreal i Blender.",
        "Ajuda: Manual d'ús (aquesta finestra, amb la guia visual de dibuixos al capdamunt), repetir l'assistent de benvinguda i Quant a NoOrbit.",
        "Barra lateral esquerra: explorador d'arxius del projecte. Centre: editor de codi (Monaco, el mateix motor que VS Code).",
        "Panell dret, pestanya Agent: sub-pestanyes Xat, Especialistes, Aprenentatge, Ordinador, Unreal Engine, Blender i Mòbil natiu. Si un equip d'especialistes està treballant, una franja viva sobre el xat mostra què fan i, amb un clic, et porta al detall.",
        "Panell dret, pestanya Treball: sub-pestanyes Procés IA (el registre de tot el que fa la IA), Git, Problemes i Consola (secció 16).",
        "Panell dret, pestanyes IA de web (secció 8) i Vista prèvia (secció 5).",
        "A baix de tot, el terminal integrat (⌘J) i la barra d'estat: connexions actives (punts verds), comptador d'errors/avisos i, quan n'hi ha, l'avís d'actualització.",
        "Cada panell gran va amb la seva pròpia xarxa de seguretat: si un panell falla, només mor aquell panell i la resta de NoOrbit continua funcionant.",
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
        "Ves al fitxer… (⌘P) obre un cercador ràpid de tots els fitxers; la paleta de comandes (⇧⌘P o F1) deixa buscar qualsevol comandament pel nom; Cerca al projecte… (⇧⌘F) busca text a tots els fitxers.",
        "L'editor reconeix automàticament el llenguatge segons l'extensió (.ts, .py, .rs, .html, .css, .json, .md…). Les imatges es previsualitzen dins l'editor, amb botons «Obri» (aplicació del sistema) i «Mostra» (Finder/explorador).",
        "— Autocompletat amb IA —",
        "Mentre escrius, la IA activa (local o remota) proposa continuacions en gris, com l'autocompletat de VS Code. S'accepta amb Tab. El botó «IA» / «IA off» de la barra de pestanyes de l'editor l'activa o desactiva en viu i es recorda.",
        "L'autocompletat no bloqueja mai el teclejat: espera una pausa, té temps límit i, si continues escrivint, la inferència anterior es cancel·la de debò al servidor (no gasta CPU/GPU de més).",
        "— Correccions ràpides —",
        "⌘1 (o Ctrl+1) obre el menú de correccions ràpides sobre l'error: les accions natives de Monaco (TypeScript/JS/JSON/CSS/HTML) i les pròpies de NoOrbit per a claus, claudàtors i cadenes sense tancar (Python, Rust, C…): afegir el tancament que falta o suprimir el que sobra.",
        "⇧⌘1 (o ⇧Ctrl+1): la IA repara la línia on tens el cursor i aplica la correcció. Útil per a llenguatges sense worker de Monaco.",
        "— Editar la selecció amb la IA —",
        "Selecciona un fragment i prem ⇧⌘2 o el botó «IA edita» de la barra de l'editor. Escriu què vols («corregeix», «refactoritza», «documenta»…) o tria un dels suggeriments d'un clic; la IA retorna el fragment nou al seu lloc.",
        "El resultat entra pel mateix flux de revisió que totes les edicions de la IA: diff verd/roig amb «Accepta»/«Rebutja» (secció 4). Res es fica davant sense que ho vegis.",
      ],
    },
    {
      title: "4. Revisió dels canvis de la IA (verd/roig)",
      intro:
        "Quan la IA escriu o modifica un fitxer — des del xat, amb «IA edita» o amb una " +
        "correcció ràpida — el canvi no es dona per bo: es revisa com a Qoder o VS Code.",
      items: [
        "L'editor mostra el codi NOU en verd i el VELL en roig al mateix editor (diff en línia). Prem «Accepta» per deixar el nou i esborrar el vell, o «Rebutja» per tornar exactament al que hi havia abans.",
        "Si no fas res, al cap de 35 segons el canvi s'accepta sol i la barra de revisió desapareix. El compte enrere arrenca quan la IA acaba d'escriure aquell fitxer.",
        "Dreceres: ⌘↵ accepta el canvi del fitxer obert; ⌘⌫ el rebutja i restaura el codi anterior.",
        "Mentre la IA encara està escrivint, l'editor en va mostrant el codi en directe com a previsualització.",
        "Si la IA ha tocat diversos fitxers que no tens oberts, una pastilla a la cantonada de l'editor els llista (amb el temps que queda) i permet «Accepta tot», «Rebutja tot» o saltar a cada fitxer.",
        "Desar o editar a mà (⌘S o teclejar) mentre es revisa un canvi el dona per acceptat: mana la teva edició, igual que fa VS Code.",
      ],
    },
    {
      title: "5. Vista prèvia (web i jocs HTML)",
      intro:
        "Si el teu projecte és una pàgina web o un joc HTML5, pots veure'l en directe:",
      items: [
        "Menú Executa ▸ Executa la vista prèvia (⌘R), o el botó «Inicia la vista prèvia» de la pestanya Vista prèvia. NoOrbit arrenca un servidor local amb la carpeta oberta.",
        "La pestanya Vista prèvia del panell dret mostra el resultat, amb la URL del servidor i el botó «Atura».",
        "Menú Executa ▸ Atura la vista prèvia per aturar el servidor.",
        "Si tens un fitxer index.html, aquest es carregarà com a pàgina inicial.",
        "Seguretat: el servidor només serveix fitxers de DINS de la carpeta del projecte (mai fitxers del sistema), demana un token propi i rebutja orígens aliens: ningú de fora pot llegir el teu projecte.",
      ],
    },
    {
      title: "6. L'agent d'IA (el xat)",
      intro:
        "L'agent converteix les teves idees en text, codi, imatges o escena 3D, i pot aplicar " +
        "els canvis directament a Blender o Unreal. Tot des d'un únic xat, amb diverses " +
        "converses alhora.",
      items: [
        "— Converses —",
        "Obre la pestanya Agent ▸ Xat. Escriu la instrucció i prem Enter per enviar (⇧Enter fa salt de línia).",
        "Cada projecte té els seus propis xats, amb historial desat: el botó «+» (Nou xat) n'obre un de nou i les pestanyes de dalt permeten saltar entre converses o tancar-les. Mentre una IA treballa, pots obrir un xat en paral·lel sense aturar-la.",
        "«Forca» (fork): en qualsevol missatge, duplica la conversa fins a aquell punt en un xat NOU per continuar amb una altra IA o un altre especialista; l'original queda intacte.",
        "Tria el model d'IA al desplegable del capdamunt: el que triïs l'usaran tots els xats, l'agent i el control de l'ordinador. La tria es recorda.",
        "Baixa models nous des del mateix xat: prem la icona de descàrrega, escriu el nom exacte (p. ex. llama3.2:3b) i prem Baixa; en acabar quedarà seleccionat.",
        "— Modes i progrés —",
        "Executa: genera el resultat i aplica els canvis al projecte. Planifica: proposa un pla de passos sense executar-lo. Resposta ràpida: consulta de text immediata.",
        "Mentre la IA treballa veuràs un cronòmetre i la línia «què està fent la IA»; el detall de cada pas (context, connexions, comandes) queda al registre de Treball ▸ Procés IA, no al xat. Quan acaba, el resultat indica quant ha trigat.",
        "Procés de pensament: si el model el proporciona, apareix un bloc desplegable amb el raonament de la IA (en català, amb alternança «Original» per veure'l com va sortir).",
        "Atura: prem «Atura» per avortar una generació en curs.",
        "— Llista de tasques i delegació —",
        "Sobre el xat hi ha la «Llista de tasques», compartida i desada per projecte. La IA l'omple amb «@todo:» i marca les fetes amb «@done:»; amb «@doing:» mostra EN VIU quina tasca té entre mans. Tu també hi pots afegir tasques, marcar-les, treure-les o buidar-la.",
        "La llista es pot amagar (queda una franja prima amb el comptador i el que fa la IA) i es recorda entre sessions. S'injecta al context de la IA principal i dels agents delegats perquè ningú es perdi cap tasca.",
        "«@use: NomAgent | tasca»: la IA principal delega en un especialista local dins del mateix torn i integra la solució a la resposta final.",
        "«@agent: Nom | Rol | Instruccions»: la IA crea especialistes sola quan la tasca ho mereix (secció 10).",
        "— Fitxers, codi i imatges —",
        "Quan la IA respon codi amb «@file: ruta», els fitxers s'escriuen sols al projecte i passen per la revisió verd/roig (secció 4). Els blocs de codi sense nom es desen a la carpeta codi_ia/.",
        "Si la conversa es fa molt llarga, el context es compacta (objectiu, pla, esquelet del codi) i la IA continua a trossos, en lloc de retallar i perdre el fil.",
        "Aplica a Blender / Unreal: escriu el que vols fer al xat i prem la barra «Aplica a» per executar-ho dins l'aplicació (seccions 11 i 12).",
        "Generació d'imatges: amb ComfyUI en marxa i prou RAM lliure, es genera en local; si no, amb els proveïdors en línia que hagis registrat amb clau teva (p. ex. OpenAI «dall-e-3»). Les imatges es desen a la carpeta generated de NoOrbitData.",
        "Imatges al xat: qualsevol imatge generada o esmentada amb la seva ruta apareix previsualitzada dins la conversa, amb botons «Copiar» (al porta-retalls, com a PNG) i «Mostra» (al Finder/explorador).",
        "Enganxar imatges al xat: prem ⌘V sobre l'entrada de text o usa el botó «Adjunta imatges» per triar fitxers. Es desen a NoOrbitData i els models amb visió (llava, gemma3, llama3.2-vision…) les reben com a entrada.",
        "— Navegador intern per a totes les IAs —",
        "Totes les IAs del xat (local, en línia o expert) poden navegar soles, sense cap botó: la IA escriu una directriu «NB|OBRIR|url», «NB|LLEGIR|» o «NB|PREGUNTA_IA|deepseek|pregunta», NoOrbit l'executa i li'n torna el resultat perquè acabi la resposta. Si una pàgina necessita JavaScript, la llegeix ja renderitzada.",
        "Si el lloc demana iniciar sessió o un captcha, la finestra apareix en pantalla perquè la completes TU, amb el teu compte: la IA mai suplanta identitats.",
        "Sense cap clau d'API, la IA pot preguntar a una IA web (DeepSeek, ChatGPT, Claude, Gemini, Perplexity, Grok o el DeepSeek Harness local): s'obre el navegador intern, TU hi inicies sessió, i NoOrbit escriu la pregunta al xat web (ho veus tot) i en llegeix la resposta visible en pantalla, guardant-la amb l'origen etiquetat. Totalment legal: mai galetes, tokens ni sessions alienes.",
        "Si la IA web dona CODI, NoOrbit l'extreu en blocs (amb el llenguatge) i la IA del xat l'integra: l'adapta i l'escriu com a fitxers reals del projecte (format @file), sense cap clic teu.",
      ],
    },
    {
      title: "7. Connectar la IA: Ollama, núvol, OpenAI i Claude",
      intro:
        "NoOrbit pot parlar amb models locals (Ollama o llama.cpp) o amb serveis en línia. " +
        "Obre el menú IA ▸ Gestor de models / Proveïdors per configurar-ho.",
      items: [
        "— Local —",
        "Ollama (local, gratuït): instal·la'l des de https://ollama.com i engega'l (escolta a localhost:11434). Els models es baixen al teu disc. Si no tens cap model, NoOrbit et proposa baixar-ne un d'inicial amb mida segons la RAM.",
        "Catàleg d'IAs (menú IA ▸ Catàleg d'IAs): abans de descarregar res, consulta la mida aproximada de cada model, si és sense censura, si accepta imatges (visió), si en genera (imatges o vídeo) i si ja el tens instal·lat. Els botons «Baixa» descàrreguen directament.",
        "Hugging Bay (menú IA ▸ Hugging Bay): cerca de models GGUF al mirror verificat, amb detalls i SHA-256. La descàrrega es fa a un fitxer «.part» i es verifica amb el SHA-256 abans de donar-lo per bo: una descàrrega interrompuda o corrupta es descarta amb un missatge clar i es pot tornar a baixar. El destí es tria des del mateix modal («Tria carpeta / disc extern…») i el model queda registrat a Ollama.",
        "Sense Ollama: el gestor de models pot engegar els GGUF baixats amb llama-server (llama.cpp, binaris oficials per a macOS, Windows i Linux); el servei es registra sol com a proveïdor local sense clau. Sense Ollama, tot funciona: el xat cau a la IA en línia si en tens.",
        "— Núvol —",
        "Models gratuïts al núvol d'Ollama (Ollama Cloud): a la secció de models veuràs una llista de models allotjats (gpt-oss, qwen3, deepseek-r1…) i un camp per escriure qualsevol altre nom del catàleg. Cal una clau gratuïta de https://ollama.com/settings/keys; no cal baixar res i l'ordinador no treballa: es resol remotament.",
        "Compte amb els noms semblants: la secció «Biblioteca d'Ollama (models per BAIXAR al disc)» descarrega el model i l'executa a la teva màquina; només la secció «Models gratuïts al núvol» treballa remotament. Si un model local triga minuts a respondre, el visor del procés t'ho explica (mida del model, RAM de l'equip i si encara el carrega o ja està generant).",
        "OpenAI: menú IA ▸ Proveïdors ▸ premeu el preset «OpenAI», enganxa la teva clau (sk-…) i tria el model (p. ex. gpt-4o-mini).",
        "Claude (Anthropic): premeu el preset «Claude», posa la teva clau (sk-ant-…) i el model (p. ex. claude-sonnet). El tipus (kind) ja ve com a «Anthropic».",
        "DeepSeek i Perplexity: presets d'un sol clic al mateix menú; només cal enganxar la clau del teu compte (api.deepseek.com / perplexity.ai). Sense clau registrada, NoOrbit mai consulta aquests serveis.",
        "Venice AI: premeu el preset «Venice AI», obre el teu compte a venice.ai (menú Setup ▸ API Keys), crea una clau (comença per «ven-») i enganxa-la sencera al camp de la clau. La URL base és https://api.venice.ai/api/v1 i el model de la documentació és «venice-uncensored» (imatges: «venice-sd15»). NoOrbit neteja i normalitza tot això sol: espais de més, la barra final, la URL sense «/api/v1» o fins i tot l'endpoint «…/chat/completions» sencer. Si la clau falla, el missatge t'indicarà exactament per què (401 = clau no acceptada o compte sense crèdit; 404 = URL o model incorrectes; sense clau = t'ho diu abans d'enviar res).",
        "Cada proveïdor té un «Tipus» (OpenAI-compatible o Anthropic); deixa'l com ve donat pels presets. Pots provar la connexió amb el botó de test.",
        "Selecciona el proveïdor actiu: l'agent, els especialistes i el control de l'ordinador l'usaran a partir d'ara.",
        "— Màquines remotes i harness —",
        "Servidor d'IA remot (Proveïdors ▸ secció «Servidor d'IA remot»): si tens una GPU en una altra màquina, omple host, usuari i contrasenya, tria SFTP, FTPS o FTP, la carpeta remota i el motor (DeepInfer o vLLM) i prem «Puja al servidor»: pujarà l'script d'instal·lació i una API OpenAI-compatible en Python/FastAPI. Després, per SSH, executa al servidor «./noorbit-ai-server.sh» (la comanda exacta la mostra el resultat); quan el script digui «Servidor EN MARXA», prem «Comprova» (si el model encara es carrega, t'ho dirà i només cal tornar a provar) i «Connecta» per afegir-lo com a proveïdor. Les IAs del servidor treballen allà i només respostes i canvis tornen al Mac: la RAM d'inferència és zero al local. El FTP passa la contrasenya en clar: fes servir SFTP si pots.",
        "DeepSeek Harness (dsh): el harness d'agents de codi obert de DeepSeek (github.com/deepseek-ai/deepseek-harness) NO és una API: és un agent complet, i per això té un menú APART al Gestor de proveïdors (secció «IAs harness»). Premeu «Instal·la DeepSeek Harness» i NoOrbit instal·la automàticament el Node.js LTS oficial (si no en tens) i el paquet «dsh» dins de les seves dades (o de NoOrbitData/ en mode portàtil/USB): sense permisos d'administrador, sense tocar res del sistema. Des d'ací també podeu «Arrenca la seva interfície» (la UI web de dsh, al port 3080) i «Obre la interfície» al navegador intern. Un cop instal·lat, NoOrbit el descobreix com a IA local externa, li delega tasques en mode headless quan la IA principal no les resol, i la IA li pot preguntar amb la directriu «NB|PREGUNTA_IA|dsh|pregunta». Cal tenir-hi configurat un proveïdor de model (a la seva interfície: Settings ▸ Models accepta DeepSeek o qualsevol API compatible amb OpenAI). Per xatejar amb la API de DeepSeek, en canvi, usa el preset «DeepSeek» dels proveïdors.",
        "DeepInfer (github.com/deepseek-ai/DeepInfer, Apache 2.0): el motor d'inferència de producció de DeepSeek. NO corre en un Mac: requereix Linux + GPU NVIDIA SM100 amb CUDA 13 o NPU Ascend 950; s'instal·la en un servidor Linux (Python 3.12, ./build.sh) i, si hi poses un servei vLLM al davant, a NoOrbit ja tens el proveïdor «DeepInfer (servidor GPU)» per editar-ne la URL i activar-lo. Si la màquina remota ara mateix no la tens, consulta la skill «DeepInfer (DeepSeek, motor local)» o activa «DeepSeek API (núvol)» amb la teva clau.",
        "Els models d'Ollama poden viure en un USB extern: NoOrbit ho detecta i ho configura sol (vegeu secció 19).",
      ],
    },
    {
      title: "8. IA de web: xats amb el teu compte",
      intro:
        "La pestanya «IA de web» del panell dret és el punt d'entrada per parlar amb les " +
        "IAs oficials per Internet sense cap clau d'API: la sessió és el TEU compte, com si " +
        "ho fessis tu.",
      items: [
        "Cada targeta (DeepSeek, ChatGPT, Claude, Gemini, Perplexity, Grok, dsh…) té dos botons: «Xat-hi» i «Sessió».",
        "«Xat-hi» obre un xat DELS DE SEMPRE a la pestanya Agent (amb historial, forks i delegacions) però amb proveïdor web: la pregunta viatja al navegador intern VISIBLE, NoOrbit l'escriu al xat de la pàgina i la resposta que la pàgina mostra torna al teu xat.",
        "«Sessió» obre el navegador intern a la IA perquè hi iniciïs sessió amb el teu compte abans de preguntar. Si la IA demana sessió o captcha mentre treballes, la finestra apareix en pantalla perquè la completis TU.",
        "La IA web té accés a Internet: pots fer cerques i preguntes d'actualitat que la IA local no pot respondre.",
        "Quan la IA web dona codi, NoOrbit en treu els blocs i la IA del xat els integra al projecte si val la pena (mateixa integració que amb la IA local, secció 6).",
        "Mai es fan servir galetes, tokens ni sessions alienes: tot passa pel teu compte i ho veus tot en pantalla.",
      ],
    },
    {
      title: "9. Control de l'ordinador per la IA",
      intro:
        "Si li dones permís, la IA pot executar accions al teu ordinador (obrir programes, llistar o moure fitxers, executar comandes…). Tot passa per una cascada de seguretat i res s'executa sense el teu vist-i-plau.",
      items: [
        "Obre Agent ▸ Ordinador al panell dret.",
        "Activa l'interruptor «Permet que la IA controli l'ordinador». Mentre estigui desactivat, cap comanda es pot executar.",
        "Marca «Demana confirmació per cada comanda» perquè la IA t'ho demani sempre abans d'executar res.",
        "Quan la IA (o tu) sol·liciti una comanda, apareixerà un diàleg amb la comanda exacta: pots Permetre o Rebutjar, i marcar «Recorda'm sempre aquest programa». El record «per sempre» no s'autoamplia amb encadenaments (npm … && curl …): cada programa nou es torna a demanar.",
        "Els programes permesos es mostren com a etiquetes; pots revocar-ne el permís en qualsevol moment.",
        "Executa una comanda tu mateix: escriu-ne una (p. ex. ls -la) i prem Executa per provar el permís directament.",
        "L'historial recull tot el que s'ha executat (comanda, codi de sortida i sortida). Es pot netejar.",
        "Per seguretat, certes comandes destructives (sudo, esborrats massius, apagada) estan blocades i no s'executen mai.",
      ],
    },
    {
      title: "10. Especialistes i treball en equip",
      intro:
        "Els especialistes són agents d'IA que crees tu (o la mateixa IA), amb un rol, unes instruccions i (opcionalment) un model propi. Pots treballar sempre amb un especialista o fer que diversos experts treballin a la vegada sobre un mateix objectiu.",
      items: [
        "La IA crea agents sola quan la tasca ho mereix: si li demanes alguna cosa complexa que barreja dominis (p. ex. una app amb base de dades i proves), la IA pot emetre la directriu «@agent: Nom | Rol | Instruccions» i NoOrbit crearà a l'instant aquest especialista al panell, com un expert real. No es dupliquen mai: si el nom ja existeix, es reutilitza.",
        "Obre Agent ▸ Especialistes i prem Nou especialista.",
        "Dona-li un nom, un rol curt (p. ex. Desenvolupador front-end, Artista 3D, Redactor) i unes instruccions de sistema que defineixen com ha de treballar.",
        "Tria el model que usarà, o deixa «Usa el model actiu» perquè sigui el que hi hagi seleccionat al xat.",
        "Pots editar o eliminar qualsevol especialista amb les icones de llapis i paperera.",
        "Per treballar sempre amb un agent: a la pestanya Xat, al selector «Agent multimodal (per defecte)» tria un especialista; d'ara endavant Executa i Resposta ràpida els resoldrà ell. La tria es recorda.",
        "La IA principal també pot delegar en un especialista dins del mateix torn amb «@use: NomAgent | tasca» (secció 6).",
        "Treball múltiple en equip: a Especialistes escriu un objectiu, marca quins hi participaran (o cap, per usar-los tots) i prem Treballa en equip. També pots crear un equip ràpid (Arquitecte/Programador/Revisor) des del xat.",
        "L'equip reparteix l'objectiu en subtasques i consolida una resposta final única. Amb models locals les subtasques es fan per TORNS (un acaba → comença l'altre, alliberant el model entre torns per no esgotar la RAM); amb models del núvol, fins a tres alhora.",
        "El que cada expert escriu es veu EN DIRECTE en mini-xats per sessió, amb el mateix format del xat i botó «Obre al xat». La franja viva sobre el xat mostra sempre que l'equip està treballant i l'últim pas.",
        "Pots aturar-lo quan vulguis amb «Atura l'equip»: es mostren els resultats que ja havien acabat.",
      ],
    },
    {
      title: "11. Blender (3D)",
      intro:
        "Connecta NoOrbit amb Blender per generar, editar i veure l'escena en directe, també des del xat.",
      items: [
        "Obre Blender al teu ordinador. La connexió es fa per un connector (add-on) que escolta al port 9876 (configurable; el port triat s'injecta també a l'script de Python).",
        "NoOrbit accepta DOS add-on diferents al mateix port i detecta automàticament quin tens instal·lat: (a) el pont propi «NoOrbit Bridge» (menú IA o panell Blender: «Instal·la el connector»), o (b) l'add-on comunitari blender-mcp (github.com/ahujasid/mcp-for-blender), sense instal·lar res de NoOrbit.",
        "Si tries blender-mcp: instal·la'l des del repositori (requisits: Blender 4.2+ i Python 3.10+), activa l'add-on a Edit ▸ Preferències ▸ Add-ons i prem «Start MCP Server» al panell N de la vista 3D. NoOrbit li parla directament: no cal servidor MCP ni client extern, NoOrbit ja fa de client.",
        "Si no el tens: al panell Blender prem «Arrenca Blender» (ho fa amb el pont integrat) o «Instal·la el connector» perquè s'arrencui sol cada vegada.",
        "Quan està connectat, el punt Blender de la barra d'estat es posa verd.",
        "Vista en directe: activa-la per rebre fotogrames de l'escena de Blender dins NoOrbit.",
        "Executa script: envia codi Python (bpy) a Blender manualment.",
        "Edita des del text: escriu al xat de l'Agent què vols fer i prem «Aplica a ▸ Blender». La IA genera l'script bpy (amb import bpy automàtic), l'executa a l'escena i, si dona error, ho torna a intentar corregint-lo. Veuràs cada pas en temps real.",
        "Vídeo i àudio: la IA pot generar música amb numpy a l'editor de seqüències, veu amb el TTS del sistema (say/espeak-ng/SAPI), boca animada amb keyframes i àudio AAC al render MP4.",
        "Exemples: «Crea un cub vermell i posa una llum a dalt», «Afegeix el text «HOLA» flotant a l'escena», «Mou l'objecte seleccionat 2 unitats a la dreta».",
      ],
    },
    {
      title: "12. Unreal Engine 5",
      intro:
        "La integració amb Unreal es fa amb la Remote Control API i el connector Python de l'editor.",
      items: [
        "Obre Agent ▸ Unreal Engine al panell dret i selecciona el projecte (.uproject) que vols obrir; NoOrbit el cerca automàticament si no el tries.",
        "Connecta amb Unreal: NoOrbit detecta les instal·lacions, parcheja el .uproject amb PythonScriptPlugin + RemoteControl (guardant una còpia .bak), arrenca l'editor amb la Remote Control API activada i s'hi connecta. Un watchdog comprova la connexió cada 5 s.",
        "Requisits a l'editor: connectors «Python Editor Script Plugin» i «Remote Control». El port per defecte és 30010.",
        "Accions ràpides: crea un BluePrint, genera un actor a l'escena, compila la il·luminació, captura el viewport o empaqueta el joc (RunUAT/BuildCookRun) amb la sortida en directe.",
        "Edita des del text: escriu al xat de l'Agent què vols fer i prem «Aplica a ▸ Unreal». La IA genera un script Python d'Editor Scripting, l'executa a l'editor i corregeix errors en un segon intent.",
        "Exemples: «Engendra un cub a l'origen i posa'l vermell», «Crea un actor de llum a (0,0,500)», «Importa malla.fbx com a asset».",
      ],
    },
    {
      title: "13. Mòbil natiu: Android i iOS",
      intro:
        "La sub-pestanya «Mòbil natiu» de l'Agent detecta els SDKs del teu ordinador i et " +
        "deixa engegar emuladors i veure'ls en directe sense sortir de NoOrbit.",
      items: [
        "Dues sub-pestanyes: Android i iOS. El botó de tornar a escanejar detecta els SDKs instal·lats i l'estat de cada plataforma.",
        "Android: cal l'Android SDK (Android Studio o cmdline-tools) amb ANDROID_HOME exportat. NoOrbit llista els AVD existents; «Engega» arrenca l'emulador, «Atura» el tanca. Si no tens cap AVD, t'indica com crear-ne un (Android Studio ▸ Device Manager o avdmanager).",
        "iOS (només macOS): cal Xcode i les eines de línia de comandes (xcode-select --install). NoOrbit llista els simuladors per versió d'iOS; «Engega» els arrenca, «Obre Simulator» porta la finestra de Simulator.app al davant i «Atura» els apaga. Si falta un runtime, t'indica Xcode ▸ Settings ▸ Platforms ▸ iOS.",
        "«Veure» activa la vista en directe del dispositiu: captures de pantalla de l'emulador/simulador dins NoOrbit, amb botó per aturar-la.",
        "Codi natiu: la IA del xat pot escriure i editar Kotlin/Swift del projecte; per compilar i instal·lar al dispositiu calen les eines del SDK (Gradle/Xcode), i el panell t'ho recorda.",
      ],
    },
    {
      title: "14. Terminal integrat",
      items: [
        "⌘J obre i tanca el terminal integrat a la part inferior (com VS Code); Terminal ▸ Terminal nou (^⌘`) obre un terminal del sistema.",
        "Escriu una comanda (ls, python main.py, git status…) i prem Intro: s'executa DINS de la carpeta del projecte obert i la sortida es mostra mentre va arribant.",
        "↑ i ↓ naveguen per l'historial de comandes.",
        "Mentre una comanda s'executa, el botó «Atura la comanda» la mata (tot el grup de processos) i les comandes tenen temps límit: res no queda penjat.",
        "El botó de goma neteja la pantalla; la fletxa avall tanca el panell.",
        "La IA també pot executar comandes pel seu compte, però només amb el permís del control de l'ordinador (secció 9).",
      ],
    },
    {
      title: "15. Git i núvol d'equip",
      intro:
        "El panell de Git (Treball ▸ Git, o Terminal ▸ Control de canvis) és la vista de " +
        "control de canvis de VS Code, i a més connecta el projecte al núvol per editar-lo " +
        "des de diversos dispositius.",
      items: [
        "Si la carpeta no és un repositori, «Inicia el repositori» el crea amb un .gitignore bàsic.",
        "La llista de canvis mostra cada fitxer amb el seu estat (M modificat, A afegit, D esborrat…). Fes clic a un fitxer per veure'n el diff costat a costat (verd/roig, com VS Code) i, si vols, el pedaç en text pla.",
        "Escriu el missatge i prem «Commit» per committar els canvis; les fletxes ↑ i ↓ fan Push i Pull.",
        "Stash: el botó d'arxivar guarda els canvis sense committar («Guarda els canvis (stash)») i els pots recuperar tots o un a un («Recupera»).",
        "«Historial de branques» mostra el gràfic de commits i branques (git log --graph).",
        "— Núvol d'equip —",
        "Enganxa la URL d'un repositori (GitHub, GitLab, el teu servidor…) i prem «Connecta»: el projecte queda lligat al remot origin.",
        "Les pastilles ↑ i ↓ indiquen quants commits queden per pujar o baixar respecte dels altres dispositius.",
        "«Sincronitza» ho fa tot en una passada: commit dels teus canvis + baixa el que els altres han fet + puja el teu treball.",
        "«Auto» repeteix la sincronització cada 90 s mentre ningú més estigui treballant; la decisió es recorda per projecte i per dispositiu.",
        "Qualsevol NoOrbit d'un altre dispositiu que apunti al mateix repositori pot editar el projecte: el Git és el núvol d'equip.",
      ],
    },
    {
      title: "16. Treball: Procés IA, Problemes i Consola",
      intro:
        "La pestanya Treball del panell dret reuneix el «darrere l'escenari» del projecte " +
        "en un sol lloc, perquè el xat quedi net només amb les respostes.",
      items: [
        "Procés IA: el registre en temps real de tot el que fa la IA (context, passos, connexions, comandes, errors i reintents), amb el temps que porta cada tasca.",
        "Problemes: NoOrbit revisa en viu tant el codi que escrius a mà com el que genera la IA (encara que no obris el fitxer): claus sense tancar, cadenes obertes i errors de sintaxi (TypeScript/JS/JSON i el motor lleuger per a Python/Rust/C…).",
        "Els diagnòstics s'agrupen per fitxer; fer clic a un problema obre el fitxer i salta a la línia i columna marcades. El comptador d'errors/avisos de la barra d'estat porta directament aquí.",
        "Des d'un error, ⌘1 aplica les correccions ràpides i ⇧⌘1 demana a la IA que repari la línia (secció 3).",
        "Consola: el registre de depuració de l'aplicació, per si alguna cosa no va fina i vols veure què ha passat.",
      ],
    },
    {
      title: "17. Connectors i Skills",
      items: [
        "Menú IA ▸ Skills i connectors: gestiona els skills instal·lats («Desinstal·la», «Torna a carregar»).",
        "Els skills són paquets de capacitats (instruccions SKILL.md i scripts) que amplien el que pot fer l'agent; el format és compatible amb Claude Code i OpenCode.",
        "«Instal·la des de…»: GitHub (PROPIETARI/REPOSITORI), una URL, un fitxer .zip o una carpeta local. Els zip tenen límit de mida (20 MB comprimits / 50 MB descomprimits) i fora de localhost només s'accepten URLs HTTPS.",
        "Cercador de skills: cerca a GitHub en viu (p. ex. «wordpress», «android», «pdf», «blender»); qualsevol repositori amb SKILL.md s'hi pot instal·lar. Si la xarxa no va, cau al catàleg de repositoris coneguts.",
        "Catàleg: skills oficials d'Anthropic (PDF, XLSX, DOCX, PPTX), la col·lecció Superpowers (depuració, TDD, planificació, revisió), recursos del format Agent Skills i exemples de docker-compose.",
        "Skills preinstal·lades: NoOrbit porta skills oficials de GitHub a punt d'ús.",
        "«Importa extensions de VS Code»: examina els IDE compatibles instal·lats (VS Code, Qoder, Cursor, Trae, Windsurf) i n'importa les extensions com a skills.",
      ],
    },
    {
      title: "18. Aparença, temes, idiomes i publicar",
      intro:
        "NoOrbit està en català per defecte, permet canviar els colors, està preparat per " +
        "traduir-se i pot publicar els teus temes WordPress al marketplace.",
      items: [
        "— Temes —",
        "Menú Visualitza (o Configuració ▸ General): tria el tema entre Fosc (predeterminat), Blanc, Gris, Verd, Groc o Verd i daurat; el canvi és immediat i es recorda.",
        "El tema afecta tota la interfície i també l'editor de codi (Monaco).",
        "— Idiomes —",
        "Menú Visualitza ▸ Configuració ▸ Idioma: baixa el catàleg en català (ca.json), tradueix els valors (mantén les claus) i puja'l amb «Afegeix un idioma des d'un fitxer».",
        "Tria l'idioma al desplegable «Idioma actual» per canviar-hi a l'instant.",
        "El Manual d'ús també es pot traduir afegint un fitxer nou a la carpeta src/help/manuals/.",
        "— Pujar un tema WordPress —",
        "Menú IA ▸ Pujar tema…: tria la carpeta del tema i NoOrbit la revisa (capçalera de style.css, screenshot.png, fitxers que no s'han de pujar) i l'empaqueta en un ZIP com WordPress demana.",
        "Identifica't amb el compte de NoOrbit (registre o inici de sessió al mateix modal) i amb les credencials del lloc (usuari WordPress + contrasenya d'aplicació); la pujada es fa via l'API REST del marketplace.",
        "Les credencials del lloc es desen al teu compte, mai al codi ni al repositori.",
      ],
    },
    {
      title: "19. Instal·lació, portabilitat i actualitzacions",
      intro:
        "Pots instal·lar NoOrbit al Mac o executar-lo des d'un USB sense deixar res al sistema.",
      items: [
        "Instal·lació normal: obre el .dmg i arrossega NoOrbit a la carpeta Aplicacions.",
        "Mode portàtil en USB: copia l'aplicació NoOrbit.app dins un volum extern (USB). En executar-la des de l'USB, la configuració, els proveïdors i les dades es guarden en una carpeta NoOrbitData al costat de l'app, no al teu Mac.",
        "Els models d'Ollama poden viure en un USB: a Configuració dels models externs, NoOrbit detecta volums muntats, configura OLLAMA_MODELS i reinicia el servei sol perquè també l'app del Finder el respecti; pot migrar els models ja baixats amb progrés.",
        "Actualitzacions: uns segons després d'arrencar, NoOrbit consulta si hi ha una versió nova. Si n'hi ha, apareix «Actualització <versió>» a la barra d'estat; fer clic obre la pàgina del release per descarregar l'instal·lador. Si no n'hi ha, no mostra res. La comprovació manual viu a Configuració ▸ Informació.",
        "A la pantalla d'inici consta l'autoria: «Creat per Joan Puigbert — programari lliure, pots distribuir-lo i modificar-lo lliurement».",
      ],
    },
    {
      title: "20. Dreceres de teclat",
      items: [
        "⇧⌘N: projecte nou. ⌘N: fitxer nou de text. ⌘O: obre fitxer. ⌘K ⌘O: obre carpeta. ⌘⌥R: projectes recents / reobre l'últim.",
        "⌘S: desa. ⌘⇧S: desa com a…. ⌥⌘S: desa-ho tot. ⌘W: tanca l'editor. ⌘Q: surt.",
        "⌘Z / ⌘⇧Z: desfés / refés (també per al codi que la IA ha tocat). ⌘X / ⌘C / ⌘V: retalla / copia / enganxa. ⌘A: selecciona-ho tot.",
        "⌘F: cerca. ⌥⌘F: substitueix. ⇧⌘F: cerca al projecte. ⇧⌥F: formateja el document. ⇧⌘D: selecciona la següent coincidència.",
        "⇧⌘P o F1: paleta de comandes. ⌘P: ves al fitxer. ⌃G: ves a la línia. ⇧⌘O: ves al símbol. F12: ves a la definició.",
        "⌘B: barra lateral. ⌥⌘J: panell dret. ⌘J: terminal integrat. ^⌘`: terminal nou del sistema. ⌥Z: ajust de línia.",
        "⌘= / ⌘- / ⌘0: amplia / redueix / restableix el zoom. ⌘,: Configuració.",
        "⌘R: executa la vista prèvia.",
        "⌘1: correccions ràpides. ⇧⌘1: la IA repara la línia del cursor. ⇧⌘2: edita la selecció amb la IA.",
        "⌘↵: accepta el canvi de la IA que tens obert. ⌘⌫: el rebutja i restaura el codi anterior.",
      ],
    },
    {
      title: "21. Resolució de problemes",
      items: [
        "Ollama no apareix connectat: comprova que Ollama estigui engegat i que escolti a localhost:11434; l'assistent de l'entorn (barra d'estat) el pot engegar o instal·lar sol.",
        "La IA no respon amb un proveïdor en línia: revisa la clau (sk-… o sk-ant-…), el «Tipus» del proveïdor (OpenAI/Anthropic) i fes servir el botó de test.",
        "404 «model not found»: NoOrbit baixa el model que falta automàticament i torna a provar; si el nom no existeix al catàleg, tria'n un altre al gestor de models.",
        "Memòria insuficient: el model és massa gran per al teu ordinador; tria'n un de més lleuger (p. ex. :3b o quantitzat q4), o mou els models a un disc extern i usa un motor remot (secció 7).",
        "Una descàrrega de Hugging Bay falla o es talla: el fitxer «.part» es descarta si no passa la verificació SHA-256; torna a prémer Baixa i, si vols, tria un disc extern com a destí.",
        "Blender no connecta: verifica que l'add-on estigui actiu i que el port 9876 no estigui ocupat. Prova «Arrenca Blender» des del panell.",
        "L'«Aplica a Blender/Unreal» falla: assegura't que l'aplicació és oberta i connectada; la IA fa un segon intent sol, però si l'escena no existeix no podrà editar-la.",
        "Unreal no connecta: assegura't que els connectors Python i Remote Control són actius i que el port 30010 és lliure. La primera connexió triga perquè arrenca l'editor; el watchdog reintenta cada 5 s.",
        "La vista prèvia no carrega: comprova que la carpeta oberta contingui un index.html o fitxers web. El servidor només serveix fitxers de dins del projecte: les rutes cap a fora es rebutgen.",
        "El terminal no executa o es penja: prem «Atura la comanda» (mata el grup de processos); les comandes tenen temps límit. Torna a obrir el panell amb ⌘J.",
        "Un panell ha desaparegut o mostra un error: cada panell té la seva xarxa de seguretat; la resta de l'app continua. Torna a obrir la pestanya o reinicia NoOrbit; el detall de l'error és a Treball ▸ Consola.",
        "La conversa és molt llarga i la IA perd el fil: el context es compacta sol; si cal, obre un xat nou o fes un fork («Forca») des del punt que t'interessi.",
      ],
    },
    {
      title: "22. Exemples de prompts i coses a fer",
      intro:
        "Copia i enganxa qualsevol d'aquests textos al xat de l'Agent. Segons el que demanis, " +
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
        "— Tasques i equip —",
        "«@todo: estructura, estils i formulari de la landing — fes-ho per passos i marca @done: en acabar cada pas».",
        "«@use: Revisor | revisa el fitxer app.ts i proposa millores de seguretat».",
        "«Dissenya una botiga online: l'equip ha de fer l'estructura, els estils i el carret».",
        "«Creeu una landing page: un redactor fa els textos, un dissenyador els estils i un dev el formulari».",
        "— Text i contingut —",
        "«Redacta un text de presentació de 150 paraules per al meu projecte».",
        "«Tradueix aquest paràgraf al català natural».",
        "«Resumeix aquest fitxer en 5 punts clau».",
        "— Blender (prem «Aplica a ▸ Blender») —",
        "«Crea un cub vermell i posa una llum a sobre».",
        "«Afegeix el text «HOLA» flotant al centre de l'escena».",
        "«Mou l'objecte actiu 2 unitats cap a la dreta».",
        "«Fes una piràmide de 4 cares i aplica-li un material daurat».",
        "— Unreal (prem «Aplica a ▸ Unreal») —",
        "«Engendra un cub a l'origen i pinta'l de vermell».",
        "«Crea un actor de llum direccional a (0,0,500)».",
        "«Compta quants actors hi ha a l'escena i mostra-ho».",
      ],
    },
    {
      title: "23. Aprenentatge local (la IA que aprèn)",
      intro:
        "NoOrbit pot APRENDRE de les teves converses: observa què preguntes i què " +
        "responen les IAs (la local, les del núvol, els especialistes i les IAs web) i " +
        "en treu lliçons curtes que tornen al context dels xats futurs. Tot és LOCAL: " +
        "res surt de l'ordinador tret que exportis el dataset.",
      items: [
        "Obre Agent ▸ Aprenentatge al panell dret i activa l'interruptor (ve desactivat per defecte).",
        "A partir d'ara cada torn (pregunta + resposta + proveïdor/model) es desa al registre local de l'aplicació (learning.json, a la carpeta de dades, mai al repositori).",
        "«Aprendre ara» destil·la les converses pendents amb el model local d'Ollama i en treu lliçons (preferències, decisions, fets del projecte). Si no hi ha cap model en marxa, es fa igual amb una heurística que detecta les preferències del text.",
        "«Aprèn sol en segon pla» fa el mateix automàticament quan hi ha converses noves: el xat NO s'espera, la destil·lació va al fons.",
        "«Injecta les lliçons al context» fa que les lliçons que apliquen a la teva pregunta s'afegeixin al prompt: la IA sap el que ja li has ensenyat i no torna a preguntar-ho. Les preferències apliquen sempre; la resta, per coincidència de paraules.",
        "Les lliçons es poden esborrar una a una (icona de paperera) i tot el registre amb «Esborra tot».",
        "«Exporta el dataset» genera un JSONL amb totes les converses en format de xat: és el material per entrenar un model local (LoRA) amb LLaMA-Factory, Unsloth o Axolotl, quan vulguis que la IA local sigui «teva» de debò.",
        "Privacitat: el registre i les lliçons viuen a la carpeta de dades de NoOrbit (a la mateixa carpeta que la configuració) i no s'envien enlloc. Per buidar-ho tot, «Esborra tot».",
      ],
    },
  ],
};