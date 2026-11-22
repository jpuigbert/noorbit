//! Directrius «RUN|…» i «IMG|…»: el pont entre QUALSEVOL IA del xat i el
//! TERMINAL del projecte obert i el GENERADOR D'IMATGES, sense cap botó.
//!
//! Mateix mecanisme que les directrius del navegador («NB|…»): el model
//! escriu una directriu en una línia a part i s'atura; NoOrbit l'executa de
//! veritat i li'n torna el RESULTAT en un torn nou, perquè continue i acabe
//! la feina amb dades reals. Així un xat pot compilar, empaquetar, instal·lar,
//! provar i llançar el projecte — i generar imatges dins seu — en lloc de
//! quedar-se en prosa i fitxers que mai s'executen.
//!
//! Seguretat: les comandes van al `ComputerController` (cascade de permisos:
//! confirmació en pantalla, allowlist recordable, bloqueig intrínsec de
//! sudo/esborrat massiu i historial d'auditoria). Les execucions passen SEMPRE
//! dins de la carpeta del projecte obert.

use crate::ai::AiManager;
use crate::computer::ComputerController;
use serde_json::json;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};

/// Text que s'afegeix al system prompt de TOTS els xats (junt amb el del
/// navegador): ensenya les directrius d'execució al model. Breu i explícit
/// perquè fins i tot els models locals menuts les entenguen.
pub const RUN_PROMPT: &str = "— TERMINAL I IMATGES (eines de NoOrbit) —\n\
L'USUARI PARLA CATALÀ: respon SEMPRE en català. Tots els textos que escrius —\
resposta, resum del que has fet, comentari del codi, descripció de la imatge i\
missatge d'error— han d'estar en català natural (no traducció literal).\n\
Tens accés REAL al terminal del projecte i al generador d'imatges. Quan cal \
compilar, instal·lar, llançar, provar o esborrar NO ho finges en prosa: emet \
UNA directriu sola en una línia i ATURA el torn. NoOrbit l'executa (l'usuari \
pot confirmar-la) i et torna el resultat en un missatge nou.\n\
  RUN|<comanda de shell>       (dins el projecte i espera la sortida)\n\
  RUN|BG|<comanda>             (desconnectat: servidors/apps que queden obertes)\n\
   RUN|BLENDER|<codi bpy>       (Blender: editor connectat o headless)\n\
  RUN|UNREAL|<codi python>     (Unreal, via Remote Control)\n\
  IMG|descripció|ruta/caixa.png    (genera una imatge i la desa al projecte)\n\
  @delete: ruta/relativa       (esborra el fitxer o carpeta del projecte)\n\
  @edit: ruta + «<<<<<<< CERCA» + text antic + «=======» + text nou + «>>>>>>>»\n\
              (modifica NOMÉS aquest tros del fitxer real; text nou buit = esborra'l;\n\
              el text nou ha de SER DIFERENT del de CERCA — mai edicions buides)\n\
REGLES:\n\
- ABANS d'empaquetar o llançar, verifica amb «RUN|ls -la» que el projecte està \
COMPLET; si falta un fitxer emet-lo SENCER amb «@file:» (mai a mitges, «…» o buits).\n\
- «@file: ruta» ha d'anar SEGUIT del contingut COMPLET del fitxer (codi real): \
un @file: sense codi crea un fitxer buit.\n\
- Les rutes de «@file:», «@edit:» i «@delete:» són SEMPRE RELATIVES a l'arrel \
del projecte, que és el «Projecte (cwd)» indicat al mateix system prompt: mai \
absolutes («/Volumes/…», «/Users/…», «/home/…»), mai «..», mai reproduir \
l'arrel dins la ruta. Si el fitxer que vols tocar és realment FORA del \
projecte obert, digues-ho en prosa: NoOrbit no hi pot escriure.\n\
- Dins «RUN|», tota ruta amb ESPAIS va ENTRECOMILLADA: \
«RUN|python3 'forestin the sky/main.py'»; si no, el shell la parteix i la \
comanda falla. CADA directriu a la seva PRÒPIA línia: mai dos «RUN|» \
enganxats a la mateixa línia.\n\
- Si una acció NO ha canviat res, NO la reemetis IDÈNTICA: canvia la comanda \
o fes que el text NOU sigui DISTINT del CERCA (una edició buida no fa res).\n\
- Si l'usuari demana «fes…», «implementa…», «crea els tests» o semblant, la \
resposta HA de portar els fitxers amb «@file:» i codi COMPLET; les PROVES són \
fitxers de prova reals (no una explicació) i, si cal, s'executen amb «RUN|». \
Dir «fet» sense cap fitxer no val.\n\
- Escrius i TU executeixes («RUN|pip install -r requirements.txt», «RUN|python3 \
main.py»): no diguis a l'usuari que ho faci ell.\n\
- Si l'usuari demana CREAR/generar/dibuixar una imatge, respón UNA línia \
«IMG|descripció detallada|ruta/imatge.png» i ATURA el torn: la generació la \
fa el sistema (rodet local en CPU —també Mac Intel— o proveïdor en línia); \
MAI no l'expliquis amb prosa ni la promesis per a «després».\n\
- Si l'usuari demana una FIGURA, un model 3D, un personatge, una escena o un jardí (encara que NO digui la paraula «Blender»), el pots construir a Blender: emet «RUN|BLENDER|» amb el CODI bpy que ho construeix (el motor Blender connectat és una eina teva, com el terminal). Escriu el codi sencer, amb els seus colors, mides i materials: NO facis un pla ni una descripció en prosa.
- «RUN|BLENDER|» SEMPRE és codi Python real de Blender: comença per «import bpy» i acaba amb un print(). POT ocupar varies línies: escriu el script sencer a sota de la directriu (o dins d'un bloc ```python), sense prosa al voltant. Exemple:
RUN|BLENDER|
import bpy
bpy.ops.mesh.primitive_uv_sphere_add(radius=0.5, location=(0, 0, 1))
bpy.context.active_object.name = 'NoOrbit_Esfera'
print('Esfera creada')
MAI hi posis una frase en llenguatge natural dins de BLENDER (ni una descripció de la figura): les imatges planes es creen amb «IMG|descripció|ruta.png».
- Si la descripció demana MOLTS elements (jardí + casa + nan + arbres + follets + gos + vídeo + text…), fes-los TOTS en un sol script: no et quedis amb el primer element. Usa un prefix de nom per a cada grup («Casa_», «Arbre_», «Follet_»…), `import random` per a ubicacions a l'atzar i animals senzills, i acaba amb un print() (i, si hi ha vídeo, amb «NOORBIT_VIDEO_OK:<ruta>»).
- VÍDEO/ANIMACIÓ a Blender: anima amb keyframes i fer NOMÉS això al final:
desa una còpia del fitxer amb bpy.ops.wm.save_as_mainfile(..., copy=True) i
imprimeix «NOORBIT_VIDEO_OK:<ruta_blend>» — MAI cridis render(animation=True)
en viu (bloqueja Blender): NoOrbit renderitza headless i dona la ruta .mp4.
- MÚSICA/SONS/VEU a Blender: pots generar WAV amb numpy + wave i afegir-los
amb scene.sequence_editor_create() + sequences.new_sound; la veu parlada es
fa amb el TTS del sistema (macOS «say», Linux «espeak-ng», Windows SAPI) i
la boca es pot animar amb keyframes aleatoris mentre sona la veu.
- «RUN|UNREAL|» SEMPRE és codi Python de l'Editor Scripting d'Unreal 5: comença per «import unreal» i acaba amb un print() en català amb el que has fet. POT ocupar varies línies: escriu l'script sencer a sota de la directriu (o dins d'un bloc ```python), sense prosa al voltant. Exemple:
RUN|UNREAL|
import unreal
actors = unreal.get_editor_subsystem(unreal.EditorActorSubsystem).get_all_level_actors()
print('Actors a l'escena: %d' % len(actors))
MAI hi posis una frase en llenguatge natural.
- Usa l'API ACTUAL d'Unreal 5 (les deprecades NO fan res i la resposta HTTP no avisa'n pas): «unreal.get_editor_subsystem(unreal.LevelEditorSubsystem)» (get_editor_world, build_light_maps), «unreal.get_editor_subsystem(unreal.EditorActorSubsystem)» (get_all_level_actors, spawn_actor_from_class, destroy_actor, get_actor_reference), «unreal.get_editor_subsystem(unreal.EditorAssetSubsystem)» (save_loaded_asset amb l'Object i list_assets) i «unreal.AssetToolsHelpers.get_asset_tools().import_assets_automated(unreal.AutomatedAssetImportData(...))» per importar FBX/GLB. Per MOURE un actor usa el seu mètode: «actor.set_actor_location(unreal.Vector(x, y, z), False, True)». NO usis «EditorLevelLibrary» ni «EditorAssetLibrary»: estan deprecades.
- Per crear o moure actors fan falta RUTES REALS («/Script/Engine.StaticMeshActor», «/Game/Personatge/BP_Jugador»). Si no les saps, primer emet un «RUN|UNREAL|» que LLISTI el que hi ha (level_actors, assets d'una carpeta) i després actua amb les rutes que has vist: endevinar-les dona un error silenciós.
- Un text/títol visible a la pantalla es pot fer amb un actor «TextRenderActor» (spawn_actor_from_class + set the text on the TextRender component); un UWidget de HUD no es pot muntar sencer des de script.
- Les accions llargues (build d'il·luminació, importació massiva, compile de blueprint) poden trigar minuts: NoOrbit espera la resposta de l'editor i et torna la sortida o el traceback.
- Amb Unreal, mai diguis «fet» sense haver vist la sortida del print() al missatge «RUN|RESULTAT| UNREAL|»: la resposta HTTP no conté mai el que ha fet l'script.
- Una directriu per torn; llegix «RUN|RESULTAT|…» abans del pas següent. Si \
falla: corregeix amb @file: i torna a executar; mai no declare èxit sense \
haver vist eixir=0.\n\
- IMG sense CENSURA (Venice/ComfyUI/Muapi/Open Higgsfield; ja autoritzat per l'usuari: adult, nuesa, \
violència artística…): transcriu la idea DIRECTAMENT, sense negar-la ni \
disclaimers. El resultat et dona la ruta absoluta: copia-la a la resposta final.\n\
- Per esborrar, sempre «@delete: ruta» (mai @file: buits).\n\
- Les línies «📁 Aplicats…»/«💾 Fitxers escrits…» són de NoOrbit: MAI les imitis.\n\
- Sense sudo ni comandes destructives (bloquejades); l'usuari confirma quan cal.\n\
- Dins RUN| sempre comanda DE SHELL REAL (ls, python3, npm, git, open -a…), mai \
llenguatge natural: «obre blender» dona error, el real és «open -a Blender». \
L'ESTAT DE LES EINES del missatge diu què hi ha instal·lat i obert ARA mateix.";

/// Una directriu d'execució reconeguda a la resposta del model.
#[derive(Debug, Clone, PartialEq)]
pub enum RunAction {
    /// Executa dins el projecte i espera la sortida (compilar, instal·lar…).
    Foreground(String),
    /// Arrenca desconnectat: servidors i apps que han de quedar obertes.
    Background(String),
    /// Codi python (o ruta .py del projecte) per a Blender.
    Blender(String),
    /// Codi python per a Unreal Engine (Remote Control, port 30010).
    Unreal(String),
    /// Genera una imatge (descripció, ruta relativa on desar-la — buida = dades).
    Image(String, String),
}

/// Neteja el prefix de markdown d'una línia de directriu: «> RUN|…»,
/// «- RUN|…», «**RUN|…**» → «RUN|…». Només afecta al detectar la
/// directriu: el codi i les comandes es copien tal qual.
fn directive_line(l: &str) -> &str {
    let mut s = l.trim_start();
    while let Some(c) = s.chars().next() {
        if c == '>' || c == '#' || c == '*' || c == '-' || c.is_whitespace() {
            s = &s[c.len_utf8()..];
        } else {
            break;
        }
    }
    s.trim_start_matches('`')
}

