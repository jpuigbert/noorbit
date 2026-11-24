//! Comandes Tauri per a IA (Ollama).

use crate::ai::{AiManager, ChatOpts, HistoryTurn, OllamaModel};
use crate::computer::ComputerController;
use crate::AppState;
use serde::Serialize;
use std::sync::Arc;
use tauri::{command, AppHandle, Emitter, State};

/// Xat on s'executa una crida sense `session` explícita.
fn sid_of(session: Option<String>) -> String {
    session.unwrap_or_else(|| crate::ai::DEFAULT_SESSION.to_string())
}

/// L'usuari dema accions reals (executar, empaquetar, llançar…) o bé la
/// RESPOSTA del model n'anuncia sense haver-les fet: «has d'executar…», «ara
/// cal compilar…», o comandes en `backticks` perquè les corre l'usuari. En
/// eixos casos, si el model NO ha emés cap directriu, li'n fem un recordatori
/// UNA sola vegada (EXEC_NUDGE): els models menuts s'obliden de les eines si
/// no se'ls empeny, i l'usuari es queda «amb la resposta i prou».
fn expects_execution(prompt: &str, answer: &str) -> bool {
    use regex::Regex;
    let user_re = match Regex::new(
        r"(?i)(execut|llan[cç]|empaquet|compil|instal|arrenca|funcionar|en marxa|prova\s|testej|comprova|fes\s+que)\w*",
    ) {
        Ok(r) => r,
        Err(_) => return false,
    };
    let answer_re = match Regex::new(
        r"(?i)(heu d'?\s*executar|has d'?\s*executar|cal executar|per\s+a?\s+executar|executa(?:'s)?\s+(?:el|la|primer|ara)|executeu|obri\s+i\s+executa|`[^`]*\b(?:python3?|npm|pip|cargo|make|bash|node|open)\b[^`]*`)",
    ) {
        Ok(r) => r,
        Err(_) => return false,
    };
    // Figures i escenes 3D (sense «Blender» al text).
    let scene_re = match Regex::new(
        r"(?i)\b(nan|nans|nomo|noms|gome|gnom|gnome|figura|figures|personatge|escena|escenes|model\s*3d|3d|jard[ií]|blender)\b",
    ) {
        Ok(r) => r,
        Err(_) => return false,
    };
    // Una PREGUNTA no demana execució («què és un model 3D?», «com es fa?»).
    let question = prompt.trim_end().ends_with('?')
        || Regex::new(r"(?i)^\s*(qu[eé]\s+es|qu[eé]\s+s[oó]|com\s|per\s+qu[eè]\s|explica|defineix)")
            .map(|r| r.is_match(prompt))
            .unwrap_or(false);
    if !question && scene_re.is_match(prompt) {
        return true;
    }
    user_re.is_match(prompt) || answer_re.is_match(answer)
}

/// També apliquem les PETICIONS DE FIGURA O ESCENA 3D («crea un nan de
/// jardí», «fes un model 3D d'una casa»): demanen una acció real — el model
/// ha d'emetre «RUN|BLENDER|» amb el codi bpy — encara que la paraula
/// «Blender» no hi aparegui. Sense això el model es limitaria a explicar com
/// fer-ho i no es creava res.
///
/// Recordatori que es torna al model quan ha parlat d'executar sense haver
/// emés cap directriu. Curto i imperatiu: fins i tot un model 1.5b el obeïx.
const EXEC_NUDGE: &str = "NoOrbit: NO has emés cap directriu «RUN|», per tant NO \
s'ha executat res i l'usuari esperava accions reals. Si la tasca requereix \
executar, compilar o llançar alguna cosa: 1) si falten fitxers, reemet-los \
sencers ara mateix amb línies «@file: ruta» + contingut; 2) després escriu UNA \
línia «RUN|<comanda>» («RUN|BG|<comanda>» per a servidors/apps que han de \
quedar oberts) i ATURA la resposta eixe torn. Si és una FIGURA o una ESCENA \
3D, emet «RUN|BLENDER|» amb el codi bpy que la construeix (pot ser de varies \
línies i ha de començar per «import bpy»): res de descripcions en prosa. \
No demanes a l'usuari que execute ell: executes tu. Si de veritat no cal cap \
execució, respon amb normalitat.";

/// Detecta fitxers que la IA ha ANNUNCIAT amb «@file: ruta» però ha deixat 
/// BUIT (cap línia de codi entre l'anunci i el següent @file:/@dir:/final de 
/// la resposta, o només prosa/promeses). Els models menuts fan sovint això: 
/// creen el fitxer i diuen «ara escric el codi» i s'aturen. Retornar les 
/// rutes buits permet donar-los un recordatori (FILE_NUDGE) perquè reemetent 
/// el contingut COMPLET en el mateix torn.
fn empty_file_paths(text: &str) -> Vec<String> {
    use regex::Regex;
    let Ok(re) = Regex::new(r"(?im)^[ \t>#*-]*(?:\d+[.)]\s+)?@?file:\s*(.+?)\s*$") else {
        return Vec::new();
    };
    let lines: Vec<&str> = text.lines().collect();
    // Marques de cada línia que inicia un bloc (@file:, @dir:, @delete:…).
    let is_marker = |l: &str| -> bool {
        Regex::new(r"(?im)^\s*[\t >#*-]*@?(file|dir|directori|carpeta|folder|delete|esborra|borra|elimina|remove|rm)\w*:\s*(.*)$")
            .map(|r| r.is_match(l))
            .unwrap_or(false)
    };
    let mut out: Vec<String> = Vec::new();
    for c in re.captures_iter(text) {
        let path = c.get(1).map(|m| m.as_str().trim().to_string()).unwrap_or_default();
        if path.is_empty() {
            continue;
        }
        // Troba la línia de l'anunci i suma el contingut fins al següent marcador.
        let Some(idx) = lines.iter().position(|l| {
            re.is_match(l) && l.to_lowercase().replace('"', "").contains(&path.to_lowercase())
        }) else {
            continue;
        };
        let mut content = String::new();
        for l in &lines[idx + 1..] {
            if is_marker(l) {
                break;
            }
            if l.trim().starts_with("```") {
                continue; // tanques de codi: no són contingut
            }
            content.push_str(l.trim());
        }
        // Buit, o només una promesa («a continuació», «ara escriuré», TODO…).
        let low = content.to_lowercase();
        let flops = content.chars().count() < 40
            || low.contains("pendent d'implementar")
            || low.contains("TODO")
            || (low.contains("a continuaci") && !low.contains('\n'));
        if flops && !out.contains(&path) {
            out.push(path);
        }
    }
    out
}

/// Recordatori per quan la IA anuncia fitxers i els deixa buits: que els 
/// reemetja amb el codi real. Una sola vegada per torn de xat.
const FILE_NUDGE: &str = "NoOrbit: has annunciat fitxers amb «@file:» però han \
quedats BUITS (cap línia de codi després de l'anunci). Un @file: sense \
contingut crea un fitxer buit: NO serveix. Reemet ara CADA fitxer que falta \
amb la seua línia «@file: ruta» i, JUST a la línia de seguida, el codi COMPLET \
(funcions, imports, lògica real; res de «…» ni promeses). Pot ser llarg: \
escriu'l sencer. Si de veritat no cal cap fitxer, respon amb normalitat.";

/// Detecta quan l'usuari ha demanat CREAR EL PROJECTE SENCER però la resposta
/// només ha emés (gairebé) cap fitxer: un README i un pla de passos en prosa
/// («instal·la… configura… després…»). Els models menuts ho fan sovint i
/// l'usuari es queda sense codi. Criteri: prompt de creació + resposta amb
/// pla (@todo o passos numerats) + ≤1 fitxer emés.
fn project_underbuilt(prompt: &str, text: &str) -> bool {
    use regex::Regex;
    let low = prompt.to_lowercase();
    let asks = low.contains("tot el projecte")
        || low.contains("tot el codi")
        || low.contains("tota la programacio")
        || low.contains("tota la programació")
        || low.contains("carpetes necess")
        || Regex::new(r"(?i)\b(crea|crear|fes|munta|genera|estructura)\b[^.]{0,80}\b(projecte|joc|botiga|web|app|aplicacio|aplicació|programa)\b")
            .map(|r| r.is_match(prompt))
            .unwrap_or(false);
    if !asks {
        return false;
    }
    let files = Regex::new(r"(?im)^[ \t>*-]*@?file:\s*\S")
        .map(|r| r.find_iter(text).count())
        .unwrap_or(0);
    if files > 1 {
        return false;
    }
    Regex::new(r"(?i)(@todo|passos?|seguents passos|seguents passos|\b1\s*[\.\)]\s)")
        .map(|r| r.is_match(text))
        .unwrap_or(false)
}

/// Recordatori per quan ha respost amb un PLA però no ha emés els fitxers:
/// que emeti CADA fitxer del pla amb @file: + codi complet, fins acabar.
const PROJECT_NUDGE: &str = "NoOrbit: has respost amb un PLA de passos però has \
emés poc més d'un fitxer. L'usuari ha demanat CREAR el projecte SENCER: emet \
ARA cada fitxer del pla amb la seua línia «@file: ruta/fitxer.ext» i, JUST \
a la línia de seguida, el codi COMPLET (res de promeses, res de «…», res de \
«després configuraríem»). Un @file: per cada fitxer, tots els que calguin, \
fins acabar el pla. Comença ara mateix pel primer fitxer pendent.";

/// Detecta quan l'usuari ha demanat CREAR/generar una IMATGE i el model ha
/// respost amb prosa (l'ha «explicada» o promesa) sense emetre cap directriu
/// «IMG|». Els models menuts ho fan molt sovint: descriuen la imatge amb
/// paraules en lloc de demanar al sistema que la generar. Criteri: prompt
/// de creació d'imatge + resposta sense cap «IMG|» que parle d'imatges.
fn image_requested(prompt: &str, text: &str) -> bool {
    use regex::Regex;
    let user_re = match Regex::new(
        r"(?i)(crea|crear|fes|genera|generar|dibuixa|dibuix|pinta|il\.?lustra|munt[ae]|disenya|dissenya|fotografia|retrat)(\w*)\s+(una\s+|un\s+|la\s+|el\s+|me\s+|\w+){0,3}(imatge|imagen|foto|pictura|dibuix|il\.?lustraci[oó]|icona|logotip|logo|p[oó]ster|cartell|retrat|escena|avat[aà]r)",
    ) {
        Ok(r) => r,
        Err(_) => return false,
    };
    if !user_re.is_match(prompt) {
        return false;
    }
    // Si el model JA ha emés una directriu «IMG|» (o la conversa ja parla de
    // rodet/imatge generada), no cal cap recordatori.
    !text.contains("IMG|")
}

/// Detecta quan l'usuari demana CODI (projecte, botiga, script…) i la IA
/// respon amb NOMÉS DOCUMENTACIÓ (README o blocs .md). Els models locals
/// dèbils ho fan molt sovint: es fan el savis i escriuen un README en lloc
/// de codi, i el projecte queda buit de codi.
fn nomes_documentacio(prompt: &str, text: &str) -> bool {
    use regex::Regex;
    let demana_codi = Regex::new(
        r"(?i)\b(fes|fer|crea|crear|genera|generar|escriu|escriure|implementa|implementar|munta|montar|constru[ïi]r|programa|programar|code|codea|build)\b[^.!?\n]{0,70}\b(codi|code|projecte|botiga|store|web|app|aplicaci[oó]|plugin|pl[oó]gin|theme|tema|script|component|servei|api|backend|frontend|dashboard|funcionalitat)\b",
    )
    .map(|r| r.is_match(prompt))
    .unwrap_or(false);
    if !demana_codi {
        return false;
    }
    // Hi ha algun fitxer de CODI (no .md) a la resposta?
    let té_codi = Regex::new(r"(?i)@file:\s*\S+\.(php|py|js|jsx|ts|tsx|rs|java|kt|kts|swift|css|scss|html|vue|svelte|go|rb|c|cpp|h|json|yml|yaml|toml|sql|sh)")
        .map(|r| r.is_match(text))
        .unwrap_or(false);
    if té_codi {
        return false;
    }
    // Hi ha «@file:»? llavors TOTS són .md → només documentació.
    let Ok(re_file) = Regex::new(r"(?i)@file:\s*([^\s]+)") else {
        return false;
    };
    let fitxers: Vec<String> = re_file
        .captures_iter(text)
        .filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))
        .collect();
    if !fitxers.is_empty() {
        return fitxers
            .iter()
            .all(|f| f.to_lowercase().ends_with(".md") || f.to_lowercase().ends_with(".markdown"));
    }
    // Sense «@file:»: només compta si la resposta parla de documentació.
    Regex::new(r"(?i)(README|documenta|millora|estructura del|caracter[íi]stiques|instal·lació|descripci[oó])")
        .map(|r| r.is_match(text))
        .unwrap_or(false)
}

/// Recordatori per quan l'usuari demanava CODI i la IA ha respost amb
/// DOCUMENTACIÓ (README/blocs .md): el CODE_NUDGE normal no s'activaria perquè
/// sí que hi ha «@file:» — però són tots .md, i un projecte de documentació no
/// és un projecte. Aquí li diem que escrigui el codi real.
const DOCS_ONLY_NUDGE: &str = "NoOrbit: l'usuari demanava CÓDI i tu has respost amb \
DOCUMENTACIÓ (README/blocs .md): això NO crea el projecte, només en descriu les \
pestanyes. Ara escriu el CÓDI REAL: un bloc «@file: ruta.ext» per cada fitxer del \
projecte (amb el codi COMPLET dins, sense «...» ni placeholders), començant pel \
més important (si el projecte és gran, fes-ho a trossos: primer els fitxers \
essencials i després la resta). ATURA el torn després d'emetre els fitxers.";

/// Recordatori per quan ha parlat de la imatge sense fer-la: que emet CLIC
/// la directriu «IMG|» i ature el torn — la generació la fa el sistema.
const IMG_NUDGE: &str = "NoOrbit: l'usuari ha demanat CREAR una IMATGE i no has \
emés cap directriu «IMG|». NO l'expliquis ni la descrigues amb paraules: \
escriu UNA línia «IMG|<descripció detallada de l'escena>|<ruta/fitxer.png>» \
(sense comandes de shell, sense «després», sense promeses) i ATURA el torn. \
La generació la fa el sistema: rodet local en CPU (també en Mac Intel) o \
proveïdor en línia amb token. El resultat et tornarà la RUTA real del fitxer \
per copiar-la a la resposta final.";

/// L'usuari demana FER/crear/modificar codi (no només preguntar-ne coses)?
/// És el criteri que activa el recordatori perquè el model emeta fitxers de
/// veritat en lloc de respondre amb prosa o un pla. Inclou les proves, que
/// sovint es demanen amb «fes els tests» o «escriu proves».
fn asks_for_code(prompt: &str) -> bool {
    use regex::Regex;
    let action = Regex::new(
        r"(?i)\b(crea|crear|fes|fes-me|implementa|implementar|afegeix|afegir|modifica|modificar|corregeix|corregir|arregla|arreglar|refactoritza|refactoritzar|escriu|escriure|genera|generar|completa|completar|desenvolupa|desenvolupar|programa|programar|monta|muntar|migra|migrar|necessito|necessitem|vull|volem|cal|calen)\b",
    )
    .map(|r| r.is_match(prompt))
    .unwrap_or(false);
    if !action {
        return false;
    }
    let object = Regex::new(
        r"(?i)\b(codi|codi font|funci[oó]|funcions|m[eè]tode|classe|component|components|m[oò]dul|m[oò]duls|api|endpoint|servei|serveis|fitxer|fitxers|arxiu|arxius|test|tests|prova|proves|suite|backend|frontend|pantalla|vista|bot[oó]|formulari|script|app|aplicaci[oó]|web|projecte|joc|login|autenticaci[oó]|crud|sql|base de dades|model|controlador|migraci[oó]|esquema|ruta|p[aà]gina|estil|css|html)\b",
    )
    .map(|r| r.is_match(prompt))
    .unwrap_or(false);
    let path_like = Regex::new(
        r"(?i)[A-Za-z0-9_./-]+\.(py|js|jsx|ts|tsx|rs|go|java|kt|swift|rb|php|c|cpp|h|hpp|cs|sql|html|css|scss|json|yaml|yml|toml|sh|md)\b",
    )
    .map(|r| r.is_match(prompt))
    .unwrap_or(false);
    object || path_like
}

/// L'usuari parla de PROVES/TESTS? (en català «proves» també pot ser «try»,
/// però només s'usa juntament amb un verb de creació).
fn asks_for_tests(prompt: &str) -> bool {
    use regex::Regex;
    Regex::new(r"(?i)\b(tests?|proves?|testej\w*|provar|unitari\w*|unit[aà]ri\w*|integraci[oó]|tdd|cobertura)\b")
        .map(|r| r.is_match(prompt))
        .unwrap_or(false)
}

