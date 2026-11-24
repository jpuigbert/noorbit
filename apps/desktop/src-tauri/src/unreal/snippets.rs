//! Snippets i patrons d'Unreal Engine 5 per a la IA local de NoOrbit.
//!
//! Tres peces que treballen juntes:
//!
//! 1. [`PATRONS_PYTHON`]: patrons de referència amb l'API REAL d'Editor
//!    Scripting (UE 5). S'injecten al system prompt de l'agent d'Unreal
//!    (`agent/mod.rs::apply_unreal`). Sense aixos, els models locals
//!    escriuen `EditorLevelLibrary` i `EditorAssetLibrary` (deprecades des
//!    d'UE 5.0/5.1) que, executades pel pont Remote Control, NO fan res:
//!    l'usuari veu «✅ canvis aplicats» i l'escena no ha canviat.
//! 2. [`cerca_epic`]: consulta els snippets de la comunitat d'Epic Games
//!    (GraphQL) i els normalitza al mateix format.
//! 3. [`cataleg_local`]: catàleg OFFLINE equivalent, perquè l'agent mai
//!    quedi sense referència (l'API d'Epic pot caure, estar darrere d'un
//!    proxy o respondre 403).
//!
//! Tot el que surt d'aquest mòdul és TEXT per a prompts: mai s'executa res
//! directament a l'editor. L'execució continua fent-la l'agent a través
//! de `unreal/python.rs` (amb el wrapper que captura stdout i traceback).

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Temps màxim d'espera de la consulta a la comunitat d'Epic: un servei
/// extern mai no ha de bloquejar l'agent.
const TIMEOUT: Duration = Duration::from_secs(8);
/// Nombre màxim de snippets que s'injecten al prompt (el model local no
/// aprofita de més i el context és el recurs escàs).
pub const MAX_SNIPPETS: usize = 6;

/// Agent de navegador: el servei d'Epic rebutja els clients sense UA (403).
const UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) \
                  AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36";

/// Endpoint GraphQL de la comunitat d'Epic Games.
const EPIC_GRAPHQL: &str = "https://dev.epicgames.com/community/api/graphql";

/// Segons durant els quals, després d'un fracàs, no es torna a preguntar a
/// Epic. Sense això, cada torn de l'agent d'Unreal pagaria els 8 s d'espera
/// quan l'usuari no té xarxa (o el servei està caigut) i la resposta seria
/// sempre el catàleg local.
const EPIC_COOLDOWN_SEGONDS: u64 = 300;

/// Instant (segons des de l'època Unix) de l'última consulta fallida a
/// Epic. 0 = mai ha fallat.
static EPIC_FALLIDA: AtomicU64 = AtomicU64::new(0);

/// El servei d'Epic és ara mateix inaccessible (o fa poc que ho era)?
fn epic_cooling_down() -> bool {
    let darrera = EPIC_FALLIDA.load(Ordering::Relaxed);
    if darrera == 0 {
        return false;
    }
    let ara = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    ara.saturating_sub(darrera) < EPIC_COOLDOWN_SEGONDS
}

/// Enregistra un fracàs per obrir el circuit (i estalviar l'espera).
fn marca_epic_fallit() {
    let ara = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    EPIC_FALLIDA.store(ara, Ordering::Relaxed);
}

/// Patrons de referència de l'API real d'Editor Scripting d'UE5.
///
/// Text pensat per anar pegat al system prompt d'un model local petit:
/// curç, amb les signatures exactes i sense les APIs deprecades.
pub const PATRONS_PYTHON: &str = r#"PATRONS DE REFERÈNCIA (API real d'Editor Scripting UE5)

Subsistemes (mai les API de pestanya obsoleta: deprecades i fallen en
silenci pel pont Remote Control):
  actors = unreal.get_editor_subsystem(unreal.EditorActorSubsystem)
  nivell = unreal.get_editor_subsystem(unreal.LevelEditorSubsystem)
  assets = unreal.get_editor_subsystem(unreal.EditorAssetSubsystem)