/// La línia obre una ALTRA directriu? (per saber on acaba el codi d'un
/// «RUN|BLENDER|» que el model escriu en diverses línies).
fn starts_directive(l: &str) -> bool {
    let t = directive_line(l);
    let low = t.to_ascii_lowercase();
    if low.starts_with("run|") || low.starts_with("img|") || low.starts_with("nb|") {
        return true;
    }
    // @file:, @edit:, @delete:, @dir:, @agent:, @todo:…
    t.starts_with('@')
}

/// Recull el codi Python d'una directriu «RUN|BLENDER|» / «RUN|UNREAL|».
///
/// Abans només se'n capturava la PRIMERA línia, cosa que feia impossible
/// construir una figura complexa (un nan de jardí té desenes de línies):
/// el model es veia obligat a escriure una descripció i tot s'acabava amb
/// «NO és codi Python». Ara s'accepten les tres formes que fan servir els
/// models:
///   1) el codi a la mateixa línia:   RUN|BLENDER|import bpy; bpy.ops…
///   2) el codi a les línies següents: RUN|BLENDER|import bpy
///                                     bpy.ops…
///   3) un bloc ```python sencer:     RUN|BLENDER|
///                                     ```python
///                                     import bpy
///                                     ```
/// Retorna el codi i quantes línies addicionals s'han consumit.
fn collect_script(lines: &[&str], start: usize, inline: &str) -> (String, usize) {
    let mut out: Vec<String> = Vec::new();
    let mut used = 0usize;
    let mut in_fence = false;
    let inline = inline.trim();
    if inline.starts_with("```") {
        // La directriu obre la tanca: el codi és a les línies següents.
        in_fence = true;
    } else if !inline.is_empty() {
        out.push(inline.to_string());
    }
    let mut idx = start;
    while idx < lines.len() {
        let l = lines[idx];
        let trimmed = l.trim_start();
        if in_fence {
            if trimmed.starts_with("```") {
                // Tanca de la tanca: el codi s'ha acabat aquí.
                used += 1;
                break;
            }
            out.push(l.to_string());
            idx += 1;
            used += 1;
            continue;
        }
        // Una tanca nova: el codi és dins, fins a la tanca de tancament.
        if trimmed.starts_with("```") {
            in_fence = true;
            idx += 1;
            used += 1;
            continue;
        }
        // Una altra directriu: aquí acaba el codi.
        if starts_directive(l) {
            break;
        }
        if crate::blender::looks_like_code_line(l) {
            out.push(l.to_string());
            idx += 1;
            used += 1;
            continue;
        }
        // Línia buida: si el següent és codi, és un salt dins del bloc; si no,
        // el codi ja s'ha acabat (aquí comença la prosa del model).
        if l.trim().is_empty() {
            let seg = lines
                .get(idx + 1)
                .map(|n| crate::blender::looks_like_code_line(n))
                .unwrap_or(false);
            if seg {
                out.push(String::new());
                idx += 1;
                used += 1;
                continue;
            }
        }
        break;
    }
    (out.join("\n"), used)
}

/// Extreu les directrius en línies pròpies: «RUN|…», «RUN|BG|…» i «IMG|…».
/// La comanda de «RUN|» és tota la resta de la línia, perquè accepta pipes
/// i «&&» sense emportar-se'ls com a separadors. «RUN|BLENDER|» i
/// «RUN|UNREAL|» poden dur varies línies: hi va tot el codi.
pub fn parse(text: &str) -> Vec<RunAction> {
    let mut out = Vec::new();
    let lines: Vec<&str> = text.lines().collect();
    let mut i = 0usize;
    while i < lines.len() {
        let t = directive_line(lines[i]);
        let is_run = t
            .get(..4)
            .map(|s| s.eq_ignore_ascii_case("run|"))
            .unwrap_or(false);
        if is_run {
            let rest = t[4..].trim_start();
            let up = rest.to_ascii_uppercase();
            if up.starts_with("BLENDER|") {
                let inline = rest["BLENDER|".len()..].trim().to_string();
                let (code, used) = collect_script(&lines, i + 1, &inline);
                if !code.trim().is_empty() {
                    out.push(RunAction::Blender(code));
                }
                i = i + 1 + used;
                continue;
            }
            if up.starts_with("UNREAL|") {
                let inline = rest["UNREAL|".len()..].trim().to_string();
                let (code, used) = collect_script(&lines, i + 1, &inline);
                if !code.trim().is_empty() {
                    out.push(RunAction::Unreal(code));
                }
                i = i + 1 + used;
                continue;
            }
            if up.starts_with("BG|") {
                let arg = rest["BG|".len()..].trim().to_string();
                if !arg.is_empty() {
                    out.push(RunAction::Background(arg));
                }
                i += 1;
                continue;
            }
            // Dues directives ENGANXADES sense salt de línia (model menut:
            // «RUN|pip install -r r.txtRUN|python3 main.py») eren UNA sola
            // comanda innombrable: el «pip install» no s'executava mai i la
            // prosa del model s'empassava la comanda. Ara es separen.
            for cmd in split_glued(&rest) {
                out.push(RunAction::Foreground(cmd));
            }
            i += 1;
            continue;
        }
        i += 1;
    }
    if let Ok(re) = regex::Regex::new(r"(?im)^[ \t>*-]*img\|([^|\r\n]+)(?:\|([^\r\n]*))?") {
        for c in re.captures_iter(text) {
            let g = |i: usize| c.get(i).map(|m| m.as_str().trim().to_string()).unwrap_or_default();
            let prompt = g(1);
            if !prompt.is_empty() {
                out.push(RunAction::Image(prompt, g(2)));
            }
        }
    }
    out
}

/// Separa les «RUN|» que el model ha enganxat a la MATEIXA línia: les
/// talla en tantes comandes directives. Només es talla davant d'un «RUN|»
/// encolat al text anterior (un de precedit per espai o inici de línia ja
/// l'agafa el bucle de `parse`) i MAI dins d'una part entrecomillada, perquè
/// «echo "a RUN|b"» no es talli.
fn split_glued(cmd: &str) -> Vec<String> {
    let chars: Vec<char> = cmd.chars().collect();
    let low: Vec<char> = cmd.to_ascii_lowercase().chars().collect();
    let mut out: Vec<String> = Vec::new();
    let mut start = 0usize;
    let mut quote: Option<char> = None;
    let mut i = 0usize;
    while i < chars.len() {
        if let Some(q) = quote {
            if chars[i] == q {
                quote = None;
            }
            i += 1;
            continue;
        }
        if chars[i] == '\'' || chars[i] == '"' {
            quote = Some(chars[i]);
            i += 1;
            continue;
        }
        let glued = i > start
            && i + 4 <= low.len()
            && low[i] == 'r'
            && low[i + 1] == 'u'
            && low[i + 2] == 'n'
            && low[i + 3] == '|'
            && !chars[i - 1].is_whitespace();
        if glued {
            let piece: String = chars[start..i].iter().collect();
            if !piece.trim().is_empty() {
                out.push(piece.trim().to_string());
            }
            start = i + 4;
            i = start;
            continue;
        }
        i += 1;
    }
    let tail: String = chars[start..].iter().collect();
    if !tail.trim().is_empty() {
        out.push(tail.trim().to_string());
    }
    out
}

/// Segell de les ACCIONS d'una resposta del model —només el que NoOrbit
/// executaria o aplicaria, mai la prosa— per detectar reemissions. Dues
/// respostes amb paraules diferents i el MESMEIX pla d'accions donen el
/// mateix segell: si això passa dos torns seguits, repetir l'acció ja no pot
/// donar un resultat nou. Sense això, un model encallat emetia 15 blocs
/// idèntics («@edit:» amb CERCA == NOU i la mateixa «RUN|») fins a esgotar
/// `MAX_ROUNDS` mentre el xat s'omplia de repetició.
///
/// Retorna `""` quan no hi ha cap acció (llavors el guard no s'aplica: una
/// resposta en prosa no pot quedar en bucle d'eines).
pub fn action_fingerprint(text: &str) -> String {
    let mut parts: Vec<String> = Vec::new();
    for a in parse(text) {
        parts.push(format!("run {a:?}"));
    }
    for b in extract_edit_blocks(text) {
        parts.push(format!("edit {}", b.path));
        for (find, replace) in &b.edits {
            parts.push(format!("  {}→{}", hash_short(find), hash_short(replace)));
        }
    }
    for (p, c) in extract_file_blocks(text) {
        parts.push(format!("file {p} {}", hash_short(&c)));
    }
    for nb in crate::browser::tools::parse(text) {
        parts.push(format!("nb {nb:?}"));
    }
    parts.join("\n")
}