/// La resposta ja inclou algun fitxer anunciat amb «@file:» (amb la mateixa
/// tolerància que el parser del frontend: calaixos, títols markdown, números).
fn has_file_blocks(text: &str) -> bool {
    use regex::Regex;
    Regex::new(r"(?im)^[ \t>#*-]*(?:\d+[.)]\s+)?@?file:\s*\S")
        .map(|r| r.is_match(text))
        .unwrap_or(false)
}

/// Hi ha cap tanca de codi ``` a la resposta? Si n'hi ha, el frontend pot
/// rescatar el codi i escriure'l; si no n'hi ha, el model només ha parlat.
fn has_code_fence(text: &str) -> bool {
    text.lines().any(|l| l.trim_start().starts_with("```"))
}

/// Recordatori quan l'usuari demana PROVES i la resposta no ha creat cap
/// fitxer de prova: les proves han de ser fitxers reals, no una explicació.
const TESTS_NUDGE: &str = "NoOrbit: l'usuari ha demanat PROVES (tests) i la \
resposta NO ha creat cap fitxer amb «@file:». Una prova explicada en prosa no \
serveix: ha de ser un FITXER REAL dins el projecte. Emet ARA, per a cada \
prova, «@file: <ruta coherent amb el projecte>» (p. ex. tests/test_<tema>.py, \
src/<tema>.test.ts o la carpeta de proves que ja use el projecte) seguit del \
codi COMPLET de la prova (imports + casos reals que fallen si el codi està \
malament, mai buits ni amb «…»). Si cal, crea també la configuració de proves \
del projecte. Després escriu UNA línia «RUN|<comanda per executar les proves>» \
i atura el torn. No expliquis res més.";

/// Recordatori general: l'usuari ha demanat fer codi i el model ha respost
/// només amb prosa o un pla. Calen fitxers reals.
const CODE_NUDGE: &str = "NoOrbit: l'usuari t'ha demanat FER una cosa de codi i \
la resposta no ha creat cap fitxer ni cap bloc de codi. No expliquis el que \
faries ni prometis-ho per a després: EMET el resultat real. Per a cada fitxer \
nou o modificat escriu «@file: ruta/real» i, JUST a la línia de seguida, el \
contingut COMPLET (imports, funcions, lògica real; res de «…», res de TODO). \
Si la tasca demanava proves, crea també els fitxers de prova. Quan acabis els \
fitxers, si cal compilar, executar o provar res, escriu «RUN|<comanda>» i \
atura el torn. Comença ara mateix pel primer fitxer.";

/// Límit de torns eina→resultat→model del bucle `chat_with_tools`: prou per a
/// cicles escriure→verificar→corregir→compilar→corregir→tornar a compilar
/// dins d'UNA tasca (el bucle de verificació obligatori consumeix torns).
const MAX_ROUNDS: usize = 10;
/// Els nudges són REPETIBLES: fins a 2 usos cadascun per torn (abans un
/// de sol cop, i els models menuts tornaven a caure en el mateix error).
const MAX_NUDGE_USES: usize = 2;

/// Torns EXTRA permesos amb EXACTAMENT el mateix pla d'accions (mateixes
/// «RUN|», mateixos blocs «@edit:»/«@file:», mateixes «NB|»). El primer cop
/// que es repeteix, el model rep `IDENTICAL_ACTIONS_NOTE` i NoOrbit NO
/// reexecuta res; si encara insisteix, el torn s'atura i ho diem a l'usuari.
///
/// Sense això, la deduplicació de reemissions només cobria els blocs «@file:»
/// (frontend): un model encallat en una ruta absoluta o en una edició
/// CERCA == NOU reemetia el MESMEIX bloc ~15 vegades fins a esgotar
/// `MAX_ROUNDS`, i el xat quedava ple de còpies del mateix text.
const MAX_IDENTICAL_ACTION_ROUNDS: usize = 1;

/// Avís al model quan reemet el mateix pla d'accions: li donem exactament el
/// que li falta per eixir del forat (les quatre causes reals que s'han vist
/// al xat real), en lloc de tornar-li a executar la mateixa comanda.
const IDENTICAL_ACTIONS_NOTE: &str = "RUN|RESULTAT| Has reemet EXACTAMENT les mateixes accions que al torn anterior i NoOrbit NO les ha tornat a executar (el resultat seria idèntic). Corregeix ABANS d'intentar res més:\n1) Les rutes de «@file:», «@edit:» i «@delete:» han de ser RELATIVES a l'arrel del projecte: mai absolutes i mai reproduir l'arrel dins la ruta.\n2) Un bloc «@edit:» amb el text NOU IDÈNTIC al CERCA no canvia res: fes-lo DISTINT o no emetis res.\n3) Les rutes amb espais dins «RUN|» van ENTRECOMILLADES.\n4) Cada directriu a la seva pròpia línia (mai dos «RUN|» enganxats).\nSi el fitxer que vols tocar és fora del projecte obert, digues-ho en prosa: NoOrbit no pot escriure-hi.";

/// Comptadors d'ús de cada nudge dins d'un torn (límit MAX_NUDGE_USES).
#[derive(Default)]
struct NudgeCounters {
    exec: usize,
    files: usize,
    img: usize,
    project: usize,
    docs: usize,
    code: usize,
    tests: usize,
}

/// Decisió PURA de la cadena de recordatoris del bucle `chat_with_tools`:
/// donats el prompt de l'usuari, la resposta del model, la ronda actual i els
/// comptadors, retorna el nudge que s'ha d'enviar (i incrementa el comptador)
/// o `None` si el torn pot acabar. L'ORDRE i les condicions són els del bucle
/// real; extreure'ls ací permet testejar el guion complet sense AppHandle.
fn next_nudge(prompt: &str, text: &str, round: usize, c: &mut NudgeCounters) -> Option<String> {
    // Sense directrius… però la tasca demanava accions reals? Si encara
    // queden usos del recordatori, donem-li una oportunitat més: el
    // torn que ve hauria d'anar ple de «RUN|» (o «@file:» si falten
    // fitxers) i NoOrbit l'executarà de veritat.
    if round < MAX_ROUNDS && c.exec < MAX_NUDGE_USES && expects_execution(prompt, text) {
        c.exec += 1;
        return Some(EXEC_NUDGE.to_string());
    }
    // I si l'usuari ha demanat PROVES i la resposta només les explica
    // (cap «@file:»)? És el cas «fes els tests»: la prova ha d'existir
    // com a fitxer real, no com a prosa. Va ABANS del recordatori de
    // «projecte sencer» perquè «fes els tests del projecte» no acabe
    // generant tot el projecte en lloc de les proves. Fins a 2 usos.
    if round < MAX_ROUNDS
        && c.tests < MAX_NUDGE_USES
        && asks_for_code(prompt)
        && asks_for_tests(prompt)
        && !has_file_blocks(text)
    {
        c.tests += 1;
        return Some(TESTS_NUDGE.to_string());
    }
    // I si ha annunciat fitxers amb «@file:» i els ha deixat BUITS
    // (el model menut crea l'arxiu i promet el codi «després»)? Un
    // recordatori, fins a 2 usos: que reemetja cada fitxer COMPLET.
    if round < MAX_ROUNDS && c.files < MAX_NUDGE_USES {
        let empties = empty_file_paths(text);
        if !empties.is_empty() {
            c.files += 1;
            return Some(format!(
                "{}\n\nFitxers que han quedat buits: {}",
                FILE_NUDGE,
                empties.join(", ")
            ));
        }
        // I si ha demanat CREAR EL PROJECTE SENCER però només ha emés
        // un README amb un pla de passos en prosa? Recordatori
        // imperatiu perquè emeti CADA fitxer del pla amb @file:.
        if c.project < MAX_NUDGE_USES && project_underbuilt(prompt, text) {
            c.project += 1;
            return Some(PROJECT_NUDGE.to_string());
        }
    }
    // I si l'usuari demanava CODI i la resposta només conté
    // DOCUMENTACIÓ (tots els «@file:» són .md)? Aquest cas NO el
    // captura el CODE_NUDGE normal (hi ha fitxers, però no són codi):
    // sense aquest recordatori el model es queda fent README i
    // «bloc_2.md» fins que s'esgota.
    if round < MAX_ROUNDS && c.docs < MAX_NUDGE_USES && nomes_documentacio(prompt, text) {
        c.docs += 1;
        return Some(DOCS_ONLY_NUDGE.to_string());
    }
    // I si l'usuari ha demanat FER codi (implementar, crear, modificar,
    // arreglar…) i la resposta no té ni fitxers ni cap bloc de codi?
    // Un recordatori imperatiu perquè emeta els fitxers de veritat.
    if round < MAX_ROUNDS
        && c.code < MAX_NUDGE_USES
        && asks_for_code(prompt)
        && !has_file_blocks(text)
        && !has_code_fence(text)
    {
        c.code += 1;
        return Some(CODE_NUDGE.to_string());
    }
    // I si ha demanat CREAR UNA IMATGE però ha respost amb prosa sense
    // emetre cap «IMG|»? Recordatori imperatiu: que la generi el
    // sistema amb la directriu, i ell tan sols la copie.
    if round < MAX_ROUNDS && c.img < MAX_NUDGE_USES && image_requested(prompt, text) {
        c.img += 1;
        return Some(IMG_NUDGE.to_string());
    }
    None
}