Crear un actor amb malla:
  mesh = unreal.load_object(None, "/Engine/BasicShapes/Cube.Cube")
  actor = actors.spawn_actor_from_class(unreal.StaticMeshActor, unreal.Vector(0, 0, 100))
  actor.set_actor_label("Cub")
  comp = actor.get_component_by_class(unreal.StaticMeshComponent)
  comp.set_static_mesh(mesh)
  comp.set_material(0, unreal.load_object(None, "/Engine/BasicShapes/BasicShapeMaterial.BasicShapeMaterial"))

Moure, girar i escalar:
  actor.set_actor_location(unreal.Vector(x, y, z))
  actor.set_actor_rotation(unreal.Rotator(pitch, yaw, roll))
  actor.set_actor_scale3d(unreal.Vector(x, y, z))

Llums:
  sol = actors.spawn_actor_from_class(unreal.DirectionalLight, unreal.Vector(0, 0, 600))
  sol.set_actor_rotation(unreal.Rotator(-45, -30, 0))
  punt = actors.spawn_actor_from_class(unreal.PointLight, unreal.Vector(300, 0, 250))
  comp = punt.get_component_by_class(unreal.PointLightComponent)
  comp.set_intensity(3000.0)
  comp.set_attenuation_radius(800.0)
  actors.spawn_actor_from_class(unreal.SpotLight, unreal.Vector(0, 0, 400))
  actors.spawn_actor_from_class(unreal.SkyLight, unreal.Vector(0, 0, 0))

Text 3D:
  txt = actors.spawn_actor_from_class(unreal.TextRenderActor, unreal.Vector(0, 0, 200))
  tr = txt.get_component_by_class(unreal.TextRenderComponent)
  tr.set_text("Hola")
  tr.set_font(unreal.load_object(None, "/Engine/EngineFonts/RobotoDistanceField.RobotoDistanceField"))

Importar un model (FBX, GLB, OBJ):
  data = unreal.AutomatedAssetImportData()
  data.set_editor_property("filenames", ["/ruta/absoluta/model.fbx"])
  data.set_editor_property("destination_path", "/Game/Imports")
  data.set_editor_property("replace_existing", True)
  importats = list(unreal.AssetToolsHelpers.get_asset_tools().import_assets_automated(data) or [])

Blueprint:
  parent = unreal.load_class(None, "/Script/Engine.Actor")
  factory = unreal.BlueprintFactory()
  factory.set_editor_property("ParentClass", parent)
  bp = unreal.AssetToolsHelpers.get_asset_tools().create_asset("BP_Enemy", "/Game/Blueprints", None, factory)
  assets.save_loaded_asset(bp)

Material amb color:
  tools = unreal.AssetToolsHelpers.get_asset_tools()
  mat = tools.create_asset("M_Mat", "/Game/Materials", unreal.Material, unreal.MaterialFactoryNew())
  expr = unreal.MaterialEditingLibrary.create_material_expression(mat, unreal.MaterialExpressionConstant3Vector, -200, 0)
  expr.set_editor_property("constant", unreal.LinearColor(0.2, 0.5, 0.9))
  unreal.MaterialEditingLibrary.connect_material_property(expr, "", unreal.MaterialProperty.MP_BASE_COLOR)
  unreal.MaterialEditingLibrary.recompile_material(mat)

Llistar actors i actius:
  for a in actors.get_all_level_actors():
      print(a.get_actor_label())
  for ad in assets.list_assets("/Game", True, True):
      print(ad.get_asset_path().to_string())

Esborrar un actor:
  actors.destroy_actor(actor)

