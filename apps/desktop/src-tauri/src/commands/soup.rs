//! Recepta SOUP exportable: afinar la IA de NoOrbit en una màquina AMB GPU.
//!
//! Soup (github.com/MakazhanAlpamys/Soup, Apache-2.0) és un CLI de
//! fine-tuning QLoRA «one YAML». Requereix CUDA (NVIDIA) o Apple Silicon
//! (MPS); en CPU és «experimental — very slow», i en un Mac Intel de 8 GB
//! NO és viable entrenar. Per això NoOrbit no entrena ací: EXPORTA una
//! recepta completa (soup.yaml + dataset alpaca + instruccions) generada del
//! seu propi coneixement —les plantilles de codegen, els protocols de
//! fitxers i les regles d'ECC de sèrie— perquè l'usuari la puga entrenar en
//! una màquina amb GPU i torne el model GGUF via Hugging Bay / Ollama.
//!
//! Honest: això NO modifica pesos en aquest equip; és una recepta per fer-ho
//! fora. El dataset és llavor demostrativa: com més files s'hi afegeixen,
//! millor afinat.

use serde_json::json;
use std::path::{Path, PathBuf};
use tauri::{command, State};

/// soup.yaml de la recepta: QLoRA 4-bit sobre un Qwen Coder petit (cap en
/// GPUs modestes de 6-8 GB; `soup doctor` ho verifica abans d'entrenar).
const SOUP_YAML: &str = r#"# Recepta Soup generada per NoOrbit — afinat de la IA nativa.
# Entrena en una màquina amb GPU (CUDA) o Apple Silicon (MPS):
#   pip install soup-cli
#   soup doctor --config soup.yaml
#   soup train --config soup.yaml
base: Qwen/Qwen2.5-Coder-1.5B-Instruct
task: sft

data:
  train: ./data/train.jsonl
  format: alpaca
  val_split: 0.1

training:
  epochs: 3
  lr: 2e-5
  batch_size: auto
  lora:
    r: 16
    alpha: 16
    quantization: 4bit

output: ./output
"#;

const LLEGEIX_ME: &str = r#"# Recepta Soup de NoOrbit

Això és una recepta de fine-tuning (QLoRA) per afinar un Qwen2.5-Coder-1.5B
amb el coneixement de NoOrbit: plantilles de codi (React, HTML, Python, Rust,
Godot, Defold, Bevy, Blender/bpy, Unreal lleuger…), el protocol de fitxers
«@file:» i les regles comunes d'ECC de sèrie.

## On entrenar

Cal una màquina amb GPU NVIDIA (CUDA) o un Mac Apple Silicon (MPS).
En CPU, Soup és experimental i molt lent: en un Mac Intel de 8 GB NO és
viable. Aquest equip (Intel) només fa d'editor de la recepta.

## Passos

```bash
pip install soup-cli
soup doctor --config soup.yaml     # verifica GPU, RAM i dependències
soup train --config soup.yaml      # QLoRA 4-bit, ~3 epochs
soup chat --model ./output         # prova el model afinat
soup export --model ./output --format gguf --quant q4_k_m
```

## Tornar el model a NoOrbit

El GGUF exportat (carpeta `output/`) es pot:

- pujar a Hugging Face i baixar-lo des del panell «Hugging Bay» de NoOrbit
  (es registra sol a Ollama), o
- registrar directament: `ollama create noorbit-qwen -f Modelfile` amb un
  Modelfile que continga `FROM /ruta/al/model.gguf`.

## Millorar el dataset

`data/train.jsonl` és format alpaca (instruction/input/output). Afegeix-hi
files amb els teus exemples (codi del teu projecte, receptes bpy provades,
scripts Unreal…). Com més dades de qualitat, millor afinat. Després:
`soup data inspect ./data/train.jsonl` per revisar-lo.
"#;

/// Una fila alpaca.
fn fila(instruction: String, output: String) -> serde_json::Value {
    json!({ "instruction": instruction, "input": "", "output": output })
}