/// Un torn de xat amb les EINES DE NOORBIT connectades: qualsevol resposta
/// del model que continga directrius «NB|…» (navegador) o «RUN|…» / «IMG|…»
/// (terminal del projecte i generador d'imatges) s'executa de veritat, se'n
/// torna el resultat al model i el model acaba la feina amb ell. Així TOTES
/// les IAs (local sense token, remota, experts) poden navegar, compilar,
/// empaquetar, llançar i crear imatges sense cap botó ni endollable: només
/// cal que sàpiguen escriure la directriu.
async fn chat_with_tools(
    app: AppHandle,
    ai: &Arc<AiManager>,
    computer: &Arc<ComputerController>,
    learning: &Arc<crate::learning::LearningStore>,
    cfg: &Arc<tokio::sync::RwLock<crate::config::AppConfig>>,
    prompt: &str,
    system: Option<&str>,
    mut opts: ChatOpts,
    sid: &str,
    streaming: bool,
    question: Option<&str>,
) -> Result<String, String> {
    use crate::agent::runtools;
    use crate::browser::tools;
    // El manual de directrius s'afegeix AL SYSTEM PROMPT DE TOTES les IAs.
    // S'hi afegeix l'ESTAT REAL de les eines ara mateix (Blender connectat?
    // Unreal? projecte obert? imatges en local?) perquè el model no propose
    // el que aquest ordinador no pot fer.
    let status = runtools::tools_status_line(computer.permissions().enabled).await;
    // Tokens: el manual del navegador (≈380 tokens) NOMÉS entra quan la
    // conversa parla de web/consultes externes — la resta de torns, econòmics.
    let hist_text = opts
        .history
        .iter()
        .map(|t| t.content.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    let manual = if tools::manual_relevant(prompt, &hist_text) {
        format!("{}\n\n{}{}", tools::DIRECTIVE_PROMPT, runtools::RUN_PROMPT, status)
    } else {
        format!("{}{}", runtools::RUN_PROMPT, status)
    };
    let sys = match system {
        Some(s) => format!("{}\n\n{}", s, manual),
        None => manual,
    };
    // ── Aprenentatge local ──────────────────────────────────────────────
    // Les lliçons que l'IA ja ha APRÈS (de les converses anteriors amb
    // qualsevol IA: la local, les remotes, els especialistes) entren al
    // system prompt: la IA sap el que l'usuari ja li ha ensenyat i no
    // torna a preguntar-ho. Tot local — cap servei al núvol hi participa.
    //
    // Les lliçons es trien segons la PREGUNTA real de l'usuari (no el prompt
    // sencer, que pot durar milers de caràcters de codi i faria que tot
    // coincidís amb alguna cosa).
    let user_question = extract_question(prompt, question);
    let learn_cfg = { cfg.read().await.learning.clone() };
    let learning_on = learn_cfg.enabled;
    let lessons_block = if learning_on && learn_cfg.inject {
        let q = user_question.clone().unwrap_or_else(|| prompt.to_string());
        learning.context_for(&q, 5)
    } else {
        String::new()
    };
    let sys = if lessons_block.is_empty() {
        sys
    } else {
        format!("{}\n\n{}", sys, lessons_block)
    };
    // ── FASE 2 · system prompt en capes (patró OpenCode, MIT) ──────────
    // Proveïdor + entorn (cwd del projecte, SO, eines detectades al PATH
    // sense spawnar res) + instruccions d'AGENTS.md/NOORBIT.md. Idempotent
    // per marques: el camí cloud no duplica les capes ja presents.
    let root = crate::commands::workspace::current_root();
    let (kind, model_info) = ai
        .provider_info(&opts.provider.clone().unwrap_or_default())
        .await;
    let sys = crate::ai::sysprompt::layered(Some(&sys), &kind, &model_info, root.as_deref());
    // ── FASE 2 · compaction al 90% del context ─────────────────────────
    // Si la conversa estimada supera el 90% de la finestra del model, els
    // torns antics es substitueixen per un resum (via proveïdor o extractiu
    // local) conservant els darrers; el xat rep la nota «ai://compacted».
    let limit = if kind == "native" {
        crate::ai::native_llm::finestra_base()
    } else {
        crate::ai::context_window_for(&model_info)
    };
    crate::ai::compact::maybe_compact(&app, ai, sid, &mut opts, prompt, Some(&sys), limit).await;
    let mut current = prompt.to_string();
    let mut text = String::new();
    // Comptadors d'ús de cada nudge (límit MAX_NUDGE_USES per torn): la
    // decisió de la cadena viu a `next_nudge` (pura, coberta per tests).
    let mut nudges = NudgeCounters::default();
    // Bucle de verificació obligatori: rondes amb errors de lint JA
    // realimentades al model aquest torn (límit MAX_VERIFY_RETRIES).
    let mut verify_retries = 0usize;
    // Anti-bucle: segell de les accions del torn anterior i quants cops
    // s'ha repetit seguidament (vegeu MAX_IDENTICAL_ACTION_ROUNDS).
    let mut prev_fp = String::new();
    let mut repeats = 0usize;
    let t0 = std::time::Instant::now();
    // SNAPSHOT del torn (git shadow): es captura l'estat del projecte ABANS
    // que el model escriga res — ni els lots del backend ni la
    // materialització «en viu» del frontend durant el streaming. Si git no
    // hi és o no hi ha projecte obert, es degrada amb nota i el xat continua.
    let snap_root = crate::commands::workspace::current_root();
    if let Some(root) = snap_root.clone() {
        let _ = tauri::async_runtime::spawn_blocking(move || {
            if let Err(e) = crate::agent::snapshot::begin_turn(&root) {
                eprintln!("NoOrbit/snapshot: sense snapshot del torn ({e})");
            }
        })
        .await;
    }
    for round in 0..=MAX_ROUNDS {
        let res = if streaming {
            crate::ai::in_session(sid, ai.chat_stream(&app, &current, Some(&sys), &opts)).await
        } else {
            crate::ai::in_session(sid, ai.chat_opts(&current, Some(&sys), &opts)).await
        };
        text = match res {
            Ok(t) => t,
            Err(e) => {
                // El visor de procés MAI ha de quedar amb un «ERROR» mut i amb el
                // temps a zero: ací es conta QUÈ ha fallat i quant s'ha esperat.
                let msg = e.to_string();
                if msg != crate::ai::CANCELLED_MSG && !ai.session_error_shown(sid) {
                    let (provider, model) = ai.session_labels(sid).await;
                    crate::ai::emit_process(
                        &app,
                        sid,
                        &provider,
                        &model,
                        "error",
                        &msg,
                        t0.elapsed().as_millis() as u64,
                    );
                }
                // Tanca el torn de snapshot: si el disc no ha canviat, el
                // baseline es descarta (no hi haurà «Desfés el pas» buit).
                if let Some(root) = snap_root.clone() {
                    let _ = tauri::async_runtime::spawn_blocking(move || {
                        crate::agent::snapshot::end_turn(&root, false, &[]);
                    })
                    .await;
                }
                return Err(msg);
            }
        };
        let nb = tools::parse(&text);
        let runs = runtools::parse(&text);
        // ── GUARD ANTI-BUCLE ────────────────────────────────────────────
        // Les PARAULES del model poden canviar entre torns i el pla d'accions
        // ser idèntic: comparem només les accions (mai la prosa).
        let fp = runtools::action_fingerprint(&text);
        repeats = if !fp.is_empty() && fp == prev_fp { repeats + 1 } else { 0 };
        prev_fp = fp;
        if repeats > MAX_IDENTICAL_ACTION_ROUNDS {
            // Ja l'hem avisat i torna a fer el mateix: aturem el torn i ho
            // diguem de manera visible a l'usuari, en lloc de deixar el xat
            // tallat sense explicació (era el transcript de 15 blocs repetits).
            text.push_str(
                "\n\n⛔ NoOrbit ha ATURAT aquest torn: la IA ha reemet EXACTAMENT les mateixes accions dues vegades seguides (la mateixa «RUN|» i la mateixa «@edit:» sense canvis), així que tornar-les a executar no hauria canviat res. Les causes habituals: una ruta ABSOLUTA en lloc d'una de RELATIVA a la carpeta oberta, o un bloc d'edició amb el text NOU igual que el CERCA. Torna-ho a demanar concretant la carpeta del projecte, o digues «aplica els canvis fitxer a fitxer».",
            );
            break;
        }
        if repeats == 1 && !(nb.is_empty() && runs.is_empty()) {
            // Primer reintent idèntic: NO tornem a executar el que ja
            // s'ha executat; enviem la nota perquè el model corregeixca el pla.
            opts.history.push(HistoryTurn {
                role: "assistant".into(),
                content: text.clone(),
            });
            current = IDENTICAL_ACTIONS_NOTE.to_string();
            continue;
        }
        if (nb.is_empty() && runs.is_empty()) || round == MAX_ROUNDS {
            // Cadena de recordatoris (decisió pura): si en queda cap d'útil,
            // el torn del model entra a l'historial i el nudge és el nou prompt.
            if let Some(nudge) = next_nudge(prompt, &text, round, &mut nudges) {
                opts.history.push(HistoryTurn {
                    role: "assistant".into(),
                    content: text.clone(),
                });
                current = nudge;
                continue;
            }
            break;
        }
        // El torn del model i els resultats de les eines entren a la conversa
        // com un torn nou: el model veu dades reals i respon sobre segur.
        let mut report = String::new();
        if !runs.is_empty() {
            // PRIMER els fitxers, DESPRÉS les ordres: si el model anuncia
            // «@file: x.py» i «RUN|python3 x.py» en el mateix torn, el fitxer
            // ha d'existir JA al disc quan s'executa (si no: «No such file»).
            // La materialització del frontend reescriurà el definitiu després.
            // L'escriptura al disc és I/O bloquejant: es fa en un fil a part
            // perquè el runtime async (streaming del xat) no quedi aturat.
            let app_pre = app.clone();
            let text_pre = text.clone();
            let pre = tauri::async_runtime::spawn_blocking(move || {
                let written = runtools::write_announced_files(&app_pre, &text_pre);
                // El que NO s'ha escrit també compta: sense aquesta llista,
                // una «@file:» amb ruta absoluta de fora del projecte es
                // descartava en silenci i el model la reemetia IDÈNTICA.
                let rejected = crate::commands::workspace::current_root()
                    .map(|r| runtools::rejected_write_paths(&r, &text_pre))
                    .unwrap_or_default();
                (written, rejected)
            })
            .await
            .unwrap_or_default();
            if !pre.0.is_empty() {
                report.push_str(&format!(
                    "RUN|RESULTAT| fitxers escrits al projecte abans d'executar: {}\n",
                    pre.0.join(", ")
                ));
            }
            if !pre.1.is_empty() {
                report.push_str(&format!(
                    "RUN|RESULTAT| ERROR: {} «@file:» NO s'han escrit perquè la ruta és FORA del projecte obert: {}. Reemet-les amb la ruta RELATIVA a l'arrel del projecte (mai absoluta, mai reproduir l'arrel dins la ruta). Si el fitxer és realment de fora, NoOrbit no hi pot escriure: digues-ho en prosa.\n",
                    pre.1.len(),
                    pre.1.join(", ")
                ));
            }
            // Edicions quirúrgiques «@edit:» (CERCA/NOU): aplicar-les TAMBÉ
            // abans d'executar perquè «RUN|python3 x.py» vega el codi ja
            // modificat. L'informe torna al model perquè corregeixi els CERCA
            // que no han coincidit.
            let app_ed = app.clone();
            let text_ed = text.clone();
            let edits = tauri::async_runtime::spawn_blocking(move || {
                runtools::apply_edit_blocks(&app_ed, &text_ed)
            })
            .await
            .unwrap_or_default();
            if !edits.is_empty() {
                report.push_str(&format!(
                    "RUN|RESULTAT| edicions aplicades: {}\n",
                    edits.join(" · ")
                ));
            }
            // BUCLE DE VERIFICACIÓ OBLIGATORI: el codi acabat d'escriure es
            // comprova amb EINES REALS (php -l / node --check) i els errors
            // es realimenten al model. MAI es reporta èxit d'escriptura amb
            // lint fallant sense informar-ne. Límit: 3 reintents per torn.
            let mut touched: Vec<String> = pre.0.clone();
            for b in runtools::extract_edit_blocks(&text) {
                if edits.iter().any(|r| r.contains("aplicades") && r.starts_with(b.path.as_str())) {
                    touched.push(b.path);
                }
            }
            touched.sort();
            touched.dedup();
            if !touched.is_empty() {
                if verify_retries < runtools::MAX_VERIFY_RETRIES {
                    if let Some(root) = crate::commands::workspace::current_root() {
                        let lint = tauri::async_runtime::spawn_blocking(move || {
                            let mut results = runtools::lint_paths_blocking(&root, &touched);
                            // Fase 3c: si hi ha sessió LSP ACTIVA per al
                            // llenguatge, els seus errors reals també es
                            // realimenten (sense spawnar cap servidor nou).
                            runtools::augment_with_active_lsp(&root, &mut results);
                            results
                        })
                        .await
                        .unwrap_or_default();
                        if lint.iter().any(|r| !r.ok) {
                            verify_retries += 1;
                        }
                        if let Some(msg) = runtools::verification_report(&lint, verify_retries) {
                            report.push_str("RUN|RESULTAT| ");
                            report.push_str(&msg);
                            report.push('\n');
                        }
                    }
                } else {
                    report.push_str(
                        "RUN|RESULTAT| Verificació: límit de 3 reintents assolit aquest torn; \
                         els errors de sintaxi anteriors no s'han pogut reverificar. NO declaris \
                         èxit sense corregir-los al codi.\n",
                    );
                }
            }
            report.push_str(&runtools::execute(&app, computer, ai, &runs, prompt).await);
            report.push('\n');
        }
        if !nb.is_empty() {
            report.push_str(&tools::execute(&app, &nb).await);
        }
        opts.history.push(HistoryTurn {
            role: "assistant".into(),
            content: text.clone(),
        });
        current = report;
    }
    // ── Aprenentatge local: es recorda el torn ──────────────────────────
    // La pregunta de l'usuari i la resposta de la IA queden al registre
    // local: serviran per destil·lar lliçons (en segon pla) i per exportar
    // un dataset per entrenar un model local (LoRA) més endavant.
    if learning_on {
        if let Some(q) = user_question {
            let (provider, model) = ai.session_labels(sid).await;
            learning.record(&q, &text, &provider, &model);
            // Destil·lació en segon pla: l'IA aprén sola, el xat NO espera.
            crate::commands::learning::maybe_auto_distill(
                app.clone(),
                ai.clone(),
                learning.clone(),
                cfg.clone(),
            );
        }
    }
    // Tanca el torn de snapshot. La resposta final pot portar codi que el
    // frontend materialitzarà DESPRÉS (``` o @file:/@edit:/@delete:/@dir:):
    // el baseline es conserva per poder desfer eixe pas. Si només era prosa
    // i el disc no ha canviat, el snapshot es descarta.
    if let Some(root) = snap_root {
        let may_write = has_file_blocks(&text)
            || has_code_fence(&text)
            || text.contains("@edit:")
            || text.contains("@delete:")
            || text.contains("@esborra:")
            || text.contains("@dir:");
        // Rutes que el frontend materialitzarà DESPRÉS d'aquest torn (blocs
        // @file:/@edit: de la resposta final): es registren al snapshot perquè
        // «Desfés el pas» puga esborrar-les (i només eixes, no les de l'usuari).
        let mut final_written: Vec<String> = runtools::extract_file_blocks(&text)
            .into_iter()
            .map(|(p, _)| p)
            .collect();
        for b in runtools::extract_edit_blocks(&text) {
            final_written.push(b.path);
        }
        let _ = tauri::async_runtime::spawn_blocking(move || {
            crate::agent::snapshot::end_turn(&root, may_write, &final_written);
        })
        .await;
    }
    Ok(text)
}

/// Verificació de sintaxi BARATA per a la UI i els bucles d'IA: `php -l`
/// (.php) i `node --check` (.js/.mjs/.cjs) sobre rutes RELATIVES dins el
/// projecte, amb contenció (rutes fora del projecte es rebutgen). Altres
/// extensions → `ok:true, output:""`. Si l'eina no és instal·lada, s'omet
/// amb nota. Contracte: `invoke("lint_paths", { projectPath, paths })` →
/// `Vec<{ path, ok, output }>`.
#[command]
pub async fn lint_paths(
    project_path: String,
    paths: Vec<String>,
) -> Result<Vec<crate::agent::runtools::LintResult>, String> {
    let root = std::path::PathBuf::from(project_path.trim());
    if !root.is_dir() {
        return Err(format!("la carpeta del projecte no existeix: {}", root.display()));
    }
    // Els processos de lint són bloquejants: en un fil a part per no aturar
    // el runtime async de la UI.
    tauri::async_runtime::spawn_blocking(move || {
        crate::agent::runtools::lint_paths_blocking(&root, &paths)
    })
    .await
    .map_err(|e| e.to_string())
}

/// La pregunta «real» de l'usuari dins del prompt enviat. El frontend la
/// passa explícitament; si no, es treu del final del prompt (on el frontend
/// posa «PREGUNTA / TASCA:»).
pub fn extract_question(prompt: &str, question: Option<&str>) -> Option<String> {
    if let Some(q) = question {
        let t = q.trim();
        if !t.is_empty() {
            return Some(t.chars().take(2000).collect());
        }
    }
    if let Some(idx) = prompt.rfind("PREGUNTA / TASCA:") {
        let tail = prompt[idx + "PREGUNTA / TASCA:".len()..].trim();
        if !tail.is_empty() {
            return Some(tail.chars().take(2000).collect());
        }
    }
    // Sense marca: si el prompt és curt, el mateix és la pregunta.
    if prompt.chars().count() <= 800 {
        let t = prompt.trim();
        if !t.is_empty() {
            return Some(t.to_string());
        }
    }
    None
}

#[command]
pub async fn send_prompt(
    app: AppHandle,
    state: State<'_, AppState>,
    prompt: String,
    system: Option<String>,
    session: Option<String>,
    provider: Option<String>,
    model: Option<String>,
    history: Option<Vec<HistoryTurn>>,
    images: Option<Vec<String>>,
    question: Option<String>,
) -> Result<String, String> {
    let ai: Arc<AiManager> = state.ai_manager.clone();
    let opts = ChatOpts {
        provider,
        model,
        history: history.unwrap_or_default(),
        images: images.unwrap_or_default(),
        long_run: false,
    };
    // Dins del xat: els events i la cancel·lació són d'aquell xat, no globals.
    chat_with_tools(
        app,
        &ai,
        &state.computer,
        &state.learning,
        &state.config,
        &prompt,
        system.as_deref(),
        opts,
        &sid_of(session),
        false,
        question.as_deref(),
    )
    .await
}

/// Xat CRU: cap eina, cap directriu, cap execució — només el text. Es fa
/// servir per generar el contingut d'UN fitxer (a trossos): com que no hi ha
/// eines connectades, el model no pot llançar cap ordre «RUN|» per accident
/// ni escriure res al projecte pel camí.
#[command]
pub async fn ai_chat_plain(
    state: State<'_, AppState>,
    prompt: String,
    system: Option<String>,
    provider: Option<String>,
    model: Option<String>,
) -> Result<String, String> {
    let ai: Arc<AiManager> = state.ai_manager.clone();
    let opts = ChatOpts {
        provider,
        model,
        history: Vec::new(),
        images: Vec::new(),
        long_run: false,
    };
    ai.chat_opts(&prompt, system.as_deref(), &opts)
        .await
        .map_err(|e| e.to_string())
}

/// Mode del motor de la IA («auto», «local», «cloud»). La UI desa la
/// preferència i la sincronitza ací a l'arrencada i en cada canvi.
#[command]
pub async fn ai_engine_mode_get(state: State<'_, AppState>) -> Result<String, String> {
    Ok(state.ai_manager.engine_mode().await)
}

/// Fixa el mode del motor. Valors desconeguts cauen a «auto» (mai trenca
/// el funcionament local). Retorna el mode normalitzat aplicat.
#[command]
pub async fn ai_engine_mode_set(
    state: State<'_, AppState>,
    mode: String,
) -> Result<String, String> {
    state.ai_manager.set_engine_mode(&mode).await;
    Ok(state.ai_manager.engine_mode().await)
}

/// Envia un prompt i va emetent fragments amb l'event «ai://chunk» mentre el
/// model genera, per que la interficie mostre el text en directe. Retorna el
/// text complet al final (igual que `send_prompt`).
/// `session`/`provider`/`model`/`history` permeten que cada xat treballe en
/// paral·lel amb la seua IA (local o remota) i continue una conversa existent
/// o un «fork» d'una altra.
#[command]
pub async fn send_prompt_stream(
    app: AppHandle,
    state: State<'_, AppState>,
    prompt: String,
    system: Option<String>,
    session: Option<String>,
    provider: Option<String>,
    model: Option<String>,
    history: Option<Vec<HistoryTurn>>,
    images: Option<Vec<String>>,
    question: Option<String>,
) -> Result<String, String> {
    let ai: Arc<AiManager> = state.ai_manager.clone();
    let opts = ChatOpts {
        provider,
        model,
        history: history.unwrap_or_default(),
        images: images.unwrap_or_default(),
        long_run: false,
    };
    // També en streaming: si el model escriu directrius «NB|…» o «RUN|…»,
    // s'executen i la segona resposta continua al mateix xat (els chunks
    // arriben igual).
    chat_with_tools(
        app,
        &ai,
        &state.computer,
        &state.learning,
        &state.config,
        &prompt,
        system.as_deref(),
        opts,
        &sid_of(session),
        true,
        question.as_deref(),
    )
    .await
}

#[command]
pub async fn list_providers(state: State<'_, AppState>) -> Result<Vec<crate::ai::ProviderInfo>, String> {
    let ai = state.ai_manager.clone();
    Ok(ai.list_providers().await)
}

#[command]
pub async fn set_active_provider(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let ai = state.ai_manager.clone();
    ai.set_active_provider(&id).await;
    Ok(())
}

#[command]
pub async fn ollama_is_running(state: State<'_, AppState>) -> Result<bool, String> {
    let ai = state.ai_manager.clone();
    Ok(ai.is_running().await)
}

/// Model INICIAL recomanat per a aquesta màquina (segons la RAM): el que
/// NoOrbit baixa automàticament quan Ollama no té cap model. La interfície
/// l'ofereix com a «Baixa el model inicial» amb un clic.
#[command]
pub async fn ollama_starter_model() -> Result<String, String> {
    Ok(AiManager::starter_model_name())
}

/// Baixa el model inicial d'Ollama (el primer cop només). Retorna el nom
/// baixat; si ja n'hi havia un, retorna el que hi ha.
#[command]
pub async fn ollama_pull_starter(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let ai = state.ai_manager.clone();
    match ai.pull_starter_model(&app).await {
        Some(name) => Ok(name),
        None => {
            // Pot ser que ja tingui models (res a fer) o que la baixada falli.
            let models = ai.list_models().await.unwrap_or_default();
            match models.first() {
                Some(m) => Ok(m.name.clone()),
                None => Err("No s'ha pogut baixar el model inicial (xarxa o espai al disc).".into()),
            }
        }
    }
}

#[command]
pub async fn ollama_list_models(state: State<'_, AppState>) -> Result<Vec<OllamaModel>, String> {
    let ai = state.ai_manager.clone();
    ai.list_models().await.map_err(|e| e.to_string())
}

#[command]
pub async fn ollama_pull_model(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
    // Segon clic conscient quan l'encaix diu que NO hi cap: baixa'l igualment.
    force: Option<bool>,
) -> Result<String, String> {
    use tauri::Emitter;
    if force == Some(true) {
        crate::ai::AiManager::autoritza_baixada(&name);
    }
    // Abans de descarregar: si hi ha un disc extern configurat, assegurem que el
    // servidor l'utilitzi (si cal, l'arrenca o el reinicia amb OLLAMA_MODELS).
    // Sense això, un Ollama en marxa iniciat des del Finder baixaria els
    // models al disc intern malgrat la configuració.
    let nota = tokio::task::spawn_blocking(crate::ai::ensure_models_dir_applied)
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    if let Some(msg) = nota {
        let _ = app.emit(
            "ai://pull",
            serde_json::json!({ "model": name, "message": msg }),
        );
    }
    let ai = state.ai_manager.clone();
    ai.pull_model(Some(&app), &name).await.map_err(|e| e.to_string())
}

#[command]
pub async fn ollama_delete_model(state: State<'_, AppState>, name: String) -> Result<(), String> {
    let ai = state.ai_manager.clone();
    ai.delete_model(&name).await.map_err(|e| e.to_string())
}

/// Models que Ollama té carregats a la RAM ara mateix (per poder alliberar-los).
#[command]
pub async fn ollama_loaded_models(
    state: State<'_, AppState>,
) -> Result<Vec<crate::ai::LoadedModel>, String> {
    let ai = state.ai_manager.clone();
    ai.loaded_models().await.map_err(|e| e.to_string())
}

/// Allibera de la RAM un model concret (o tots els carregats si no se'n tria cap).
#[command]
pub async fn ollama_unload_models(
    state: State<'_, AppState>,
    model: Option<String>,
) -> Result<usize, String> {
    let ai = state.ai_manager.clone();
    ai.unload_models(model.as_deref())
        .await
        .map_err(|e| e.to_string())
}

/// ALLIBERA la memòria que NoOrbit ja NO necessita per funcionar.
///
/// Fa tres coses d'un sol cop:
/// 1. Descarrega els models que Ollama té a la RAM (`keep_alive: 0`).
/// 2. Atura els motors locals (llama.cpp) que tenen el model carregat: cada
///    `llama-server` es menja gigabytes.
/// 3. Ho mesura (memòria usada abans i després) perquè l'usuari vegi si
///    realment s'ha recuperat alguna cosa.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RamFreed {
    /// Models d'Ollama descarregats de la RAM.
    pub models: usize,
    /// Motors locals (llama.cpp) aturats.
    pub engines: usize,
    /// Memòria usada abans (bytes).
    pub used_before: u64,
    /// Memòria usada després (bytes).
    pub used_after: u64,
    /// Bytes recuperats.
    pub freed: u64,
}