Il·luminació construïda i nivell desat:
  qualitat = getattr(unreal.LightingBuildQuality, "QUALITY_PRODUCTION", None)
  if qualitat is None:
      qualitat = getattr(unreal.LightingBuildQuality, "QUALITY_HIGH", None)
  if not nivell.build_light_maps(qualitat, False):
      raise RuntimeError("Unreal ha refusat construir els lightmaps")
  nivell.save_current_level()

REGLES: un sol bloc de codi Python que comenci per «import unreal»; acaba
amb print(...) del que has fet; si load_object o load_class tornen None,
aixeca RuntimeError amb el motiu; no facis servir mai APIs deprecades."#;

/// Un snippet de la comunitat d'Epic (o del catàleg local de reserva).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnippetUnreal {
    /// Identificador únic (a Epic és l'id del snippet; al local, l'índex).
    pub id: String,
    /// Títol llegible del snippet.
    pub titol: String,
    /// Autor (a Epic, el nom del membre de la comunitat).
    pub autor: String,
    /// Enllaç al snippet original (buit al catàleg local).
    pub url: String,
    /// Codi o resum del snippet.
    pub codi: String,
    /// Etiquetes (serveixen per filtrar per rellevància).
    pub tags: Vec<String>,
    /// Font del snippet: «epic» (comunitat) o «local» (reserva offline).
    pub font: String,
}

