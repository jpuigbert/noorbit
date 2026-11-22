//! Adaptador mínim del protocol PROPIS del `tsserver` de TypeScript (el que
//! usa VSCode per dins). El tsserver NO parla LSP: parla JSON line-delimited
//! amb `{seq, type:"request", command, arguments}` i respon
//! `{type:"response", request_seq, success, body}` + esdeveniments
//! `{type:"event", event:"syntaxDiag"|"semanticDiag", body}`.
//!
//! Reutilitzem el `tsserver.js` del VSCode INSTAL·LAT per l'usuari (eina
//! externa, Apache-2.0 — no se'n copia cap codi, vegeu NOTICE) o, si no hi és,
//! el `tsserver` del PATH o el typescript del projecte (`node_modules`).
//! Node es detecta al PATH amb els fallbacks habituals de macOS.

use super::session::{CompletionEntry, Diag};
use serde_json::Value;
use std::path::{Path, PathBuf};

/// Com arrencar el tsserver detectat: script JS amb node, o binari directe.
#[derive(Debug, Clone)]
pub enum TsLaunch {
    Script { node: PathBuf, script: PathBuf },
    Bin(PathBuf),
}

/// Rutes habituals del tsserver.js dins del VSCode instal·lat (macOS).
const VSCODE_PATHS: &[&str] = &[
    "/Applications/Visual Studio Code.app/Contents/Resources/app/extensions/node_modules/typescript/lib/tsserver.js",
    "/Applications/Visual Studio Code - Insiders.app/Contents/Resources/app/extensions/node_modules/typescript/lib/tsserver.js",
];

/// Detecta un tsserver utilitzable. Ordre: VSCode instal·lat → `tsserver` al
/// PATH → typescript del projecte. Retorna el llançador i un nom honest per
/// a l'indicador d'estat. `None` = degradació honesta (sense IntelliSense TS).
pub fn detect(root: &Path) -> Option<(TsLaunch, String)> {
    let node = crate::agent::runtools::node_binary();
    // 1) El VSCode de l'usuari (el camí preferent: ja el té instal·lat).
    if let Some(node) = &node {
        let mut candidates: Vec<PathBuf> = VSCODE_PATHS.iter().map(PathBuf::from).collect();
        if let Some(home) = dirs::home_dir() {
            candidates.push(home.join(
                "Applications/Visual Studio Code.app/Contents/Resources/app/extensions/node_modules/typescript/lib/tsserver.js",
            ));
        }
        for script in candidates {
            if script.is_file() {
                return Some((
                    TsLaunch::Script { node: node.clone(), script },
                    "tsserver (VSCode)".to_string(),
                ));
            }
        }
    }
    // 2) Un `tsserver` global (npm i -g typescript): parla el protocol directament.
    if let Ok(bin) = which::which("tsserver") {
        return Some((TsLaunch::Bin(bin), "tsserver".to_string()));
    }
    // 3) El typescript del propi projecte.
    if let Some(node) = &node {
        let script = root.join("node_modules/typescript/lib/tsserver.js");
        if script.is_file() {
            return Some((
                TsLaunch::Script { node: node.clone(), script },
                "tsserver (projecte)".to_string(),
            ));
        }
    }
    None
}

/// Categoria del tsserver → gravetat nostra (1-based line/offset ja ho són).
fn severity_of(category: &str) -> &'static str {
    match category {
        "error" => "error",
        "warning" => "warning",
        _ => "info", // «suggestion» i desconegudes
    }
}