#[command]
pub async fn ai_free_ram(state: State<'_, AppState>) -> Result<RamFreed, String> {
    let used_before = crate::commands::system::system_memory_status()
        .await
        .map(|m| m.used)
        .unwrap_or(0);
    // 1) Models d'Ollama a la RAM.
    let models = state.ai_manager.unload_models(None).await.unwrap_or(0);
    // 2) Motors locals: cada `llama-server` té el model sencer a memòria.
    let engines = crate::ai::llamacpp::servers().len();
    crate::ai::llamacpp::stop_all();
    // 3) Mesura: el SO retorna la memòria a poc a poc, donem-li un moment.
    tokio::time::sleep(std::time::Duration::from_millis(700)).await;
    let used_after = crate::commands::system::system_memory_status()
        .await
        .map(|m| m.used)
        .unwrap_or(used_before);
    Ok(RamFreed {
        models,
        engines,
        used_before,
        used_after,
        freed: used_before.saturating_sub(used_after),
    })
}

/// MEMÒRIA que està usant la IA LOCAL ara mateix.
///
/// L'usuari ha de poder veure quanta RAM es consumeix de veritat: el pes del
/// model que està carregat a memòria (Ollama), els motors locals (llama.cpp)
/// i el consum TOTAL del sistema. Sense aquestes dades el panell només
/// mostrava el nom de la IA i l'usuari no podia saber si el model es menjava
/// 2 GB o 16 GB.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalMemory {
    /// RAM total del sistema (bytes).
    pub total: u64,
    /// RAM usada pel sistema ara mateix (bytes).
    pub used: u64,
    /// Models d'Ollama que són a la memòria ara mateix.
    pub models: Vec<crate::ai::LoadedModel>,
    /// Pes total dels models carregats (bytes).
    pub model_bytes: u64,
    /// Motors locals (llama.cpp) en marxa.
    pub engines: usize,
    /// L'IA activa és local (Ollama)?
    pub local: bool,
}

#[command]
pub async fn ai_local_memory(state: State<'_, AppState>) -> Result<LocalMemory, String> {
    let mem = crate::commands::system::system_memory_status().await?;
    // Si Ollama no respon, la llista és buida: el panell ho mostra com a
    // «cap model carregat» en lloc de fallar (la memòria del sistema encara
    // es pot ensenyar).
    let models = state.ai_manager.loaded_models().await.unwrap_or_default();
    let model_bytes = models.iter().map(|m| m.size).sum();
    Ok(LocalMemory {
        total: mem.total,
        used: mem.used,
        models,
        model_bytes,
        engines: crate::ai::llamacpp::servers().len(),
        local: state.ai_manager.active_provider().await == "ollama",
    })
}

/// Quant de temps Ollama manté els models a la memòria (buit = «1m» recomanat).
#[command]
pub async fn ollama_get_keep_alive(state: State<'_, AppState>) -> Result<String, String> {
    Ok(state.ai_manager.keep_alive().await.unwrap_or_default())
}

/// Canvia el temps que els models romanen a la RAM i ho desa a la configuració.
#[command]
pub async fn ollama_set_keep_alive(
    state: State<'_, AppState>,
    value: Option<String>,
) -> Result<(), String> {
    let clean = value
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    let mut cfg = crate::config::AppConfig::load().map_err(|e| e.to_string())?;
    cfg.ai.keep_alive = clean.clone().unwrap_or_default();
    cfg.save().map_err(|e| e.to_string())?;
    state.ai_manager.set_keep_alive(clean).await;
    Ok(())
}

#[command]
pub async fn ollama_suggested_models() -> Result<Vec<serde_json::Value>, String> {
    Ok(AiManager::suggested_models())
}

/// PREDICCIÓ D'ENCAIX d'un model concret en aquest equip (idea de `soup
/// doctor` / hardware-fit): la interfície la consulta per als resultats de
/// cerca que NoOrbit no té a la seva taula, així el xip «NO hi cap» apareix
/// ABANS de prémer «Baixa». Sense mida ni RAM no es fingeix res: el veredicte
/// és «desconegut» i el motiu diu què falta.
#[command]
pub async fn ai_fit_check(
    name: Option<String>,
    size_bytes: Option<u64>,
) -> Result<serde_json::Value, String> {
    let mida = size_bytes.or_else(|| name.as_deref().and_then(AiManager::mida_sugerida));
    let e = crate::ai::fit::comprova(
        mida,
        crate::ai::fit::ram_total_aqui(),
        crate::ai::fit::ram_lliure_aqui(),
        crate::ai::fit::CONTEXT_ESTIMAT,
    );
    // S'hi afegeix l'etiqueta curta: els xips de la UI (Hugging Bay) mostren el
    // mateix text que el catàleg, i el text el decideix `ai::fit`, no el front.
    let mut v = e.to_json();
    if let Some(o) = v.as_object_mut() {
        o.insert(
            "etiqueta".to_string(),
            serde_json::Value::String(e.etiqueta()),
        );
    }
    Ok(v)
}

/// Encaix d'un model de Hugging Bay ABANS de baixar-lo. La mida real és la del
/// fitxer GGUF; si el mirror a més dona la seva xifra de RAM, es respecta (diu
/// més que la nostra aproximació). `None` = es pot baixar; `Some(missatge)` =
/// no hi cap i cal un segon clic conscient.
fn encaix_bay(det: &crate::ai::hf_bay::BayDetails, force: Option<bool>) -> Option<String> {
    use crate::ai::fit;
    if force == Some(true) {
        return None;
    }
    let mida = det
        .recommended
        .as_ref()
        .and_then(|f| f.size)
        .or(det.model.size);
    let e = fit::comprova(
        mida,
        fit::ram_total_aqui(),
        fit::ram_lliure_aqui(),
        fit::CONTEXT_ESTIMAT,
    );
    // El mirror també calcula quanta RAM demana: si en demana més del sostre
    // d'aquest equip, tampoc hi cap (i ho diu ell, no ho endevinem nosaltres).
    let demana = det.ram_gb.filter(|g| *g > 0);
    let demaisa = demana.map(|g| (g * 1_000_000_000) > e.sostre && e.sostre > 0);
    if e.verdicte == fit::Verdicte::NoCap || demaisa == Some(true) {
        let mut m = e.missatge();
        if demaisa == Some(true) {
            m = format!(
                "{} La font demana {} GB de RAM per aquest model.",
                m,
                demana.unwrap_or(0)
            );
        }
        Some(format!("{} Prem de nou per baixar-lo IGUALMENT.", m))
    } else {
        None
    }
}

/// Llista/cerca models al NÚVOL d'ollama.com. No requereix Ollama en marxa
/// ni genera tokens: només descarrega la pàgina de la biblioteca.
#[command]
pub async fn ollama_search_cloud(
    state: State<'_, AppState>,
    query: Option<String>,
) -> Result<Vec<crate::ai::CloudModel>, String> {
    let ai = state.ai_manager.clone();
    ai.search_cloud(query.as_deref().unwrap_or(""))
        .await
        .map_err(|e| e.to_string())
}

/// Cerca models a INTERNET (Hugging Face) per si NO són a Ollama. Retorna noms
/// «hf.co/…» que es poden baixar en natiu amb `ollama_pull_model`.
#[command]
pub async fn ollama_search_huggingface(
    state: State<'_, AppState>,
    query: String,
) -> Result<Vec<crate::ai::CloudModel>, String> {
    let ai = state.ai_manager.clone();
    ai.search_huggingface(&query)
        .await
        .map_err(|e| e.to_string())
}

/// Cerca models d'imatge a CivitAI (tercera font, amb URL de descàrrega
/// directa cap al rodet de CPU «sd-models/»).
#[command]
pub async fn civitai_search(
    state: State<'_, AppState>,
    query: String,
) -> Result<Vec<crate::ai::CloudModel>, String> {
    let ai = state.ai_manager.clone();
    ai.search_civitai(&query).await.map_err(|e| e.to_string())
}

/// Estat del motor local sense Ollama (llama.cpp) i els models GGUF ja
/// baixats al disc per poder-los engegar.
#[derive(Debug, Clone, Serialize)]
pub struct LlamaCppStatus {
    /// La llama.cpp està instal·lada (hi ha «llama-server»).
    pub installed: bool,
    /// On és el binari.
    pub binary: Option<String>,
    /// Models GGUF baixats (ruta + mida).
    pub models: Vec<(String, u64)>,
    /// Servidors locals en marxa.
    pub running: Vec<crate::ai::llamacpp::LocalServer>,
    /// Carpeta on es desen els models GGUF (pot ser un disc extern).
    pub models_dir: String,
    /// Carpeta on viu el motor local (pot ser un disc extern).
    pub engine_dir: String,
}

/// TRIA les carpetes del motor local i dels models GGUF: permet posar-los
/// en un DISC EXTERN (USB, HDD) i treballar des d'allà. Una carpeta buida
/// torna al comportament per defecte (dades de NoOrbit).
#[command]
pub async fn llamacpp_set_dirs(
    models_dir: Option<String>,
    engine_dir: Option<String>,
) -> Result<String, String> {
    let mut cfg = crate::config::AppConfig::load().map_err(|e| e.to_string())?;
    if let Some(d) = models_dir {
        let d = d.trim().to_string();
        if !d.is_empty() && std::fs::create_dir_all(&d).is_err() {
            return Err(format!("No puc crear la carpeta «{}».", d));
        }
        cfg.ai.gguf_dir = d;
    }
    if let Some(d) = engine_dir {
        let d = d.trim().to_string();
        if !d.is_empty() && std::fs::create_dir_all(&d).is_err() {
            return Err(format!("No puc crear la carpeta «{}».", d));
        }
        cfg.ai.llamacpp_dir = d;
    }
    cfg.save().map_err(|e| e.to_string())?;
    let mut msg = String::new();
    if cfg.ai.gguf_dir.is_empty() {
        msg.push_str("Els models GGUF es desen a les dades de NoOrbit.");
    } else {
        msg.push_str(&format!("Models GGUF: {}. ", cfg.ai.gguf_dir));
    }
    if cfg.ai.llamacpp_dir.is_empty() {
        msg.push_str("Motor local a les dades de NoOrbit.");
    } else {
        msg.push_str(&format!("Motor local: {}.", cfg.ai.llamacpp_dir));
    }
    Ok(msg)
}

#[command]
pub async fn llamacpp_status() -> Result<LlamaCppStatus, String> {
    let bin = crate::ai::llamacpp::binary();
    let cfg = crate::config::AppConfig::load().ok();
    Ok(LlamaCppStatus {
        installed: bin.is_some(),
        binary: bin.map(|p| p.to_string_lossy().to_string()),
        models: crate::ai::llamacpp::downloaded_models(),
        running: crate::ai::llamacpp::servers(),
        models_dir: cfg
            .as_ref()
            .map(|c| c.ai.gguf_dir.clone())
            .unwrap_or_default(),
        engine_dir: cfg
            .as_ref()
            .map(|c| c.ai.llamacpp_dir.clone())
            .unwrap_or_default(),
    })
}