/// Catàleg local de reserva: patrons d'Editor Scripting equivalents als
/// snippets de la comunitat d'Epic. És la garantia que l'agent SEMPRE té
/// referència, passi el que passi amb la xarxa.
pub fn cataleg_local() -> Vec<SnippetUnreal> {
    let base: Vec<(&str, &str, &[&str], &str)> = vec![
        (
            "Crear un actor amb malla",
            "unreal",
            &["actor", "malla", "mesh", "cub", "spawn"],
            r#"mesh = unreal.load_object(None, "/Engine/BasicShapes/Cube.Cube")
actor = actors.spawn_actor_from_class(unreal.StaticMeshActor, unreal.Vector(0, 0, 100))
actor.set_actor_label("Cub")
comp = actor.get_component_by_class(unreal.StaticMeshComponent)
comp.set_static_mesh(mesh)"#,
        ),
        (
            "Moure, girar i escalar actors",
            "unreal",
            &["moure", "girar", "escala", "transform", "posicio"],
            r#"actor.set_actor_location(unreal.Vector(x, y, z))
actor.set_actor_rotation(unreal.Rotator(pitch, yaw, roll))
actor.set_actor_scale3d(unreal.Vector(x, y, z))"#,
        ),
        (
            "Afegir llums a l'escena",
            "unreal",
            &["llum", "llums", "sol", "puntual", "spot", "iluminacio"],
            r#"sol = actors.spawn_actor_from_class(unreal.DirectionalLight, unreal.Vector(0, 0, 600))
sol.set_actor_rotation(unreal.Rotator(-45, -30, 0))
punt = actors.spawn_actor_from_class(unreal.PointLight, unreal.Vector(300, 0, 250))
comp = punt.get_component_by_class(unreal.PointLightComponent)
comp.set_intensity(3000.0)
comp.set_attenuation_radius(800.0)"#,
        ),
        (
            "Text 3D (títol) a l'escena",
            "unreal",
            &["text", "titul", "3d", "textrender", "etiqueta"],
            r#"txt = actors.spawn_actor_from_class(unreal.TextRenderActor, unreal.Vector(0, 0, 200))
tr = txt.get_component_by_class(unreal.TextRenderComponent)
tr.set_text("Hola")
tr.set_font(unreal.load_object(None, "/Engine/EngineFonts/RobotoDistanceField.RobotoDistanceField"))"#,
        ),
        (
            "Importar un model FBX, GLB o OBJ",
            "unreal",
            &["importar", "import", "fbx", "glb", "obj", "model", "asset"],
            r#"data = unreal.AutomatedAssetImportData()
data.set_editor_property("filenames", ["/ruta/absoluta/model.fbx"])
data.set_editor_property("destination_path", "/Game/Imports")
data.set_editor_property("replace_existing", True)
importats = list(unreal.AssetToolsHelpers.get_asset_tools().import_assets_automated(data) or [])"#,
        ),
        (
            "Crear un Blueprint",
            "unreal",
            &["blueprint", "classe", "asset", "bp"],
            r#"parent = unreal.load_class(None, "/Script/Engine.Actor")
factory = unreal.BlueprintFactory()
factory.set_editor_property("ParentClass", parent)
bp = unreal.AssetToolsHelpers.get_asset_tools().create_asset("BP_Enemy", "/Game/Blueprints", None, factory)
assets.save_loaded_asset(bp)"#,
        ),
        (
            "Material amb color",
            "unreal",
            &["material", "color", "textura", "graf"],
            r#"tools = unreal.AssetToolsHelpers.get_asset_tools()
mat = tools.create_asset("M_Mat", "/Game/Materials", unreal.Material, unreal.MaterialFactoryNew())
expr = unreal.MaterialEditingLibrary.create_material_expression(mat, unreal.MaterialExpressionConstant3Vector, -200, 0)
expr.set_editor_property("constant", unreal.LinearColor(0.2, 0.5, 0.9))
unreal.MaterialEditingLibrary.connect_material_property(expr, "", unreal.MaterialProperty.MP_BASE_COLOR)
unreal.MaterialEditingLibrary.recompile_material(mat)"#,
        ),
        (
            "Construir la il·luminació (lightmaps)",
            "unreal",
            &["iluminacio", "lightmap", "render", "sombra", "nivell"],
            r#"qualitat = getattr(unreal.LightingBuildQuality, "QUALITY_PRODUCTION", None)
if qualitat is None:
    qualitat = getattr(unreal.LightingBuildQuality, "QUALITY_HIGH", None)
if not nivell.build_light_maps(qualitat, False):
    raise RuntimeError("Unreal ha refusat construir els lightmaps")
nivell.save_current_level()"#,
        ),
        (
            "Llistar els actors i els actius del projecte",
            "unreal",
            &["llistar", "actors", "assets", "actius", "escena", "nivell"],
            r#"for a in actors.get_all_level_actors():
    print(a.get_actor_label())
for ad in assets.list_assets("/Game", True, True):
    print(ad.get_asset_path().to_string())"#,
        ),
        (
            "Esborrar un actor de l'escena",
            "unreal",
            &["esborrar", "borrar", "delete", "treure", "actor"],
            r#"actors.destroy_actor(actor)"#,
        ),
    ];

    base.into_iter()
        .enumerate()
        .map(|(i, (titol, autor, tags, codi))| SnippetUnreal {
            id: format!("local-{i}"),
            titol: titol.to_string(),
            autor: autor.to_string(),
            url: String::new(),
            codi: codi.to_string(),
            tags: tags.iter().map(|t| t.to_string()).collect(),
            font: "local".to_string(),
        })
        .collect()
}

/// Snippets del catàleg local que tenen rellevància amb la consulta.
pub fn rellevants(consulta: &str, max: usize) -> Vec<SnippetUnreal> {
    let tokens = crate::learning::tokens(consulta);
    if tokens.is_empty() {
        return Vec::new();
    }
    let mut scored: Vec<(usize, SnippetUnreal)> = cataleg_local()
        .into_iter()
        .map(|s| {
            let text = format!("{} {} {}", s.titol, s.codi, s.tags.join(" "));
            let t = crate::learning::tokens(&text);
            let n = tokens.iter().filter(|w| t.contains(w)).count();
            (n, s)
        })
        .filter(|(n, _)| *n > 0)
        .collect();
    // Estabil: a igualtat es queda l'ordre del catàleg (del més genèric al
    // més específic), que ja està ordenat per utilitat.
    scored.sort_by(|a, b| b.0.cmp(&a.0));
    scored.into_iter().take(max).map(|(_, s)| s).collect()
}