/// Hash curt i estable (FNV-1a) per al segell d'accions: no cal criptografia,
/// només comparar dos torns consecutius del mateix model sense emportar-se
/// tot el codi a la memòria.
fn hash_short(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// Neteja el nom que ve darrere de «@file:»: cometes, barres inverses,
/// asteriscos de markdown i puntuació final. La ruta neta és el que s'escriu.
fn clean_file_path(p: &str) -> String {
    p.trim()
        .trim_matches(|c: char| c == '`' || c == '"' || c == '\'' || c == '*' || c == '«' || c == '»')
        .trim_end_matches(|c: char| c == ';' || c == ':' || c == ',')
        .trim()
        .to_string()
}

/// Neteja els ARTEFACTES DE LA INTERFÍCIE que el model pot copiar del xat:
/// l'etiqueta del llenguatge («PHP», «Python»…) i el botó «Copia» que hi ha
/// just damunt del codi. No són codi i feien que cap edició coincidís.
pub fn strip_ui_artifacts(content: &str) -> String {
    let langs = [
        "php", "python", "javascript", "typescript", "css", "scss", "html", "json",
        "bash", "shell", "sql", "yaml", "yml", "toml", "rust", "go", "java", "c",
        "cpp", "csharp", "cs", "diff", "markdown", "text", "plaintext", "xml", "ini",
        "swift", "kt", "kotlin", "ruby", "rb", "vue", "svelte", "astro", "graphql",
        "dockerfile", "makefile", "code",
    ];
    let mut lines = content.lines();
    let mut out: Vec<&str> = Vec::new();
    let mut started = false;
    for line in lines.by_ref() {
        let t = line.trim();
        if !started {
            if t.is_empty() {
                continue;
            }
            let low = t.to_lowercase();
            if low == "copia" || low == "còpia" || low == "copy" || langs.contains(&low.as_str()) {
                continue;
            }
            started = true;
        }
        out.push(line);
    }
    let mut s = out.join("\n");
    if !s.is_empty() {
        s.push('\n');
    }
    s
}

/// Extreu els blocs «@file: ruta» de la resposta del model com a
/// (ruta relativa, contingut). HEURÍSTICA: si el model embolica el codi en
/// tanques ``` , el contingut és NOMÉS el de DINS de les tanques; si no en
/// porta, les línies nues fins al següent marcador o directriu. Les
/// directrius «RUN|/IMG|/NB|», els «@dir:» i els «@delete:» INTERROMPEN el
/// bloc: la prosa i les ordres posteriors no entren al fitxer.
pub fn extract_file_blocks(text: &str) -> Vec<(String, String)> {
    let Ok(file_re) = regex::Regex::new(r"(?i)^[ \t>#*-]*(?:\d+[.)]\s+)?@?file:\s*(.+?)\s*$") else {
        return Vec::new();
    };
    let Ok(stop_re) = regex::Regex::new(
        r"(?i)^[ \t>#*-]*(?:\d+[.)]\s+)?@?(dir|directori|carpeta|folder|delete|esborra|borra|elimina|remove|rm)\w*:|^[ \t>#*-]*(?:\d+[.)]\s+)?@?(edit|modifica|substitueix|reemplaça)\w*:|^[ \t>#*-]*(run|img|nb)\|",
    ) else {
        return Vec::new();
    };

    #[derive(Default)]
    struct Block {
        path: String,
        fenced: Vec<String>,
        bare: Vec<String>,
        in_fence: bool,
        had_fence: bool,
    }

    let mut blocks: Vec<Block> = Vec::new();
    let mut cur: Option<Block> = None;
    for line in text.lines() {
        if let Some(c) = file_re.captures(line) {
            if let Some(b) = cur.take() {
                blocks.push(b);
            }
            let mut b = Block::default();
            b.path = clean_file_path(c.get(1).map(|m| m.as_str()).unwrap_or(""));
            cur = Some(b);
            continue;
        }
        let Some(b) = cur.as_mut() else { continue };
        if line.trim_start().starts_with("```") {
            b.in_fence = !b.in_fence;
            if b.in_fence {
                b.had_fence = true;
            }
            continue;
        }
        if stop_re.is_match(line) {
            if let Some(done) = cur.take() {
                blocks.push(done);
            }
            continue;
        }
        if b.in_fence {
            b.fenced.push(line.to_string());
        } else {
            b.bare.push(line.to_string());
        }
    }
    if let Some(b) = cur.take() {
        blocks.push(b);
    }

    blocks
        .into_iter()
        .filter(|b| !b.path.is_empty())
        .map(|b| {
            let src = if b.had_fence && !b.fenced.is_empty() {
                &b.fenced
            } else {
                &b.bare
            };
            let content = strip_ui_artifacts(&src.join("\n").trim_matches('\n').to_string());
            (b.path, content)
        })
        .filter(|(_, c)| !c.trim().is_empty())
        .collect()
}

/// Extreu els blocs «@edit: ruta» (edicions QUIRÚRGIQUES) de la resposta:
/// cada bloc porta una o més seccions «CERCA / NOU» que NoOrbit substitueix
/// dins del fitxer REAL. És la forma que poden seguir els models locals
/// menuts per MODIFICAR o ESBORRAR codi (NOU buit = esborrar) sense haver
/// de reemetre el fitxer sencer amb «@file:» (que tallen o inventen).
///
/// Format tolerat:
///   @edit: ruta/relativa
///   <<<<<<< CERCA
///   codi antic
///   =======
///   codi nou (buit = esborrar)
///   >>>>>>>
pub struct EditBlock {
    pub path: String,
    /// (cerca, nou) per cada secció del bloc.
    pub edits: Vec<(String, String)>,
}

pub fn extract_edit_blocks(text: &str) -> Vec<EditBlock> {
    let Ok(edit_re) = regex::Regex::new(r"(?i)^[ \t>*-]*@?(edit|modifica|substitueix|reemplaça)\w*:\s*(.+?)\s*$")
    else {
        return Vec::new();
    };
    let is_marker = |l: &str, m: &str| -> bool {
        let t = l.trim();
        (t.starts_with("<<<<<<<") && (m == "open"))
            || (t.starts_with("=======") && (m == "mid"))
            || (t.starts_with(">>>>>>>") && (m == "close"))
    };
    let mut blocks: Vec<EditBlock> = Vec::new();
    let mut cur: Option<EditBlock> = None;
    let mut section: Option<(Vec<String>, Vec<String>, u8)> = None; // 0=cerca 1=nou
    let flush_section = |cur: &mut Option<EditBlock>, s: &mut Option<(Vec<String>, Vec<String>, u8)>| {
        if let (Some(b), Some((find, rep, _))) = (cur.as_mut(), s.take()) {
            let f = find.join("\n").trim_matches('\n').to_string();
            let r = strip_ui_artifacts(&rep.join("\n").trim_matches('\n').to_string())
                .trim_end_matches('\n')
                .to_string();
            if !f.trim().is_empty() {
                b.edits.push((f, r));
            }
        } else {
            *s = None;
        }
    };
    for line in text.lines() {
        if let Some(c) = edit_re.captures(line) {
            flush_section(&mut cur, &mut section);
            let new_path = clean_file_path(c.get(2).map(|m| m.as_str()).unwrap_or(""));
            let same = matches!(
                cur.as_ref(),
                Some(b) if !new_path.is_empty() && b.path == new_path
            );
            if !same {
                if let Some(b) = cur.take() {
                    blocks.push(b);
                }
                cur = Some(EditBlock {
                    path: new_path,
                    edits: Vec::new(),
                });
            }
            continue;
        }
        if is_marker(line, "open") {
            flush_section(&mut cur, &mut section);
            section = Some((Vec::new(), Vec::new(), 0));
            continue;
        }
        if is_marker(line, "mid") {
            if let Some((_, _, phase)) = section.as_mut() {
                *phase = 1;
            }
            continue;
        }
        if is_marker(line, "close") {
            flush_section(&mut cur, &mut section);
            continue;
        }
        let Some(s) = section.as_mut() else { continue };
        if cur.is_none() {
            continue;
        }
        match s.2 {
            0 => s.0.push(line.to_string()),
            _ => s.1.push(line.to_string()),
        }
    }
    flush_section(&mut cur, &mut section);
    if let Some(b) = cur.take() {
        blocks.push(b);
    }
    blocks.into_iter().filter(|b| !b.path.is_empty() && !b.edits.is_empty()).collect()
}

/// Aplica els blocs «@edit:» AL DISC (abans d'executar les «RUN|»): cerca el
/// text CERCA dins el fitxer real (exacte, o per línies retallades —tolera
/// diferències d'indentació—) i el substitueix per NOU (buit = esborrar).
/// Retorna un informe «ruta: …» per tornar-lo al model com a RESULTAT.
pub fn apply_edit_blocks(app: &AppHandle, text: &str) -> Vec<String> {
    let Some(root) = crate::commands::workspace::current_root() else {
        return Vec::new();
    };
    let report = apply_edit_blocks_at(&root, text);
    if !report.is_empty() {
        progress(app, &format!("✏️ Edicions: {}", clip(&report.join(" · "), 160)), true);
    }
    report
}

/// Versió amb ARREL explícita: la fan servir les feines autònomes (Jam), que
/// corren en un subprocés sense cap projecte «obert» a la UI i, per tant, no
/// poden dependre de `workspace::current_root()`.
pub fn apply_edit_blocks_at(root: &std::path::Path, text: &str) -> Vec<String> {
    let mut report: Vec<String> = Vec::new();
    // Rutes REALMENT editades (escrites al disc): es registren al snapshot
    // perquè «Desfés el pas» només esborre fitxers de l'agent, no de l'usuari.
    let mut edited: Vec<String> = Vec::new();
    let blocks = extract_edit_blocks(text);
    // SNAPSHOT abans del lot d'escriptures (git shadow): si git no hi és,
    // es degrada amb nota i les edicions es fan igualment.
    if !blocks.is_empty() {
        if let Err(e) = crate::agent::snapshot::take(root) {
            eprintln!("NoOrbit/snapshot: sense snapshot del lot d'edicions ({e})");
        }
    }
    for block in blocks {
        let Some(rel) = write_rel(root, &block.path) else {
            // Abans aquest cas feia `continue` MUT: el model rebia un informe
            // BUÍT, no sabia que la seva ruta era invàlida i la reemetia
            // EXACTAMENT igual torn rere torn.
            report.push(format!(
                "{}: ERROR la ruta és FORA del projecte obert (l'arrel és «{}»). Les «@edit:» només accepten rutes RELATIVES a l'arrel: reemet-les amb la ruta relativa (p. ex. «main.py» o «src/main.py»). Si el fitxer és de fora del projecte, NoOrbit no pot tocar-lo: digues-ho en prosa.",
                block.path,
                root.display()
            ));
            continue;
        };
        let path = root.join(&rel);
        let Ok(old) = std::fs::read_to_string(&path) else {
            report.push(format!("{}: ERROR no existeix (no es pot editar)", rel));
            continue;
        };
        let mut new_content = old.clone();
        let mut ok = 0usize;
        let mut fail = 0usize;
        let mut noop = 0usize;
        // Fragments REALS més semblants als CERCA que no han coincidit: es
        // tornen al model perquè corregeixi en UN sol reintent (abans només
        // rebia «cal el text EXACTE» i tornava a fallar a cegues).
        let mut miss_snippets: Vec<String> = Vec::new();
        for (find, replace) in &block.edits {
            // Secció NO-OP: el model copia el MATEIX text a CERCA i a NOU —
            // no canvia res. Comptar-la com a «aplicada» mentia a la UI i
            // distreia el bucle (el model no rebia cap avís).
            if find.trim_end() == replace.trim_end() {
                noop += 1;
                continue;
            }
            if old.contains(find.as_str()) {
                new_content = new_content.replacen(find.as_str(), replace, 1);
                ok += 1;
                continue;
            }
            // Coincidència per LÍNIES retallades (tolera indentació distinta):
            let find_lines: Vec<String> = find.lines().map(|l| l.trim().to_string()).collect();
            let content_lines: Vec<String> = new_content.lines().map(|l| l.to_string()).collect();
            let n = find_lines.len();
            let mut hit: Option<usize> = None;
            if n > 0 && n <= content_lines.len() {
                for i in 0..=(content_lines.len() - n) {
                    if (0..n).all(|k| content_lines[i + k].trim() == find_lines[k]) {
                        hit = Some(i);
                        break;
                    }
                }
            }
            if let Some(i) = hit {
                let repl_lines: Vec<String> = if replace.is_empty() {
                    Vec::new()
                } else {
                    replace.lines().map(|l| l.to_string()).collect()
                };
                let mut out: Vec<String> = Vec::new();
                out.extend_from_slice(&content_lines[..i]);
                out.extend(repl_lines);
                out.extend_from_slice(&content_lines[i + n..]);
                new_content = out.join("\n");
                if !new_content.ends_with('\n') {
                    new_content.push('\n');
                }
                ok += 1;
            } else {
                fail += 1;
                if miss_snippets.len() < 2 {
                    if let Some(snip) = closest_snippet(&new_content, find) {
                        miss_snippets.push(snip);
                    }
                }
            }
        }
        let miss_help = if miss_snippets.is_empty() {
            String::new()
        } else {
            format!(
                "\n{}\nCorregeix el CERCA copiant el text EXACTE d'aquest fragment real (mateixa indentació) i reemet l'edició en UN sol reintent.",
                miss_snippets.join("\n")
            )
        };
        if ok > 0 && std::fs::write(&path, &new_content).is_ok() {
            edited.push(rel.clone());
            report.push(format!("{}: {} edició/ons aplicades{}{}{}", rel, ok,
                if fail > 0 { format!(" ({} no trobades)", fail) } else { String::new() },
                if noop > 0 { format!(" ({} seccions no-op ignorades)", noop) } else { String::new() },
                miss_help));
        } else if ok == 0 && noop > 0 {
            // Tot el bloc era CERCA == NOU: no hi ha cap canvi què escriure.
            report.push(format!(
                "{}: les {} seccions CERCA/NOU són IDÈNTIQUES (no canvien res) — edició inútil, no aplicada. Si el fitxer ja és correcte, NO emetis res; si has de canviar alguna cosa, emet el text NOU DISTINT.",
                rel, noop
            ));
        } else if fail > 0 {
            report.push(format!(
                "{}: ERROR el text CERCA no coincideix amb el fitxer real ({} seccions).{}{}",
                rel, fail,
                if miss_snippets.is_empty() { " Reemet amb el text EXACTE.".to_string() } else { String::new() },
                miss_help
            ));
        }
    }
    if !edited.is_empty() {
        crate::agent::snapshot::record_written(root, &edited);
    }
    report
}

/// Escriu AL DISC, dins el projecte obert, els fitxers anunciats amb «@file:»
/// ABANS d'executar les «RUN|» del torn. Sense això, «python3 script.py» en el
/// mateix torn que l'anunci fallava amb «No such file or directory»: el
/// frontend no escrivirà el fitxer fins que la resposta acabe. És idempotent:
/// la materialització del frontend (autoMaterialize) torna a escriure'l al
/// final del torn amb el contingut definitiu. Rutes absolutes o amb «..»
/// es rebutgen (seguretat).
pub fn write_announced_files(app: &AppHandle, text: &str) -> Vec<String> {
    let Some(root) = crate::commands::workspace::current_root() else {
        return Vec::new();
    };
    // SNAPSHOT abans del lot d'escriptures (git shadow), amb degradació.
    if !extract_file_blocks(text).is_empty() {
        if let Err(e) = crate::agent::snapshot::take(&root) {
            eprintln!("NoOrbit/snapshot: sense snapshot del lot de fitxers ({e})");
        }
    }
    let written = write_announced_files_at(&root, text);
    if !written.is_empty() {
        // Registra les rutes escrites al snapshot del pas (undo selectiu).
        crate::agent::snapshot::record_written(&root, &written);
        progress(
            app,
            &format!("💾 Fitxers escrits abans d'executar: {}", clip(&written.join(", "), 140)),
            true,
        );
    }
    written
}

/// Bucle REAL d'escriptura dels «@file:» sobre una arrel donada (sense
/// AppHandle ni snapshot): el compartixen `write_announced_files` i els tests
/// d'integració «frase → fitxers → verificació». Mateixa contenció que les
/// edicions: `write_rel` relativitza les rutes que reprodueixen l'arrel i
/// rebutja les absolutes de fora (abans `safe_rel` les callava i el frontend
/// les escrivia dins d'una carpeta fantasma «Volumes/…»).
pub fn write_announced_files_at(root: &std::path::Path, text: &str) -> Vec<String> {
    let mut written: Vec<String> = Vec::new();
    for (rel, content) in extract_file_blocks(text) {
        let Some(rel) = write_rel(root, &rel) else { continue };
        let path = root.join(&rel);
        if let Some(par) = path.parent() {
            let _ = std::fs::create_dir_all(par);
        }
        if std::fs::write(&path, content).is_ok() {
            written.push(rel);
        }
    }
    written
}

/// Rutes «@file:» que `write_rel` REBUTJA (absolutes de FORA de l'arrel o amb
/// «..»), tal com el model les va escriure: només se les ha de poder
/// reconèixer a l'informe. Les edicions ja ho fan (`apply_edit_blocks_at`),
/// però els fitxers anunciats es descartaven amb un `continue` MUT i el
/// model tornava a emetre la mateixa ruta invàlida torn rere torn.
pub fn rejected_write_paths(root: &std::path::Path, text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for (raw, _) in extract_file_blocks(text) {
        let t = raw.trim().to_string();
        if write_rel(root, &t).is_none() && !out.contains(&t) {
            out.push(t);
        }
    }
    out
}

/// Avança el pas de progrés al xat (mateix event que la resta de l'agent).
fn progress(app: &AppHandle, detail: &str, ok: bool) {
    let _ = app.emit(
        "agent://progress",
        json!({
            "label": "Terminal del projecte",
            "detail": detail,
            "ok": ok,
            "session": crate::ai::current_session(),
        }),
    );
}

/// Retalla text per no inflar el context del model.
fn clip(s: &str, n: usize) -> String {
    let v: Vec<char> = s.chars().collect();
    if v.len() <= n {
        s.to_string()
    } else {
        format!("{}…[retallat]", v[..n].iter().collect::<String>())
    }
}

/// Empra la comanda amb cometes simples per a «cd» (rutes amb espais).
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// Ruta relativa neta dins el projecte (mai «..», mai absoluta).
fn safe_rel(p: &str) -> Option<String> {
    let t = p.trim().replace('\\', "/");
    if t.is_empty()
        || t.starts_with('/')
        || t.split('/').any(|s| s == "..")
        || t.contains(':')
    {
        return None;
    }
    Some(t)
}

/// Ruta relativa de la IA per a UNA escriptura (o un lint) dins del projecte.
///
/// Els models copien el «Projecte (cwd): /Volumes/…» que `sysprompt::env_layer`
/// els injecta i posen aquesta ruta ABSOLUTA dins «@file:»/«@edit:». Abans
/// això fallava pels dos camins, i de manera contradictòria:
///  - el backend (`safe_rel`) rebutjava l'absoluta i el bloc es descartava
///    SENSE dir-ho al model (reemissió infinita);
///  - el frontend (`autoMaterialize`) arrossegava la barra inicial i
///    rebasteixia la ruta literalment DINS de l'arrel: naixia una carpeta
///    fantasma «<arrel>/Volumes/Toshi/programes/…» que `guard_in_root` no
///    veia —perquè sí que era sota l'arrel— i el projecte quedava brut.
///
/// Ara la ruta que REPRODUEIX l'arrel (amb o sense barra inicial) es
/// relativitza i apunta al fitxer que el model volia dir; les absolutes de
/// FORA de l'arrel i les que porten «..» continuen sent `None` (mai
/// s'escriuen), però ja amb un missatge explícit al report.
pub fn write_rel(root: &std::path::Path, raw: &str) -> Option<String> {
    let t = raw.trim().replace('\\', "/");
    if t.is_empty() || t.split('/').any(|s| s == "..") {
        return None;
    }
    if t.starts_with('/') {
        return under_root(root, &t).or_else(|| under_root_ic(root, &t));
    }
    // Ruta RELATIVA: és la forma correcta del protocol i es respecta TAL QUAL.
    // (Relativitzar-ne una de relativa podria menjar-se'n el primer segment
    // quan coincideix amb el nom de l'arrel.)
    if t.contains('/') {
        // Excepció: el cwd que el model torna a escriure SENSE barra inicial
        // («Volumes/Toshi/programes/forestin the sky/main.py») és EXACTAMENT
        // la ruta de l'arrel; tractar-la com a relativa creava la carpeta falsa.
        if let Some(rel) = under_root(root, &format!("/{t}")) {
            return Some(rel);
        }
    }
    safe_rel(&t)
}

/// Part relativa d'una ruta ABSOLUTA si cau sota l'arrel (prefix exacte) o si
/// en REPRODUEIX el començament (arrel montada en un altre punt de muntatge o
/// amb majúscules diferents: `/Volumes/Toshi` vs `/Volumes/toshi`). Dona `None`
/// si n'és fora o si és l'arrel mateixa (que no és cap fitxer).
fn under_root(root: &std::path::Path, abs: &str) -> Option<String> {
    abs_rel_under_root(root, abs, false)
}

/// Segon intent, sense distingir majúscules: macOS munta els volums amb el nom
/// que va escriure l'usuari i el model el copia a mitges.
fn under_root_ic(root: &std::path::Path, abs: &str) -> Option<String> {
    abs_rel_under_root(root, abs, true)
}

fn abs_rel_under_root(root: &std::path::Path, abs: &str, ignore_case: bool) -> Option<String> {
    if !std::path::Path::new(abs).is_absolute() || !root.is_absolute() {
        return None;
    }
    let eq = |a: &str, b: &str| {
        if ignore_case {
            a.eq_ignore_ascii_case(b)
        } else {
            a == b
        }
    };
    let seg = |p: &std::path::Path| -> Vec<String> {
        p.components()
            .map(|c| c.as_os_str().to_string_lossy().to_string())
            .collect()
    };
    // Variant canònica de l'arrel: amb `/Volumes/…`, symlink o `../` resolts,
    // el prefix escrit pel model només coincideix amb la forma real del disc.
    let root_canon = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let roots: [Vec<String>; 2] = [seg(root), seg(&root_canon)];
    let p_canon = std::path::Path::new(abs)
        .canonicalize()
        .unwrap_or_else(|_| std::path::PathBuf::from(abs));
    let mut ps = seg(std::path::Path::new(abs));
    let pscan = seg(&p_canon);
    if pscan.len() > ps.len() {
        ps = pscan;
    }
    if ps.first().map(|s| s.as_str()) != Some("/") {
        return None;
    }
    // La ruta pot REPRODUIR l'arrel diverses vegades (la forma real del
    // transcript: «/arrel/arrel/main.py»); es retalla tantes com apareixi al
    // capdamunt, mai a l'inrevés: una ruta que no és del projecte no s'hi cola.
    let mut i = 1usize;
    loop {
        let mut cops = 0usize;
        for r in &roots {
            if r.len() < 2 || r[0] != "/" {
                continue;
            }
            let depth = r.len() - 1;
            // L'arrel ha de ser prefix REAL i n'ha de quedar res darrere (si no,
            // la ruta és l'arrel mateixa o més curta: no és cap fitxer del projecte).
            if ps.len() - i < depth || ps.len() - (i + depth) < 1 {
                continue;
            }
            if ps[i..].starts_with(&r[1..]) {
                cops = depth;
                break;
            }
            if ignore_case
                && ps[i..]
                    .iter()
                    .zip(r[1..].iter())
                    .all(|(a, b)| eq(a, b))
            {
                cops = depth;
                break;
            }
        }
        if cops == 0 {
            break;
        }
        i += cops;
    }
    if i == 1 {
        return None; // la ruta no comença per l'arrel: és fora del projecte
    }
    safe_rel(&ps[i..].join("/"))
}

/// Ruta relativa per al LINT: mateixa regla que les escriptures (`write_rel`),
/// perquè el frontend de vegades envia rutes ABSOLUTES de pestanya i, si no
/// es relativitzen igual que les escriptures, el lint diria «ruta fora del
/// projecte» sobre un fitxer que sí que és del projecte.
pub fn lint_rel(root: &std::path::Path, raw: &str) -> Option<String> {
    write_rel(root, raw)
}

/// Fragment REAL del fitxer més semblant al text CERCA que no ha coincidit:
/// finestra de línies amb màxima coincidència de línies NORMALITZADES
/// (espais col·lapsats). Inclou els números de línia perquè el model puga
/// copiar el text exacte i corregir l'edició en un sol reintent.
pub fn closest_snippet(content: &str, find: &str) -> Option<String> {
    let norm = |l: &str| l.split_whitespace().collect::<Vec<_>>().join(" ");
    let find_lines: Vec<String> = find.lines().map(norm).collect();
    let n = find_lines.len();
    if n == 0 {
        return None;
    }
    let content_lines: Vec<&str> = content.lines().collect();
    if content_lines.is_empty() {
        return None;
    }
    let content_norm: Vec<String> = content_lines.iter().map(|l| norm(l)).collect();
    let window = n.min(content_lines.len());
    // Puntuació per finestra: BIGRAMES de tokens compartits amb el CERCA
    // (capturen l'ORDRE real del codi, no sols quines línies hi ha) + pes
    // fort per línies alineades en la mateixa posició + ancoratge de la
    // primera línia (la signatura del bloc). Així, un bloc «resta» amb una
    // línia errònia NO s'aparella amb el bloc «suma» veí.
    let bigrams = |text: &str| -> Vec<(String, String)> {
        let toks: Vec<&str> = text.split_whitespace().collect();
        toks.windows(2)
            .map(|w| (w[0].to_string(), w[1].to_string()))
            .collect()
    };
    let find_bg: std::collections::HashSet<(String, String)> =
        bigrams(&find_lines.join(" ")).into_iter().collect();
    let mut best: Option<(usize, usize)> = None; // (puntuació, inici)
    for i in 0..=(content_lines.len() - window) {
        let win_bg = bigrams(&content_norm[i..i + window].join(" "));
        let shared = win_bg.iter().filter(|b| find_bg.contains(*b)).count();
        let aligned = (0..window)
            .filter(|&k| content_norm[i + k] == find_lines[k])
            .count();
        let anchor = usize::from(content_norm[i] == find_lines[0]);
        let score = shared + 3 * aligned + 2 * anchor;
        if score > 0 && best.map(|(s, _)| score > s).unwrap_or(true) {
            best = Some((score, i));
        }
    }
    let (_, start) = best?;
    let end = (start + window).min(content_lines.len()).min(start + 40);
    let mut s = format!("Fragment real més semblant (línies {}-{}):\n", start + 1, end);
    for (idx, l) in content_lines[start..end].iter().enumerate() {
        s.push_str(&format!("{} | {}\n", start + 1 + idx, l));
    }
    Some(s)
}

// ─────────────────────────────────────────────────────────────────────────────
//  VERIFICACIÓ OBLIGATÒRIA del codi escrit (lint barat amb eines reals)
// ─────────────────────────────────────────────────────────────────────────────

/// Límit de reintents de verificació PER TORN de xat: després de 3 rondes
/// d'errors realimentades, el bucle no torna a comprovar (el model ja ho sap).
pub const MAX_VERIFY_RETRIES: usize = 3;

/// Resultat de verificar UN fitxer. Contracte exacte per al frontend:
/// `{ path, ok, output }` (el camp intern `checked` NO se serialitza).
#[derive(Debug, Clone, serde::Serialize)]
pub struct LintResult {
    pub path: String,
    pub ok: bool,
    pub output: String,
    /// Intern: si realment s'ha executat una eina de lint sobre el fitxer.
    #[serde(skip)]
    pub checked: bool,
}

/// Binari de PHP: el del PATH amb fallback a la ruta d'instal·lació local
/// (les apps llançades des del Finder tenen un PATH mínim).
pub fn php_binary() -> Option<std::path::PathBuf> {
    if let Ok(p) = which::which("php") {
        return Some(p);
    }
    let fallback = std::path::Path::new("/usr/local/bin/php");
    if fallback.is_file() {
        return Some(fallback.to_path_buf());
    }
    None
}

/// Binari de Node.js: el del PATH amb fallback a les rutes habituals.
pub fn node_binary() -> Option<std::path::PathBuf> {
    if let Ok(p) = which::which("node") {
        return Some(p);
    }
    for p in ["/usr/local/bin/node", "/opt/homebrew/bin/node"] {
        let pb = std::path::Path::new(p);
        if pb.is_file() {
            return Some(pb.to_path_buf());
        }
    }
    None
}

/// Verifica UN fitxer (relatiu a `root`, amb contenció): `.php` → `php -l`,
/// `.js/.mjs/.cjs` → `node --check`. Altres extensions → ok:true sense
/// sortida. Si l'eina no existeix, s'OMET amb nota (no és un error del codi).
/// Rutes fora del projecte es rebutgen. BLOQUEJANT: cridar-la en un fil a part.
fn lint_one(root: &std::path::Path, raw: &str) -> LintResult {
    let Some(rel) = lint_rel(root, raw) else {
        return LintResult {
            path: raw.to_string(),
            ok: false,
            output: "ruta fora del projecte o no segura: rebutjada".into(),
            checked: false,
        };
    };
    let path = root.join(&rel);
    let ext = path.extension().and_then(|x| x.to_str()).unwrap_or("").to_ascii_lowercase();
    let (bin, arg, tool): (Option<std::path::PathBuf>, &str, &str) = match ext.as_str() {
        "php" => (php_binary(), "-l", "php"),
        "js" | "mjs" | "cjs" => (node_binary(), "--check", "node"),
        _ => {
            return LintResult { path: rel, ok: true, output: String::new(), checked: false };
        }
    };
    if !path.is_file() {
        return LintResult { path: rel, ok: false, output: "el fitxer no existeix".into(), checked: false };
    }
    let Some(bin) = bin else {
        return LintResult {
            path: rel,
            ok: true,
            output: format!("verificació omesa: {tool} no instal·lat"),
            checked: false,
        };
    };
    match std::process::Command::new(&bin)
        .arg(arg)
        .arg(&path)
        .current_dir(root)
        .stdin(std::process::Stdio::null())
        .output()
    {
        Ok(out) => {
            let ok = out.status.success();
            let mut text = String::from_utf8_lossy(&out.stdout).trim().to_string();
            let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
            if !err.is_empty() {
                if !text.is_empty() {
                    text.push('\n');
                }
                text.push_str(&err);
            }
            LintResult {
                path: rel,
                ok,
                // Els errors solen ser a la CUA: es conserva principi i final.
                output: if ok { String::new() } else { clip_head_tail(&text, 400, 800) },
                checked: true,
            }
        }
        Err(e) => LintResult {
            path: rel,
            ok: true,
            output: format!("verificació omesa: no s'ha pogut executar {tool} ({e})"),
            checked: false,
        },
    }
}

/// Verifica una llista de rutes relatives dins el projecte (mateix motor per
/// al xat, l'autopilotatge, la Jam i el comandament `lint_paths` de la UI).
pub fn lint_paths_blocking(root: &std::path::Path, paths: &[String]) -> Vec<LintResult> {
    paths.iter().map(|p| lint_one(root, p)).collect()
}

/// Fase 3c: enriquix l'informe de verificació amb els diagnòstics d'ERROR del
/// servidor LSP, però NOMÉS si ja hi ha una sessió ACTIVA per a eixe
/// (llenguatge, arrel): consulta barata al gestor, MAI no spawna un servidor
/// des del bucle de la IA. Temps límit curt (3 s): si el servidor va lent,
/// l'informe es queda com estava (degradació honesta).
pub fn augment_with_active_lsp(root: &std::path::Path, results: &mut [LintResult]) {
    for r in results.iter_mut() {
        let abs = root.join(&r.path);
        let Some(sess) = crate::lsp::session::active_for(root, &abs) else {
            continue;
        };
        let Ok(diags) = sess.diagnostics(&abs, std::time::Duration::from_secs(3)) else {
            continue;
        };
        let errors: Vec<&crate::lsp::session::Diag> =
            diags.iter().filter(|d| d.severity == "error").collect();
        if errors.is_empty() {
            continue;
        }
        let mut text = format!("Errors del servidor LSP ({}):\n", sess.server);
        for d in errors.iter().take(8) {
            text.push_str(&format!("línia {}, col {}: {}\n", d.line, d.col, d.message));
        }
        if !r.output.is_empty() {
            r.output.push('\n');
        }
        r.output.push_str(&clip_head_tail(&text, 300, 500));
        r.ok = false;
        r.checked = true;
    }
}

/// Construeix el text de VERIFICACIÓ que es realimenta al model. `retries_used`
/// és el nombre de rondes amb errors JA realimentades aquest torn (inclosa
/// l'actual): en arribar a `MAX_VERIFY_RETRIES` s'avisa del límit. MAI es
/// pot donar per bo un codi amb lint fallant sense dir-ho al model.
pub fn verification_report(results: &[LintResult], retries_used: usize) -> Option<String> {
    let errors: Vec<&LintResult> = results.iter().filter(|r| !r.ok).collect();
    let checked = results.iter().filter(|r| r.checked).count();
    if errors.is_empty() {
        if checked > 0 {
            return Some(format!(
                "Verificació: cap error de sintaxi ({checked} fitxer(s) comprovats amb eines reals). Pots continuar."
            ));
        }
        let skips: Vec<&LintResult> = results.iter().filter(|r| !r.output.is_empty()).collect();
        if skips.is_empty() {
            return None;
        }
        return Some(format!("Verificació: {} ({} fitxer(s) sense comprovar).", skips[0].output, skips.len()));
    }
    let mut s = format!(
        "Verificació: {} error(s) de sintaxi en {} fitxer(s) comprovats. NO donis la feina per acabada: corregeix-los ARA (reemet els fitxers amb «@file:» o «@edit:» amb el codi corregit):\n",
        errors.len(),
        checked.max(errors.len())
    );
    for e in errors.iter().take(6) {
        s.push_str(&format!("— {}: {}\n", e.path, clip_head_tail(&e.output, 300, 500)));
    }
    if retries_used >= MAX_VERIFY_RETRIES {
        s.push_str(
            "S'ha assolit el límit de 3 reintents de verificació d'aquest torn: corregeix els errors al codi i explica-ho a la resposta final; no es faran més comprovades.\n",
        );
    }
    Some(s)
}

/// Retalla conservant el PRINCIPI i la CUA (on són els errors de compilació
/// i de lint): abans només es guardava el principi i el model no veia mai
/// l'error real que hi ha al final de la sortida.
pub fn clip_head_tail(s: &str, head: usize, tail: usize) -> String {
    let v: Vec<char> = s.chars().collect();
    if v.len() <= head + tail {
        return s.to_string();
    }
    let omitted = v.len() - head - tail;
    format!(
        "{}\n…[retallat: {omitted} caràcters omitits; es conserven el principi i la CUA amb els errors]…\n{}",
        v[..head].iter().collect::<String>(),
        v[v.len() - tail..].iter().collect::<String>()
    )
}

/// Cerca un fitxer «.uproject» dins la carpeta del projecte (només nivell
/// arrel + subcarpetes d'un nivell): és el que necessita `auto_connect`.
fn find_uproject(root: &std::path::Path) -> Option<std::path::PathBuf> {
    let entries = std::fs::read_dir(root).ok()?;
    let mut nested: Option<std::path::PathBuf> = None;
    for e in entries.flatten() {
        let p = e.path();
        if p.extension().and_then(|x| x.to_str()) == Some("uproject") {
            return Some(p);
        }
        if p.is_dir() && nested.is_none() {
            if let Ok(sub) = std::fs::read_dir(&p) {
                for s in sub.flatten() {
                    let sp = s.path();
                    if sp.extension().and_then(|x| x.to_str()) == Some("uproject") {
                        nested = Some(sp);
                        break;
                    }
                }
            }
        }
    }
    nested
}

/// Executa les directrius en ordre i construeix l'informe («RUN|RESULTAT|…»)
/// que es tornarà al model com a torn d'usuari.
/// `user_prompt` és l'ordre ORIGINAL de l'usuari: serveix de context per a
/// l'agent 3D quan el model envia una descripció en lloc de codi bpy.
pub async fn execute(
    app: &AppHandle,
    computer: &Arc<ComputerController>,
    ai: &Arc<AiManager>,
    actions: &[RunAction],
    user_prompt: &str,
) -> String {
    let mut report = String::from("RUN|RESULTAT|—\n");
    let root = crate::commands::workspace::current_root();

    for a in actions {
        let line = match a {
            RunAction::Foreground(cmd) => match &root {
                None => "RUN|RESULTAT| ERROR: no hi ha cap projecte obert. Demana \
                        a l'usuari que obra la carpeta (barra lateral ▸ «Obre \
                        carpeta») i torna a intentar-ho."
                    .to_string(),
                Some(r) => {
                    progress(app, &format!("Executant: {}", clip(cmd, 90)), true);
                    let full = format!(
                        "cd {} && {}",
                        shell_quote(&r.to_string_lossy()),
                        cmd
                    );
                    match computer.execute(app, &full).await {
                        Ok(rep) => {
                            let mut s = format!(
                                "RUN|RESULTAT| $ {} | eixit={}{}",
                                clip(cmd, 200),
                                rep.exit_code,
                                if rep.timed_out { " (temps exhaurit: divideix la \
                                    tasca o usa RUN|BG|)" } else { "" }
                            );
                            let out = rep.stdout.trim();
                            let err = rep.stderr.trim();
                            // Truncament intel·ligent: capçalera (~1.000) I
                            // CUA (~1.500) — els errors solen ser al final.
                            if !out.is_empty() {
                                s.push_str("\nSORTIDA:\n");
                                s.push_str(&clip_head_tail(out, 1000, 1500));
                            }
                            if !err.is_empty() {
                                s.push_str("\nERRORS:\n");
                                s.push_str(&clip_head_tail(err, 1000, 1500));
                            }
                            if out.is_empty() && err.is_empty() {
                                s.push_str(" (cap sortida)");
                            }
                            s.push_str("\nSi cal, corregeix i torna una nova directriu.");
                            s
                        }
                        Err(e) => {
                            progress(app, &format!("Bloquejada: {}", clip(cmd, 60)), false);
                            format!("RUN|RESULTAT| $ {} | NO EXECUTADA: {}", clip(cmd, 200), e)
                        }
                    }
                }
            },
            RunAction::Background(cmd) => match &root {
                None => "RUN|RESULTAT| ERROR: no hi ha cap projecte obert per \
                        arrencar res."
                    .to_string(),
                Some(r) => {
                    progress(app, &format!("Arrencant (segon pla): {}", clip(cmd, 90)), true);
                    // nohup + & desconnecta el procés: la shell torna de seguida.
                    let full = format!(
                        "cd {} && nohup {} >/dev/null 2>&1 & echo NoOrbit:proc-desconnectat",
                        shell_quote(&r.to_string_lossy()),
                        cmd
                    );
                    match computer.execute(app, &full).await {
                        Ok(rep) if rep.ok => format!(
                            "RUN|RESULTAT| $ {} | arrencat en segon pla (continua \
                             obert fora del xat). Sortida: {}",
                            clip(cmd, 200),
                            clip(rep.stdout.trim(), 200)
                        ),
                        Ok(rep) => format!(
                            "RUN|RESULTAT| $ {} | eixit={} — {}",
                            clip(cmd, 200),
                            rep.exit_code,
                            clip(&rep.stderr, 600)
                        ),
                        Err(e) => format!("RUN|RESULTAT| $ {} | NO EXECUTADA: {}", clip(cmd, 200), e),
                    }
                }
            },
            RunAction::Blender(arg) => {
                use crate::blender::{self, BlenderSocketClient, DEFAULT_SOCKET_PORT};
                progress(app, &format!("Blender: {}", clip(arg, 80)), true);
                // Si l'argument és la ruta d'un .py del projecte, se'n llegeix
                // el contingut; si no, es toma com a codi Python literal.
                let code = match (&root, arg.trim().ends_with(".py")) {
                    (Some(r), true) => {
                        let rel = arg.trim().replace('\\', "/");
                        let rel = rel.trim_start_matches("./").to_string();
                        std::fs::read_to_string(r.join(&rel)).unwrap_or_else(|_| arg.clone())
                    }
                    _ => arg.clone(),
                };
                // Guarda: mai enviem a Blender text que no sigui codi Python.
                // Abans una frase («crea una imatge…») arribava a exec() dins
                // Blender i provocava un SyntaxError que la IA no sabia corregir.
                if let Err(e) = crate::blender::guard_python(&code) {
                    // El model ha enviat una DESCRIPCIÓ («crea un nan de jardí…»)
                    // en lloc de codi. En lloc de deixar l'usuari amb un error
                    // tècnic, li donem la feina a l'AGENT 3D de NoOrbit: té les
                    // receptes bpy (nan de jardí, materials, càmera…) i sap
                    // escriure scripts de varies línies i executar-los de veritat.
                    progress(
                        app,
                        "Blender: el model ha enviat una descripció; l'agent 3D hi posa el codi…",
                        true,
                    );
                    let desc = code.trim().to_string();
                    let task = if user_prompt.trim().is_empty() {
                        desc.clone()
                    } else {
                        format!("{}\n\n{}\n", user_prompt.trim(), desc)
                    };
                    let orch = crate::agent::AgentOrchestrator::new(
                        crate::agent::AgentConfig::default(),
                    );
                    match orch.apply_blender(ai, app, &task).await {
                        Ok((msg, ok)) => {
                            let _ = ok;
                            format!("RUN|RESULTAT| BLENDER| {}\n{}", e, msg)
                        }
                        Err(e2) => format!(
                            "RUN|RESULTAT| BLENDER| NO EXECUTADA: {} No s'ha pogut \
aplicar amb l'agent 3D ({}). Torna a emetre «RUN|BLENDER|» amb el CODI PYTHON \
real (comença per «import bpy» i acaba amb un print() en català), no amb una \
frase en llenguatge natural.",
                            e, e2
                        ),
                    }
                } else {
                    // El codi és Python però sense «import bpy» (els models locals
                    // sovint l'obliden): l'hi afegim nosaltres (és inocu si ja hi és).
                    let code = if !code.contains("import bpy") && !code.contains("from bpy") {
                        format!("import bpy\n{}", code)
                    } else {
                        code
                    };
                    let st = blender::status();
                    let exec = if st.running {
                        let code2 = code.clone();
                        tokio::task::spawn_blocking(move || {
                            BlenderSocketClient::new(DEFAULT_SOCKET_PORT).execute_auto(&code2)
                        })
                        .await
                        .map_err(|e| e.to_string())
                        .and_then(|r| r.map_err(|e| e.to_string()))
                    } else if st.installed {
                        let code2 = code.clone();
                        tokio::task::spawn_blocking(move || blender::run_blender_headless(&code2))
                            .await
                            .map_err(|e| e.to_string())
                            .and_then(|r| r.map_err(|e| e.to_string()))
                    } else {
                        Err("Blender no està instal·lat en aquest ordinador.".to_string())
                    };
                    match exec {
                        Ok(out) => {
                            let base = format!(
                                "RUN|RESULTAT| BLENDER| ({}) eixit=0 — sortida:\n{}",
                                if st.running { "editor connectat" } else { "headless" },
                                clip(out.trim(), 2000)
                            );
                            // Vídeo: si l'script ha imprimit el marcador
                            // NOORBIT_VIDEO_OK (animació + còpia .blend desada),
                            // llença el render headless i afegeix el resultat:
                            if let Some(blend) = out.lines().find_map(|l| {
                                l.split(blender::VIDEO_MARKER).nth(1).map(|p| {
                                    p.trim().trim_matches('"').split_whitespace().next().unwrap_or("").to_string()
                                })
                            }).filter(|p| !p.is_empty()) {
                                progress(app, "Blender: 🎬 render del vídeo (pot trigar)…", true);
                                match tokio::task::spawn_blocking(move || {
                                    blender::render_blender_video(&blend)
                                })
                                .await
                                .map_err(|e| e.to_string())
                                .and_then(|r| r.map_err(|e| e.to_string()))
                                {
                                    Ok(video) => {
                                        progress(app, "🎬 Vídeo renderitzat", true);
                                        return format!("{}\n\n🎬 Vídeo renderitzat: {}", base, video);
                                    }
                                    Err(e) => {
                                        return format!(
                                            "{}\n\n⚠️ El render del vídeo ha fallat: {}",
                                            base, e
                                        );
                                    }
                                }
                            }
                            base
                        }
                        Err(e) => {
                            progress(app, &format!("Blender: {}", clip(&e, 70)), false);
                            format!("RUN|RESULTAT| BLENDER| error: {}", e)
                        }
                    }
                }
            }
            RunAction::Unreal(arg) => {
                use crate::unreal::{client::UnrealClient, launcher, python};
                progress(app, &format!("Unreal: {}", clip(arg, 80)), true);
                // .py del projecte o codi literal (com a Blender): la IA pot
                // escriure l'script dins del projecte i executar-lo després.
                let code = match (&root, arg.trim().ends_with(".py")) {
                    (Some(r), true) => {
                        let rel = arg.trim().replace('\\', "/");
                        let rel = rel.trim_start_matches("./").to_string();
                        std::fs::read_to_string(r.join(&rel)).unwrap_or_else(|_| arg.clone())
                    }
                    _ => arg.clone(),
                };
                // Guarda: mai enviem a l'editor text que no sigui Python. Abans
                // una frase en prosa arribava a l'interpretador d'Unreal i la
                // resposta HTTP no deia res d'error: la IA afirmava «fet» sense
                // que hagués canviat res a l'escena.
                if let Err(e) = crate::unreal::guard_python(&code) {
                    progress(
                        app,
                        "Unreal: el model ha enviat una descripció; l'agent d'Unreal hi posa el codi…",
                        true,
                    );
                    let desc = code.trim().to_string();
                    let task = if user_prompt.trim().is_empty() {
                        desc.clone()
                    } else {
                        format!("{}\n\n{}\n", user_prompt.trim(), desc)
                    };
                    let orch =
                        crate::agent::AgentOrchestrator::new(crate::agent::AgentConfig::default());
                    let config = crate::unreal::commands::UnrealConfig::default();
                    match orch.apply_unreal(ai, app, &task, &config).await {
                        Ok((msg, ok)) => format!(
                            "RUN|RESULTAT| UNREAL| {} {}",
                            if ok { "eixit=0 —" } else { "sense aplicar:" },
                            clip(&msg, 1800)
                        ),
                        Err(e2) => format!(
                            "RUN|RESULTAT| UNREAL| NO EXECUTADA: {} L'agent d'Unreal \
tampoc ho ha pogut aplicar ({}). Torna a emetre «RUN|UNREAL|» amb l'script \
Python REAL (comença per «import unreal» i acaba amb un print()), no amb una \
frase en llenguatge natural.",
                            e, e2
                        ),
                    }
                } else {
                    // Models locals: sovint obliden la importació (inocu si ja hi és).
                    let code = if !code.contains("import unreal") && !code.contains("from unreal")
                    {
                        format!("import unreal\n{}", code)
                    } else {
                        code
                    };
                    // UnrealClient::new() ja fa servir el port configurat
                    // (`unreal.rc_port`), no sempre el 30010 fixe.
                    let client = UnrealClient::new();
                    let mut editor = client.ping().await.unwrap_or(false);
                    if !editor {
                        // Escaneig del disc (pot trigar): en un fil a part.
                        let engines = tauri::async_runtime::spawn_blocking(
                            launcher::detect_engine_installations,
                        )
                        .await
                        .unwrap_or_default();
                        let uproject = root.as_ref().and_then(|r| find_uproject(r));
                        if engines.is_empty() {
                            return format!(
                                "RUN|RESULTAT| UNREAL| Unreal Engine no està instal·lat. \
Obre l'editor amb un projecte i activa «Window → Virtual Production → Remote \
Control» (port {}).",
                                client.port()
                            );
                        }
                        match uproject {
                            None => {
                                return format!(
                                    "RUN|RESULTAT| UNREAL| l'editor no respon al port {} i \
no hi ha cap .uproject al projecte obert per connectar-s'hi. Obre Unreal amb \
el projecte o indica'n la ruta.",
                                    client.port()
                                )
                            }
                            Some(proj) => {
                                progress(app, "Unreal: arrencant l'editor amb el projecte…", true);
                                match launcher::auto_connect(&proj, 90, None).await {
                                    Ok(_) => editor = client.ping().await.unwrap_or(false),
                                    Err(e) => {
                                        return format!(
                                            "RUN|RESULTAT| UNREAL| no s'ha pogut connectar: {}",
                                            e
                                        )
                                    }
                                }
                            }
                        }
                    }
                    if !editor {
                        return "RUN|RESULTAT| UNREAL| he arrencat l'editor però Remote \
Control no respon encara; torna a provar-ho en uns segons."
                            .to_string();
                    }
                    // Execució amb captura REAL de sortida i traceback (fitxer
                    // temporal compartit): la resposta HTTP d'Unreal no en porta
                    // mai, així que sense això l'agent no sap què ha passat.
                    match python::run_script(&client, &code).await {
                        Ok(res) if res.success => {
                            progress(app, "Unreal: canvis aplicats", true);
                            let out = res.output.join("\n");
                            let mut s = String::from("RUN|RESULTAT| UNREAL| eixit=0 — sortida de l'editor:\n");
                            if out.trim().is_empty() {
                                s.push_str(" (cap sortida: l'script no ha imprès res; \
                          afegeix-hi un print() per verificar el que ha fet)");
                            } else {
                                s.push_str(&clip(out.trim(), 2000));
                            }
                            s
                        }
                        Ok(res) => {
                            progress(app, "Unreal: l'script ha fallat", false);
                            let mut s = String::from("RUN|RESULTAT| UNREAL| error Python:\n");
                            s.push_str(&clip(&res.errors.join("\n"), 1800));
                            let out = res.output.join("\n");
                            if !out.trim().is_empty() {
                                s.push_str("\nSORTIDA ABANS DE L'ERROR:\n");
                                s.push_str(&clip(out.trim(), 600));
                            }
                            s.push_str(
                                "\nCorrigeix l'script i torna a emetre «RUN|UNREAL|»; mai \
declaris èxit sense haver vist la sortida.",
                            );
                            s
                        }
                        Err(e) => {
                            // `e` és un `anyhow::Error` (no un `String`): no es
                            // converteix sol a `&str`, cal materialitzar-lo.
                            progress(
                                app,
                                &format!("Unreal: {}", clip(&e.to_string(), 70)),
                                false,
                            );
                            format!("RUN|RESULTAT| UNREAL| error: {}", e)
                        }
                    }
                }
            }
            RunAction::Image(prompt, path) => {
                progress(app, &format!("Generant imatge: {}", clip(prompt, 80)), true);
                let mut opts = crate::imgen::GenOpts::default();
                opts.prompt = prompt.clone();
                match crate::imgen::generate(ai.remote_manager(), &opts).await {
                    Ok(img) => {
                        // Si el projecte està obert i la IA ha demanat una ruta
                        // relativa, la còpia fica la imatge DINS del projecte.
                        let mut dest = img.path.clone();
                        match (safe_rel(path), &root) {
                            (Some(rel), Some(r)) => {
                                let abs = r.join(&rel);
                                if let Some(par) = abs.parent() {
                                    let _ = std::fs::create_dir_all(par);
                                }
                                match std::fs::copy(&img.path, &abs) {
                                    Ok(_) => dest = abs.to_string_lossy().to_string(),
                                    Err(e) => {
                                        dest = format!(
                                            "{} (avís: no s'ha pogut copiar al \
                                             projecte a «{}»: {})",
                                            img.path, rel, e
                                        )
                                    }
                                }
                            }
                            _ => {}
                        }
                        progress(app, &format!("Imatge desada: {}", clip(&dest, 90)), true);
                        // Notifiqua la UI la ruta DEFINITIVA perquè la imatge
                        // es mostre al xat EN EIXE INSTANT, sense esperar que
                        // el model acabe la resposta i copie la ruta.
                        let _ = app.emit(
                            "runtools://image",
                            json!({
                                "path": dest,
                                "session": crate::ai::current_session(),
                            }),
                        );
                        format!(
                            "RUN|RESULTAT| IMG| imatge generada amb «{}» — RUTA: \
                             {} — copia la RUTA en la resposta final.",
                            img.backend, dest
                        )
                    }
                    Err(e) => format!(
                        "RUN|RESULTAT| IMG| error: {} (l'usuari pot activar \
                         ComfyUI local o un proveïdor d'imatges amb token a \
                         Configuració ▸ Proveïdors)",
                        e
                    ),
                }
            }
        };
        report.push_str(&line);
        report.push('\n');
    }
    report
}

/// Estat de les eines del xat EN ESTE MOMENT, afegit al manual del prompt de
/// sistema perquè el model sàpia què pot fer de veritat: si Blender té
/// l'editor connectat (execució en viu) o només cap·cer (headless), si hi ha
/// Unreal, si existeix un projecte obert i si la generació d'imatges en local
/// és possible. En Mac Intel (x86_64) ComfyUI local NO és compatible i ho diu
/// perquè el model derive la generació a un proveïdor en línia.
/// `computer_enabled`: si l'usuari ha donat permís de control a l'ordinador
/// (panell «Ordinador»). Sense ell, les RUN| seran rebutjades: el model ho
/// digues a l'usuari i li demana que l'active, en lloc de provar-ho a cegues.
pub async fn tools_status_line(computer_enabled: bool) -> String {
    let root = crate::commands::workspace::current_root();
    let mut l = String::from("\n\n— ESTAT DE LES EINES (ara mateix) —\n");
    let proj = match &root {
        Some(r) => format!("• Projecte obert: {} (les comandes RUN| s'executen ací)\n", r.to_string_lossy()),
        None => "• Projecte obert: NESSÚ. RUN| no funcionarà fins que l'usuari àbria una carpeta.\n".to_string(),
    };
    l.push_str(&proj);

    let bst = tokio::task::spawn_blocking(crate::blender::status)
        .await
        .unwrap_or_else(|_| crate::blender::BlenderStatus { installed: false, running: false, binary: None });
    l.push_str(if bst.running {
        "• Blender: INSTAL·LAT i AMB EDITOR CONNECTAT (port 9876). «RUN|BLENDER|codi» s'executa EN VIU dins l'escena oberta.\n"
    } else if bst.installed {
        "• Blender: instal·lat però SENSE EDITOR CONNECTAT. «RUN|BLENDER|codi» s'executa en mode sense interfície (cal que l'usuari àbria Blender si vol veure els resultats).\n"
    } else {
        "• Blender: no detectat al sistema.\n"
    });

    let uc = crate::unreal::UnrealClient::new();
    let rc_open = uc.ping().await.unwrap_or(false);
    let has_engine = tokio::task::spawn_blocking(crate::unreal::launcher::detect_engine_installations)
        .await
        .map(|v| !v.is_empty())
        .unwrap_or(false);
    l.push_str(if rc_open {
        "• Unreal Engine: editor connectat (Remote Control 30010). «RUN|UNREAL|codi python» s'executa dins l'editor.\n"
    } else if has_engine {
        "• Unreal Engine: motor instal·lat però editor TANCAT. «RUN|UNREAL|…» intentarà obrir-lo si hi ha un .uproject al projecte.\n"
    } else {
        "• Unreal Engine: no instal·lat.\n"
    });

    let intel_mac = cfg!(all(target_os = "macos", target_arch = "x86_64"));
    let comfy = crate::ai::comfyui_running();
    let sd_status = crate::imgen::sd_status_text();
    let img_line: String = if intel_mac {
        match &sd_status {
            Some(desc) if desc.starts_with("model") => format!(
                "• Imatges: rodet de CPU «sd» LLEST ({}); «IMG|» genera en local \
                 (més lent: treballa amb el processador).\n",
                desc
            ),
            Some(_) => "• Imatges: rodet de CPU «sd» instal·lat però SENSE MODEL: \
                        demana a l'usuari que el baixe al Gestor de models \
                        (secció «Rodet d'imatges en CPU») abans de fer «IMG|».\n"
                .to_string(),
            None => "• Imatges: aquest Mac Intel no pot usar ComfyUI. «IMG|» cal \
                     el rodet de CPU (botó «Instal·la», secció «Rodet d'imatges \
                     en CPU» del Gestor de models) o un proveïdor en línia amb \
                     token (Configuració ▸ Proveïdors ▸ model d'imatges).\n"
                .to_string(),
        }
    } else if comfy {
        "• Imatges: ComfyUI local en marxa (port 8188); «IMG|» funcionarà en local.\n".into()
    } else {
        "• Imatges: sense ComfyUI local en marxa; «IMG|» requerix un proveïdor en línia amb token o arrencar ComfyUI.\n".into()
    };
    l.push_str(&img_line);

    l.push_str(if computer_enabled {
        "• Control de l'ordinador: PERMÉS per l'usuari. Les RUN| s'executen (pot \n      aparéixer un diàleg de confirmació en pantalla: és normal, l'usuari l'ha \n      d'acceptar o rebutjar).\n"
    } else {
        "• Control de l'ordinador: DESACTIVAT. Les RUN| seran rebutjades. Demana-ho\n      a l'usuari ara mateix amb un missatge clar: que obria el panell «Ordinador» de\n      la dreta (icona d'escut) i active «Permet que la IA controli l'ordinador».\n"
    });
    // Muapi.ai / Open Higgsfield: imatges via l'endpoint OpenAI-compatible.
    // Accepta MUAPI_API_KEY, HIGGSFIELD_API_KEY o OPEN_HIGGSFIELD_READ_WRITE_TOKEN.
    let muapi_ok = std::env::var("MUAPI_API_KEY").map(|v| !v.trim().is_empty()).unwrap_or(false)
        || std::env::var("HIGGSFIELD_API_KEY").map(|v| !v.trim().is_empty()).unwrap_or(false)
        || std::env::var("OPEN_HIGGSFIELD_READ_WRITE_TOKEN").map(|v| !v.trim().is_empty()).unwrap_or(false);
    if muapi_ok {
        let base = std::env::var("HF_API_BASE_URL").unwrap_or_else(|_| "https://api.muapi.ai".to_string());
        l.push_str(&format!("• Imatges/Vídeo: Muapi.ai / Open Higgsfield / Open Generative AI configurat ({}); \
  «IMG|» genera imatges i vídeos amb 200+ models (Flux, SDXL, Midjourney, Kling, …)\n", base));
    }

    l.push_str("• Per obrir una app des del xat, usa la comanda REAL (p. ex. «RUN|open -a Blender»), no llenguatge natural.\n");
    l
}

#[cfg(test)]
mod run_tests {
    use super::*;

    fn blender_code(text: &str) -> String {
        let acts = parse(text);
        let b = acts
            .iter()
            .find_map(|a| match a {
                RunAction::Blender(c) => Some(c.clone()),
                _ => None,
            })
            .unwrap_or_else(|| panic!("no hi ha cap directriu BLENDER: {:?}", acts));
        b
    }

    #[test]
    fn blender_directive_one_line() {
        let code = blender_code("RUN|BLENDER|import bpy; bpy.ops.mesh.primitive_cube_add()");
        assert!(code.contains("primitive_cube_add"));
    }

    #[test]
    fn blender_directive_multi_line() {
        // El cas que feia fallar tot: una figura complexa NO cap en una línia.
        let text = "Ara ho construeixo:\n\
RUN|BLENDER|import bpy\n\
bpy.ops.mesh.primitive_uv_sphere_add(radius=0.3, location=(0, 0, 1.4))\n\
bpy.context.active_object.name = \"Nan_Cap\"\n\
print(\"Nan de jardí creat\")\n\
Ja està fet.";
        let code = blender_code(text);
        assert!(code.starts_with("import bpy"), "codi: {}", code);
        assert!(code.contains("Nan_Cap"), "codi: {}", code);
        assert!(code.contains("print(\"Nan de jardí creat\")"), "codi: {}", code);
        // La prosa del model NO entra al codi.
        assert!(!code.contains("Ja està fet"), "codi: {}", code);
        // I el que hem capturat és, de veritat, codi Python.
        assert!(crate::blender::guard_python(&code).is_ok(), "codi: {}", code);
    }

    #[test]
    fn blender_directive_fenced_block() {
        let text = "RUN|BLENDER|\n\
```python\n\
import bpy\n\
print('hola')\n\
```\n\
Fi.";
        let code = blender_code(text);
        assert!(code.contains("import bpy"), "codi: {}", code);
        assert!(code.contains("print('hola')"), "codi: {}", code);
        assert!(!code.contains("```"), "codi: {}", code);
    }

    #[test]
    fn blender_directive_stops_at_the_next_directive() {
        let text = "RUN|BLENDER|import bpy\n\
print('x')\n\
RUN|ls -la";
        let acts = parse(text);
        let b = acts
            .iter()
            .find_map(|a| match a {
                RunAction::Blender(c) => Some(c.clone()),
                _ => None,
            })
            .expect("hi ha d'haver una directriu BLENDER");
        assert!(b.contains("print('x')"));
        assert!(!b.contains("ls -la"));
        assert!(acts
            .iter()
            .any(|a| matches!(a, RunAction::Foreground(c) if c == "ls -la")));
    }

    #[test]
    fn a_description_is_not_code() {
        // El model ha respost amb una descripció: la guarda l'ha de rebutjar
        // (i aleshores NoOrbit la passa a l'agent 3D).
        let code = blender_code("RUN|BLENDER|crea un nomo de jardi ,");
        assert!(crate::blender::guard_python(&code).is_err(), "codi: {}", code);
    }
}

#[cfg(test)]
mod edit_tests {
    use super::*;

    #[test]
    fn parses_edit_blocks_and_deletions() {
        let text = "Vaig a modificar-ho:\n\
@edit: src/app.py\n\
<<<<<<< CERCA\n\
old_line\n\
=======\n\
new_line\n\
>>>>>>>\n\
I ara esborro el codi trencat:\n\
@edit: src/app.py\n\
<<<<<<< CERCA\n\
del_me()\n\
=======\n\
>>>>>>>\n";
        let blocks = extract_edit_blocks(text);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].path, "src/app.py");
        assert_eq!(blocks[0].edits.len(), 2);
        assert_eq!(blocks[0].edits[0].0, "old_line");
        assert_eq!(blocks[0].edits[0].1, "new_line");
        // NOU buit = esborrar: el replacement ha d'arribar buit.
        assert_eq!(blocks[0].edits[1].0, "del_me()");
        assert_eq!(blocks[0].edits[1].1, "");
    }

    #[test]
    fn noop_editions_are_not_applied() {
        // Error clàssic dels models: emetre CERCA i NOU IDÈNTICS (p. ex.
        // «afegeix una dependència» i el NOU repeteix el CERCA sencer).
        // No ha de comptar com a edició aplicada ni tocar el disc.
        let dir = std::env::temp_dir().join(format!("noorbit-noop-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("requirements.txt"), "flask==2.0.1\nrequests==2.25.1\n").unwrap();
        let text = "@edit: requirements.txt\n\
<<<<<<< CERCA\n\
flask==2.0.1\n\
requests==2.25.1\n\
=======\n\
flask==2.0.1\n\
requests==2.25.1\n\
>>>>>>>\n";
        let report = apply_edit_blocks_at(&dir, text);
        assert_eq!(report.len(), 1);
        assert!(report[0].contains("IDÈNTIQUES"), "informe: {}", report[0]);
        // El fitxer queda intacte.
        assert_eq!(
            std::fs::read_to_string(dir.join("requirements.txt")).unwrap(),
            "flask==2.0.1\nrequests==2.25.1\n"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn cerca_fallida_retorna_el_fragment_real_mes_semblant() {
        // El CERCA no coincideix (una línia canviada): l'error ha de portar
        // el fragment REAL amb números de línia, no només «text EXACTE».
        let dir = std::env::temp_dir().join(format!("noorbit-fuzzy-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("fitxer.php"), "<?php\n$a = 1;\n$b = 2;\n").unwrap();
        let text = "@edit: fitxer.php\n<<<<<<< CERCA\n$a = 1;\n$b = 3;\n=======\n$a = 9;\n$b = 3;\n>>>>>>>\n";
        let report = apply_edit_blocks_at(&dir, text);
        assert_eq!(report.len(), 1);
        assert!(report[0].contains("Fragment real més semblant"), "informe: {}", report[0]);
        assert!(report[0].contains("línies 2-3"), "informe: {}", report[0]);
        assert!(report[0].contains("3 | $b = 2;"), "informe: {}", report[0]);
        // El fitxer NO s'ha tocat (cap secció aplicada).
        assert_eq!(
            std::fs::read_to_string(dir.join("fitxer.php")).unwrap(),
            "<?php\n$a = 1;\n$b = 2;\n"
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}

#[cfg(test)]
mod verify_tests {
    use super::*;

    #[test]
    fn fuzzy_snippet_troba_la_finestra_correcta() {
        let content = "<?php\nfunction suma($a, $b) {\n    return $a + $b;\n}\nfunction resta($a, $b) {\n    return $a - $b;\n}\n";
        // CERCA gairebé idèntic al bloc de «resta» però amb una línia errònia.
        let find = "function resta($a, $b) {\n    return $a + $b;\n}";
        let snip = closest_snippet(content, find).expect("hi ha d'haver fragment");
        assert!(snip.contains("línies 5-7"), "fragment: {}", snip);
        assert!(snip.contains("5 | function resta($a, $b) {"), "fragment: {}", snip);
        assert!(snip.contains("return $a - $b;"), "fragment: {}", snip);
        assert!(!snip.contains("function suma"), "fragment: {}", snip);
    }

    #[test]
    fn fuzzy_snippet_sense_cap_coincidencia() {
        assert!(closest_snippet("alpha\nbeta\n", "gamma\ndelta").is_none());
        assert!(closest_snippet("", "gamma").is_none());
        assert!(closest_snippet("alpha\n", "").is_none());
    }

    #[test]
    fn head_tail_conserva_principi_i_cua() {
        let s = format!("INICI-{}-ERROR_FINAL", "x".repeat(5000));
        let c = clip_head_tail(&s, 1000, 1500);
        assert!(c.starts_with("INICI-"), "capçalera perduda");
        assert!(c.ends_with("ERROR_FINAL"), "cua perduda");
        assert!(c.contains("retallat"));
        assert!(c.chars().count() < s.chars().count());
        // Text curt: queda intacte.
        assert_eq!(clip_head_tail("curt", 1000, 1500), "curt");
    }

    #[test]
    fn lint_php_valid_invalid_i_contencio() {
        if php_binary().is_none() {
            eprintln!("php no disponible en aquesta màquina: test omès");
            return;
        }
        let dir = std::env::temp_dir().join(format!("noorbit-lint-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("bo.php"), "<?php echo 1;\n").unwrap();
        std::fs::write(dir.join("dolent.php"), "<?php function( {\n").unwrap();
        std::fs::write(dir.join("notes.txt"), "text pla").unwrap();
        let paths: Vec<String> = vec![
            "bo.php".into(),
            "dolent.php".into(),
            "notes.txt".into(),
            "../fora.php".into(),
        ];
        let res = lint_paths_blocking(&dir, &paths);
        assert_eq!(res.len(), 4);
        assert!(res[0].ok && res[0].checked, "PHP vàlid ha de passar: {:?}", res[0]);
        assert!(!res[1].ok && res[1].checked, "PHP invàlid ha de fallar: {:?}", res[1]);
        assert!(res[1].output.to_lowercase().contains("error"), "sortida: {}", res[1].output);
        // Altres extensions: ok:true i output buit (contracte lint_paths).
        assert!(res[2].ok && res[2].output.is_empty() && !res[2].checked);
        // Contenció: ruta fora del projecte rebutjada.
        assert!(!res[3].ok && res[3].output.contains("rebutjada"), "{:?}", res[3]);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn limit_de_reintents_de_verificacio() {
        let errs = vec![LintResult {
            path: "a.php".into(),
            ok: false,
            output: "PHP Parse error: syntax error".into(),
            checked: true,
        }];
        let r1 = verification_report(&errs, 1).unwrap();
        assert!(r1.contains("Verificació: 1"), "informe: {}", r1);
        assert!(r1.contains("corregeix-los"), "informe: {}", r1);
        assert!(!r1.contains("límit"), "informe: {}", r1);
        // A la tercera ronda amb errors: s'avisa del límit de 3 reintents.
        let r3 = verification_report(&errs, MAX_VERIFY_RETRIES).unwrap();
        assert!(r3.contains("límit de 3 reintents"), "informe: {}", r3);
        // Sense fitxers comprovats ni notes: no hi ha res a dir.
        assert!(verification_report(&[], 0).is_none());
        // Tot correcte: missatge positiu (mai «èxit» mut amb lint fallant).
        let oks = vec![LintResult { path: "a.php".into(), ok: true, output: String::new(), checked: true }];
        assert!(verification_report(&oks, 0).unwrap().contains("cap error"));
    }
}

#[cfg(test)]
mod path_tests {
    use super::*;

    /// Arrel inventada en un volum extern: el cas real que reproduïa el
    /// transcript degenerat (rutes absolutes reemetudes torn rere torn).
    const CWD: &str = "/Volumes/Toshi/programes/forestin the sky";

    fn tmp_arrel(nom: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("noorbit-reltests-{nom}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn write_rel_respecta_la_ruta_relativa() {
        let root = std::path::Path::new(CWD);
        // La forma correcta del protocol MAI s'ha de tocar.
        assert_eq!(write_rel(root, "src/main.py").as_deref(), Some("src/main.py"));
        assert_eq!(write_rel(root, "main.py").as_deref(), Some("main.py"));
        assert_eq!(write_rel(root, "src\\app.py").as_deref(), Some("src/app.py"));
        // Escapament i ruta bruta: rebutjades (no se'ls normalitza cap «..»).
        assert_eq!(write_rel(root, "../etc/passwd"), None);
        assert_eq!(write_rel(root, ""), None);
    }

    #[test]
    fn write_rel_relativitza_l_absoluta_sota_l_arrel() {
        let root = std::path::Path::new(CWD);
        // L'absoluta que el model copia del seu system prompt («Projecte
        // (cwd)») apunta a un fitxer del projecte: ha de quedar relativa.
        assert_eq!(
            write_rel(root, "/Volumes/Toshi/programes/forestin the sky/main.py").as_deref(),
            Some("main.py")
        );
        assert_eq!(
            write_rel(root, "/Volumes/Toshi/programes/forestin the sky/src/config.py").as_deref(),
            Some("src/config.py")
        );
        // El mateix cwd escrit SENSE barra inicial: mateixa resolució (abans
        // creava «forestin the sky/Volumes/Toshi/…» dins del projecte).
        assert_eq!(
            write_rel(root, "Volumes/Toshi/programes/forestin the sky/sky/main.py").as_deref(),
            Some("sky/main.py")
        );
        // L'arrel repetida DINS de la ruta (la duplicació real del transcript).
        assert_eq!(
            write_rel(
                root,
                "/Volumes/Toshi/programes/forestin the sky/Volumes/Toshi/programes/forestin the sky/main.py"
            )
            .as_deref(),
            Some("main.py")
        );
        // Tres vegades: es retalla tantes com aparega al capdamunt.
        assert_eq!(
            write_rel(root, &format!("{CWD}{CWD}{CWD}/sky/main.py")).as_deref(),
            Some("sky/main.py")
        );
    }

    #[test]
    fn write_rel_rebutja_l_absoluta_fora_de_l_arrel() {
        let root = std::path::Path::new(CWD);
        assert_eq!(write_rel(root, "/Users/altra/cosa/main.py"), None);
        assert_eq!(write_rel(root, "/etc/passwd"), None);
        // Un prefix comú NO la fa del projecte: no se'n retalla res.
        assert_eq!(write_rel(root, "/Volumes/Toshi/programes/altre joc/main.py"), None);
        // L'arrel mateixa no és cap fitxer.
        assert_eq!(write_rel(root, CWD), None);
    }

    #[test]
    fn write_rel_funciona_amb_una_arrel_real() {
        let root = tmp_arrel("real");
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/a.py"), "print(1)\n").unwrap();
        let abs = root.join("src/a.py").to_string_lossy().to_string();
        assert_eq!(write_rel(&root, &abs).as_deref(), Some("src/a.py"));
        assert_eq!(lint_rel(&root, &abs).as_deref(), Some("src/a.py"));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn fitxers_fora_del_projecte_es_deuen_poder_reportar() {
        // Regressió paral·lela a la de les «@edit:»: un «@file:» amb ruta
        // absoluta de fora NO s'escriu, però la ruta bruta ha de tornar al
        // model com a informe (abans era un `continue` mut).
        let root = tmp_arrel("filefora");
        let text = "@file: /noorbit-fora/main.py\n```python\nprint(1)\n```\n@file: src/be.py\n```python\nprint(2)\n```";
        let written = write_announced_files_at(&root, text);
        assert_eq!(written, vec!["src/be.py".to_string()]);
        assert_eq!(rejected_write_paths(&root, text), vec!["/noorbit-fora/main.py".to_string()]);
        // La que reprodueix l'arrel NO és cap rebutja: s'escriu relativitzada.
        let arrel = std::path::Path::new(CWD);
        let text2 = "@file: /Volumes/Toshi/programes/forestin the sky/main.py\n```python\nprint(1)\n```";
        assert!(rejected_write_paths(arrel, text2).is_empty());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn run_enganxats_se_separen_en_tantes_comandes() {
        // El cas real: «pip install» no s'executava mai perquè les dues
        // directrius enganxades eren UNA sola comanda innombrable.
        let acts = parse("RUN|pip install -r requirements.txtRUN|python3 main.py");
        assert_eq!(
            acts,
            vec![
                RunAction::Foreground("pip install -r requirements.txt".into()),
                RunAction::Foreground("python3 main.py".into()),
            ],
            "{:?}",
            acts
        );
        // Dins d'una cometa, «run|» és text: NO es talla.
        let acts = parse(r"RUN|grep -n 'pip install\|run|' fitxer.txt");
        assert_eq!(acts.len(), 1, "{:?}", acts);
        // Rutes amb espais dins d'una sola comanda: es conserven senceres.
        let acts = parse("RUN|python3 'forestin the sky/main.py'");
        assert_eq!(
            acts,
            vec![RunAction::Foreground("python3 'forestin the sky/main.py'".into())]
        );
    }

    #[test]
    fn segell_d_accions_detecta_la_reemissio() {
        // Prosa pura: cap segell (el guard anti-bucle no s'ha d'aplicar mai).
        assert_eq!(action_fingerprint("Et explico què faria, res més."), "");
        // Dues respostes amb paraules DISTINTES i el MESMEIX pla: idèntic.
        let a = action_fingerprint("Ho arreglo:\n@edit: a.py\n<<<<<<< CERCA\novell\n=======\novell\n>>>>>>>\nRUN|python3 a.py");
        let b = action_fingerprint("Perdó, hi torno:\n@edit: a.py\n<<<<<<< CERCA\novell\n=======\novell\n>>>>>>>\nRUN|python3 a.py");
        assert!(!a.is_empty());
        assert_eq!(a, b);
        // El pla canvia (NOU de debò): el segell també.
        let c = action_fingerprint("Ara sí que ho he arreglat:\n@edit: a.py\n<<<<<<< CERCA\novell\n=======\nnou\n>>>>>>>\nRUN|python3 a.py");
        assert_ne!(a, c);
    }

    #[test]
    fn editar_fora_del_projecte_es_reporta_i_no_de_mut() {
        // Regressió del `continue` mut: sense informe, el model no sabia que la
        // seva ruta era invàlida i la reemetia EXACTAMENT igual cada torn.
        let root = tmp_arrel("editfora");
        let text = "@edit: /noorbit-fora/main.py\n<<<<<<< CERCA\novell\n=======\nnou\n>>>>>>>";
        let rep = apply_edit_blocks_at(&root, text);
        assert_eq!(rep.len(), 1, "informe: {rep:?}");
        assert!(rep[0].contains("/noorbit-fora/main.py"), "informe: {rep:?}");
        assert!(rep[0].contains("FORA del projecte obert"), "informe: {rep:?}");
        std::fs::remove_dir_all(&root).ok();
    }
}