/// INSTAL·LA la llama.cpp (el motor local sense Ollama) amb els binaris
/// oficials de GitHub — funciona a tothom, també on Homebrew no en té
/// (macOS vell) o on l'usuari no vol installar res a mà. Si el binari ja hi
/// és, no fa res.
#[command]
pub async fn llamacpp_install(
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<String, String> {
    if let Some(bin) = crate::ai::llamacpp::binary() {
        return Ok(format!("La llama.cpp ja hi és: {}", bin.to_string_lossy()));
    }
    let http = state.ai_manager.http_client();
    let bin = crate::ai::llamacpp::install_from_github(&http, Some(&app))
        .await
        .map_err(|e| e.to_string())?;
    Ok(format!(
        "llama.cpp instal·lada a {} — ja pots fer córrer els teus models en local.",
        bin.to_string_lossy()
    ))
}

/// Registra un servei local (llama.cpp) com a PROVEÏDOR del xat, de manera
/// que l'usuari el pugui triar com qualsevol altre IA (sense cap clau: és
/// local).
fn register_local_provider(
    state: &AppState,
    id: &str,
    name: &str,
    base: &str,
    model: &str,
) -> Result<(), String> {
    let mut api = state.api_manager.lock().map_err(|e| e.to_string())?;
    let item = crate::api::AiProvider {
        id: id.to_string(),
        name: name.to_string(),
        base_url: base.to_string(),
        model: model.to_string(),
        kind: "openai".to_string(),
        auth: crate::api::AuthScheme::None,
        header_name: String::new(),
        token: String::new(),
        enabled: true,
    };
    // Si ja existia (servidor reiniciat), l'actualitzem; si no, l'afegim.
    if api.list().iter().any(|p| p.id == id) {
        let _ = api.update(
            id,
            Some(item.name.clone()),
            Some(item.base_url.clone()),
            Some(item.model.clone()),
            None,
            // L'autenticació S'HI POSA explícitament: és un servei LOCAL i no
            // demana cap clau (amb «Bearer» buit alguns servidors hi responen
            // amb 401 i el model sembla «que no es pot fer servir»).
            Some(crate::api::AuthScheme::None),
            None,
            None,
            None,
        );
    } else {
        let _ = api.add(item);
    }
    Ok(())
}

/// Engega un model GGUF amb llama.cpp (sense Ollama) i registra el servei
/// local com a proveïdor, perquè el xat el pugui fer servir.
#[command]
pub async fn llamacpp_start(
    state: State<'_, AppState>,
    model_path: String,
    port: Option<u16>,
    ctx: Option<u32>,
) -> Result<String, String> {
    let http = state.ai_manager.http_client();
    let port = port.unwrap_or(crate::ai::llamacpp::DEFAULT_PORT);
    let ctx = ctx.unwrap_or(4096);
    let (id, name, base) = crate::ai::llamacpp::start(&http, &model_path, port, ctx)
        .await
        .map_err(|e| e.to_string())?;

    // Registrem el servei local com a proveïdor (sense clau: és local).
    let model = name.rsplit("· ").next().unwrap_or("").trim().to_string();
    register_local_provider(&state, &id, &name, &base, &model)?;
    Ok(format!("{} · {}", name, base))
}

/// Atura un servidor local de llama.cpp i en treu el proveïdor.
#[command]
pub async fn llamacpp_stop(state: State<'_, AppState>, port: u16) -> Result<bool, String> {
    let aturat = crate::ai::llamacpp::stop(port);
    let mut api = state.api_manager.lock().map_err(|e| e.to_string())?;
    let id = format!("llamacpp_{}", port);
    let _ = api.remove(&id);
    Ok(aturat)
}

/// ESBORRA un model GGUF instal·lat: atura el servidor que l'estigui
/// servint (i en treu el proveïdor) i esborra el fitxer del disc. Sense
/// això, un model baixat ocupava gigues per sempre i no hi havia manera
/// de treure'l.
#[command]
pub async fn llamacpp_remove_model(
    state: State<'_, AppState>,
    path: String,
) -> Result<String, String> {
    // 1) Servidors que serveixen AQUEST model: s'aturen i el seu proveïdor
    //    es treu de la llista (si no, quedaria apuntant a un servei mort).
    let ports: Vec<u16> = crate::ai::llamacpp::servers()
        .into_iter()
        .filter(|s| s.path == path)
        .map(|s| s.port)
        .collect();
    if !ports.is_empty() {
        let mut api = state.api_manager.lock().map_err(|e| e.to_string())?;
        for p in ports {
            let _ = crate::ai::llamacpp::stop(p);
            let _ = api.remove(&format!("llamacpp_{}", p));
        }
    }
    // 2) El fitxer del disc.
    crate::ai::llamacpp::remove_model_file(&path)?;
    let nom = std::path::Path::new(&path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.clone());
    Ok(format!("Model esborrat: {}", nom))
}

/// Cerca models GGUF a HUGGING BAY (huggingbay.xyz), el mirror verificat de
/// Hugging Face. Els resultats es poden baixar i queden registrats a Ollama.
#[command]
pub async fn hf_bay_search(
    state: State<'_, AppState>,
    query: String,
    limit: Option<usize>,
) -> Result<Vec<crate::ai::CloudModel>, String> {
    let http = state.ai_manager.http_client();
    let models = crate::ai::hf_bay::search(&http, &query, limit.unwrap_or(20))
        .await
        .map_err(|e| e.to_string())?;
    Ok(models
        .iter()
        .map(crate::ai::hf_bay::a_cloud_model)
        .collect())
}

/// PÀGINA de resultats amb ordenació i cursor: és el que permet el «menú
/// Hugging Bay» amb els 5 més descarregats de cada pàgina i la paginació.
#[derive(Debug, Clone, Serialize)]
pub struct BayPage {
    pub models: Vec<crate::ai::hf_bay::BayModel>,
    /// Cursor de la pàgina següent (buit = no hi ha més).
    pub next_cursor: Option<String>,
    /// Total de models que hi ha al mirror.
    pub total: u64,
}

#[command]
pub async fn hf_bay_page(
    state: State<'_, AppState>,
    query: Option<String>,
    sort: Option<String>,
    cursor: Option<String>,
    limit: Option<usize>,
) -> Result<BayPage, String> {
    let http = state.ai_manager.http_client();
    let (models, next, total) = crate::ai::hf_bay::search_page(
        &http,
        query.as_deref().unwrap_or(""),
        sort.as_deref().unwrap_or("best"),
        cursor.as_deref(),
        limit.unwrap_or(5),
    )
    .await
    .map_err(|e| e.to_string())?;
    Ok(BayPage {
        models,
        next_cursor: next,
        total,
    })
}

/// Detalls d'un model de Hugging Bay: fitxer recomanat, mida, SHA-256 i
/// l'enllaç de descàrrega signat. Amb `quant` es demana una quantització
/// concreta (p. ex. «Q4_K_M»).
#[command]
pub async fn hf_bay_details(
    state: State<'_, AppState>,
    id: String,
    quant: Option<String>,
) -> Result<crate::ai::hf_bay::BayDetails, String> {
    let http = state.ai_manager.http_client();
    crate::ai::hf_bay::details(&http, &id, quant.as_deref())
        .await
        .map_err(|e| e.to_string())
}

/// DESCARREGA un model de Hugging Bay i l'INSTAL·LA **NATIVAMENT**: es
/// guarda el GGUF i s'engega amb llama.cpp (el motor de GGUF), de manera que
/// el model es pot fer servir al xat **sense tenir Ollama instal·lat**.
///
/// Si la llama.cpp no hi és, NoOrbit la instal·la (binaris oficials) i
/// després engega el model. El servei es registra com a proveïdor local.
#[command]
pub async fn hf_bay_install_native(
    state: State<'_, AppState>,
    app: AppHandle,
    id: String,
    quant: Option<String>,
    // Segon clic conscient quan l'encaix diu que el model NO hi cap.
    force: Option<bool>,
) -> Result<BayDownloadResult, String> {
    use tauri::Emitter;
    let http = state.ai_manager.http_client();
    let avisa = |m: &str| {
        let _ = app.emit(
            "ai://pull",
            serde_json::json!({ "model": format!("huggingbay:{}", id), "message": m }),
        );
    };
    // Abans de baixar res: el model serveix per XATAR? Els d'embeddings
    // ocupen centenars de MB i no serveixen per a res dins de NoOrbit.
    {
        let det = crate::ai::hf_bay::details(&http, &id, quant.as_deref())
            .await
            .map_err(|e| e.to_string())?;
        if let Some(w) = crate::ai::hf_bay::avisa_si_no_xateja(&det.model) {
            return Err(w);
        }
        // I ARA S'HI CAP? Es comprova ABANS de baixar un GGUF de gigabytes que
        // aquest equip no podrà mai carregar (el motor natiu llama.cpp no té
        // el marge d'Ollama: necessita el model + el context a la RAM).
        if let Some(w) = encaix_bay(&det, force) {
            return Err(w);
        }
    }
    avisa("descarregant el model de Hugging Bay…");
    let (det, path) = crate::ai::hf_bay::fetch_gguf(&http, Some(&app), &id, quant.as_deref())
        .await
        .map_err(|e| e.to_string())?;
    let nom = crate::ai::hf_bay::ollama_name(&det.model.repo);

    // El motor local (llama.cpp): si no hi és, l'instal·lem nosaltres.
    if !crate::ai::llamacpp::installed() {
        avisa("instal·lant el motor local (llama.cpp)…");
        crate::ai::llamacpp::install_from_github(&http, Some(&app))
            .await
            .map_err(|e| format!("No s'ha pogut instal·lar el motor local: {}", e))?;
    }

    // Engega el model i registra'l com a proveïdor del xat.
    avisa(&format!("engegant «{}»…", nom));
    let port = crate::ai::llamacpp::DEFAULT_PORT;
    let (prov_id, name, base) = crate::ai::llamacpp::start(
        &http,
        &path.to_string_lossy(),
        port,
        4096,
    )
    .await
    .map_err(|e| {
        format!(
            "He baixat «{}» a {}, però no l'he pogut engegar: {}",
            nom,
            path.to_string_lossy(),
            e
        )
    })?;
    let model = name.rsplit("· ").next().unwrap_or("").trim().to_string();

    // POT XATEJAR? Els models d'EMBEDDINGS (vectors) i altres tipus
    // especialitzats s'engeuen igual, però tota petició de xat acaba amb
    // «does not support logits computation»: l'usuari tria el model al
    // selector i sembla que «no deixa fer-lo servir». Ho comprovem i, si no
    // serveix, NO el registrem i ho expliquem.
    avisa("comprovant que el model serveix per xatar…");
    if let Some(false) = crate::ai::llamacpp::chat_capability(&http, port, &model).await {
        // S'atura el servidor: no serveix per a res i ocupa memòria.
        let _ = crate::ai::llamacpp::stop(port);
        return Ok(BayDownloadResult {
            ok: false,
            model: None,
            message: format!(
                "«{}» és un model d'EMBEDDINGS: serveix per calcular vectors, no per xatejar. \
                 L'he baixat al disc, però NO el registro com a IA (serva l'ha deixat aturat). \
                 Per crear temes i altres tasques necessites un model DE XAT: baixa un de \
                 text-generation (p. ex. «SmolLM2-135M-Instruct» o \
                 «Qwen2.5-Coder-7B-Instruct-Q4_K_M»).",
                nom
            ),
        });
    }

    register_local_provider(&state, &prov_id, &name, &base, &model)?;

    Ok(BayDownloadResult {
        ok: true,
        model: Some(nom.clone()),
        message: format!(
            "«{}» instal·lat NATIVAMENT (llama.cpp, sense Ollama) i engegat a {}. \
             Tria'l al selector de proveïdor del xat i ja hi podràs parlar.",
            nom, base
        ),
    })
}

/// DESCARREGA un model de Hugging Bay (amb progrés) i el registra a Ollama
/// perquè aparegui al gestor de models i es pugui usar al xat. Retorna el nom
/// del model ja instal·lat.
/// Resultat de la descàrrega d'un model de Hugging Bay.
#[derive(Debug, Clone, Serialize)]
pub struct BayDownloadResult {
    /// Cert si el model ja es pot fer servir (registrat a Ollama).
    pub ok: bool,
    /// Nom del model a Ollama (si s'ha pogut registrar).
    pub model: Option<String>,
    /// Explicació per a l'usuari (progrés, avisos o instruccions).
    pub message: String,
}

#[command]
pub async fn hf_bay_download(
    state: State<'_, AppState>,
    app: AppHandle,
    id: String,
    quant: Option<String>,
    // Segon clic conscient quan l'encaix diu que el model NO hi cap.
    force: Option<bool>,
) -> Result<BayDownloadResult, String> {
    use tauri::Emitter;
    let http = state.ai_manager.http_client();
    let det = crate::ai::hf_bay::details(&http, &id, quant.as_deref())
        .await
        .map_err(|e| e.to_string())?;
    // Abans de baixar res: el model serveix per XATAR? (els d'embeddings
    // ocupen centenars de MB i no serveixen per a res dins de NoOrbit).
    if let Some(w) = crate::ai::hf_bay::avisa_si_no_xateja(&det.model) {
        return Err(w);
    }
    // I ARA S'HI CAP a la RAM d'aquest equip? Es diu ABANS de gastar la
    // descàrrega, no cinc minuts després de carregar-lo.
    if let Some(w) = encaix_bay(&det, force) {
        return Err(w);
    }
    let file = det
        .recommended
        .clone()
        .ok_or_else(|| "Aquest model no té cap fitxer descargable.".to_string())?;
    let url = file
        .download_url
        .clone()
        .ok_or_else(|| "El mirror no dona enllaç de descàrrega per a aquest model.".to_string())?;

    let _ = app.emit(
        "ai://pull",
        serde_json::json!({
            "model": format!("huggingbay:{}", id),
            "message": format!("descarregant {} des de Hugging Bay…", det.model.name)
        }),
    );

    let base = crate::ai::hf_bay::ollama_name(&det.model.repo);
    let file_name = file
        .path
        .rsplit('/')
        .next()
        .unwrap_or("model.gguf")
        .to_string();
    // Evitem el nom repetit («mxbai-embed-large-v1-mxbai-embed-large-v1-f16»).
    let dest_name = if file_name.to_lowercase().starts_with(&base.to_lowercase()) {
        file_name
    } else {
        format!("{}-{}", base, file_name)
    };
    let path = crate::ai::hf_bay::download_file(
        &http,
        Some(&app),
        &id,
        &url,
        &dest_name,
        file.sha256.as_deref(),
    )
    .await
    .map_err(|e| e.to_string())?;

    let nom = crate::ai::hf_bay::ollama_name(&det.model.repo);
    let _ = app.emit(
        "ai://pull",
        serde_json::json!({
            "model": format!("huggingbay:{}", id),
            "message": format!("registrant «{}» a Ollama…", nom)
        }),
    );
    // SENSE OLLAMA: el model es baixa i es conserva, i el motor NATIU
    // (llama.cpp) el pot fer servir igual. No cal instal·lar Ollama per usar
    // un model de Hugging Bay: el menú «Hugging Bay» ofereix les dues vies.
    if crate::ai::ollama_binary().is_none() {
        let _ = app.emit(
            "ai://pull",
            serde_json::json!({
                "model": format!("huggingbay:{}", id),
                "message": format!("«{}» baixat (el motor local el pot fer servir)", nom)
            }),
        );
        return Ok(BayDownloadResult {
            ok: false,
            model: None,
            message: format!(
                "He baixat «{}» a {}. No tens Ollama instal·lat, però el model és \
                 LLIBRE: prem «Instal·la nativament» (motor local llama.cpp) per \
                 poder-lo fer servir al xat sense instal·lar res més.",
                nom,
                path.to_string_lossy()
            ),
        });
    }

    crate::ai::hf_bay::register_in_ollama(&nom, &path)
        .await
        .map_err(|e| e.to_string())?;

    // El GGUF ja és dins d'Ollama: es pot esborrar la còpia per no tarar el
    // disc (Ollama en crea la seva pròpia).
    let _ = std::fs::remove_file(&path);

    let _ = app.emit(
        "ai://pull",
        serde_json::json!({
            "model": format!("huggingbay:{}", id),
            "message": format!("«{}» a punt!", nom)
        }),
    );
    Ok(BayDownloadResult {
        ok: true,
        model: Some(nom.clone()),
        message: format!("«{}» instal·lat i a punt!", nom),
    })
}

/// CANCEL·LA la descàrrega en curs d'un model de Hugging Bay (per id). El
/// bucle de descàrrega consulta el flag en cada fragment: atura, esborra el
/// «.part» i retorna un error clar. Els models pendents d'una cua es
/// cancel·len des del frontend (encara no han començat).
#[command]
pub async fn hf_bay_cancel(id: String) -> Result<(), String> {
    crate::ai::hf_bay::request_cancel(&id);
    Ok(())
}

/// Espai al disc destí dels models GGUF: `free_bytes` (lliure, per avisar ABANS
/// de baixar un model que no hi cap) i `total_bytes` (mida del disc/USB, per
/// mostrar-la a la interfície). Tots dos són null si el sistema no els pot donar.
#[derive(Debug, Clone, Serialize)]
pub struct BayDiskSpace {
    pub dir: String,
    pub free_bytes: Option<u64>,
    pub total_bytes: Option<u64>,
}

#[command]
pub async fn hf_bay_disk_space() -> Result<BayDiskSpace, String> {
    let dir = crate::ai::hf_bay::models_dir();
    let (free_bytes, total_bytes) = match crate::ai::hf_bay::disk_usage() {
        Some((free, total)) => (Some(free), Some(total)),
        None => (None, None),
    };
    Ok(BayDiskSpace {
        dir: dir.to_string_lossy().to_string(),
        free_bytes,
        total_bytes,
    })
}

/// Instal·la una ALTRA IA LOCAL apart d'Ollama: LM Studio, GPT4All, Jan
/// (cask de Homebrew en macOS; pàgina de descàrrega a Linux/Windows) o
/// llama.cpp (formula). NoOrbit les DETECTA sols un cop instal·lades
/// («Descobreix IA locals externes»).
#[command]
pub async fn install_local_ai(app: AppHandle, which: String) -> Result<String, String> {
    let (cask_names, url) = match which.as_str() {
        "lmstudio" => (Some("--cask lm-studio"), "https://lmstudio.ai/"),
        "gpt4all" => (Some("--cask gpt4all"), "https://gpt4all.io/"),
        "jan" => (Some("--cask jan"), "https://jan.ai/"),
        "llamacpp" => (Some("--formula llama.cpp"), "https://github.com/ggml-org/llama.cpp"),
        _ => return Err(format!("IA local desconeguda: {}", which)),
    };

    #[cfg(target_os = "macos")]
    {
        let _ = &app;
        if which::which("brew").is_ok() {
            let script = format!(
                "tell application \"Terminal\" to do script \"brew install {}\"",
                cask_names.unwrap_or_default()
            );
            std::process::Command::new("osascript")
                .arg("-e")
                .arg(&script)
                .spawn()
                .map_err(|e| format!("No s'ha pogut obrir el Terminal: {}", e))?;
            return Ok(format!(
                "S'ha obert el Terminal per instal·lar {}. Quan acabi, NoOrbit \
                 el detectarà automàticament.",
                which
            ));
        }
        std::process::Command::new("open")
            .arg(url)
            .spawn()
            .map_err(|e| format!("No s'ha pogut obrir el navegador: {}", e))?;
        return Ok("S'ha obert la pàgina de descàrrega.".into());
    }

    #[cfg(target_os = "linux")]
    {
        let _ = &app;
        // llama.cpp a Linux: es compila al moment (no hi ha paquet fàcil a
        // tothom). Obrem una terminal amb les comandes exactes.
        if which == "llamacpp" {
            let script = "git clone https://github.com/ggml-org/llama.cpp && cd llama.cpp && \
                         cmake -B build && cmake --build build --config Release -j && \
                         sudo cp build/bin/llama-server /usr/local/bin/";
            if let Ok(term) = which::which("gnome-terminal") {
                let _ = std::process::Command::new(term)
                    .arg("--")
                    .arg("bash")
                    .arg("-c")
                    .arg(script)
                    .spawn();
                return Ok("S'ha obert una terminal per compilar la llama.cpp (uns minuts). Quan acabi, NoOrbit la detectarà.".into());
            }
        }
        std::process::Command::new("xdg-open")
            .arg(url)
            .spawn()
            .map_err(|e| format!("No s'ha pogut obrir el navegador: {}", e))?;
        return Ok("S'ha obert la pàgina de descàrrega.".into());
    }

    #[cfg(windows)]
    {
        let _ = &app;
        std::process::Command::new("cmd")
            .arg("/C")
            .arg("start")
            .arg(url)
            .spawn()
            .map_err(|e| format!("No s'ha pogut obrir el navegador: {}", e))?;
        return Ok("S'ha obert la pàgina de descàrrega.".into());
    }

    #[allow(unreachable_code)]
    Err("Instal·lació no suportada en aquest sistema".into())
}

/// Diu si el binari/app d'Ollama és al sistema (per oferir instal·lar-lo).
#[command]
pub async fn ollama_installed() -> Result<bool, String> {
    Ok(crate::ai::ollama_installed())
}

/// Arrenca el servei `ollama serve` en segon pla (si està instal·lat),
/// aplicant la carpeta de models configurada (disc extern). Pot reiniciar
/// un Ollama ja en marxa si no la feia servir; per això es fa en blocking.
#[command]
pub async fn start_ollama() -> Result<String, String> {
    let msg = tokio::task::spawn_blocking(|| {
        crate::ai::start_ollama_server()?;
        // Si Ollama ja rodava sense la carpeta, start_ollama_server l'ha
        // reiniciat; ho confirmem amb el missatge de l'aplicació.
        anyhow::Ok(crate::ai::apply_models_dir(false).ok().flatten())
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;
    Ok(msg.unwrap_or_else(|| "Ollama és en marxa.".to_string()))
}

/// Instal·la Ollama. Obre el mètode adequat a cada sistema:
/// - macOS: Terminal amb `brew install ollama` si hi ha Homebrew; si no, la
///   pàgina de descàrrega oficial.
/// - Linux: Terminal amb l'instal·lador oficial (curl … | sh).
/// - Windows: pàgina de descàrrega oficial.
/// Sempre és transparent: l'usuari veu i autoritza el que s'executa.
#[command]
pub async fn install_ollama(app: AppHandle) -> Result<String, String> {
    // Instal·lació AUTOMÀTICA (sense Terminal ni navegador): descarrega
    // l'oficial d'ollama.com i l'instal·la, i després engega el servei.
    let msg = tokio::task::spawn_blocking(|| {
        let r = crate::ai::ensure_ollama_installed();
        if r.is_ok() {
            let _ = crate::ai::start_ollama_server();
        }
        r
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;
    // USUARI NOU: un Ollama acabat d'instal·lar no té CAP model (el primer
    // xat donaria 404). Baixem el model inicial EN SEGON PLA (progrés via
    // «ai://pull») sense fer esperar la interfície.
    use tauri::Manager as _;
    let state = app.state::<AppState>();
    let ai = state.ai_manager.clone();
    drop(state);
    let bg = app.clone();
    tauri::async_runtime::spawn(async move {
        ai.ensure_default_model(&bg).await;
    });
    Ok(format!("{} — servei arrencat en segon pla (port 11434).", msg))
}

// ── DeepSeek Harness (dsh): instal·lació autogestionada, sense admin ───────

/// Diu si DeepSeek Harness (dsh) està disponible: la instal·lació del
/// sistema (npm global, brew…) o la LOCAL que NoOrbit gestiona dins de les
/// seves dades (amb el seu propi Node.js, si al sistema no n'hi ha).
#[command]
pub async fn dsh_installed() -> Result<bool, String> {
    Ok(crate::external_orchestrator::dsh::dsh_installed())
}

/// Instal·la DeepSeek Harness automaticament: baixa el Node.js LTS oficial
/// (si no n'hi ha cap) i el paquet @deepseek-ai/dsh, tot DINS de les dades
/// de NoOrbit (o de NoOrbitData/ en mode portàtil/USB). No demanà
/// administració, no modifica el sistema i es pot emportar en un USB.
#[command]
pub async fn install_dsh(app: AppHandle) -> Result<String, String> {
    crate::external_orchestrator::dsh::install(app)
        .await
        .map_err(|e| e.to_string())
}

/// Arrenca la interfície web de dsh («dsh web», port 3080) en segon pla,
/// perquè l'usuari la configuri (models, claus) i la puga obrir al
/// navegador intern amb «NB|PREGUNTA_IA|dsh|…» o des d'esta mateixa secció.
#[command]
pub async fn start_dsh_web() -> Result<String, String> {
    crate::external_orchestrator::dsh::start_web().map_err(|e| e.to_string())?;
    Ok(
        "He arrencat «dsh web»: la seva interfície estarà a http://127.0.0.1:3080 \
         (si triga, torna a prémer en uns segons)."
            .into(),
    )
}

/// Diu si la UI web de dsh respon ja al port 3080 (per a l'etiqueta
/// «en marxa» de la secció de harnessos del Gestor de proveïdors).
#[command]
pub async fn dsh_web_running() -> Result<bool, String> {
    Ok(std::net::TcpStream::connect_timeout(
        &std::net::SocketAddr::from(([127, 0, 0, 1], 3080)),
        std::time::Duration::from_millis(400),
    )
    .is_ok())
}

// ── GitHub Copilot (motor ACP): detecció, instal·lació i permisos ─────────

/// Diu si el CLI de GitHub Copilot està disponible: la instal·lació del
/// sistema (npm global, Homebrew…) o la LOCAL que NoOrbit gestiona dins de
/// les seves dades (amb el seu propi Node.js, si al sistema no n'hi ha).
#[command]
pub async fn copilot_installed() -> Result<bool, String> {
    Ok(crate::external_orchestrator::copilot::copilot_installed())
}

/// Versió del CLI de Copilot instal·lat (si n'hi ha).
#[command]
pub async fn copilot_version() -> Result<Option<String>, String> {
    Ok(crate::external_orchestrator::copilot::version())
}

/// Instal·la el CLI de GitHub Copilot automaticament: baixa el Node.js LTS
/// oficial (si no n'hi ha cap) i el paquet @github/copilot, tot DINS de les
/// dades de NoOrbit (o de NoOrbitData/ en mode portàtil/USB). No demanà
/// administració, no modifica el sistema i es pot emportar en un USB.
#[command]
pub async fn install_copilot(app: AppHandle) -> Result<String, String> {
    crate::external_orchestrator::copilot::install(app)
        .await
        .map_err(|e| e.to_string())
}

/// Contesta una petició de permís de l'agent ACP (el CLI de Copilot demana
/// permís abans d'usar una eina o d'escriure un fitxer). `option_id` és
/// l'opció triada; si no n'hi ha, l'operació es rebutja.
#[command]
pub fn acp_permission_reply(id: String, option_id: Option<String>) -> Result<bool, String> {
    Ok(crate::acp::answer_permission(&id, option_id))
}

/// Diu si ComfyUI és al sistema (per oferir instal·lar-lo o arrencar-lo).
#[command]
pub async fn comfyui_installed() -> Result<bool, String> {
    Ok(crate::ai::comfyui_installed())
}

/// Diu si el servidor ComfyUI respon ja al port 8188.
#[command]
pub async fn comfyui_running() -> Result<bool, String> {
    Ok(crate::ai::comfyui_running())
}

/// Arrenca el servei ComfyUI en segon pla (si està instal·lat).
#[command]
pub async fn start_comfyui() -> Result<(), String> {
    crate::ai::start_comfyui().map_err(|e| e.to_string())
}

/// Instal·la ComfyUI. mateix procés transparent que amb Ollama:
/// - macOS: Terminal amb `brew install --cask comfyui` si hi ha Homebrew; si
///   no, la pàgina de descàrrega oficial.
/// - Linux/Windows: pàgina de descàrrega oficial (instal·ladors i portàtil).
/// L'usuari sempre veu i autoritza el que s'executa.
#[command]
pub async fn install_comfyui() -> Result<String, String> {
    let download_url = "https://www.comfy.org/download";
    #[cfg(target_os = "macos")]
    {
        // ComfyUI oficial només funciona en Apple Silicon. En un Mac Intel
        // (x86_64) no cal intentar instal·lar-lo: hi ha el rodet de CPU
        // (stable-diffusion.cpp), que roda natiu, o un proveïdor en línia.
        #[cfg(target_arch = "x86_64")]
        {
            let _ = download_url;
            return Err(
                "ComfyUI no està disponible per a Mac Intel (només Apple \
                 Silicon). Alternatives que SÍ funcionen ací: 1) el rodet \
                 d'imatges en CPU — botó «Instal·la» de la secció «Rodet \
                 d'imatges en CPU» del Gestor de models (stable-diffusion.cpp, \
                 instal·la'l amb Homebrew) —; 2) un proveïdor en línia amb \
                 token (Configuració ▸ Proveïdors ▸ model d'imatges)."
                    .into(),
            );
        }
        #[cfg(target_arch = "aarch64")]
        {
            if which::which("brew").is_ok() {
                let script = "tell application \"Terminal\" to do script \"brew install --cask comfyui && open -a ComfyUI\"";
                std::process::Command::new("osascript")
                    .arg("-e")
                    .arg(script)
                    .spawn()
                    .map_err(|e| format!("No s'ha pogut obrir el Terminal: {}", e))?;
                return Ok("S'ha obert el Terminal per instal·lar ComfyUI amb Homebrew.".into());
            }
            std::process::Command::new("open")
                .arg(download_url)
                .spawn()
                .map_err(|e| format!("No s'ha pogut obrir el navegador: {}", e))?;
            return Ok("S'ha obert la pàgina de descàrrega de ComfyUI.".into());
        }
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(download_url)
            .spawn()
            .map_err(|e| format!("No s'ha pogut obrir el navegador: {}", e))?;
        return Ok("S'ha obert la pàgina de descàrrega de ComfyUI.".into());
    }

    #[cfg(windows)]
    {
        std::process::Command::new("cmd")
            .arg("/C")
            .arg("start")
            .arg(download_url)
            .spawn()
            .map_err(|e| format!("No s'ha pogut obrir el navegador: {}", e))?;
        return Ok("S'ha obert la pàgina de descàrrega de ComfyUI.".into());
    }

    #[allow(unreachable_code)]
    Err("Instal·lació no suportada en aquest sistema".into())
}

/// EDITORS DE VÍDEO amb IA, descarregables amb 1 clic (mateix patre que
/// ComfyUI: Homebrew en macOS Apple Silicon, pàgina de descàrrega en
/// la resta). El que s'instal·la és l'editor REAL (Kdenlive, OpenShot,
/// DaVinci Resolve); la part d'IA del xat (NoOrbit) hi pot treballar
/// per damunt: el vídeo es genera amb «IMG|»/ComfyUI i es munta aquí.
#[derive(Debug, Clone, Serialize)]
pub struct VideoEditorInfo {
    pub id: String,
    pub cask: String,
    pub url: String,
}

fn video_editor_spec(id: &str) -> Option<VideoEditorInfo> {
    let (cask, url) = match id {
        "kdenlive" => ("kdenlive", "https://kdenlive.org/download/"),
        "openshot" => ("openshot", "https://www.openshot.org/download/"),
        "davinci" => ("davinci-resolve", "https://www.blackmagicdesign.com/products/davinciresolve"),
        "shotcut" => ("shotcut", "https://shotcut.org/download/"),
        _ => return None,
    };
    Some(VideoEditorInfo {
        id: id.to_string(),
        cask: cask.to_string(),
        url: url.to_string(),
    })
}

#[command]
pub async fn install_video_editor(editor: String) -> Result<String, String> {
    let spec = video_editor_spec(&editor)
        .ok_or_else(|| format!("Editor de vídeo desconegut: {}", editor))?;

    #[cfg(target_os = "macos")]
    {
        if which::which("brew").is_ok() {
            let script = format!(
                "tell application \"Terminal\" to do script \"brew install --cask {}\"",
                spec.cask
            );
            std::process::Command::new("osascript")
                .arg("-e")
                .arg(&script)
                .spawn()
                .map_err(|e| format!("No s'ha pogut obrir el Terminal: {}", e))?;
            return Ok(format!(
                "S'ha obert el Terminal per instal·lar {} amb Homebrew. Quan acabi, obre'l des del Launchpad.",
                editor
            ));
        }
        std::process::Command::new("open")
            .arg(&spec.url)
            .spawn()
            .map_err(|e| format!("No s'ha pogut obrir el navegador: {}", e))?;
        return Ok("S'ha obert la pàgina de descàrrega.".into());
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&spec.url)
            .spawn()
            .map_err(|e| format!("No s'ha pogut obrir el navegador: {}", e))?;
        return Ok("S'ha obert la pàgina de descàrrega.".into());
    }

    #[cfg(windows)]
    {
        std::process::Command::new("cmd")
            .arg("/C")
            .arg("start")
            .arg(&spec.url)
            .spawn()
            .map_err(|e| format!("No s'ha pogut obrir el navegador: {}", e))?;
        return Ok("S'ha obert la pàgina de descàrrega.".into());
    }

    #[allow(unreachable_code)]
    Err("Instal·lació no suportada en aquest sistema".into())
}

/// Defineix el model actiu d'Ollama per a tots els xats i agents.
#[command]
pub async fn ollama_set_model(
    state: State<'_, AppState>,
    name: Option<String>,
) -> Result<(), String> {
    let ai = state.ai_manager.clone();
    ai.set_selected_model(name).await;
    Ok(())
}

/// Model que s'està fent servir ara mateix (triat o per defecte).
#[command]
pub async fn ollama_active_model(state: State<'_, AppState>) -> Result<String, String> {
    let ai = state.ai_manager.clone();
    Ok(ai.effective_model().await)
}

/// Últim pensament (raonament) emès per un xat, per mostrar-lo a la UI.
/// Es buida a cada generació nova; retorna None si el model no raona.
#[command]
pub async fn ai_last_thinking(
    state: State<'_, AppState>,
    session: Option<String>,
) -> Result<Option<String>, String> {
    let ai = state.ai_manager.clone();
    Ok(ai.last_thinking(&sid_of(session)).await)
}

/// Instal·la el RODET D'IMATGES EN CPU (stable-diffusion.cpp): roda a
/// qualsevol Mac — també Intel, on ComfyUI no està publicat — i els models
/// són fitxers .gguf/.safetensors a la carpeta «sd-models/». El xat fa servir
/// el binari «sd» directament per a la directriu «IMG|».
#[command]
pub async fn install_image_cpu() -> Result<String, String> {
    let releases = "https://github.com/leejet/stable-diffusion.cpp/releases";

    #[cfg(target_os = "macos")]
    {
        if which::which("brew").is_ok() {
            let script = "tell application \"Terminal\" to do script \
                          \"brew install stable-diffusion.cpp\"";
            std::process::Command::new("osascript")
                .arg("-e")
                .arg(&script)
                .spawn()
                .map_err(|e| format!("No s'ha pogut obrir el Terminal: {}", e))?;
            return Ok(
                "S'ha obert el Terminal per instal·lar el rodet d'imatges en \
                 CPU (stable-diffusion.cpp). Quan acabi, torna al Gestor de \
                 models per baixar el model inicial."
                    .into(),
            );
        }
        std::process::Command::new("open")
            .arg(releases)
            .spawn()
            .map_err(|e| format!("No s'ha pogut obrir el navegador: {}", e))?;
        return Ok("S'ha obert la pàgina de descàrrega del rodet de CPU.".into());
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(releases)
            .spawn()
            .map_err(|e| format!("No s'ha pogut obrir el navegador: {}", e))?;
        return Ok("S'ha obert la pàgina de descàrrega del rodet de CPU.".into());
    }

    #[cfg(windows)]
    {
        std::process::Command::new("cmd")
            .arg("/C")
            .arg("start")
            .arg(releases)
            .spawn()
            .map_err(|e| format!("No s'ha pogut obrir el navegador: {}", e))?;
        return Ok("S'ha obert la pàgina de descàrrega del rodet de CPU.".into());
    }

    #[allow(unreachable_code)]
    Err("Instal·lació no suportada en aquest sistema".into())
}

/// Estat del rodet d'imatges en CPU, perquè el Gestor de models el mostre:
/// binari instal·lat, carpeta de models i el model disponible (si n'hi ha).
#[command]
pub async fn image_cpu_info() -> Result<crate::imgen::SdInfo, String> {
    crate::imgen::sd_info()
}

/// ESTAL·LA I CONFIGURA EL RODET D'IMATGES EN CPU amb UN SOL BOTÓ:
/// instal·la el motor (stable-diffusion.cpp) i baixa el model inicial
/// (SD 1.5, ≈2 GB). L'usuari no ha de fer dos passos ni obrir cap Terminal:
/// un clic i el xat ja pot generar imatges en local amb «IMG|».
#[command]
pub async fn image_cpu_setup(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    use tauri::Emitter;
    let http = state.ai_manager.http_client();

    // 1) El motor (binari «sd»).
    if crate::imgen::sd_binary().is_none() {
        let _ = app.emit(
            "ai://sd-model",
            serde_json::json!({ "message": "instal·lant el rodet d'imatges en CPU…" }),
        );
        crate::imgen::sd_install(&http, Some(&app))
            .await
            .map_err(|e| e.to_string())?;
    }

    // 2) El model inicial (si encara no n'hi ha cap).
    if crate::imgen::sd_model_path().is_none() {
        let _ = app.emit(
            "ai://sd-model",
            serde_json::json!({ "message": "baixant el model inicial (≈2 GB)…" }),
        );
        image_cpu_download_model(app.clone(), None, None)
            .await
            .map_err(|e| e.to_string())?;
    }

    let info = crate::imgen::sd_info().map_err(|e| e.to_string())?;
    Ok(format!(
        "Rodet d'imatges en CPU a punt: {} (model: {}). El xat ja pot generar \
         imatges en local amb «IMG|».",
        if info.installed {
            "instal·lat"
        } else {
            "sense instal·lar"
        },
        info.model.unwrap_or_else(|| "cap model".into())
    ))
}

/// Model d'imatge per defecte per al rodet de CPU: SD 1.5 en safetensors.
const CPU_MODEL_DEFAULT_URL: &str =
    "https://huggingface.co/stable-diffusion-v1-5/stable-diffusion-v1-5/resolve/main/v1-5-pruned-emaonly.safetensors";
const CPU_MODEL_DEFAULT_NAME: &str = "sd-v1-5-pruned-emaonly.safetensors";

/// Valida un nom de fitxer de model: un únic component de ruta (sense
/// separadors, sense `..`) i amb extensió de model coneguda.
fn sanitize_model_file_name(name: &str) -> Option<String> {
    let n = name.trim();
    if n.is_empty()
        || n == "."
        || n == ".."
        || n.contains('/')
        || n.contains('\\')
        || n.contains('\0')
    {
        return None;
    }
    let lower = n.to_lowercase();
    const OK: &[&str] = &[
        "gguf", "safetensors", "bin", "pt", "pth", "ckpt", "onnx", "model",
    ];
    let ext = lower.rsplit('.').next().unwrap_or("");
    if !OK.contains(&ext) {
        return None;
    }
    Some(n.to_string())
}

/// Valida la URL de descàrrega: només http(s) (mai `file://` ni FTP) i sense
/// credencials embarcades.
fn sanitize_download_url(url: &str) -> String {
    let u = url.trim();
    if u.starts_with("https://") || u.starts_with("http://") {
        return u.to_string();
    }
    CPU_MODEL_DEFAULT_URL.to_string()
}

/// Baixa un model d'imatge (.gguf/.safetensors) a la carpeta del rodet de CPU
/// amb progrés a «ai://sd-model». Per defecte, el SD 1.5 llest per CPU (≈2 GB).
#[command]
pub async fn image_cpu_download_model(
    app: AppHandle,
    url: Option<String>,
    name: Option<String>,
) -> Result<String, String> {
    let url = url
        .filter(|u| !u.trim().is_empty())
        .unwrap_or_else(|| CPU_MODEL_DEFAULT_URL.to_string());
    let name = name
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| CPU_MODEL_DEFAULT_NAME.to_string());
    // El nom ve de l'exterior: ha de ser un únic component de ruta (sinó
    // `../../` escriuria fora de la carpeta de models) i una extensió de
    // model coneguda (evita que es desi un executable).
    let name = sanitize_model_file_name(&name)
        .ok_or_else(|| "Nom de model no vàlid.".to_string())?;
    let url = sanitize_download_url(&url);
    let dir = crate::imgen::sd_models_dir().map_err(|e| e.to_string())?;
    let final_path = dir.join(&name);
    if final_path.exists() {
        return Ok(format!("El model «{}» ja era a la carpeta de models.", name));
    }
    let part_path = dir.join(format!("{}.part", name));
    let progress = std::sync::Arc::new(tokio::sync::Mutex::new(0u64));
    let emit = {
        let app = app.clone();
        let progress = progress.clone();
        tokio::spawn(async move {
            loop {
                let n = *progress.lock().await;
                let msg = format!("Baixant el model: {} MB…", n / 1_000_000);
                let _ = app.emit("ai://sd-model", serde_json::json!({ "message": msg }));
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            }
        })
    };
    let res: Result<String, String> = (async {
        let client = reqwest::Client::builder()
            .user_agent("NoOrbit")
            .build()
            .map_err(|e| e.to_string())?;
        let mut resp = client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("No s'ha pogut iniciar la descàrrega: {}", e))?;
        resp.error_for_status_ref()
            .map_err(|e| format!("La descàrrega ha fallat: {}", e))?;
        let mut file = tokio::fs::File::create(&part_path)
            .await
            .map_err(|e| format!("No s'ha pogut crear «{}»: {}", part_path.display(), e))?;
        use tokio::io::AsyncWriteExt;
        let mut total: u64 = 0;
        loop {
            match resp.chunk().await {
                Ok(Some(bytes)) if !bytes.is_empty() => {
                    file.write_all(&bytes)
                        .await
                        .map_err(|e| format!("No s'han pogut desar bytes: {}", e))?;
                    *progress.lock().await += bytes.len() as u64;
                    total += bytes.len() as u64;
                }
                _ => break,
            }
        }
        file.flush().await.map_err(|e| e.to_string())?;
        drop(file);
        if total < 10_000_000 {
            return Err(format!(
                "La descàrrega és massa petita ({:.1} MB): la URL pot ser \
                 incorrecta o el fitxer bloquejat. S'ha descartat.",
                total as f64 / 1_000_000.0
            ));
        }
        tokio::fs::rename(&part_path, &final_path)
            .await
            .map_err(|e| e.to_string())?;
        Ok(format!(
            "Model desat a «{}» ({} MB). Ja pot generar imatges en CPU.",
            final_path.display(),
            total / 1_000_000
        ))
    })
    .await;
    emit.abort();
    if res.is_err() {
        let _ = tokio::fs::remove_file(&part_path).await;
    }
    res
}

/// Descobreix les IAs locals EXTERNALS presents al sistema (Ollama, LM
/// Studio, Jan, llama.cpp, vLLM…): en marxa o només instal·lades.
#[command]
pub async fn external_ias_discover(
    state: State<'_, AppState>,
) -> Result<Vec<crate::external_orchestrator::ExternalIa>, String> {
    let ai = state.ai_manager.clone();
    Ok(ai.external.discover().await)
}

/// Delega una tasca directament a una IA local externa o a un proveïdor
/// oficial amb token (prova les IAs en marxa i retorna la primera resposta
/// vàlida, amb el seu origen etiquetat). El xat normal ja fa este bot de
/// rescat quan la IA principal no resol la tasca; esta comanda serveix per a
/// provar-lo o usar-lo de forma explícita.
#[command]
pub async fn delegate_external_ia(
    state: State<'_, AppState>,
    prompt: String,
    system: Option<String>,
) -> Result<String, String> {
    let ai = state.ai_manager.clone();
    ai.external
        .delegate(&prompt, system.as_deref())
        .await
        .map(|resp| resp.text)
        .map_err(|e| e.to_string())
}

/// Retorna la carpeta on Ollama desa els models (buide = per defecte).
#[command]
pub async fn ollama_get_models_dir() -> Result<String, String> {
    Ok(crate::config::AppConfig::load()
        .map(|c| c.ai.models_dir)
        .unwrap_or_default())
}

/// Defineix la carpeta de models (p. ex. un USB extern), la desa i L'APLICA
/// de seguida: reinicia Ollama amb OLLAMA_MODELS perquè les descàrregues
/// següents vagin al disc extern (i no al dur, com passava abans).
#[command]
pub async fn ollama_set_models_dir(path: Option<String>) -> Result<String, String> {
    let mut cfg = crate::config::AppConfig::load().map_err(|e| e.to_string())?;
    cfg.ai.models_dir = path.unwrap_or_default().trim().to_string();
    cfg.save().map_err(|e| e.to_string())?;
    let dir = cfg.ai.models_dir.clone();
    let msg = tokio::task::spawn_blocking(|| crate::ai::apply_models_dir(true))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    Ok(msg.unwrap_or_else(|| {
        if dir.is_empty() {
            "Els models tornaran a la carpeta per defecte del sistema.".to_string()
        } else {
            format!("Els models es baixaran i s'executaran des de {}.", dir)
        }
    }))
}

/// Mou els models ja baixats al disc intern cap al disc extern configurat i
/// reinicia Ollama apuntant-hi. Emet progrés a «ai://migrate».
#[command]
pub async fn ollama_migrate_models(app: AppHandle) -> Result<String, String> {
    tokio::task::spawn_blocking(move || crate::ai::migrate_models(&app))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// Llista els volums muntats (USB/discos externs) per suggerir-los a la UI.
/// A Windows no s'exposa res: l'usuari tria la unitat des del diàleg natiu.
#[command]
pub async fn list_external_volumes() -> Result<Vec<String>, String> {
    #[cfg(windows)]
    {
        Ok(vec![])
    }
    #[cfg(not(windows))]
    {
        #[cfg(target_os = "macos")]
        let base = "/Volumes";
        #[cfg(target_os = "linux")]
        let base = "/media";
        let mut out = Vec::new();
        if let Ok(entries) = std::fs::read_dir(base) {
            for e in entries.flatten() {
                let name = e.file_name().to_string_lossy().to_string();
                if name.starts_with('.') {
                    continue;
                }
                let p = e.path().to_string_lossy().to_string();
                if std::fs::metadata(&p).map(|m| m.is_dir()).unwrap_or(false) {
                    out.push(p);
                }
            }
        }
        Ok(out)
    }
}

// ── Catàleg d'IAs: característiques ABANS de descarregar-les ────────────────

/// Entrada del catàleg que mostra el menú IA: mida aproximada, si és sense
/// censura, si ACCEPTA imatges (visió), si EN genera (imatges/vídeo) i si ja
/// és instal·lada. Mida 0 = model al núvol, no cal baixar res.
#[derive(Debug, Clone, serde::Serialize)]
pub struct IaCatalogEntry {
    pub name: String,
    /// «ollama» per als models locals; nom del proveïdor per als en línia.
    pub source: String,
    pub approx_size_gb: f64,
    pub uncensored: bool,
    pub vision: bool,
    pub image_gen: bool,
    pub video_gen: bool,
    pub installed: bool,
    pub note: String,
}

/// Consulta el catàleg: taula curated dels models locals més rellevants
/// (mida aproximada de l'etiqueta estàndard; les variants «:3b» o q4 pesen
/// molt menys) + els proveïdors en línia que l'usuari haja registrat.
#[command]
pub async fn ia_catalog(state: State<'_, AppState>) -> Result<Vec<IaCatalogEntry>, String> {
    // (nom, GB aproximats, sense censura, visió, nota)
    const LOCAL: [(&str, f64, bool, bool, &str); 27] = [
        ("qwen3:8b", 5.2, false, false, "Raonament i codi; pesos oberts (Apache 2.0)"),
        ("qwen2.5-coder:7b", 4.7, false, false, "Codi — equilibri qualitat/velocitat"),
        ("qwen2.5-coder:3b", 1.9, false, false, "Codi — per a màquines modestes"),
        ("llama3.2:3b", 2.0, false, false, "General — ràpid i multilingüe"),
        ("mistral:7b", 4.4, false, false, "General — bo seguint instruccions"),
        ("codellama:13b", 7.5, false, false, "Codi — més qualitat, més lent"),
        ("deepseek-r1:8b", 5.2, false, false, "Raonament pas a pas (thinking visible)"),
        ("gpt-oss:20b", 12.6, false, false, "OpenAI de pesos oberts — MoE eficient"),
        ("gpt-oss:120b", 61.0, false, false, "Màxima qualitat local — requereix ~60 GB de RAM"),
        ("dolphin-mistral:7b", 4.4, true, false, "Sense filtres — codi i raonament"),
        ("dolphin-mixtral:8x7b", 26.0, true, false, "Sense filtres — MoE 8x7b (cal molta RAM)"),
        ("goekdenizguelmez/JOSIEFIED-Qwen3", 5.0, true, false, "Sense filtres — Qwen3-8B abliterated"),
        ("richardyoung/qwen3-8b-abliterated", 5.0, true, false, "Sense filtres — Qwen3 per a ús lliure"),
        ("alibayram/mimo-7b-rl", 4.7, false, false, "Xiaomi MiMo — raonament mat/codi, thinking visible"),
        ("maternion/mimo-v2.6", 6.0, false, false, "Xiaomi MiMo-V2.6 — agent 9B amb eines"),
        ("llava:7b", 4.2, false, true, "Visió: descriu i respon sobre imatges"),
        ("llava:13b", 8.0, false, true, "Visió — més qualitat que el 7B"),
        ("llama3.2-vision:11b", 7.9, false, true, "Visió — Meta, bona lectura d'imatges"),
        ("gemma3:4b", 3.3, false, true, "Visió — lleuger, de Google"),
        ("gemma3:12b", 8.1, false, true, "Visió — equilibri"),
        ("gemma3:27b", 17.0, false, true, "Visió — el millor gemma3, cal RAM"),
        ("minicpm-v:8b", 6.6, false, true, "Visió — excel·lent en detalls i OCR"),
        ("moondream:latest", 1.7, false, true, "Visió — miniatura (1.7 GB), ràpid"),
        ("stable-diffusion-v2:latest", 4.9, false, false, "Genera imatges, però NoOrbit recomana ComfyUI (qualitat i control)"),
        ("qwen3.5:0.8b", 0.45, false, false, "Edge: model petit, ideal per a dispositius modestos (smolmaxxing)"),
        ("qwen3.5:2b", 1.0, false, false, "Edge: millor equilibri qualitat/velocitat en ~1 GB (smolmaxxing)"),
        ("smollm3:3b", 1.9, false, false, "Edge: petit i obert (Hugging Face), cost 0"),
    ];
    let installed: Vec<String> = state
        .ai_manager
        .list_models()
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|m| m.name)
        .collect();
    let mut out: Vec<IaCatalogEntry> = LOCAL
        .iter()
        .map(|(name, gb, unc, vis, note)| {
            let fam = name.split(':').next().unwrap_or(name);
            IaCatalogEntry {
                name: name.to_string(),
                source: "ollama".into(),
                approx_size_gb: *gb,
                uncensored: *unc,
                vision: *vis,
                image_gen: name.starts_with("stable-diffusion"),
                // Cap model d'aquesta llista genera vídeo avui.
                video_gen: false,
                installed: installed
                    .iter()
                    .any(|i| i == name || i.split(':').next().unwrap_or(i) == fam),
                note: note.to_string(),
            }
        })
        .collect();

    // Proveïdors en línia registrats per l'usuari: característiques conegudes
    // segons la URL base (cap no genera vídeo de moment).
    let guard = state.api_manager.lock().map_err(|e| e.to_string())?;
    for p in guard.providers.iter().filter(|p| p.enabled) {
        let base = p.base_url.to_lowercase();
        let model = p.model.to_lowercase();
        let (unc, vis, gen, note) = if base.contains("openai.com") {
            (
                false,
                model.contains("4o") || model.contains("4.1") || model.contains("o1") || model.contains("o3"),
                true,
                "Genera imatges amb «dall-e-3»/«gpt-image-1» via l'API d'imatges".to_string(),
            )
        } else if base.contains("deepseek") {
            // DeepSeek és de text (la seva entrada d'estil Anthropic tampoc no
            // rep imatges) però porta 1M de context: dir-ho evita que es
            // legis com una limitació de la clau.
            (
                false,
                false,
                false,
                "Només text, amb context d'1M (deepseek-flash / deepseek-v4-pro)".to_string(),
            )
        } else if base.contains("anthropic") {
            (
                false,
                true,
                false,
                "Rep imatges com a entrada (visió); no en genera. Context d'1M i \
                 128k de sortida als models 4.6, 4.7, 4.8 i 5.x".to_string(),
            )
        } else if base.contains("ollama.com") {
            (false, false, false, "Models al núvol d'Ollama: no cal baixar res".to_string())
        } else if base.contains("venice") {
            (
                true,
                false,
                true,
                "Inferència sense filtres; model de xat «venice-uncensored», \
                 imatges amb «venice-sd15»; API pròpia".to_string(),
            )
        } else if base.contains("perplexity") {
            (false, false, false, "Cerca web en línia amb citacions; només text".to_string())
        } else {
            (false, false, false, "Sense metadades conegudes per a aquesta URL".to_string())
        };
        out.push(IaCatalogEntry {
            name: format!("{} · {}", p.name, p.model),
            source: p.name.clone(),
            approx_size_gb: 0.0,
            uncensored: unc,
            vision: vis,
            image_gen: gen,
            video_gen: false,
            installed: !p.token.trim().is_empty(),
            note,
        });
    }
    drop(guard);
    Ok(out)
}

// ── APRENENTATGE AUTÒNOM EN REPÒS (la IA nativa navega sola) ──────────────

/// Estat del vagueig per al panell d'Ajustos: si està activat, quant fa que
/// ningú no toca el programa, PER QUÈ no aprèn ara, què ha après i si hi ha
/// accés real a la Internet profunda.
#[command]
pub async fn wander_status() -> Result<crate::ai::wander::Estat, String> {
    Ok(crate::ai::wander::estat())
}

/// Activa o desactiva l'aprenentatge en repòs (per defecte: ACTIVAT).
#[command]
pub async fn wander_set(actiu: bool) -> Result<bool, String> {
    let mut cfg = crate::config::AppConfig::load().unwrap_or_default();
    cfg.ai.aprenentatge_repos = actiu;
    cfg.save().map_err(|e| e.to_string())?;
    Ok(actiu)
}

/// Una sessió ARA mateix (el botó «Aprèn ara»). Salta el refrigeri, però MAI
/// no comença si hi ha una generació en curs: l'usuari no es pot quedar sense
/// resposta perquè la nativa estava llegint pàgines.
#[command]
pub async fn wander_run(app: AppHandle) -> Result<usize, String> {
    if crate::ai::wander::ia_occupada() {
        return Err("hi ha una resposta de la IA en curs: espera que acabi".to_string());
    }
    Ok(crate::ai::wander::sessio(Some(&app)).await)
}

/// Punts d'entrada a la Internet profunda: llista d'adreces `.onion` (separa
/// per comes o salts). BUIDA = no s'hi navega mai. Cal a més un proxy HTTP
/// local (Privoxy/Tor): sense proxy, la interfície ho diu clar.
#[command]
pub async fn wander_profunda_set(adreces: String) -> Result<usize, String> {
    let mut cfg = crate::config::AppConfig::load().unwrap_or_default();
    cfg.ai.internet_profunda = adreces.trim().to_string();
    cfg.save().map_err(|e| e.to_string())?;
    Ok(crate::ai::wander::adreces_profundes().len())
}

/// Cor d'activitat des de la interfície: cada vegada que l'usuari fa res, el
/// compte enrere del vagueig torna a zero (mai no aprèn mentre algú el fa
/// servir, encara que toqui teclats que el backend no veu).
#[command]
pub async fn native_note_activity() -> Result<(), String> {
    crate::ai::wander::note_activity();
    Ok(())
}

#[cfg(test)]
mod nudge_tests {
    use super::*;

    #[test]
    fn detects_code_requests_including_tests() {
        assert!(asks_for_code("fes els tests de la calculadora"));
        assert!(asks_for_code("implementa el login amb JWT"));
        assert!(asks_for_code("crea el component Boto.tsx"));
        assert!(asks_for_code("arregla src/app.py"));
        assert!(asks_for_tests("fes els tests de la calculadora"));
        assert!(asks_for_tests("escriu proves unitàries"));
        // Una pregunta o una xarrada no és una ordre de codi.
        assert!(!asks_for_code("què és un test unitari?"));
        assert!(!asks_for_code("explica'm com funciona el login"));
    }

    #[test]
    fn detects_announced_files_with_markdown_variants() {
        assert!(has_file_blocks("@file: src/app.py\nprint(1)\n"));
        assert!(has_file_blocks("### @file: tests/test_app.py\nprint(1)\n"));
        assert!(has_file_blocks("1. @file: src/app.py\n"));
        assert!(has_file_blocks("**@file: src/app.py**\n"));
        assert!(!has_file_blocks("He creat el fitxer src/app.py"));
        assert!(has_code_fence("prova:\n```python\nx = 1\n```"));
        assert!(!has_code_fence("només prosa"));
    }

    /// El cas que es repetia: l'usuari demana el codi del projecte i la IA
    /// respon amb README + blocs .md (documentació) en lloc de codi.
    #[test]
    fn detecta_nomes_documentacio() {
        // Només .md → és documentació (cal el recordatori «escriu el codi»).
        assert!(nomes_documentacio(
            "FES TOT EL CODI DE EL PROGECTE!",
            "@file: README.md\n# Store wp one\n\n## Característiques\n- Responsive\n"
        ));
        // Tot .md però sense demanar codi → no es toca.
        assert!(!nomes_documentacio(
            "explica'm el README",
            "@file: README.md\n# Store\n"
        ));
        // Hi ha codi de veritat (.php) → no és «només documentació».
        assert!(!nomes_documentacio(
            "fes el codi de la botiga en WordPress",
            "@file: functions.php\n<?php\necho 'hola';\n"
        ));
        // Sense «@file:» però parlant de millores/documentació.
        assert!(nomes_documentacio(
            "fes el codi del projecte",
            "Millora de l'estructura del README.md: això seria el document."
        ));
    }

    /// Peticions de figura o escena 3D: l'usuari demana una acció real
    /// (el model ha d'emetre «RUN|BLENDER|» amb el codi bpy), encara que
    /// digui «Blender» o no.
    #[test]
    fn detecta_peticions_3d() {
        assert!(expects_execution("crea un nomo de jardi", ""));
        assert!(expects_execution("fes un model 3d d'una casa", ""));
        assert!(expects_execution("crea una escena 3d del parc", ""));
        assert!(expects_execution("fes un personatge a blender", ""));
        // Una pregunta NO demana execució.
        assert!(!expects_execution("què és un model 3d?", ""));
        assert!(!expects_execution("explica'm què és un nan de jardí", ""));
        // Una xerrada tampoc.
        assert!(!expects_execution("bon dia, com va tot?", ""));
    }
}

/// Tests GUIONITZATS del bucle `chat_with_tools` amb proveïdor SIMULAT.
///
/// Per què no s'injecta un proveïdor fals al comandament sencer: la costura
/// seria massa invasiva — `chat_with_tools` exigeix un `AppHandle` real de
/// Tauri (el crate no activa la feature "test", no hi ha `mock_app`), un
/// `AiManager`, un `ComputerController`, un `LearningStore` i l'estat GLOBAL
/// del workspace; simular tot això canviaria el comportament real o exigiria
/// refactoritzar el comandament. En lloc d'això, es cobreixen els DOS nuclis
/// de decisió del bucle amb el MATEIX codi que usa el comandament:
///   1. `next_nudge` — la cadena de recordatoris (extreta pura, sense canvis
///      d'ordre ni de condicions respecte del bucle original).
///   2. El cicle escriure→verificar→realimentar (`write_announced_files_at`
///      + `lint_paths_blocking` + `verification_report`) amb `php -l` REAL.
/// El conductor de sota reprodueix l'estructura del bucle (rondes, historial,
/// nudges, reintents de verificació) sense Tauri.
#[cfg(test)]
mod bucle_guionitzat {
    use super::*;
    use crate::agent::runtools;

    /// Prompt que demana codi (activa `asks_for_code`): el cas real del nucli
    /// xat→codi. «tema» sol no és objecte de `asks_for_code`; amb «codi» el
    /// recordatori CODE_NUDGE és el que dispara el bucle real.
    const PROMPT: &str = "crea el codi d'un tema de wordpress anomenat Orbit";

    /// Torn 1: prosa sense codi → ha de rebre el CODE_NUDGE.
    const TORN1_PROSA: &str =
        "D'acord: crearé el tema de WordPress amb els fitxers necessaris i després els revisaré.";
    /// Torn 2: @file: amb PHP INVÀLID + RUN| → verificació amb error real.
    const TORN2_PHP_INVALID: &str = "@file: wp-content/themes/orbit/functions.php\n\
        ```php\n\
        <?php\n\
        function orbit_setup( {\n\
        ```\n\
        RUN|php -l wp-content/themes/orbit/functions.php";
    /// Torn 3: correcció → lint verd.
    const TORN3_CORREGIT: &str = "@file: wp-content/themes/orbit/functions.php\n\
        ```php\n\
        <?php\n\
        function orbit_setup() {\n\
            add_theme_support('title-tag');\n\
        }\n\
        ```\n\
        RUN|php -l wp-content/themes/orbit/functions.php";

    /// Guion complet: prosa → nudge; PHP invàlid → error realimentat;
    /// correcció → verificació verda. Cap èxit sense verificació real.
    #[test]
    fn guio_prosa_nudge_php_invalid_correccio_verd() {
        if runtools::php_binary().is_none() {
            eprintln!("php no disponible en aquesta màquina: test omès");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let guio = [TORN1_PROSA, TORN2_PHP_INVALID, TORN3_CORREGIT];
        let mut c = NudgeCounters::default();
        let mut verify_retries = 0usize;
        let mut history: Vec<HistoryTurn> = Vec::new();
        let mut feedbacks: Vec<String> = Vec::new();
        for (round, text) in guio.iter().enumerate() {
            let runs = runtools::parse(text);
            if runs.is_empty() {
                // Sense directrius: cadena de nudges (com el bucle real).
                if let Some(nudge) = next_nudge(PROMPT, text, round, &mut c) {
                    history.push(HistoryTurn { role: "assistant".into(), content: text.to_string() });
                    feedbacks.push(nudge);
                    continue;
                }
                break;
            }
            // Amb RUN|: PRIMER fitxers, DESPRÉS verificació amb eines reals.
            let pre = runtools::write_announced_files_at(root, text);
            assert_eq!(pre, vec!["wp-content/themes/orbit/functions.php".to_string()]);
            let lint = runtools::lint_paths_blocking(root, &pre);
            if lint.iter().any(|r| !r.ok) {
                verify_retries += 1;
            }
            if let Some(msg) = runtools::verification_report(&lint, verify_retries) {
                feedbacks.push(msg);
            }
            history.push(HistoryTurn { role: "assistant".into(), content: text.to_string() });
        }
        assert_eq!(history.len(), 3, "els tres torns del guion han d'entrar a l'historial");
        assert_eq!(feedbacks.len(), 3, "nudge + informe fallit + informe verd");
        // Torn 1: el recordatori imperatiu de codi, no una altra cosa.
        assert_eq!(feedbacks[0], CODE_NUDGE);
        assert_eq!(c.code, 1);
        // Torn 2: l'error REAL de php -l es realimenta, amb l'ordre de corregir.
        assert!(feedbacks[1].contains("NO donis la feina per acabada"), "informe: {}", feedbacks[1]);
        assert!(feedbacks[1].contains("functions.php"), "informe: {}", feedbacks[1]);
        assert!(feedbacks[1].to_lowercase().contains("error"), "informe: {}", feedbacks[1]);
        assert_eq!(verify_retries, 1);
        // Torn 3: verificació verda amb eina real (mai èxit mut).
        assert!(feedbacks[2].contains("cap error de sintaxi"), "informe: {}", feedbacks[2]);
        // El fitxer final al disc és el CORREGIT.
        let final_code = std::fs::read_to_string(root.join("wp-content/themes/orbit/functions.php")).unwrap();
        assert!(final_code.contains("add_theme_support"));
        assert!(!final_code.contains("orbit_setup( {"));
    }

    /// Els nudges són REPETIBLES però amb SOSTRE: màxim MAX_NUDGE_USES (2)
    /// usos de cada tipus per torn; després la cadena passa al següent o acaba.
    #[test]
    fn nudges_repetibles_com_a_maxim_dos_usos() {
        assert_eq!(MAX_NUDGE_USES, 2);
        let mut c = NudgeCounters::default();
        let prosa = "El tema tindria les parts habituals i es podria activar des del panell.";
        let n1 = next_nudge(PROMPT, prosa, 0, &mut c);
        let n2 = next_nudge(PROMPT, prosa, 1, &mut c);
        let n3 = next_nudge(PROMPT, prosa, 2, &mut c);
        assert_eq!(n1.as_deref(), Some(CODE_NUDGE));
        assert_eq!(n2.as_deref(), Some(CODE_NUDGE), "el nudge s'ha de poder repetir");
        assert!(n3.is_none(), "al tercer cop ja no queden usos: el torn pot acabar");
        assert_eq!(c.code, MAX_NUDGE_USES);
        // Un altre tipus de nudge té el seu PROPI comptador: «fes els tests»
        // gasta TESTS_NUDGE (2 usos) i després cau al CODE_NUDGE.
        let mut c2 = NudgeCounters::default();
        let p_tests = "fes els tests del projecte";
        assert_eq!(next_nudge(p_tests, prosa, 0, &mut c2).as_deref(), Some(TESTS_NUDGE));
        assert_eq!(next_nudge(p_tests, prosa, 1, &mut c2).as_deref(), Some(TESTS_NUDGE));
        assert_eq!(next_nudge(p_tests, prosa, 2, &mut c2).as_deref(), Some(CODE_NUDGE));
        assert_eq!(c2.tests, MAX_NUDGE_USES);
        assert_eq!(c2.code, 1);
    }

    /// El límit de rondes del bucle és 10 i TALLA els nudges: a la ronda
    /// MAX_ROUNDS el torn acaba encara que la resposta siga prosa buida.
    #[test]
    fn max_rounds_deu_i_talla_els_nudges() {
        assert_eq!(MAX_ROUNDS, 10);
        let mut c = NudgeCounters::default();
        let prosa = "només prosa sense cap resultat";
        assert!(next_nudge(PROMPT, prosa, MAX_ROUNDS, &mut c).is_none());
        assert!(next_nudge(PROMPT, prosa, MAX_ROUNDS - 1, &mut c).is_some());
        // Conductor: un model que MAI deixa de parlar s'atura pel sostre de
        // nudges molt abans de MAX_ROUNDS (2 emissions), mai bucle infinit.
        let mut c2 = NudgeCounters::default();
        let mut emissions = 0usize;
        for round in 0..=MAX_ROUNDS {
            if next_nudge(PROMPT, prosa, round, &mut c2).is_some() {
                emissions += 1;
            } else {
                break;
            }
        }
        assert_eq!(emissions, MAX_NUDGE_USES);
    }

    /// L'ORDRE de la cadena es preserva: execució > tests > fitxers buits >
    /// projecte sencer > docs > codi > imatge.
    #[test]
    fn l_ordre_de_la_cadena_es_preserva() {
        let mut c = NudgeCounters::default();
        // «compila» demana execució: EXEC_NUDGE va primer que CODE_NUDGE.
        assert_eq!(
            next_nudge("compila el projecte", "ara ho faré", 0, &mut c).as_deref(),
            Some(EXEC_NUDGE)
        );
        // Prosa davant «fes els tests»: TESTS_NUDGE abans que CODE_NUDGE.
        let mut c2 = NudgeCounters::default();
        assert_eq!(
            next_nudge("fes els tests del projecte", "els tests es poden escriure amb pytest", 0, &mut c2).as_deref(),
            Some(TESTS_NUDGE)
        );
        // Fitxers anunciats BUITS: FILE_NUDGE amb la llista de rutes.
        let mut c3 = NudgeCounters::default();
        let buits = "@file: src/app.py\n\n@file: src/util.py\n";
        let n = next_nudge("crea el codi de l'app", buits, 0, &mut c3).unwrap();
        assert!(n.starts_with(FILE_NUDGE), "nudge: {n}");
        assert!(n.contains("src/app.py") && n.contains("src/util.py"), "nudge: {n}");
        // Només documentació (.md) demanant codi: DOCS_ONLY_NUDGE. El README
        // ha de tindre contingut real (≥40 caràcters): si quedara BUIT, el
        // FILE_NUDGE aniria primer — és l'ordre del bucle de veritat.
        let mut c4 = NudgeCounters::default();
        let readme = "@file: README.md\n# Pla del projecte\n\nAquest projecte conté una botiga en línia feta amb WordPress i WooCommerce.\n";
        assert_eq!(
            next_nudge("fes el codi del projecte", readme, 0, &mut c4).as_deref(),
            Some(DOCS_ONLY_NUDGE)
        );
    }
}

#[cfg(test)]
mod proves_descarga {
    use super::{sanitize_download_url, sanitize_model_file_name};

    /// El nom del model és un únic component de ruta: sense això, un
    /// «../../evil.gguf» escriuria fora de la carpeta de models.
    #[test]
    fn el_nom_del_model_es_saneja() {
        assert_eq!(
            sanitize_model_file_name("model.gguf").as_deref(),
            Some("model.gguf")
        );
        assert_eq!(
            sanitize_model_file_name("sd-v1-5-pruned-emaonly.safetensors").as_deref(),
            Some("sd-v1-5-pruned-emaonly.safetensors")
        );
        // Els perillosos es rebutgen.
        assert!(sanitize_model_file_name("../evil.gguf").is_none());
        assert!(sanitize_model_file_name("/etc/passwd").is_none());
        assert!(sanitize_model_file_name("..").is_none());
        // Extensions que no són de model: rebutjades (no es desi un executable).
        assert!(sanitize_model_file_name("model.sh").is_none());
        assert!(sanitize_model_file_name("model.exe").is_none());
    }

    /// Només s'accepten URL http(s): mai `file://` ni FTP.
    #[test]
    fn la_url_de_descarga_es_saneja() {
        assert_eq!(
            sanitize_download_url("https://exemple.com/model.gguf"),
            "https://exemple.com/model.gguf"
        );
        assert_eq!(
            sanitize_download_url("file:///etc/passwd"),
            super::CPU_MODEL_DEFAULT_URL
        );
    }
}