/// Consulta la comunitat d'Epic Games. Si res (xarxa, 403, JSON diferent)
/// torna una llista buida: la reserva local és la que mana.
///
/// Després d'un fracàs, el circuit s'obre i les properes consultes no
/// s'esperen: l'agent torna al catàleg local de seguida.
pub async fn cerca_epic(consulta: &str) -> Vec<SnippetUnreal> {
    if epic_cooling_down() {
        return Vec::new();
    }
    let out = cerca_epic_directa(consulta).await;
    if out.is_empty() {
        marca_epic_fallit();
    }
    out
}

/// Consulta directa a Epic, sense circuit.
async fn cerca_epic_directa(consulta: &str) -> Vec<SnippetUnreal> {
    let paraules: Vec<String> = crate::learning::tokens(consulta).into_iter().take(4).collect();
    if paraules.is_empty() {
        return Vec::new();
    }
    let query = "query SearchSnippets($first: Int!, $keywords: [String!]) { \
        searchSnippets(first: $first, keywords: $keywords) { \
        nodes { id title description code url author { name } tags } } }";
    let body = serde_json::json!({
        "query": query,
        "variables": { "first": MAX_SNIPPETS, "keywords": paraules },
    });
    let client = match reqwest::Client::builder()
        .timeout(TIMEOUT)
        .user_agent(UA)
        .build()
    {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    let resp = match client.post(EPIC_GRAPHQL).json(&body).send().await {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };
    if !resp.status().is_success() {
        return Vec::new();
    }
    let json: serde_json::Value = match resp.json().await {
        Ok(j) => j,
        Err(_) => return Vec::new(),
    };
    // Si GraphQL respon amb errors, la resposta no serveix.
    if let Some(errs) = json.get("errors").and_then(|e| e.as_array()) {
        if !errs.is_empty() {
            return Vec::new();
        }
    }
    let mut out = extreu_snippets(&json);
    out.truncate(MAX_SNIPPETS);
    out
}

/// Cerca de snippets amb reserva: primer la comunitat d'Epic i, si no hi ha
/// res, el catàleg local rellevant.
pub async fn cerca(consulta: &str) -> Vec<SnippetUnreal> {
    let epic = cerca_epic(consulta).await;
    if !epic.is_empty() {
        return epic;
    }
    let mut locals = rellevants(consulta, MAX_SNIPPETS);
    if locals.is_empty() {
        locals = cataleg_local()
            .into_iter()
            .take(4)
            .collect();
    }
    locals
}

/// Text de context per injectar al prompt de l'agent d'Unreal: els patrons
/// de l'API (sempre) i els snippets trobats (si n'hi ha).
pub fn context_prompt(consulta: &str, snippets: &[SnippetUnreal]) -> String {
    let mut s = String::new();
    s.push_str(PATRONS_PYTHON);
    if !snippets.is_empty() {
        s.push_str("\n\nSNIPPETS DE LA COMUNITAT D'EPIC (adapta'ls a la tasca):\n");
        for sn in snippets.iter().take(MAX_SNIPPETS) {
            s.push_str(&format!("\n# {}\n", sn.titol));
            if !sn.autor.is_empty() {
                s.push_str(&format!("# autor: {}\n", sn.autor));
            }
            if !sn.url.is_empty() {
                s.push_str(&format!("# {}\n", sn.url));
            }
            s.push_str(sn.codi.trim());
            s.push('\n');
        }
    }
    s.push_str("\n");
    let _ = consulta;
    s
}

/// Primer valor de text no buit d'una llista de claus.
fn txt(o: &serde_json::Map<String, serde_json::Value>, keys: &[&str]) -> Option<String> {
    for k in keys {
        if let Some(v) = o.get(*k).and_then(|v| v.as_str()) {
            if !v.trim().is_empty() {
                return Some(v.trim().to_string());
            }
        }
    }
    None
}

/// Converteix un objecte JSON en snippet, si en té prou informació.
fn snippet_de(v: &serde_json::Value) -> Option<SnippetUnreal> {
    let o = v.as_object()?;
    let codi = txt(o, &["code", "content", "snippetcode", "snippet", "body"]);
    let desc = txt(o, &["description", "summary", "excerpt", "body"]);
    // Cal un títol i un cos: això descarta les llistes d'etiquetes i altres
    // objectes auxiliaris que la resposta de GraphQL també pot contenir.
    let titol = txt(o, &["title", "name", "snippettitle"]).or_else(|| desc.clone())?;
    let codi = codi.or(desc)?;
    if codi.trim().is_empty() {
        return None;
    }
    let autor = o
        .get("author")
        .and_then(|a| {
            a.as_str()
                .map(|s| s.to_string())
                .or_else(|| a.get("name").and_then(|n| n.as_str()).map(|s| s.to_string()))
        })
        .or_else(|| txt(o, &["authorname", "username"]))
        .unwrap_or_default();
    let url = txt(o, &["url", "link", "weburl", "permalink"]).unwrap_or_default();
    let id = txt(o, &["id", "snippetid"]).unwrap_or_else(|| url.clone());
    let tags = o
        .get("tags")
        .and_then(|t| t.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|x| {
                    x.as_str()
                        .map(|s| s.to_string())
                        .or_else(|| {
                            x.get("name")
                                .and_then(|n| n.as_str())
                                .map(|s| s.to_string())
                        })
                })
                .collect()
        })
        .unwrap_or_default();
    Some(SnippetUnreal {
        id,
        titol,
        autor,
        url,
        codi,
        tags,
        font: "epic".to_string(),
    })
}

