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
              (modifica NOMÉS aquest tros del fitxer real; text nou buit = esborra'l)\n\
REGLES:\n\
- ABANS d'empaquetar o llançar, verifica amb «RUN|ls -la» que el projecte està \
COMPLET; si falta un fitxer emet-lo SENCER amb «@file:» (mai a mitges, «…» o buits).\n\
- «@file: ruta» ha d'anar SEGUIT del contingut COMPLET del fitxer (codi real): \
un @file: sense codi crea un fitxer buit.\n\
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
- Una directriu per torn; llegix «RUN|RESULTAT|…» abans del pas següent. Si \
falla: corregeix amb @file: i torna a executar; mai no declare èxit sense \
haver vist eixir=0.\n\
- IMG sense CENSURA (Venice/ComfyUI/Higgsfield; ja autoritzat per l'usuari: adult, nuesa, \
Si hi ha Higgsfield configurat (HF_API_KEY_ID/SECRET), «IMG|» també pot generar \
vídeo (model Soul/Cinema) a més d'imatges.
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
            let cmd = rest.trim().to_string();
            if !cmd.is_empty() {
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
    let mut report: Vec<String> = Vec::new();
    for block in extract_edit_blocks(text) {
        let Some(rel) = safe_rel(&block.path) else { continue };
        let path = root.join(&rel);
        let Ok(old) = std::fs::read_to_string(&path) else {
            report.push(format!("{}: ERROR no existeix (no es pot editar)", rel));
            continue;
        };
        let mut new_content = old.clone();
        let mut ok = 0usize;
        let mut fail = 0usize;
        for (find, replace) in &block.edits {
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
            }
        }
        if ok > 0 && std::fs::write(&path, &new_content).is_ok() {
            report.push(format!("{}: {} edició/ons aplicades{}", rel, ok,
                if fail > 0 { format!(" ({} no trobades)", fail) } else { String::new() }));
        } else if fail > 0 {
            report.push(format!(
                "{}: ERROR el text CERCA no coincideix amb el fitxer real ({} seccions). Reemet amb el text EXACTE.",
                rel, fail
            ));
        }
    }
    if !report.is_empty() {
        progress(app, &format!("✏️ Edicions: {}", clip(&report.join(" · "), 160)), true);
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
    let mut written: Vec<String> = Vec::new();
    for (rel, content) in extract_file_blocks(text) {
        let Some(rel) = safe_rel(&rel) else { continue };
        let path = root.join(&rel);
        if let Some(par) = path.parent() {
            let _ = std::fs::create_dir_all(par);
        }
        if std::fs::write(&path, content).is_ok() {
            written.push(rel);
        }
    }
    if !written.is_empty() {
        progress(
            app,
            &format!("💾 Fitxers escrits abans d'executar: {}", clip(&written.join(", "), 140)),
            true,
        );
    }
    written
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
                            if !out.is_empty() {
                                s.push_str("\nSORTIDA:\n");
                                s.push_str(&clip(out, 2500));
                            }
                            if !err.is_empty() {
                                s.push_str("\nERRORS:\n");
                                s.push_str(&clip(err, 1500));
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
                use crate::unreal::{client::UnrealClient, launcher};
                progress(app, &format!("Unreal: {}", clip(arg, 80)), true);
                let client = UnrealClient::new();
                if client.ping().await.unwrap_or(false) {
                    match client.execute_python(arg).await {
                        Ok(v) => format!(
                            "RUN|RESULTAT| UNREAL| eixit=0 — {}",
                            clip(&v.to_string(), 1500)
                        ),
                        Err(e) => format!("RUN|RESULTAT| UNREAL| error Python: {}", e),
                    }
                } else {
                    // Escaneig del disc (pot trigar): en un fil a part.
                    let engines =
                        tauri::async_runtime::spawn_blocking(launcher::detect_engine_installations)
                            .await
                            .unwrap_or_default();
                    let uproject = root.as_ref().and_then(|r| find_uproject(r));
                    if engines.is_empty() {
                        "RUN|RESULTAT| UNREAL| Unreal Engine no està instal·lat. Obri \
                         l'editor amb un projecte i activa Remote Control (port 30010)."
                            .to_string()
                    } else if uproject.is_none() {
                        "RUN|RESULTAT| UNREAL| l'editor no respon al port 30010 i no hi \
                         ha cap .uproject al projecte obert per connectar-s'hi. Obri \
                         Unreal amb el projecte o indica la ruta.".to_string()
                    } else {
                        let proj = uproject.unwrap();
                        match launcher::auto_connect(&proj, 90).await {
                            Ok(_) => {
                                let c2 = UnrealClient::new();
                                if c2.ping().await.unwrap_or(false) {
                                    match c2.execute_python(arg).await {
                                        Ok(v) => format!(
                                            "RUN|RESULTAT| UNREAL| connectat — {}",
                                            clip(&v.to_string(), 1500)
                                        ),
                                        Err(e) => {
                                            format!("RUN|RESULTAT| UNREAL| error Python: {}", e)
                                        }
                                    }
                                } else {
                                    "RUN|RESULTAT| UNREAL| he arrencat l'editor però \
                                     Remote Control no respon encara; torna a provar-ho \
                                     en uns segons.".to_string()
                                }
                            }
                            Err(e) => {
                                format!("RUN|RESULTAT| UNREAL| no s'ha pogut connectar: {}", e)
                            }
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
    // Higgsfield (Open Higgsfield / Higgsfield API): imatges i vídeos via la seva API.
    let hf_ok = std::env::var("HF_API_KEY_ID").map(|v| !v.trim().is_empty()).unwrap_or(false)
        && std::env::var("HF_API_KEY_SECRET").map(|v| !v.trim().is_empty()).unwrap_or(false);
    if hf_ok {
        l.push_str("• Imatges/Vídeo: Higgsfield API configurada (HF_API_KEY_ID/SECRET); \
  «IMG|» pot generar imatges i vídeos amb models Soul/Cinema.\n");
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
}