/// Dataset llavor generat del coneixement REAL de NoOrbit: cada plantilla de
/// codegen esdevé un exemple del protocol «@file:», més files de protocol i
/// estil, més les regles comunes d'ECC de sèrie (si la còpia hi és).
pub fn dataset(app: Option<&tauri::AppHandle>) -> Vec<serde_json::Value> {
    let mut files = Vec::new();
    for s in crate::ai::codegen::catalog() {
        let file = s.file.replace("{nom}", "exemple").replace("{Nom}", "Exemple");
        let code = s.code.replace("{nom}", "exemple").replace("{Nom}", "Exemple");
        files.push(fila(
            format!("Crea al projecte una plantilla de {} (fitxer {}).", s.title, file),
            format!("@file: {file}\n```{}\n{code}```", s.lang),
        ));
    }
    // Protocol i estil de NoOrbit: el que la IA ha de respectar sempre.
    files.push(fila(
        "Com has de respondre quan crees fitxers al projecte de NoOrbit?".into(),
        "Sempre en format materialitzable: una línia «@file: ruta/del/fitxer» seguida d'una tanca \
         de codi amb el llenguatge (```python, ```tsx…). El xat de NoOrbit converteix eixos blocs \
         en fitxers reals amb revisió verd/roig. Per editar: «@edit: ruta» amb el canvi; per \
         esborrar: «@delete: ruta». Respon en català, breu i directe, i no inventes capacitats: \
         si una cosa no es pot fer, digues-ho.".into(),
    ));
    files.push(fila(
        "Quin és el camí més lleuger per programar jocs en Unreal des de NoOrbit?".into(),
        "Python dins l'editor via el pont Remote Control (directriu RUN|UNREAL|), sense compilar \
         C++ ni Blueprints. Fes servir els subsistemes moderns \
         (unreal.get_editor_subsystem(unreal.EditorActorSubsystem), spawn_actor_from_class); \
         EditorLevelLibrary està deprecat des de UE 5.1. Projecte lleuger: .uproject amb \
         PythonScriptPlugin i RemoteControl, i Config/DefaultEngine.ini sense Lumen \
         (r.DynamicGlobalIlluminationMethod=0), sense Nanite, sense ombres virtuals \
         (r.Shadow.Virtual.Enable=0) i escalabilitat baixa.".into(),
    ));
    files.push(fila(
        "Com renderitze en Blender en un Mac Intel amb GPU integrada?".into(),
        "Amb EEVEE, mai Cycles: és el motor lleuger. En Blender 4.2+ el nom canvia: prova \
         escena.render.engine = 'BLENDER_EEVEE_NEXT' i, si llança TypeError, cau a \
         'BLENDER_EEVEE'. Mostres baixes (taa_render_samples = 32) i resolució 1280x720. \
         Els scripts bpy es poden executar dins Blender (Scripting) o headless: \
         blender --background --python script.py. NoOrbit els envia pel pont socket \
         (directriu RUN|BLENDER|).".into(),
    ));
    // Regles comunes d'ECC de sèrie: coneixement real, no inventat.
    if let Some(repo) = crate::commands::ecc::bundled_ecc_dir(app) {
        if let Ok(entries) = std::fs::read_dir(repo.join("rules").join("common")) {
            for e in entries.flatten() {
                let path = e.path();
                if path.extension().map(|x| x != "md").unwrap_or(true) {
                    continue;
                }
                let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                let Ok(raw) = std::fs::read_to_string(&path) else { continue };
                let (_, _, body) = crate::commands::ecc::parse_ecc_md(&raw, &stem);
                if body.is_empty() {
                    continue;
                }
                files.push(fila(
                    format!("Què diu la regla comuna d'ECC «{stem}» per a agents de codi?"),
                    body,
                ));
            }
        }
    }
    files
}

/// Exporta la recepta a la carpeta triada per l'usuari. Retorna un resum.
#[command]
pub async fn soup_export_recipe(
    app: tauri::AppHandle,
    _state: State<'_, crate::AppState>,
    dir: String,
) -> Result<String, String> {
    let dest = PathBuf::from(dir.trim());
    if dest.as_os_str().is_empty() {
        return Err("Cal triar una carpeta de destinació.".into());
    }
    let files = dataset(Some(&app));
    let n = files.len();
    let jsonl: String = files
        .iter()
        .map(|f| f.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    let dest2 = dest.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        std::fs::create_dir_all(dest2.join("data")).map_err(|e| e.to_string())?;
        std::fs::write(dest2.join("soup.yaml"), SOUP_YAML).map_err(|e| e.to_string())?;
        std::fs::write(dest2.join("data").join("train.jsonl"), jsonl).map_err(|e| e.to_string())?;
        std::fs::write(dest2.join("LLEGEIX-ME.md"), LLEGEIX_ME).map_err(|e| e.to_string())?;
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())??;
    Ok(format!(
        "Recepta Soup exportada a {} ({n} exemples: plantilles, protocols i regles ECC). \
         Entrena-la en una màquina amb GPU: soup train --config soup.yaml",
        dest.display()
    ))
}

/// Comprova que la recepta escrita és usable (soup.yaml + jsonl vàlid).
#[allow(dead_code)]
fn valida(dest: &Path) -> bool {
    let yaml = std::fs::read_to_string(dest.join("soup.yaml")).unwrap_or_default();
    if !yaml.contains("task: sft") || !yaml.contains("Qwen2.5-Coder") {
        return false;
    }
    let Ok(jsonl) = std::fs::read_to_string(dest.join("data").join("train.jsonl")) else {
        return false;
    };
    jsonl.lines().all(|l| serde_json::from_str::<serde_json::Value>(l).is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dataset_te_plantilles_i_regles() {
        let files = dataset(None);
        // Totes les plantilles del catàleg hi són, més protocol i estil.
        assert!(files.len() >= crate::ai::codegen::catalog().len() + 3);
        for f in &files {
            assert!(f["instruction"].as_str().unwrap().len() > 10);
            assert!(f["output"].as_str().unwrap().len() > 20);
        }
        // El protocol @file: s'ensenya amb el format exacte de NoOrbit.
        let text = files.iter().map(|f| f.to_string()).collect::<Vec<_>>().join("\n");
        assert!(text.contains("@file:"));
        assert!(text.contains("EditorActorSubsystem"));
        // Les regles ECC de sèrie hi són (la còpia del repositori existeix).
        assert!(text.contains("ECC"));
    }

    #[test]
    fn exporta_recepta_usable() {
        let tmp = std::env::temp_dir().join(format!("noorbit-soup-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(tmp.join("data")).unwrap();
        let files = dataset(None);
        let jsonl = files.iter().map(|f| f.to_string()).collect::<Vec<_>>().join("\n");
        std::fs::write(tmp.join("soup.yaml"), SOUP_YAML).unwrap();
        std::fs::write(tmp.join("data").join("train.jsonl"), jsonl).unwrap();
        std::fs::write(tmp.join("LLEGEIX-ME.md"), LLEGEIX_ME).unwrap();
        assert!(valida(&tmp));
        let _ = std::fs::remove_dir_all(&tmp);
    }
}