/// Cerca recursiva del primer array que sembli una llista de snippets.
/// L'API d'Epic no està documentada i la forma del JSON pot canviar: en
/// lloc de fixar una ruta, es busca el primer array d'objectes amb títol i
/// cos.
fn extreu_snippets(v: &serde_json::Value) -> Vec<SnippetUnreal> {
    match v {
        serde_json::Value::Array(items) => {
            let directes: Vec<SnippetUnreal> = items.iter().filter_map(snippet_de).collect();
            if !directes.is_empty() {
                return directes;
            }
            for it in items {
                let r = extreu_snippets(it);
                if !r.is_empty() {
                    return r;
                }
            }
            Vec::new()
        }
        serde_json::Value::Object(map) => {
            // Prioritat als contenidors coneguts de la GraphQL d'Epic: així
            // un camp auxiliar (p. ex. «code» dins d'un node) no s'escaneja
            // abans que «nodes».
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort_by_key(|k| prioritat(k));
            for k in keys {
                if let Some(val) = map.get(k) {
                    let r = extreu_snippets(val);
                    if !r.is_empty() {
                        return r;
                    }
                }
            }
            Vec::new()
        }
        _ => Vec::new(),
    }
}

/// Prioritat de clau en la cerca recursiva (0 = abans).
fn prioritat(k: &str) -> u8 {
    match k.to_lowercase().as_str() {
        "nodes" | "items" | "results" | "snippets" | "edges" | "data" => 0,
        _ => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patrons_no_usen_apis_deprecades() {
        assert!(PATRONS_PYTHON.contains("unreal."));
        // Els patrons NO han de contenir ni el nom de les APIs deprecades: un
        // model local copia el que llegeix i pot acabar cridant-les (que, per
        // Remote Control, fallen en silenci). Només hi ha el subsistema actual.
        assert!(!PATRONS_PYTHON.contains("EditorLevelLibrary"));
        assert!(!PATRONS_PYTHON.contains("EditorAssetLibrary"));
        // Els patrons han de ser els que funciona la API real.
        assert!(PATRONS_PYTHON.contains("build_light_maps"));
        assert!(PATRONS_PYTHON.contains("import_assets_automated"));
    }

    #[test]
    fn circuit_obert_despres_dun_fracas() {
        // Sense fracàs anterior, el circuit és obert (es pot preguntar a Epic).
        EPIC_FALLIDA.store(0, Ordering::Relaxed);
        assert!(!epic_cooling_down());
        // Un fracàs obre el circuit: les properes consultes NO esperen.
        marca_epic_fallit();
        assert!(epic_cooling_down());
        // Un fracàs fora de la finestra el torna a tancar.
        let vell = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
            .saturating_sub(EPIC_COOLDOWN_SEGONDS + 10);
        EPIC_FALLIDA.store(vell, Ordering::Relaxed);
        assert!(!epic_cooling_down());
        // Es deixa el circuit tancat per no molestar altres proves.
        EPIC_FALLIDA.store(0, Ordering::Relaxed);
    }

    #[test]
    fn cataleg_local_tot_te_codi_util() {
        let cat = cataleg_local();
        assert!(cat.len() >= 8, "catàleg massa curt: {}", cat.len());
        for s in &cat {
            assert!(!s.codi.trim().is_empty(), "sense codi: {}", s.titol);
            assert!(!s.tags.is_empty(), "sense etiquetes: {}", s.titol);
            assert!(s.font == "local", "{}", s.titol);
            assert!(!s.codi.contains("EditorLevelLibrary"), "{}", s.titol);
            assert!(!s.codi.contains("EditorAssetLibrary"), "{}", s.titol);
        }
    }

    #[test]
    fn rellevants_filtre_per_la_consulta() {
        let llums = rellevants("afegeix una llum a l'escena d'unreal", 3);
        assert!(!llums.is_empty());
        assert!(llums.iter().any(|s| s.titol.contains("llum")), "{:?}", llums);
        // Una consulta sense relació no ha de retornar res.
        assert!(rellevants("fes una recepta de pastís", 3).is_empty());
    }

    #[test]
    fn extreu_snippets_parseja_una_resposta_graphql() {
        let json = serde_json::json!({
            "data": {
                "searchSnippets": {
                    "nodes": [
                        {
                            "id": "abc",
                            "title": "Crear un cub",
                            "code": "import unreal\nactor = actors.spawn_actor_from_class(unreal.StaticMeshActor, unreal.Vector(0,0,0))",
                            "url": "https://dev.epicgames.com/community/snippet/abc",
                            "author": { "name": "Community" },
                            "tags": ["python", "actors"]
                        },
                        { "id": "def", "title": "Buit", "code": "" }
                    ]
                }
            }
        });
        let out = extreu_snippets(&json);
        assert_eq!(out.len(), 1, "{:?}", out);
        assert_eq!(out[0].titol, "Crear un cub");
        assert_eq!(out[0].autor, "Community");
        assert_eq!(out[0].font, "epic");
        assert_eq!(out[0].tags.len(), 2);
    }

    #[test]
    fn extreu_snippets_ignora_reponses_sense_snippets() {
        let json = serde_json::json!({ "data": { "searchSnippets": { "nodes": [] } } });
        assert!(extreu_snippets(&json).is_empty());
        let errors = serde_json::json!({ "errors": [{ "message": "boom" }] });
        assert!(extreu_snippets(&errors).is_empty());
    }

    #[test]
    fn context_prompt_inclou_patrons_i_snippets() {
        let sn = cataleg_local();
        let ctx = context_prompt("llum", &sn);
        assert!(ctx.contains("build_light_maps"), "{}", ctx);
        assert!(ctx.contains("Crear un actor amb malla"), "{}", ctx);
        // Sense snippets, només els patrons.
        let ctx2 = context_prompt("llum", &[]);
        assert!(ctx2.contains("build_light_maps"), "{}", ctx2);
        assert!(!ctx2.contains("SNIPPETS DE LA COMUNITAT"), "{}", ctx2);
    }
}