/// Mapa els diagnòstics d'un esdeveniment `syntaxDiag`/`semanticDiag`
/// (`body.diagnostics[]` amb `start:{line,offset}`, `text`, `category`).
pub fn map_diagnostics(body: &Value, source: &str) -> Vec<Diag> {
    let Some(arr) = body.get("diagnostics").and_then(|d| d.as_array()) else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|d| {
            let start = d.get("start")?;
            let line = start.get("line").and_then(|v| v.as_u64())? as usize;
            let col = start.get("offset").and_then(|v| v.as_u64()).unwrap_or(1) as usize;
            let text = d.get("text").and_then(|v| v.as_str()).unwrap_or("");
            let category = d.get("category").and_then(|v| v.as_str()).unwrap_or("error");
            let code = d.get("code").and_then(|v| v.as_u64());
            let message = match code {
                Some(c) => format!("{text} (TS{c})"),
                None => text.to_string(),
            };
            Some(Diag {
                line: line.max(1),
                col: col.max(1),
                severity: severity_of(category).to_string(),
                message,
                source: source.to_string(),
            })
        })
        .collect()
}

/// Tipus de completion del tsserver → CompletionItemKind numèric (LSP/Monaco).
pub fn kind_from_ts(kind: &str) -> u32 {
    match kind {
        "keyword" => 14,
        "module" | "external module name" => 9,
        "class" => 7,
        "interface" => 8,
        "enum" => 13,
        "enum member" => 20,
        "function" => 3,
        "method" | "call signature" | "construct signature" => 2,
        "property" | "getter" | "setter" => 10,
        "field" => 5,
        "var" | "let" | "const" | "local var" | "local function" => 6,
        "parameter" => 6,
        "type" | "type parameter" => 25,
        "alias" => 18,
        "string" => 12,
        "script" | "directory" => 17,
        _ => 1, // Text
    }
}

/// Mapa el `body` d'una resposta `completions` (llista d'entrades amb
/// `name`, `kind`, `insertText` opcional) al contracte de la UI.
pub fn map_completions(body: &Value) -> Vec<CompletionEntry> {
    let Some(arr) = body.as_array() else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|e| {
            let name = e.get("name").and_then(|v| v.as_str())?;
            let kind = e.get("kind").and_then(|v| v.as_str()).unwrap_or("");
            Some(CompletionEntry {
                label: name.to_string(),
                insert_text: e
                    .get("insertText")
                    .and_then(|v| v.as_str())
                    .unwrap_or(name)
                    .to_string(),
                detail: kind.to_string(),
                kind: kind_from_ts(kind),
            })
        })
        .collect()
}

/// Mapa el `body` de `quickinfo` a un text pla per al hover de Monaco.
pub fn map_quickinfo(body: &Value) -> Option<String> {
    let display = body.get("displayString").and_then(|v| v.as_str()).unwrap_or("");
    if display.is_empty() {
        return None;
    }
    let mut s = format!("```typescript\n{display}\n```");
    let doc = body
        .get("documentation")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    if !doc.is_empty() {
        s.push_str("\n\n");
        s.push_str(doc);
    }
    Some(s)
}

/// Mapa el `body` de `definition` (llista de FileSpan) a (fitxer, línia, col).
pub fn map_definition(body: &Value) -> Option<(String, usize, usize)> {
    let first = body.as_array()?.first()?;
    let file = first.get("file").and_then(|v| v.as_str())?;
    let start = first.get("start")?;
    let line = start.get("line").and_then(|v| v.as_u64()).unwrap_or(1) as usize;
    let col = start.get("offset").and_then(|v| v.as_u64()).unwrap_or(1) as usize;
    Some((file.to_string(), line.max(1), col.max(1)))
}

