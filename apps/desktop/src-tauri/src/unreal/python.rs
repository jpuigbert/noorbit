//! Execució de scripts Python dins Unreal Editor.

use super::client::UnrealClient;
use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PythonResult {
    pub success: bool,
    pub output: Vec<String>,
    pub errors: Vec<String>,
}

/// Executa un script Python dins Unreal Editor.
pub async fn run_script(client: &UnrealClient, code: &str) -> Result<PythonResult> {
    let result = client.execute_python(code).await?;

    let success = result
        .get("ReturnValue")
        .and_then(|v| v.get("Success"))
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    let output = result
        .get("ReturnValue")
        .and_then(|v| v.get("LogOutput"))
        .and_then(|v| v.get("Entries"))
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|e| {
                    let type_ = e.get("Type")?.as_str()?;
                    let msg = e.get("Output")?.as_str()?;
                    if type_ == "Error" {
                        None
                    } else {
                        Some(msg.to_string())
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    let errors = result
        .get("ReturnValue")
        .and_then(|v| v.get("LogOutput"))
        .and_then(|v| v.get("Entries"))
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|e| {
                    let type_ = e.get("Type")?.as_str()?;
                    let msg = e.get("Output")?.as_str()?;
                    if type_ == "Error" {
                        Some(msg.to_string())
                    } else {
                        None
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(PythonResult { success, output, errors })
}

/// Script: crea un Blueprint buit.
pub fn script_create_blueprint(name: &str, parent_class: &str) -> String {
    format!(
        r#"
import unreal

factory = unreal.BlueprintFactory()
factory.set_editor_property("ParentClass", unreal.load_class(None, "/Script/Engine.{parent}"))

asset_tools = unreal.AssetToolsHelpers.get_asset_tools()
bp = asset_tools.create_asset("{name}", "/Game/Blueprints", None, factory)

if bp:
    unreal.EditorAssetLibrary.save_asset(bp.get_path_name())
    print(f"Blueprint creat: {{bp.get_path_name()}}")
else:
    print("Error creant Blueprint")
"#,
        name = name,
        parent = parent_class
    )
}

/// Script: fa spawn d'un actor al nivell actual.
pub fn script_spawn_actor(class_path: &str, location: [f64; 3]) -> String {
    format!(
        r#"
import unreal

world = unreal.EditorLevelLibrary.get_editor_world()
actor_class = unreal.load_class(None, "{class_path}")
loc = unreal.Vector({x}, {y}, {z})
rot = unreal.Rotator(0, 0, 0)

actor = unreal.EditorLevelLibrary.spawn_actor_from_class(actor_class, loc, rot)
if actor:
    print(f"Actor creat: {{actor.get_name()}} a {{loc}}")
else:
    print("Error creant actor")
"#,
        class_path = class_path,
        x = location[0],
        y = location[1],
        z = location[2]
    )
}

/// Script: importa un asset (FBX, OBJ, GLB, etc.).
pub fn script_import_asset(file_path: &str, dest_path: &str) -> String {
    format!(
        r#"
import unreal

task = unreal.AssetImportTask()
task.set_editor_property("filename", r"{file}")
task.set_editor_property("destination_path", "{dest}")
task.set_editor_property("automated", True)
task.set_editor_property("replace_existing", True)
task.set_editor_property("save", True)

unreal.AssetToolsHelpers.get_asset_tools().import_asset_tasks([task])
print("Asset importat correctament")
"#,
        file = file_path,
        dest = dest_path
    )
}

/// Script: construeix la il·luminació del nivell.
pub fn script_build_lighting(quality: &str) -> String {
    format!(
        r#"
import unreal

quality_map = {{
    "preview": unreal.LightingBuildQuality.PREVIEW,
    "medium": unreal.LightingBuildQuality.MEDIUM,
    "high": unreal.LightingBuildQuality.HIGH,
    "production": unreal.LightingBuildQuality.PRODUCTION,
}}

q = quality_map.get("{quality}", unreal.LightingBuildQuality.HIGH)
unreal.EditorLevelLibrary.editor_build_lighting()
print(f"Il·luminació construïda amb qualitat {{q}}")
"#,
        quality = quality
    )
}