/// Mapa el `body` de `references` (`refs[]` amb `file` i `start`).
pub fn map_references(body: &Value) -> Vec<(String, usize, usize)> {
    let Some(refs) = body.get("refs").and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    refs.iter()
        .filter_map(|r| {
            let file = r.get("file").and_then(|v| v.as_str())?;
            let start = r.get("start")?;
            let line = start.get("line").and_then(|v| v.as_u64()).unwrap_or(1) as usize;
            let col = start.get("offset").and_then(|v| v.as_u64()).unwrap_or(1) as usize;
            Some((file.to_string(), line.max(1), col.max(1)))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn diagnostic_semantic_amb_codi_i_gravetat() {
        let body = json!({
            "file": "/p/a.ts",
            "diagnostics": [
                {"start":{"line":3,"offset":7},"end":{"line":3,"offset":10},
                 "text":"Cannot find name 'foo'.","category":"error","code":2304},
                {"start":{"line":9,"offset":1},"end":{"line":9,"offset":5},
                 "text":"'x' is declared but its value is never read.","category":"warning","code":6133}
            ]
        });
        let diags = map_diagnostics(&body, "tsserver");
        assert_eq!(diags.len(), 2);
        assert_eq!(diags[0].line, 3);
        assert_eq!(diags[0].col, 7);
        assert_eq!(diags[0].severity, "error");
        assert!(diags[0].message.contains("Cannot find name 'foo'."));
        assert!(diags[0].message.contains("TS2304"));
        assert_eq!(diags[0].source, "tsserver");
        assert_eq!(diags[1].severity, "warning");
    }

    #[test]
    fn diagnostic_suggestion_es_info_i_body_buit_no_peta() {
        let body = json!({"diagnostics":[
            {"start":{"line":1,"offset":1},"text":"potser…","category":"suggestion"}
        ]});
        assert_eq!(map_diagnostics(&body, "tsserver")[0].severity, "info");
        assert!(map_diagnostics(&json!({}), "tsserver").is_empty());
        assert!(map_diagnostics(&json!({"diagnostics":"no"}), "tsserver").is_empty());
    }

    #[test]
    fn completions_mapatge_de_kind_i_inserttext() {
        let body = json!([
            {"name":"console","kind":"var","sortText":"0"},
            {"name":"log","kind":"method","sortText":"0","insertText":"log($1)"},
            {"name":"desconegut","kind":"cosa estranya","sortText":"1"}
        ]);
        let cs = map_completions(&body);
        assert_eq!(cs.len(), 3);
        assert_eq!(cs[0].label, "console");
        assert_eq!(cs[0].insert_text, "console");
        assert_eq!(cs[0].kind, 6); // Variable
        assert_eq!(cs[1].insert_text, "log($1)");
        assert_eq!(cs[1].kind, 2); // Method
        assert_eq!(cs[2].kind, 1); // fallback Text
        assert!(map_completions(&json!({"no":"llista"})).is_empty());
    }

    #[test]
    fn quickinfo_amb_documentacio_i_buit() {
        let body = json!({
            "displayString":"const x: number",
            "documentation":"La constant del projecte.",
            "tags":[]
        });
        let s = map_quickinfo(&body).unwrap();
        assert!(s.contains("const x: number"));
        assert!(s.contains("La constant del projecte."));
        assert!(map_quickinfo(&json!({})).is_none());
        assert!(map_quickinfo(&json!({"displayString":""})).is_none());
    }

    #[test]
    fn definition_i_references_filespans() {
        let def = json!([{"file":"/p/lib.ts","start":{"line":12,"offset":3},"end":{"line":12,"offset":8}}]);
        assert_eq!(map_definition(&def).unwrap(), ("/p/lib.ts".to_string(), 12, 3));
        assert!(map_definition(&json!([])).is_none());
        let refs = json!({
            "refs":[
                {"file":"/p/a.ts","start":{"line":2,"offset":5},"isDefinition":true},
                {"file":"/p/b.ts","start":{"line":7,"offset":1},"isDefinition":false}
            ],
            "symbolName":"x"
        });
        let rs = map_references(&refs);
        assert_eq!(rs.len(), 2);
        assert_eq!(rs[1], ("/p/b.ts".to_string(), 7, 1));
        assert!(map_references(&json!({})).is_empty());
    }

    #[test]
    fn deteccio_vscode_o_degradacio() {
        // Si el VSCode hi és (màquina de desenvolupament), detecta l'script;
        // si no, la degradació és honesta (None o una altra via vàlida).
        let got = detect(Path::new("."));
        if Path::new(VSCODE_PATHS[0]).is_file() && crate::agent::runtools::node_binary().is_some() {
            let (launch, name) = got.expect("amb VSCode + node ha de detectar tsserver");
            assert!(matches!(launch, TsLaunch::Script { .. }));
            assert!(name.contains("tsserver"));
        }
    }
}