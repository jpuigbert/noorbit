//! Generació de CODI de la IA nativa: quart component del motor simbòlic
//! (després de matemàtiques, cervell i aprenentatge).
//!
//! El motor natiu no és un LLM i no pot inventar programari arbitrari, però
//! SÍ que serveix per programar: un catàleg de plantilles d'arrencada (React,
//! HTML, CSS, Python, Rust, Node, PHP, TypeScript) viu COMPILAT al binari.
//! Quan la petició encaixa amb una plantilla, la nativa respon amb el format
//! estricte «@file: ruta» + tanca de codi que el xat ja sap materialitzar
//! (`parseFileBlocks` de `stores/agent/fileBlocks.ts` + `materialize` +
//! revisió verd/roig), de manera que la IA nativa CREA FITXERS REALS al
//! projecte sense Ollama, sense token i sense baixar res.

/// Una plantilla del catàleg: dispara per paraules clau i emet un fitxer.
pub struct Snippet {
    /// Nom legible per a l'usuari («Component React»).
    pub title: &'static str,
    /// Ruta per defecte dins el projecte (pot contenir `{nom}`).
    pub file: &'static str,
    /// Llenguatge de la tanca (`tsx`, `python`, `html`…).
    pub lang: &'static str,
    /// Paraules que sumen per disparar la plantilla (minúscules i sense
    /// accents: es comparen amb els tokens ja normalitzats de la petició).
    pub triggers: &'static [&'static str],
    /// Paraules que per elles soles identifiquen la plantilla (llenguatge o
    /// format concret: «react», «python», «php»…).
    pub strong: &'static [&'static str],
    /// Contingut. `{nom}` és el nom en slug (rutes i fitxers) i `{Nom}` en
    /// identificador (funcions, classes, títols).
    pub code: &'static str,
}

/// Plantilles d'arrencada integrades al binari: el que la nativa sap
/// programar de soca-rel. Totes sense dependències exòtiques. Els motors de
/// joc van AL DAVANT: si una petició empata en punts amb una altra plantilla
/// («fes un joc en godot» també toca «joc» del joc de consola), guanya el
/// motor perquè és la intenció més concreta.
pub fn catalog() -> Vec<Snippet> {
    vec![
        // ── BLENDER i UNREAL: disseny 3D i jocs des de la nativa ──
        // L'export glTF va ABANS de l'escena Blender: «exporta de blender a
        // godot» empata en punts amb l'escena i guanya el primer del catàleg.
        Snippet {
            title: "Blender → glTF (assets per a jocs)",
            file: "blender/export_{nom}.py",
            lang: "python",
            triggers: &["exporta", "exportar", "blender", "godot", "joc", "asset", "gltf"],
            strong: &["gltf"],
            code: r##"""{Nom} — exportació glTF per a motors de joc (Godot/Unreal/Bevy).

Executa'l dins Blender (Scripting) o headless:
    blender --background --python export_{nom}.py
Selecciona els objectes a exportar ABANS d'executar (o exporta tota l'escena).
"""
import bpy
import os

sortida = os.path.join(os.path.dirname(bpy.data.filepath) or ".", "{nom}.gltf")

# glTF separat (.gltf + .bin + textures): el format que Godot 4 importa millor.
bpy.ops.export_scene.gltf(
    filepath=sortida,
    export_format='GLTF_SEPARATE',
    use_selection=False,
    export_apply=True,
    export_yup=True,
)
print("Exportat " + sortida + " — arrossega'l al projecte de Godot/Unreal")
"##,
        },
        Snippet {
            title: "Escena Blender (bpy)",
            file: "blender/{nom}.py",
            lang: "python",
            triggers: &["3d", "escena", "modelatge", "render", "disseny", "figura"],
            strong: &["blender", "bpy"],
            code: r##"""{Nom} — escena Blender generada per la IA nativa de NoOrbit.

Executa'l dins Blender (pestanya Scripting) o headless:
    blender --background --python {nom}.py
EEVEE (no Cycles): el render més lleuger per a Mac Intel amb GPU integrada.
"""
import bpy
import os

# Escena neta: esborra el cub/llum/càmera per defecte sense tocar la resta.
for nom_obj in ("Cube", "Light", "Camera"):
    if nom_obj in bpy.data.objects:
        bpy.data.objects.remove(bpy.data.objects[nom_obj], do_unlink=True)

# Terra + figura principal amb material de color.
bpy.ops.mesh.primitive_plane_add(size=12, location=(0, 0, 0))
terra = bpy.context.active_object
terra.name = "{Nom}_Terra"

bpy.ops.mesh.primitive_uv_sphere_add(radius=1.2, location=(0, 0, 1.2))
figura = bpy.context.active_object
figura.name = "{Nom}_Figura"
bpy.ops.object.shade_smooth()

mat = bpy.data.materials.new("{Nom}_Mat")
mat.use_nodes = True
bsdf = mat.node_tree.nodes["Principled BSDF"]
bsdf.inputs["Base Color"].default_value = (0.15, 0.45, 0.9, 1.0)
bsdf.inputs["Roughness"].default_value = 0.35
figura.data.materials.append(mat)

# Llum suau + càmera orientada cap a la figura.
bpy.ops.object.light_add(type='SUN', location=(5, -5, 10))
llum = bpy.context.active_object
llum.data.energy = 3.0
bpy.ops.object.camera_add(location=(7, -6, 5), rotation=(1.1, 0.0, 0.8))
bpy.context.scene.camera = bpy.context.active_object

# EEVEE amb fallback de versió (4.2+ canvia el nom del motor).
escena = bpy.context.scene
try:
    escena.render.engine = 'BLENDER_EEVEE_NEXT'
except TypeError:
    escena.render.engine = 'BLENDER_EEVEE'
escena.render.resolution_x = 1280
escena.render.resolution_y = 720
escena.eevee.taa_render_samples = 32

# Desa el .blend i renderitza una imatge de mostra.
dir_sortida = os.path.dirname(bpy.data.filepath) or "."
bpy.ops.wm.save_as_mainfile(filepath=os.path.join(dir_sortida, "{nom}.blend"))
escena.render.filepath = os.path.join(dir_sortida, "{nom}.png")
bpy.ops.render.render(write_still=True)
print("Escena {Nom} creada: {nom}.blend + {nom}.png")
"##,
        },
        Snippet {
            title: "Animació Blender (bpy)",
            file: "blender/animacio_{nom}.py",
            lang: "python",
            triggers: &["animacio", "animar", "anima", "keyframes", "moviment", "video", "orbita"],
            strong: &["blender", "bpy"],
            code: r##"""{Nom} — animació Blender generada per la IA nativa de NoOrbit.

Executa'l dins Blender amb l'escena oberta: anima la figura «{Nom}_Figura"
(si no existeix, en crea una) i deixa el vídeo preparat per renderitzar.
"""
import bpy

nom_figura = "{Nom}_Figura"
if nom_figura not in bpy.data.objects:
    bpy.ops.mesh.primitive_uv_sphere_add(radius=1.0, location=(0, 0, 1))
    bpy.context.active_object.name = nom_figura
ob = bpy.data.objects[nom_figura]

# Òrbita senzilla: 4 keyframes de posició + rotació contínua.
escena = bpy.context.scene
escena.frame_start = 1
escena.frame_end = 120
for frame, pos in ((1, (3, 0, 1)), (30, (0, 3, 2)), (60, (-3, 0, 1)), (90, (0, -3, 2)), (120, (3, 0, 1))):
    ob.location = pos
    ob.keyframe_insert(data_path="location", frame=frame)
ob.rotation_euler = (0, 0, 0)
ob.keyframe_insert(data_path="rotation_euler", frame=1)
ob.rotation_euler = (0, 0, 6.283)
ob.keyframe_insert(data_path="rotation_euler", frame=120)

# Interpolació suau (BEZIER) a totes les corbes.
if ob.animation_data and ob.animation_data.action:
    for fc in ob.animation_data.action.fcurves:
        for kp in fc.keyframe_points:
            kp.interpolation = 'BEZIER'

print("Animació {Nom} creada (frames 1..120). Render ▸ Render Animation per al vídeo.")
"##,
        },
        Snippet {
            title: "Material Blender (bpy)",
            file: "blender/material_{nom}.py",
            lang: "python",
            triggers: &["material", "textura", "color", "metal", "vidre", "brillant"],
            strong: &["blender", "bpy"],
            code: r##"""{Nom} — material procedural Blender generat per la IA nativa.

Executa'l dins Blender: crea un material metàl·lic acolorit i l'aplica a
l'objecte actiu (o a tots els que no en tenen cap).
"""
import bpy

mat = bpy.data.materials.new("{Nom}_Mat")
mat.use_nodes = True
nodes = mat.node_tree.nodes
bsdf = nodes["Principled BSDF"]
bsdf.inputs["Base Color"].default_value = (0.75, 0.35, 0.1, 1.0)
bsdf.inputs["Metallic"].default_value = 0.85
bsdf.inputs["Roughness"].default_value = 0.25

# Textura procedural de soroll per trencar la uniformitat (sense fitxers).
soroll = nodes.new("ShaderNodeTexNoise")
soroll.inputs["Scale"].default_value = 8.0
rampa = nodes.new("ShaderNodeValToRGB")
rampa.color_ramp.elements[0].color = (0.6, 0.3, 0.1, 1.0)
rampa.color_ramp.elements[1].color = (0.9, 0.6, 0.2, 1.0)
mat.node_tree.links.new(soroll.outputs["Fac"], rampa.inputs["Fac"])
mat.node_tree.links.new(rampa.outputs["Color"], bsdf.inputs["Base Color"])

actiu = bpy.context.view_layer.objects.active
objectius = [actiu] if actiu else [o for o in bpy.data.objects if o.type == 'MESH' and not o.data.materials]
for ob in objectius:
    if ob and ob.type == 'MESH':
        ob.data.materials.clear()
        ob.data.materials.append(mat)
print("Material {Nom}_Mat aplicat a " + str(len(objectius)) + " objecte(s)")
"##,
        },
        // ── UNREAL: patrons concrets de l'Editor Scripting ──
        // Aquests tres van ABANS del «Projecte Unreal lleuger» i del «Script
        // Unreal (Python)» perquè, en cas d'empat de punts, guanya el patró
        // MÉS ESPECÍFIC (el genèric sempre va darrere). Els disparadors són
        // en minúscules i SENSE accents (els tokens es normalitzen així:
        // «il·luminació» es converteix en «luminacio»).
        Snippet {
            title: "Il·luminació Unreal (Python)",
            file: "unreal/iluminacio_{nom}.py",
            lang: "python",
            triggers: &[
                "luminacio", "iluminacio", "llum", "llums", "lumina", "lluminar", "iluminar",
                "lightmap", "lightmaps", "sombra", "ombres", "render", "renderitzar",
            ],
            strong: &["unreal", "ue5"],
            code: r##"""{Nom} — il·luminació i construcció de lightmaps a Unreal amb Python.

La via més lleugera: Python dins l'editor, sense compilar C++ ni fer
Blueprints. Executa'l a l'editor (Output Log ▸ Python) o deixa que NoOrbit
l'enviï pel pont Remote Control (directriu RUN|UNREAL|).
"""
import unreal

# Subsistemes moderns: els de pestanya obsoleta no fan res via Remote
# Control, de manera que aquí només s'usen els subsistemes actuals.
actors = unreal.get_editor_subsystem(unreal.EditorActorSubsystem)
nivell = unreal.get_editor_subsystem(unreal.LevelEditorSubsystem)

# 1) Llums: sol (ombres), sky light (ambient) i una puntual per donar contrast.
sol = actors.spawn_actor_from_class(unreal.DirectionalLight, unreal.Vector(0, 0, 600))
sol.set_actor_label("{Nom}_Sol")
sol.set_actor_rotation(unreal.Rotator(-45.0, -30.0, 0.0))

cel = actors.spawn_actor_from_class(unreal.SkyLight, unreal.Vector(0, 0, 0))
cel.set_actor_label("{Nom}_Cel")

llum = actors.spawn_actor_from_class(unreal.PointLight, unreal.Vector(300, 0, 250))
llum.set_actor_label("{Nom}_Llum")
comp = llum.get_component_by_class(unreal.PointLightComponent)
if comp is not None:
    comp.set_intensity(3000.0)
    comp.set_attenuation_radius(800.0)

# 2) Lightmaps: la via CORRECTA és build_light_maps del LevelEditorSubsystem
# (execute_console_command no es pot fer servir des d'un script).
qualitat = getattr(unreal.LightingBuildQuality, "QUALITY_PRODUCTION", None)
if qualitat is None:
    qualitat = getattr(unreal.LightingBuildQuality, "QUALITY_HIGH", None)
if qualitat is None:
    raise RuntimeError("Aquesta versió de Unreal no coneix LightingBuildQuality")

if not nivell.build_light_maps(qualitat, False):
    raise RuntimeError("Unreal ha refusat construir els lightmaps (prova QUALITY_MEDIUM)")

# 3) Desa el nivell amb la il·luminació ja construïda.
nivell.save_current_level()
print("{Nom}: il·luminació construïda (3 llums + lightmaps) i nivell desat")
"##,
        },
        Snippet {
            title: "Importació d'assets Unreal (Python)",
            file: "unreal/importa_{nom}.py",
            lang: "python",
            triggers: &[
                "importa", "importar", "import", "importacio", "importaciones", "fbx", "glb",
                "gltf", "obj", "asset", "assets", "malla", "malle", "textura", "model",
            ],
            strong: &["unreal", "ue5"],
            code: r##"""{Nom} — importació d'assets a Unreal amb Python.

Executa'l a l'editor (Output Log ▸ Python) o deixa que NoOrbit l'enviï pel
pont Remote Control (directriu RUN|UNREAL|).

EDITA les dues constants del capdamunt abans d'executar: RUTA_ORIGEN ha de
ser el fitxer REAL (FBX, GLB, OBJ…) i DESTINACIO la carpeta dins el projecte
on es vol guardar.
"""
import os
import unreal

RUTA_ORIGEN = "/ruta/absoluta/al/model.fbx"
DESTINACIO = "/Game/Imports"

if not os.path.isfile(RUTA_ORIGEN):
    raise RuntimeError("No existeix el fitxer: " + RUTA_ORIGEN)

assets = unreal.get_editor_subsystem(unreal.EditorAssetSubsystem)
actors = unreal.get_editor_subsystem(unreal.EditorActorSubsystem)

data = unreal.AutomatedAssetImportData()
data.set_editor_property("filenames", [RUTA_ORIGEN])
data.set_editor_property("destination_path", DESTINACIO)
data.set_editor_property("replace_existing", True)
data.set_editor_property("group_with_root", False)

# import_assets_automated RETORNA els assets importats; import_asset_tasks
# retorna None, per això no l'fem servir com a via principal.
importats = list(unreal.AssetToolsHelpers.get_asset_tools().import_assets_automated(data) or [])
if not importats:
    raise RuntimeError("Unreal no ha importat res. Revisa el format del fitxer i el log de l'editor.")

noms = []
for obj in importats:
    noms.append(obj.get_name())
    assets.save_loaded_asset(obj)
    # Si és una malla, la posa a l'escena per veure-la de seguida.
    if isinstance(obj, unreal.StaticMesh):
        actor = actors.spawn_actor_from_class(unreal.StaticMeshActor, unreal.Vector(0, 0, 0))
        actor.set_actor_label(obj.get_name())
        comp = actor.get_component_by_class(unreal.StaticMeshComponent)
        if comp is not None:
            comp.set_static_mesh(obj)

print("{Nom}: importats " + str(len(importats)) + " asset(s) a " + DESTINACIO + " -> " + ", ".join(noms))
"##,
        },
        Snippet {
            title: "Escena Unreal (Python)",
            file: "unreal/escena_{nom}.py",
            lang: "python",
            triggers: &["escena", "nivel", "nivell", "cub", "plataforma", "sala", "mapa", "geometria"],
            strong: &["unreal", "ue5"],
            code: r##"""{Nom} — escena Unreal creada amb Python (Editor Scripting).

La via més lleugera: Python dins l'editor, sense compilar C++ ni fer
Blueprints. Executa'l a l'editor (Output Log ▸ Python) o deixa que NoOrbit
l'enviï pel pont Remote Control (directriu RUN|UNREAL|).
"""
import unreal

# Subsistemes moderns: els de pestanya obsoleta no fan res via Remote
# Control, de manera que aquí només s'usen els subsistemes actuals.
actors = unreal.get_editor_subsystem(unreal.EditorActorSubsystem)
nivell = unreal.get_editor_subsystem(unreal.LevelEditorSubsystem)

MATERIAL = unreal.load_object(None, "/Engine/BasicShapes/BasicShapeMaterial.BasicShapeMaterial")


def crea_mesh(classe, mesh_path, etiqueta, x, y, z, escala=1.0):
    mesh = unreal.load_object(None, mesh_path)
    if mesh is None:
        raise RuntimeError("No he trobat el mesh " + mesh_path)
    actor = actors.spawn_actor_from_class(classe, unreal.Vector(x, y, z))
    actor.set_actor_label(etiqueta)
    actor.set_actor_scale3d(unreal.Vector(escala, escala, escala))
    comp = actor.get_component_by_class(unreal.StaticMeshComponent)
    if comp is None:
        raise RuntimeError("L'actor " + etiqueta + " no te StaticMeshComponent")
    comp.set_static_mesh(mesh)
    if MATERIAL is not None:
        comp.set_material(0, MATERIAL)
    return actor


# Terra + tres volumes: una base llegible sense importar cap asset extern.
crea_mesh(unreal.StaticMeshActor, "/Engine/BasicShapes/Plane.Plane", "{Nom}_Terra", 0, 0, 0, 4.0)
crea_mesh(unreal.StaticMeshActor, "/Engine/BasicShapes/Cube.Cube", "{Nom}_Cub", 0, 0, 100)
crea_mesh(unreal.StaticMeshActor, "/Engine/BasicShapes/Cylinder.Cylinder", "{Nom}_Columna", 300, 0, 150)

# Sol + llum puntual: sense llums l'escena surt negra.
sol = actors.spawn_actor_from_class(unreal.DirectionalLight, unreal.Vector(0, 0, 600))
sol.set_actor_label("{Nom}_Sol")
sol.set_actor_rotation(unreal.Rotator(-45.0, -30.0, 0.0))
actors.spawn_actor_from_class(unreal.PointLight, unreal.Vector(0, 0, 300))

# Desa el nivell: així els canvis no es perden en sortir de l'editor.
nivell.save_current_level()
print("{Nom}: escena creada (3 actors + 2 llums) i nivell desat")
"##,
        },
        Snippet {
            title: "Projecte Unreal lleuger",
            file: "{nom}.uproject",
            lang: "json",
            triggers: &["joc", "lleuger", "projecte", "escena"],
            strong: &["unreal", "ue5"],
            code: r##"{
  "FileVersion": 3,
  "EngineAssociation": "5.3",
  "Category": "Game",
  "Description": "{Nom} — projecte Unreal lleuger generat per la IA nativa de NoOrbit. Sense Lumen, Nanite ni ombres virtuals: el mínim consum possible.",
  "Plugins": [
    { "Name": "PythonScriptPlugin", "Enabled": true },
    { "Name": "RemoteControl", "Enabled": true }
  ]
}
"##,
        },
        Snippet {
            title: "Script Unreal (Python)",
            file: "unreal/{nom}.py",
            lang: "python",
            triggers: &["script", "python", "actor", "spawn", "escena", "nivel"],
            strong: &["unreal", "ue5"],
            code: r##"""{Nom} — script Unreal generat per la IA nativa de NoOrbit.

La via MÉS LLEUGERA per programar Unreal: Python dins l'editor (pont Remote
Control de NoOrbit, directriu RUN|UNREAL|), sense compilar C++ ni Blueprints.
Executa'l a l'editor (Output Log ▸ Python) o deixa que NoOrbit l'enviï.
"""
import unreal

# Subsistema modern (els de pestanya obsoleta estan deprecats des d'UE 5.1).
subs = unreal.get_editor_subsystem(unreal.EditorActorSubsystem)

# Actor bàsic amb el cub del motor: sense assets externs, sense Nanite.
mesh = unreal.load_object(None, "/Engine/BasicShapes/Cube.Cube")
actor = subs.spawn_actor_from_class(unreal.StaticMeshActor, unreal.Vector(0, 0, 100))
actor.set_actor_label("{Nom}_Cub")
comp = actor.static_mesh_component
comp.set_static_mesh(mesh)
comp.set_material(0, unreal.load_object(None, "/Engine/BasicShapes/BasicShapeMaterial.BasicShapeMaterial"))

# Llums i escalat baixos: el projecte lleuger ja porta la configuració.
subs.spawn_actor_from_class(unreal.DirectionalLight, unreal.Vector(0, 0, 500))
print("{Nom}: cub i llum creados a l'escena (via Python, sense C++)")
"##,
        },
        Snippet {
            title: "Projecte Godot 4",
            file: "project.godot",
            lang: "text",
            triggers: &["2d", "escena", "plataformes", "sprite"],
            // «joc»/«videojoc» genèrics → Godot 4 (projecte REAL), MAI
            // l'endevinament Python de consola com a resposta per defecte.
            strong: &["godot", "gdscript", "videojoc", "joc"],
            // Mateixa font que el projecte creat automàticament
            // (`godot::setup`): el projecte de la IA i el projecte creat a mà
            // són IDÈNTICS i no pod divergir.
            code: crate::godot::setup::GODOT_PROJECT_GODOT,
        },
        Snippet {
            title: "Projecte Defold",
            file: "game.project",
            lang: "text",
            triggers: &["2d", "lleuger", "mobil", "escena", "joc"],
            strong: &["defold", "lua"],
            code: r#"; {Nom} — projecte Defold generat per la IA nativa de NoOrbit.
; El motor més lleuger (editor d'1-3 MB): obre'l amb l'editor de Defold.

[bootstrap]
main_collection = /main.collectionc

[project]
title = {Nom}

[display]
width = 960
height = 540
"#,
        },
        Snippet {
            title: "Joc Bevy (Rust)",
            file: "Cargo.toml",
            lang: "toml",
            triggers: &["joc", "entitat", "ecs", "finestra"],
            strong: &["bevy"],
            code: r#"# {Nom} — joc Bevy generat per la IA nativa de NoOrbit.

[package]
name = "{nom}"
version = "0.1.0"
edition = "2021"

[dependencies]
bevy = "0.14"
"#,
        },
        Snippet {
            title: "Escena Panda3D (Python)",
            file: "{nom}.py",
            lang: "python",
            triggers: &["3d", "escena", "camera", "model", "joc"],
            strong: &["panda3d", "panda"],
            code: r#"""{Nom} — escena Panda3D generada per la IA nativa de NoOrbit.

    pip install panda3d
    python3 {nom}.py
"""
from direct.showbase.ShowBase import ShowBase
from panda3d.core import TextNode


class Joc(ShowBase):
    def __init__(self):
        ShowBase.__init__(self)
        titol = TextNode("titol")
        titol.setText("{Nom} — Panda3D en marxa!")
        node = self.aspect2d.attachNewNode(titol)
        node.setScale(0.08)
        node.setPos(-0.4, 0, 0)
        self.cam.setPos(0, -20, 5)
        self.cam.lookAt(0, 0, 0)


Joc().run()
"#,
        },
        Snippet {
            title: "Component React",
            file: "src/{Nom}.tsx",
            lang: "tsx",
            triggers: &["interficie", "widget", "formulari", "panell"],
            strong: &["react", "tsx", "jsx", "vue", "component"],
            code: r#"// {Nom} — component generat per la IA nativa de NoOrbit.
import { useState } from "react";

export default function {Nom}() {
  const [valor, setValor] = useState("");
  const [comptador, setComptador] = useState(0);

  return (
    <section className="{nom}">
      <h2>{Nom}</h2>
      <input
        value={valor}
        onChange={(e) => setValor(e.target.value)}
        placeholder="Escriu alguna cosa…"
      />
      <p>Diu: <strong>{valor || "res encara"}</strong></p>
      <button onClick={() => setComptador((c) => c + 1)}>Clics: {comptador}</button>
    </section>
  );
}
"#,
        },
        Snippet {
            title: "Pàgina HTML",
            file: "{nom}.html",
            lang: "html",
            triggers: &["formulari", "index", "portfolio", "landing"],
            strong: &["html", "pagina", "web", "lloc", "site"],
            code: r#"<!doctype html>
<!-- {Nom} — pàgina generada per la IA nativa de NoOrbit. -->
<html lang="ca">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>{Nom}</title>
    <style>
      body { font-family: system-ui, sans-serif; margin: 0; min-height: 100vh;
             display: grid; place-items: center; background: #f4f4f6; }
      main { max-width: 640px; padding: 2rem; background: #fff; border-radius: 12px;
             box-shadow: 0 8px 30px rgb(0 0 0 / 8%); }
      h1 { margin-top: 0; }
      input, button { font: inherit; padding: .5rem .8rem; border-radius: 8px; border: 1px solid #ccc; }
      button { background: #2563eb; color: #fff; border-color: #2563eb; cursor: pointer; }
    </style>
  </head>
  <body>
    <main>
      <h1>{Nom}</h1>
      <p>Pàgina d'arrencada creada per la IA nativa de NoOrbit.</p>
      <form onsubmit="event.preventDefault(); document.getElementById('sortida').textContent = 'Hola, ' + this.nom.value + '!';">
        <label>Nom: <input name="nom" required /></label>
        <button type="submit">Saluda'm</button>
      </form>
      <p id="sortida" role="status"></p>
    </main>
  </body>
</html>
"#,
        },
        Snippet {
            title: "Full CSS",
            file: "{nom}.css",
            lang: "css",
            triggers: &["full", "butlla", "targetes"],
            strong: &["css", "scss", "estil", "estils", "disseny"],
            code: r#"/* {Nom} — estils generats per la IA nativa de NoOrbit. */
:root {
  --color-primari: #2563eb;
  --color-fons: #f4f4f6;
  --radi: 12px;
}

body {
  margin: 0;
  font-family: system-ui, sans-serif;
  background: var(--color-fons);
}

.contingut {
  max-width: 960px;
  margin-inline: auto;
  padding: 1.5rem;
  display: grid;
  gap: 1rem;
  grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
}

.targeta {
  background: #fff;
  border-radius: var(--radi);
  padding: 1rem 1.25rem;
  box-shadow: 0 4px 16px rgb(0 0 0 / 6%);
}

.botio {
  appearance: none;
  border: 0;
  padding: .6rem 1rem;
  border-radius: 999px;
  background: var(--color-primari);
  color: #fff;
  font: inherit;
  cursor: pointer;
}
.botio:hover { filter: brightness(1.1); }
"#,
        },
        Snippet {
            title: "Servidor HTTP Python",
            file: "{nom}.py",
            lang: "python",
            triggers: &["backend", "http", "api", "flask"],
            strong: &["python", "servidor", "server"],
            code: r#""""{Nom} — servidor HTTP generat per la IA nativa de NoOrbit.

Sense dependències: només la biblioteca estàndard del Python 3.
    python3 {nom}.py        →  http://127.0.0.1:8000
"""
import json
from http.server import BaseHTTPRequestHandler, HTTPServer

PORT = 8000


class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path == "/api/dades":
            cos = json.dumps({"missatge": "Hola des de {Nom}!", "ara": self.date_time_string()})
            self._resposta(200, "application/json", cos)
        else:
            html = (
                "<h1>{Nom}</h1>"
                f"<p>Servidor Python en marxa al port {PORT}.</p>"
                '<p><a href="/api/dades">/api/dades</a></p>'
            )
            self._resposta(200, "text/html; charset=utf-8", html)

    def _resposta(self, codi: int, tipus: str, cos: str):
        self.send_response(codi)
        self.send_header("Content-Type", tipus)
        self.end_headers()
        self.wfile.write(cos.encode("utf-8"))


if __name__ == "__main__":
    print(f"{Nom} → http://127.0.0.1:{PORT}  (Control-C per aturar)")
    HTTPServer(("127.0.0.1", PORT), Handler).serve_forever()
"#,
        },
        Snippet {
            title: "Eina de terminal Python",
            file: "{nom}.py",
            lang: "python",
            triggers: &["llista", "comandes", "tareas", "argparse"],
            strong: &["script", "cli", "terminal", "tasques", "todo", "gestor"],
            code: r#""""{Nom} — gestor de tasques de terminal generat per la IA nativa de NoOrbit.

    python3 {nom}.py add "Escriure l'informe"
    python3 {nom}.py            # llistar
    python3 {nom}.py fer 1      # marcar la primera com a feta
"""
import json
import sys
from pathlib import Path

DADES = Path(__file__).with_name("{nom}.json")


def carregar() -> list:
    if DADES.exists():
        return json.loads(DADES.read_text(encoding="utf-8"))
    return []


def desar(tasques: list) -> None:
    DADES.write_text(json.dumps(tasques, ensure_ascii=False, indent=2), encoding="utf-8")


def principal() -> None:
    accio = sys.argv[1] if len(sys.argv) > 1 else "llistar"
    tasques = carregar()

    if accio == "add" and len(sys.argv) > 2:
        tasques.append({"text": " ".join(sys.argv[2:]), "feta": False})
        desar(tasques)
        print(f"Tasca afegida ({len(tasques)} en total).")
    elif accio == "fer" and len(sys.argv) > 2:
        i = int(sys.argv[2]) - 1
        if 0 <= i < len(tasques):
            tasques[i]["feta"] = True
            desar(tasques)
    elif accio == "esborrar" and len(sys.argv) > 2:
        i = int(sys.argv[2]) - 1
        if 0 <= i < len(tasques):
            tasques.pop(i)
            desar(tasques)

    for i, t in enumerate(tasques, 1):
        marca = "✔" if t["feta"] else "·"
        print(f"  {i} [{marca}] {t['text']}")
    if not tasques:
        print('(llista buida — afegeix amb: python3 {nom}.py add "algo")')


if __name__ == "__main__":
    principal()
"#,
        },
        Snippet {
            title: "Joc d'endevinament (Python)",
            file: "{nom}.py",
            lang: "python",
            triggers: &["random", "console", "numero", "joc"],
            // NOMÉS quan la petició demana EXPLÍCITAMENT endevinar (o un joc
            // en Python, via la ruta dedicada de `match_snippet`). Un «fes un
            // joc» genèric ha d'anar a un motor de joc real (Godot 4).
            strong: &["endevina", "endevinar", "endevinament", "adivina"],
            code: r#""""{Nom} — joc de consola generat per la IA nativa de NoOrbit.

    python3 {nom}.py
"""
import random

INTENTS = 7


def principal() -> None:
    secret = random.randint(1, 100)
    print(f"He pensat un número entre 1 i 100. Tens {INTENTS} intents.")

    for intent in range(1, INTENTS + 1):
        try:
            entrada = input(f"Intent {intent}/{INTENTS}: ").strip()
        except EOFError:
            break
        if not entrada.isdigit():
            print("Posa un número, si us plau.")
            continue
        if int(entrada) == secret:
            print(f"Encertat! Era {secret}.")
            return
        pista = "més amunt" if int(entrada) < secret else "més avall"
        print(f"   → prova cap a {pista}.")
    print(f"Ben jugat! El número era {secret}.")


if __name__ == "__main__":
    principal()
"#,
        },
        Snippet {
            title: "Programa Rust",
            file: "src/main.rs",
            lang: "rust",
            triggers: &["binari", "recompte", "crates"],
            strong: &["rust", "cargo", "programa", "aplicacio"],
            code: r#"// {Nom} — programa generat per la IA nativa de NoOrbit.
// Compila amb: cargo run

use std::collections::HashMap;

fn recomptes(text: &str) -> Vec<(String, usize)> {
    let mut mapa: HashMap<String, usize> = HashMap::new();
    for paraula in text.split_whitespace() {
        *mapa.entry(paraula.to_lowercase()).or_insert(0) += 1;
    }
    let mut llista: Vec<(String, usize)> = mapa.into_iter().collect();
    llista.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    llista
}

fn main() {
    let demo = "la IA nativa de NoOrbit programa amb Rust i la IA raona";
    println!("Recompte de paraules:\n");
    for (paraula, n) in recomptes(demo) {
        println!("  {n:2} × {paraula}");
    }
}
"#,
        },
        Snippet {
            title: "Servidor Node",
            file: "{nom}.js",
            lang: "javascript",
            triggers: &["http", "api", "backend"],
            strong: &["node", "javascript", "express"],
            code: r#"// {Nom} — servidor Node generat per la IA nativa de NoOrbit.
// Sense dependències: node {nom}.js  →  http://127.0.0.1:3000

const http = require("http");

const PORT = 3000;
const inici = Date.now();

const servidor = http.createServer((req, res) => {
  if (req.url === "/api/estat") {
    res.writeHead(200, { "Content-Type": "application/json; charset=utf-8" });
    res.end(JSON.stringify({
      missatge: "Hola des de {Nom}!",
      segonsActiu: Math.round((Date.now() - inici) / 1000),
    }));
    return;
  }
  res.writeHead(200, { "Content-Type": "text/html; charset=utf-8" });
  res.end(
    `<h1>{Nom}</h1><p>Node escoltant al port ${PORT}.</p>` +
      `<p><a href="/api/estat">/api/estat</a></p>`
  );
});

servidor.listen(PORT, "127.0.0.1", () => {
  console.log(`{Nom} → http://127.0.0.1:${PORT} (Control-C per aturar)`);
});
"#,
        },
        Snippet {
            title: "Pàgina PHP",
            file: "{nom}.php",
            lang: "php",
            triggers: &["taula", "formulari", "dinamic", "pagina"],
            // «wordpress» NO és un marcador d'aquesta pàgina genèrica: un
            // «crea un tema de wordpress» demana el TEMA (plantilla dedicada),
            // no una pàgina PHP de taula. «pagina» desempata amb l'HTML quan
            // la petició diu «pàgina php» (el llenguatge mana).
            strong: &["php"],
            code: r#"<?php
/**
 * {Nom} — pàgina PHP generada per la IA nativa de NoOrbit.
 * Serveix-la amb: php -S 127.0.0.1:8080 {nom}.php
 */
$titol = '{Nom}';
$dades = [
  ['nom' => 'Rust',  'paper' => 'backend de NoOrbit'],
  ['nom' => 'React', 'paper' => 'interfície'],
  ['nom' => 'Tauri', 'paper' => 'escriptori multiplataforma'],
];
?>
<!doctype html>
<html lang="ca">
<head>
  <meta charset="utf-8">
  <title><?= htmlspecialchars($titol) ?></title>
</head>
<body>
  <h1><?= htmlspecialchars($titol) ?></h1>
  <ul>
    <?php foreach ($dades as $d): ?>
      <li><strong><?= htmlspecialchars($d['nom']) ?></strong> — <?= htmlspecialchars($d['paper']) ?></li>
    <?php endforeach; ?>
  </ul>
  <p>Hora del servidor: <?= date('H:i:s') ?></p>
</body>
</html>
"#,
        },
        Snippet {
            title: "Tema WordPress (mínim)",
            file: "wp-content/themes/{nom}/style.css",
            lang: "css",
            triggers: &["tema", "theme", "wordpress"],
            strong: &["wordpress"],
            code: r#"/*
Theme Name: {Nom}
Theme URI: https://example.com/{nom}
Author: NoOrbit
Author URI: https://noorbit.app
Description: Tema mínim de WordPress generat per la IA nativa de NoOrbit: base sòlida i vàlida per personalitzar.
Version: 1.0.0
Requires at least: 6.0
Tested up to: 6.7
Requires PHP: 7.4
License: GNU General Public License v2 or later
License URI: https://www.gnu.org/licenses/gpl-2.0.html
Text Domain: {nom}
Tags: blog, one-column
*/

body {
  margin: 0;
  font-family: system-ui, sans-serif;
  line-height: 1.6;
  color: #1a1a1a;
  background: #fafafa;
}
.capcalera, .peu { padding: 1rem 1.5rem; background: #111; color: #fff; }
.capcalera a, .peu a { color: #fff; }
.lloc-contingut { max-width: 720px; margin: 2rem auto; padding: 0 1rem; }
article { margin-bottom: 2rem; }
"#,
        },
        Snippet {
            title: "Extensió de VS Code (TypeScript)",
            file: "{nom}.ts",
            lang: "typescript",
            triggers: &["comando", "barra"],
            strong: &["typescript", "vscode", "extensio", "plugin"],
            code: r#"// {Nom} — extensió de VS Code generada per la IA nativa de NoOrbit.
// Prem F5 (Depurar: extensió); l'ordre apareix a la paleta de comandes.

import * as vscode from "vscode";

export function activate(context: vscode.ExtensionContext) {
  const disposable = vscode.commands.registerCommand("{nom}.saluda", async () => {
    const nom = await vscode.window.showInputBox({ prompt: "Com et dius?" });
    if (!nom) return;
    vscode.window.showInformationMessage(`Hola, ${nom}! ({Nom})`);
  });
  context.subscriptions.push(disposable);
}

export function deactivate() {}
"#,
        },
    ]
}

// ── Fitxers EXTRA de les plantilles (projectes de diversos fitxers) ─────

/// Fitxers addicionals d'algunes plantilles: els motors de joc necessiten
/// més d'un fitxer per arrencar (escena + script + configuració). Claus: el
/// títol de la plantilla. Emplenables amb {nom}/{Nom}, com `code`.
fn extras(title: &str) -> &'static [(&'static str, &'static str, &'static str)] {
    match title {
        "Projecte Godot 4" => &[
            (
                "main.tscn",
                "text",
                crate::godot::setup::GODOT_MAIN_TSCN,
            ),
            (
                "main.gd",
                "gdscript",
                crate::godot::setup::GODOT_MAIN_GD,
            ),
        ],
        "Projecte Defold" => &[
            (
                "main.collection",
                "text",
                r#"name: "Main"
embedded_instances {
  id: "go"
  data: "components {\n  id: \"script\"\n  component: \"/main.script\"\n}\n"
}
"#,
            ),
            (
                "main.script",
                "lua",
                r#"-- {Nom} — script principal generat per la IA nativa de NoOrbit.
-- El cub de l'escena bota en bucle amb una animació.

function init(self)
    print("Hola des de {Nom}!")
    msg.post(".", "bota")
end

function on_message(self, message_id, message, sender)
    if message_id == hash("bota") then
        go.animate(".", "scale", go.PLAYBACK_LOOP_PINGPONG,
                   vmath.vector3(1.2), go.EASING_INOUTSINE, 0.8)
    end
end
"#,
            ),
        ],
        "Projecte Unreal lleuger" => &[
            (
                "Config/DefaultEngine.ini",
                "text",
                r#"; {Nom} — configuració LLEUGERA generada per la IA nativa de NoOrbit.
; Sense Lumen, sense Nanite, sense ombres virtuals ni camps de distància:
; el mínim consum de GPU/RAM per a Mac Intel amb gràfica integrada.

[/Script/Engine.RendererSettings]
r.DynamicGlobalIlluminationMethod=0
r.ReflectionMethod=0
r.Shadow.Virtual.Enable=0
r.GenerateMeshDistanceFields=False
r.Nanite.ProjectEnabled=False
r.DefaultFeature.AutoExposure=False
r.DefaultFeature.MotionBlur=False
r.DefaultFeature.AmbientOcclusion=False
r.AntiAliasingMethod=1

[SystemSettings]
sg.ViewDistanceQuality=1
sg.AntiAliasingQuality=1
sg.ShadowQuality=1
sg.PostProcessQuality=0
sg.TextureQuality=1
sg.EffectsQuality=1
sg.FoliageQuality=0

[/Script/EngineSettings.GameMapsSettings]
GameDefaultMap=/Engine/Maps/Entry.Entry

[/Script/PythonScriptPlugin.PythonScriptPluginSettings]
bDeveloperMode=True
"#,
            ),
            (
                "Content/Python/inici.py",
                "python",
                r#"""{Nom} — escena inicial via Python (la via més lleugera: sense C++).

Executa'l a l'editor (Output Log ▸ Python) o deixa que NoOrbit l'enviï pel
pont Remote Control (directriu RUN|UNREAL|). Fa servir els subsistemes
moderns (els de pestanya obsoleta estan deprecats des d'UE 5.1).
"""
import unreal

subs = unreal.get_editor_subsystem(unreal.EditorActorSubsystem)

# Cub del motor + llum direccional: zero assets externs, zero Nanite.
mesh = unreal.load_object(None, "/Engine/BasicShapes/Cube.Cube")
actor = subs.spawn_actor_from_class(unreal.StaticMeshActor, unreal.Vector(0, 0, 100))
actor.set_actor_label("{Nom}_Cub")
comp = actor.static_mesh_component
comp.set_static_mesh(mesh)
comp.set_material(0, unreal.load_object(None, "/Engine/BasicShapes/BasicShapeMaterial.BasicShapeMaterial"))
subs.spawn_actor_from_class(unreal.DirectionalLight, unreal.Vector(0, 0, 500))
print("{Nom}: escena lleugera creada via Python (sense compilar C++)")
"#,
            ),
        ],
        "Tema WordPress (mínim)" => &[
            (
                "wp-content/themes/{nom}/functions.php",
                "php",
                r#"<?php
/**
 * {Nom} — funcions del tema: enqueue d'estils i suports bàsics.
 *
 * @package {Nom}
 */

if ( ! defined( 'ABSPATH' ) ) {
    exit; // Accés directe prohibit.
}

/**
 * Encua el full d'estils del tema (style.css amb el header canònic).
 * Prefix PER TEMA ({Nom}): dos temes generats poden conviure sense
 * «Cannot redeclare» (les funcions globals de PHP xoquen si el nom és fix).
 */
function {Nom}_encua_estils() {
    wp_enqueue_style(
        '{nom}-estil',
        get_stylesheet_uri(),
        array(),
        wp_get_theme()->get( 'Version' )
    );
}
add_action( 'wp_enqueue_scripts', '{Nom}_encua_estils' );

/**
 * Suports bàsics del tema (prefix per tema, com dalt).
 */
function {Nom}_suports() {
    add_theme_support( 'title-tag' );
    add_theme_support( 'post-thumbnails' );
    add_theme_support( 'html5', array( 'search-form', 'gallery', 'caption' ) );
}
add_action( 'after_setup_theme', '{Nom}_suports' );
"#,
            ),
            (
                "wp-content/themes/{nom}/index.php",
                "php",
                r#"<?php
/**
 * {Nom} — plantilla principal: el loop bàsic de WordPress.
 *
 * @package {Nom}
 */

get_header();
?>
<main id="contingut" class="lloc-contingut">
    <?php if ( have_posts() ) : ?>
        <?php
        while ( have_posts() ) :
            the_post();
            ?>
            <article id="post-<?php the_ID(); ?>" <?php post_class(); ?>>
                <h2><a href="<?php the_permalink(); ?>"><?php the_title(); ?></a></h2>
                <?php the_excerpt(); ?>
            </article>
        <?php endwhile; ?>
        <?php the_posts_navigation(); ?>
    <?php else : ?>
        <p><?php esc_html_e( 'Encara no hi ha contingut.', '{nom}' ); ?></p>
    <?php endif; ?>
</main>
<?php
get_footer();
"#,
            ),
            (
                "wp-content/themes/{nom}/header.php",
                "php",
                r#"<?php
/**
 * {Nom} — capçalera del tema.
 *
 * @package {Nom}
 */
?>
<!doctype html>
<html <?php language_attributes(); ?>>
<head>
    <meta charset="<?php bloginfo( 'charset' ); ?>">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <?php wp_head(); ?>
</head>
<body <?php body_class(); ?>>
<?php wp_body_open(); ?>
<header class="capcalera">
    <h1><a href="<?php echo esc_url( home_url( '/' ) ); ?>"><?php bloginfo( 'name' ); ?></a></h1>
</header>
"#,
            ),
            (
                "wp-content/themes/{nom}/footer.php",
                "php",
                r#"<?php
/**
 * {Nom} — peu de pàgina del tema.
 *
 * @package {Nom}
 */
?>
<footer class="peu">
    <p>&copy; <?php echo esc_html( gmdate( 'Y' ) ); ?> <?php bloginfo( 'name' ); ?></p>
</footer>
<?php wp_footer(); ?>
</body>
</html>
"#,
            ),
        ],
        "Joc Bevy (Rust)" => &[(
            "src/main.rs",
            "rust",
            r#"// {Nom} — joc Bevy generat per la IA nativa de NoOrbit.
// Executa'l amb: cargo run
use bevy::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_systems(Startup, munta)
        .run();
}

fn munta(mut commands: Commands) {
    commands.spawn(Camera2d);
    commands.spawn(Text::new("{Nom} — Bevy en marxa!"));
}
"#,
        )],
        _ => &[],
    }
}

// ── Interpretació de la petició ───────────────────────────────────────────

/// Text en minúscules i sense accents, per comparar paraules d'una petició.
pub fn pla(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|c| match c {
            'à' | 'á' | 'â' | 'ä' => 'a',
            'è' | 'é' | 'ê' | 'ë' => 'e',
            'í' | 'ï' | 'î' | 'ì' => 'i',
            'ò' | 'ó' | 'ô' | 'ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            'ñ' => 'n',
            _ => c,
        })
        .collect()
}

/// Arrels de verbs i expressions que declaren una intenció de CREAR codi. Es
/// busquen sobre el text brut perquè `learning::tokens` descarta aquestes
/// paraules com a buides («fes», «fer», «vull» són *stopwords*).
const INTENCIONS: &[&str] = &[
    "crea", "crear", "creem", "genera", "generar", "escriu", "escriure", "programa",
    "programar", "instal", "munta", "muntar", "vull", "vol", "necessit", "fes", "fer",
    "fem", "feu", "nou", "nova", "arrencada", "plantilla", "esquelet", "demo", "exemple",
    "comenc", "comen", "exporta", "exportar",
];

/// Paraules que OBRIEN una pregunta. Si la petició comença amb una, les arrels
/// fluixes («fer», «comenc»…) no declaren intenció de crear codi: «què fa aquest
/// projecte» o «què fa un render en blender» són PREGUNTES, no ordres. Cal un
/// verb inequívoc («crea», «fes»…) o una petició educada («com puc fer…»).
const INTERROGATIUS: &[&str] = &[
    "que", "com", "on", "quan", "qui", "quant", "quina", "quin", "quines", "quins",
    "what", "how", "where", "when", "who", "why", "which",
];

/// Verbs d'intenció que valen fins i tot dins una pregunta.
const INTENCIONS_FORTS: &[&str] = &[
    "crea", "crear", "creem", "genera", "generar", "escriu", "escriure", "programa",
    "programar", "fes", "fem", "feu", "vull", "vol", "necessit", "munta", "muntar",
    "exporta", "exportar", "instal",
];

fn te_intencio(prompt: &str) -> bool {
    let low = format!("{} ", pla(prompt));
    let mots: Vec<&str> = low
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    let pregunta = comenca_amb_pregunta(prompt);
    // «com puc fer…» és una ordre educada, no una pregunta d'informació.
    let educada = mots
        .iter()
        .any(|w| matches!(*w, "puc" | "podries" | "pots" | "podem" | "podeu"));
    mots.iter().any(|w| {
        let fluixa = INTENCIONS.iter().any(|i| w.starts_with(i.trim_end()));
        if !fluixa {
            return false;
        }
        !pregunta || educada || INTENCIONS_FORTS.iter().any(|f| w.starts_with(f))
    })
}

/// La petició comença amb un interrogatiu («què», «com», «how»…)?
fn comenca_amb_pregunta(prompt: &str) -> bool {
    let low = pla(prompt);
    low.split(|c: char| !c.is_alphanumeric())
        .find(|w| !w.is_empty())
        .map(|m| INTERROGATIUS.contains(&m))
        .unwrap_or(false)
}

/// Petició INEQUÍVOCA de tema WordPress: «tema de wordpress», «wordpress
/// theme», «tema wp», «crear tema wordpress» (i variants sense accents, via
/// `pla`). Cal el concepte de TEMA + WordPress: «crea un plugin de wordpress»
/// o «fes un lloc wordpress» NO són aquesta plantilla. «wp» es compara sobre
/// el text normalitzat perquè `learning::tokens` descarta mots de 2 lletres.
fn demana_tema_wp(prompt: &str) -> bool {
    let low = pla(prompt);
    let mots: Vec<&str> = low
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    let tema = mots.iter().any(|w| matches!(*w, "tema" | "temes" | "theme" | "themes"));
    let wp = mots.iter().any(|w| matches!(*w, "wordpress" | "wp"));
    tema && wp
}

/// Plantilla que millor encaixa amb la petició, si n'hi ha cap. Un sol mot
/// fort (llenguatge o format) ja n'hi ha prou; si no, calen dos tokens de la
/// plantilla. Així «fes un component React» dispara, però «què és React» no
/// (porta cap intenció de crear) i «crea un projecte nou» tampoc (cap token
/// de plantilla).
pub fn match_snippet(prompt: &str) -> Option<Snippet> {
    let tokens = crate::learning::tokens(prompt);
    if tokens.is_empty() {
        return None;
    }
    // TEMA WORDPRESS: ruta dedicada per frases inequívoques. Guanya ABANS de
    // puntuar perquè cap matcher genèric (pàgina PHP, HTML…) no segresti la
    // petició, i perquè «tema wp» perdria el token «wp» (massa curt).
    // «wordpress theme» sense verb també és una petició clara; una pregunta
    // («què és un tema de wordpress?») no dispara res.
    if demana_tema_wp(prompt) && (te_intencio(prompt) || !comenca_amb_pregunta(prompt)) {
        return catalog().into_iter().find(|s| s.title == "Tema WordPress (mínim)");
    }
    if !te_intencio(prompt) {
        return None;
    }
    // Joc EN PYTHON explícit («fes un joc en python»): l'única plantilla de
    // joc Python és l'endevinament de consola. MAI per a un «fes un joc»
    // genèric, que va al Godot 4 real.
    if tokens.iter().any(|w| w == "python")
        && tokens.iter().any(|w| matches!(w.as_str(), "joc" | "jocs" | "videojoc"))
    {
        return catalog().into_iter().find(|s| s.title == "Joc d'endevinament (Python)");
    }
    let mut millor: Option<(usize, Snippet)> = None;
    for s in catalog() {
        // El tema WordPress NOMÉS es dispara per la frase inequívoca de dalt:
        // «wordpress» sol no ha de crear un tema (podria ser un plugin, un
        // lloc, una consulta…).
        if s.title == "Tema WordPress (mínim)" {
            continue;
        }
        let forts = s
            .strong
            .iter()
            .filter(|t| tokens.iter().any(|w| w == *t))
            .count();
        let fluixos = s
            .triggers
            .iter()
            .filter(|t| tokens.iter().any(|w| w == *t))
            .count();
        // Un mot fort val més que dos de fluixos.
        let punts = forts * 3 + fluixos;
        if forts == 0 && fluixos < 2 {
            continue;
        }
        if millor.as_ref().map(|(p, _)| punts > *p).unwrap_or(true) {
            millor = Some((punts, s));
        }
    }
    millor.map(|(_, s)| s)
}

/// Nom per a la plantilla: l'explícit («que s'anomeni Cart», «anomenat X»,
/// «de nom X»), si no un identificador especial del prompt (CamelCase o amb
/// guions, que no sigui paraula de la plantilla), i si no el per defecte.
fn nom_demanat(prompt: &str, s: &Snippet) -> String {
    // Busquem sobre el text en minúscules SENSE reescriure els accents: així
    // els índexs de bytes segueixen caient en frontera de caràcter (retallar
    // sobre un text amb accents convertits podria fer un panic).
    let low = prompt.to_lowercase();
    for marca in [
        "s'anomeni ", "s'anomena ", "anomeni ", "anomenat ", "anomenada ", "de nom ",
        "called ", "named ",
    ] {
        if let Some(i) = low.find(marca) {
            let cua = low[i + marca.len()..].trim_start();
            let paraula: String = cua
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
                .collect();
            let n = paraula.chars().count();
            if (2..=32).contains(&n) {
                return paraula;
            }
        }
    }
    for raw in prompt
        .split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '-'))
        .filter(|w| (3..=32).contains(&w.len()))
    {
        let baixa = pla(raw);
        if s.triggers.contains(&baixa.as_str()) || s.strong.contains(&baixa.as_str()) {
            continue;
        }
        let especial =
            raw.chars().any(|c| c.is_uppercase()) || raw.contains('-') || raw.contains('_');
        let te_vocals = "aeiouàèéíòóú".chars().any(|v| baixa.contains(v));
        if especial && te_vocals {
            return raw.to_string();
        }
    }
    default_per_titol(s).to_string()
}

fn default_per_titol(s: &Snippet) -> &'static str {
    match s.title {
        "Component React" => "Exemple",
        "Pàgina HTML" => "index",
        "Full CSS" => "estils",
        "Servidor HTTP Python" | "Servidor Node" => "servidor",
        "Eina de terminal Python" => "tasques",
        "Joc d'endevinament (Python)" => "endevina",
        "Programa Rust" => "main",
        "Pàgina PHP" => "taula",
        "Tema WordPress (mínim)" => "tema",
        "Projecte Godot 4" | "Projecte Defold" => "Joc",
        "Joc Bevy (Rust)" | "Escena Panda3D (Python)" => "joc",
        "Escena Blender (bpy)" => "escena",
        "Animació Blender (bpy)" => "animacio",
        "Material Blender (bpy)" => "material",
        "Blender → glTF (assets per a jocs)" => "export",
        "Projecte Unreal lleuger" | "Script Unreal (Python)" => "joc",
        "Escena Unreal (Python)" => "escena",
        "Il·luminació Unreal (Python)" => "iluminacio",
        "Importació d'assets Unreal (Python)" => "importa",
        _ => "app",
    }
}

/// Nom en slug per a les rutes: minúscules, sense accents, amb guions.
fn slug(nom: &str) -> String {
    let n = crate::learning::tokens(nom).join("-");
    if n.is_empty() {
        "app".to_string()
    } else {
        n
    }
}

/// Nom en identificador per al codi (CamelCase, sempre lletra inicial).
fn identificador(nom: &str) -> String {
    let parts: Vec<String> = crate::learning::tokens(nom)
        .into_iter()
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(a) => a.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect();
    let id = parts.join("");
    let be = id.chars().next().map(|c| c.is_alphabetic()).unwrap_or(false);
    if !be {
        "Exemple".to_string()
    } else {
        id
    }
}

fn omple(text: &str, nom_slug: &str, nom_id: &str) -> String {
    text.replace("{nom}", nom_slug).replace("{Nom}", nom_id)
}

/// Una línia amb el catàleg, per a la reserva i per al «què pots fer».
pub fn catalog_line() -> String {
    let noms: Vec<&str> = catalog().iter().map(|s| s.title).collect();
    format!("Plantilles integrades: {}.", noms.join(", "))
}

/// Resposta MATERIALITZABLE de la nativa: format estricte «@file:» + tanca de
/// codi, exactament el que `parseFileBlocks` (fitxers) i `chatSegments`
/// (renderitzat del xat) reconeixen. L'usuari veu la targeta del fitxer amb
/// el botó per crear'l; amb l'aplicació automàtica activa, el fitxer escriu
/// sol i es mostra amb la revisió verd/roig de sempre.
pub fn starter(prompt: &str) -> Option<String> {
    // Només la PREGUNTA de l'usuari: el context del projecte que el xat
    // anteposa («PREGUNTA / TASCA:») pot contenir «blender», «render» o el
    // nom del projecte, i llavors una pregunta innocent («què fa aquest
    // projecte») dispararia una plantilla per culpa del README.
    let q = super::project::question_of(prompt);
    let s = match_snippet(&q)?;
    Some(resposta_plantilla(&s, &q))
}

/// Resposta MATERIALITZABLE d'una plantilla JA triada: la part comuna entre
/// `starter` (puntuació del catàleg) i `scaffold_joc_generic` (joc genèric
/// també quan hi ha LLM local disponible). Mateix format estricte «@file:».
fn resposta_plantilla(s: &Snippet, q: &str) -> String {
    let demanat = nom_demanat(q, s);
    let nom_slug = slug(&demanat);
    let nom_id = identificador(&demanat);
    let fitxer = omple(s.file, &nom_slug, &nom_id);
    let cos = omple(s.code, &nom_slug, &nom_id);
    // Fitxers EXTRA (motors de joc: escena + script + configuració).
    let mut extres = String::new();
    let mut n_extra = 0usize;
    for (f, lang, c) in extras(s.title) {
        let f = omple(f, &nom_slug, &nom_id);
        let c = omple(c, &nom_slug, &nom_id);
        extres.push_str(&format!("\n\n@file: {f}\n```{lang}\n{c}```"));
        n_extra += 1;
    }
    let quants = if n_extra > 0 {
        format!(" i {n_extra} fitxers més")
    } else {
        String::new()
    };
    // TEMA WORDPRESS: missatge propi — és una base sòlida i vàlida (5 fitxers
    // canònics) que el model personalitzarà quan l'usuari demani canvis.
    if s.title == "Tema WordPress (mínim)" {
        return format!(
            "Et creo una **base sòlida de tema WordPress**: els 5 fitxers canònics \
             (style.css amb el header «Theme Name:», functions.php amb l'enqueue \
             d'estils, index.php amb el loop, header.php i footer.php), mínims i \
             vàlids. És el punt de partida que el model personalitzarà quan li \
             demanis canvis (colors, plantilles, parts…). Prem «Crea al projecte» \
             per materialitzar-los.\n\n\
             @file: {fitxer}\n```{lang}\n{cos}```{extres}\n\
             Si el vols amb un altre nom, digues-ho («crea un tema de wordpress \
             que s'anomeni X»).",
            fitxer = fitxer,
            lang = s.lang,
            cos = cos,
            extres = extres,
        );
    }
    // Joc genèric resolt amb Godot: si l'usuari no ha demanat cap motor
    // concret, oferim també la Jam (planificació i progrés multi-dia).
    let jam = if s.title == "Projecte Godot 4"
        && !crate::learning::tokens(q).iter().any(|w| w == "godot" || w == "gdscript")
    {
        "\nSi el que vols és una game jam completa (planificació, tasques i progrés \
         multi-dia), prova la **Jam** de NoOrbit."
    } else {
        ""
    };
    format!(
        "Això ho sé programar: et creo **{fitxer}**{quants} ({title}). \
         Prem «Crea al projecte» a la targeta de sota per materialitzar'l.\n\n\
         @file: {fitxer}\n```{lang}\n{cos}```{extres}\n\
         Si el vols amb un altre nom, digues-ho («fes un {lower} que s'anomeni X»). \
         {catalog}{jam}",
        title = s.title,
        lower = s.title.to_lowercase(),
        lang = s.lang,
        catalog = catalog_line(),
        jam = jam,
    )
}

/// Petició de joc GENÈRIC («fes un joc», «crea un videojoc», «make a game»,
/// «un videojoc») sense cap motor ni llenguatge EXPLÍCIT: es resol amb el
/// scaffold determinista Godot 4 (els 3 fitxers canònics, ja verificat).
/// Pensat per al camí de xat amb LLM local disponible (Ollama): els models
/// petits improvisen un Pygame/Python si ningú els fixa el motor per defecte.
/// Les mencions explícites («joc en pygame», «joc en Unreal», «joc en
/// Godot»…) retornen `None`: la tria de l'usuari es respecta SEMPRE.
pub fn scaffold_joc_generic(prompt: &str) -> Option<String> {
    // Invariant dels deterministes: només la PREGUNTA darrere del marcador
    // «PREGUNTA / TASCA:», mai el context del projecte que el xat anteposa.
    let q = super::project::question_of(prompt);
    let t = q.trim_start();
    // Els informes del bucle d'eines i els recordatoris de NoOrbit no són
    // peticions de l'usuari (poden contenir «joc» per una ruta o un text
    // anterior) i no han de disparar cap scaffold.
    if t.starts_with("RUN|RESULTAT|") || t.starts_with("NoOrbit:") {
        return None;
    }
    // Una pregunta d'informació («què és un joc?», «how to make a game»)
    // no és una petició de crear res.
    if comenca_amb_pregunta(&q) {
        return None;
    }
    let low = pla(&q);
    let mots: Vec<&str> = low
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    const JOC: &[&str] = &[
        "joc", "jocs", "videojoc", "videojocs",
        "juego", "juegos", "videojuego", "videojuegos",
        "game", "games", "videogame", "videogames",
    ];
    const CREAR: &[&str] = &[
        "fes", "fer", "fem", "feu", "crea", "crear", "creem", "genera",
        "generar", "programa", "programar", "munta", "muntar", "vull",
        "necessit", "haz", "hazme", "hacer", "quiero", "necesito",
        "make", "create", "build", "generate", "write", "develop", "code",
    ];
    // Motors/llenguatges EXPLÍCITS: no s'intercepten mai (el LLM o la
    // plantilla corresponent decideix). «c#»/«c++» es miren sobre el text
    // sencer perquè el separador de mots els partiria.
    const EXPLICITS: &[&str] = &[
        "pygame", "python", "unreal", "ue5", "ue4", "bevy", "rust", "defold",
        "lua", "panda3d", "panda", "unity", "godot", "gdscript", "javascript",
        "typescript", "phaser", "threejs", "web", "html", "canvas", "consola",
        "terminal", "tkinter", "raylib", "sdl", "opengl", "love2d", "monogame",
        "csharp", "cpp", "java", "kotlin", "swift", "golang", "node", "react",
        "vue", "js", "ts",
    ];
    if !mots.iter().any(|w| JOC.contains(w)) {
        return None;
    }
    if mots.iter().any(|w| EXPLICITS.contains(w)) || low.contains("c#") || low.contains("c++") {
        return None;
    }
    // Si el catàleg determinista ja tria UNA ALTRA plantilla (p. ex.
    // l'endevinament Python per «fes un joc d'endevinar»), no la
    // sobreescriguem: la petició específica es respecta tal qual.
    if let Some(s) = match_snippet(&q) {
        if s.title != "Projecte Godot 4" {
            return None;
        }
    }
    // Verb de crear, o petició molt curta («un videojoc», «a game»): anomenar
    // un joc sense res més ja és una petició de crear-lo.
    let te_verb = mots.iter().any(|w| CREAR.contains(w));
    if !te_verb && mots.len() > 4 {
        return None;
    }
    let s = catalog().into_iter().find(|s| s.title == "Projecte Godot 4")?;
    Some(resposta_plantilla(&s, &q))
}

/// Llenguatges i formats que la nativa RECONÈIX però pels quals no té
/// plantilla escrita. Sense aquesta llista, «fes un backend en golang»
/// cauria a la reserva genèrica («tria una IA amb eines») i la resposta
/// seria inútil. Només noms de 3+ lletres: `learning::tokens` descarta els
/// mots de menys de tres caràcters (per això «golang», no «go»).
const SENSE_PLANTILLA: &[&str] = &[
    "golang", "java", "kotlin", "swift", "cpp", "csharp", "ruby", "perl", "scala",
    "haskell", "elixir", "erlang", "dart", "lua", "julia", "sql", "bash", "zsh",
    "powershell", "docker", "dockerfile", "makefile", "yaml", "assembly", "zig",
    "ocaml", "fortran", "matlab",
];

/// La petició vol programar però no encaixa amb cap plantilla: catàleg honest
/// en lloc de la reserva genèrica. Només respon si esmenta un llenguatge que
/// reconèixerem però que NO tenim escrit (una conversa sobre «crear una
/// història» no mereix un catàleg de codi).
pub fn sense_plantilla(prompt: &str) -> Option<String> {
    // Mateixa regla que a `starter`: només la PREGUNTA, mai el context.
    let q = super::project::question_of(prompt);
    let tokens = crate::learning::tokens(&q);
    if tokens.is_empty() || !te_intencio(&q) || match_snippet(&q).is_some() {
        return None;
    }
    let menciona = SENSE_PLANTILLA
        .iter()
        .any(|t| tokens.iter().any(|w| w == t));
    if !menciona {
        return None;
    }
    let llista = catalog_line()
        .replace("Plantilles integrades: ", "")
        .trim_end_matches('.')
        .to_string();
    Some(format!(
        "Aquest llenguatge el comprenc, però no tinc cap plantilla escrita per a ell. \
         De soca-rel sí que sé programar: {}. Dig-me'n una i te la creo com a fitxer real \
         al projecte. Si el que necessites va més lluny (un programa sencer des de zero en \
         {}), demana-ho a una IA amb eines: jo treballo amb el catàleg integrat i amb el \
         que hagi après dels llocs lliures (StackOverflow i companyia).",
        llista,
        tokens
            .iter()
            .find(|w| SENSE_PLANTILLA.contains(&w.as_str()))
            .cloned()
            .unwrap_or_else(|| "aquest llenguatge".to_string())
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plantilles_no_son_buides() {
        for s in catalog() {
            assert!(s.code.chars().count() > 120, "{}: buida", s.title);
            assert!(!s.triggers.is_empty() || !s.strong.is_empty());
            assert!(!s.file.is_empty());
            // Cap marcador sense emplenar després del `omple` (cos i extres).
            let omplert = omple(s.code, "exemple", "Exemple");
            assert!(!omplert.contains("{nom}"), "{}: queda {{nom}}", s.title);
            assert!(!omplert.contains("{Nom}"), "{}: queda {{Nom}}", s.title);
            for (f, _, c) in extras(s.title) {
                let fc = omple(f, "exemple", "Exemple");
                assert!(!fc.contains("{nom}"), "{}: extra {{nom}} a {f}", s.title);
                assert!(!fc.contains("{Nom}"), "{}: extra {{Nom}} a {f}", s.title);
                let cc = omple(c, "exemple", "Exemple");
                assert!(!cc.contains("{nom}"), "{}: extra {{nom}} al codi de {f}", s.title);
                assert!(!cc.contains("{Nom}"), "{}: extra {{Nom}} al codi de {f}", s.title);
            }
        }
    }

    #[test]
    fn motors_de_joc_emeten_projectes_complets() {
        // Godot: tres fitxers i el renderer «Compatibility» per a Mac Intel.
        let r = starter("fes un joc en godot").unwrap();
        assert!(r.contains("@file: project.godot"), "{}", r);
        assert!(r.contains("@file: main.tscn"), "{}", r);
        assert!(r.contains("@file: main.gd"), "{}", r);
        assert!(r.contains("gl_compatibility"), "{}", r);
        // Defold: configuració + col·lecció + script Lua.
        let r = starter("crea un joc amb defold").unwrap();
        assert!(r.contains("@file: game.project"), "{}", r);
        assert!(r.contains("@file: main.collection"), "{}", r);
        assert!(r.contains("@file: main.script"), "{}", r);
        // Bevy: Cargo.toml + src/main.rs.
        let r = starter("vull un joc bevy").unwrap();
        assert!(r.contains("@file: Cargo.toml"), "{}", r);
        assert!(r.contains("@file: src/main.rs"), "{}", r);
        // Panda3D: un sol fitxer Python.
        let r = starter("fes una escena panda3d").unwrap();
        assert!(r.contains("@file: joc.py"), "{}", r);
        // El nom explícit es respecta també als motors.
        let r = starter("fes un joc en godot que s'anomeni Salt").unwrap();
        assert!(r.contains("config/name=\"Salt\""), "{}", r);
    }

    #[test]
    fn blender_des_de_la_nativa() {
        // Escena: bpy + EEVEE (el render lleuger per a Mac Intel).
        let r = starter("fes una escena en blender").unwrap();
        assert!(r.contains("@file: blender/escena.py"), "{}", r);
        assert!(r.contains("import bpy"), "{}", r);
        assert!(r.contains("EEVEE"), "{}", r);
        // Animació: keyframes reals, no un simple print.
        let r = starter("crea una animacio en blender").unwrap();
        assert!(r.contains("@file: blender/animacio_"), "{}", r);
        assert!(r.contains("keyframe_insert"), "{}", r);
        // Material procedural amb nodes.
        let r = starter("crea un material en blender").unwrap();
        assert!(r.contains("@file: blender/material_"), "{}", r);
        assert!(r.contains("ShaderNodeTexNoise"), "{}", r);
        // Export glTF per a motors: guanya l'empat amb l'escena (és primer).
        let r = starter("exporta de blender a godot un asset que s'anomeni Nau").unwrap();
        assert!(r.contains("@file: blender/export_nau.py"), "{}", r);
        assert!(r.contains("export_scene.gltf"), "{}", r);
    }

    #[test]
    fn unreal_lleuger_des_de_la_nativa() {
        // Projecte: .uproject + configuració sense Lumen/Nanite + Python inicial.
        let r = starter("fes un joc en unreal").unwrap();
        assert!(r.contains("@file: joc.uproject"), "{}", r);
        assert!(r.contains("@file: Config/DefaultEngine.ini"), "{}", r);
        assert!(r.contains("@file: Content/Python/inici.py"), "{}", r);
        assert!(r.contains("PythonScriptPlugin"), "{}", r);
        assert!(r.contains("r.DynamicGlobalIlluminationMethod=0"), "{}", r);
        assert!(r.contains("r.Shadow.Virtual.Enable=0"), "{}", r);
        // Script: Python via subsistema modern, MAI EditorLevelLibrary (deprecat).
        let r = starter("crea un script python per unreal").unwrap();
        assert!(r.contains("@file: unreal/joc.py"), "{}", r);
        assert!(r.contains("EditorActorSubsystem"), "{}", r);
        assert!(!r.contains("EditorLevelLibrary."), "{}", r);
        // Regressió: Godot continua guanyant les seves peticions.
        let r = starter("fes un joc en godot").unwrap();
        assert!(r.contains("@file: project.godot"), "{}", r);
    }

    #[test]
    fn detecta_peticions_de_programar() {
        assert!(match_snippet("fes un component React").is_some());
        assert!(match_snippet("crea una pàgina html").is_some());
        assert!(match_snippet("genera un servidor python").is_some());
        assert!(match_snippet("vull un joc en python").is_some());
        assert!(match_snippet("escriu un programa en rust").is_some());
        assert!(match_snippet("crea un full css").is_some());
    }

    #[test]
    fn no_segresta_converses() {
        assert!(match_snippet("hola com estàs").is_none());
        assert!(match_snippet("calcula 2+2").is_none());
        assert!(match_snippet("què és NoOrbit").is_none());
        // Intenció de crear, però cap paraula de plantilla.
        assert!(match_snippet("crea un projecte nou de cero").is_none());
        // Parla del llenguatge, no demana crear res.
        assert!(match_snippet("què és rust").is_none());
    }

    #[test]
    fn preguntes_no_disparen_plantilles() {
        // Una pregunta sobre el projecte no pot crear fitxers, ni que el
        // context del xat continga «blender», «render» o el nom del projecte:
        // només es mira el text darrere de «PREGUNTA / TASCA:».
        let amb_context = "Context: carpeta blender/, render EEVEE i NoOrbit.\n\n---\nPREGUNTA / TASCA:\nquè fa aquest projecte";
        assert!(starter(amb_context).is_none());
        // Ni una pregunta directa sobre una tecnologia de plantilla.
        assert!(match_snippet("què fa un render en blender").is_none());
        assert!(match_snippet("què fa aquest projecte").is_none());
        // En canvi, l'ordre de crear sí que dispara, també amb context.
        let ordre = "Context: projecte amb godot.\n\n---\nPREGUNTA / TASCA:\nfes una escena en blender";
        let r = starter(ordre).unwrap();
        assert!(r.contains("@file: blender/escena.py"), "{}", r);
        // Les peticions educades («com puc fer…») continuen sent ordres.
        assert!(match_snippet("com puc fer un servidor en python").is_some());
        assert!(match_snippet("com puc crear una animacio en blender").is_some());
    }

    #[test]
    fn emet_format_materialitzable() {
        let r = starter("fes un component React que s'anomeni Cart").unwrap();
        assert!(r.contains("@file: src/Cart.tsx"), "{}", r);
        assert!(r.contains("```tsx"), "{}", r);
        assert!(r.contains("export default function Cart"), "{}", r);
        // La tanca ha d'estar tancada (2 marcadors).
        assert_eq!(r.matches("```").count(), 2, "{}", r);
    }

    #[test]
    fn nom_explicite_vs_arrencada() {
        let amb_nom = starter("crea una pàgina html anomenada botiga").unwrap();
        assert!(amb_nom.contains("@file: botiga.html"), "{}", amb_nom);
        let sense = starter("fes una pàgina html").unwrap();
        assert!(sense.contains("@file: index.html"), "{}", sense);
    }

    #[test]
    fn peticio_sense_plantilla_respon_catalog() {
        let r = sense_plantilla("fes un backend en golang").unwrap();
        assert!(r.contains("Component React"), "{}", r);
        // «golang» no té plantilla, però sí que és un llenguatge reconegut.
        assert!(r.contains("golang"), "{}", r);
        assert!(sense_plantilla("fes un resum del projecte").is_none());
        // Amb plantilla: no cal el catàleg de reserva.
        assert!(sense_plantilla("fes un component React").is_none());
    }

    #[test]
    fn codi_generat_no_deixa_marcadors() {
        // Un bug clàssic: queda «{Nom}» al fitxer creat i el codi no compila.
        for (prompt, marcador) in [
            ("fes un component React", "{Nom}"),
            ("crea una pàgina html", "{nom}"),
            ("genera un servidor python", "{Nom}"),
            ("crea un programa rust", "{nom}"),
        ] {
            let r = starter(prompt).unwrap_or_default();
            assert!(!r.contains(marcador), "{}: queda {} al codi", prompt, marcador);
        }
    }

    /// Extreu els blocs «@file: ruta» + tanca de codi d'una resposta (tests).
    fn blocs_file(text: &str) -> Vec<(String, String)> {
        let mut out = Vec::new();
        let mut rest = text;
        while let Some(i) = rest.find("@file: ") {
            let after = &rest[i + "@file: ".len()..];
            let path = after.lines().next().unwrap_or("").trim().to_string();
            let Some(a) = after.find("```") else { break };
            let cos = &after[a + 3..];
            let Some(nl) = cos.find('\n') else { break };
            let cos = &cos[nl + 1..];
            let Some(b) = cos.find("```") else { break };
            out.push((path, cos[..b].to_string()));
            rest = &cos[b + 3..];
        }
        out
    }

    #[test]
    fn tema_wordpress_routing_inequivoque() {
        for p in [
            "crea un tema de wordpress",
            "crear tema wordpress",
            "fes un tema wp",
            "wordpress theme",
            "vull un tema de WordPress",
            "genera un tema wordpress",
        ] {
            let s = match_snippet(p).unwrap_or_else(|| panic!("cap plantilla per a «{}»", p));
            assert_eq!(s.title, "Tema WordPress (mínim)", "{}", p);
        }
        let r = starter("crea un tema de wordpress").unwrap();
        for f in ["style.css", "functions.php", "index.php", "header.php", "footer.php"] {
            assert!(r.contains(&format!("@file: wp-content/themes/tema/{}", f)), "{}", r);
        }
        assert!(r.contains("Theme Name:"), "{}", r);
        assert!(r.contains("base sòlida"), "{}", r);
        // NO la pàgina PHP genèrica (l'antic segrest «wordpress» → taula PHP).
        assert!(!r.contains("pàgina PHP"), "{}", r);
        assert!(!r.contains("Serveix-la amb"), "{}", r);
        // Nom derivat de la petició quan és clar.
        let r = starter("crea un tema de wordpress que s'anomeni Botiga").unwrap();
        assert!(r.contains("wp-content/themes/botiga/style.css"), "{}", r);
        assert!(r.contains("Theme Name: Botiga"), "{}", r);
        // Preguntes i peticions que NO són un tema: cap segrest cap al tema.
        assert!(match_snippet("què és un tema de wordpress").is_none());
        assert!(match_snippet("crea un plugin de wordpress")
            .map(|s| s.title != "Tema WordPress (mínim)")
            .unwrap_or(true));
    }

    #[test]
    fn plantilla_wordpress_php_valid() {
        // Els 5 fitxers canònics es materialitzen en un tempdir i els 4 PHP
        // han de passar `php -l` (PHP real de la màquina; si no n'hi ha,
        // s'omet la verificació de sintaxi i es comprova l'estructura).
        let r = starter("crea un tema de wordpress").unwrap();
        let blocs = blocs_file(&r);
        assert_eq!(blocs.len(), 5, "blocs: {:?}", blocs.iter().map(|(p, _)| p).collect::<Vec<_>>());
        for (_, c) in &blocs {
            assert!(!c.contains("{nom}") && !c.contains("{Nom}"), "marcador sense emplenar");
        }
        let css = blocs.iter().find(|(p, _)| p.ends_with("style.css")).unwrap();
        assert!(css.1.contains("Theme Name:"));
        let func = blocs.iter().find(|(p, _)| p.ends_with("functions.php")).unwrap();
        assert!(func.1.contains("wp_enqueue_scripts") && func.1.contains("wp_enqueue_style"));
        let index = blocs.iter().find(|(p, _)| p.ends_with("index.php")).unwrap();
        assert!(index.1.contains("get_header") && index.1.contains("get_footer") && index.1.contains("have_posts"));
        let header = blocs.iter().find(|(p, _)| p.ends_with("header.php")).unwrap();
        assert!(header.1.contains("wp_head") && header.1.contains("language_attributes"));
        let footer = blocs.iter().find(|(p, _)| p.ends_with("footer.php")).unwrap();
        assert!(footer.1.contains("wp_footer"));

        let php = if std::path::Path::new("/usr/local/bin/php").exists() {
            "/usr/local/bin/php".to_string()
        } else {
            "php".to_string()
        };
        let dir = std::env::temp_dir().join(format!("noorbit-wp-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut php_files = 0;
        for (path, code) in &blocs {
            let dest = dir.join(path);
            std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
            std::fs::write(&dest, code).unwrap();
            if path.ends_with(".php") {
                php_files += 1;
                let out = match std::process::Command::new(&php).arg("-l").arg(&dest).output() {
                    Ok(o) => o,
                    Err(e) => {
                        eprintln!("php -l no disponible ({}): s'omet la verificació de sintaxi", e);
                        let _ = std::fs::remove_dir_all(&dir);
                        return;
                    }
                };
                assert!(
                    out.status.success(),
                    "php -l falla a {}: {}",
                    path,
                    String::from_utf8_lossy(&out.stderr)
                );
            }
        }
        assert_eq!(php_files, 4);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn funcions_wp_prefixades_per_tema_sense_xoc_entre_dos_temes() {
        // Dos temes generats amb noms diferents NO comparteixen cap nom de
        // funció global: poden conviure a la mateixa instal·lació sense
        // «Cannot redeclare» (abans el prefix noorbit_tema_* era fix).
        let a = starter("crea un tema de wordpress anomenat Orbita").unwrap();
        let b = starter("crea un tema de wordpress anomenat Farola").unwrap();
        let funcions = |r: &str| -> Vec<String> {
            blocs_file(r)
                .into_iter()
                .find(|(p, _)| p.ends_with("functions.php"))
                .expect("functions.php")
                .1
                .lines()
                .filter_map(|l| l.trim().strip_prefix("function "))
                .map(|l| l.trim_end_matches("() {").trim().to_string())
                .collect()
        };
        let fa = funcions(&a);
        let fb = funcions(&b);
        assert!(fa.contains(&"Orbita_encua_estils".to_string()), "{fa:?}");
        assert!(fa.contains(&"Orbita_suports".to_string()), "{fa:?}");
        assert!(fb.contains(&"Farola_encua_estils".to_string()), "{fb:?}");
        assert!(fa.iter().all(|f| !fb.contains(f)), "xoc de noms: {fa:?} vs {fb:?}");
    }

    /// «Fes un joc» genèric TAMBÉ amb LLM local disponible (camí Ollama):
    /// el scaffold determinista Godot 4 resol la petició en català, castellà
    /// i anglès, sense passar per l'endevinament Python.
    #[test]
    fn joc_generic_amb_llm_local_resol_el_scaffold_godot() {
        for p in [
            "fes un joc", "crea un joc", "make a game", "un videojoc",
            "crea un videojoc", "haz un juego", "fes un videojoc",
            "create a game", "vull un joc", "fes un joc 2d",
        ] {
            let r = scaffold_joc_generic(p)
                .unwrap_or_else(|| panic!("cap scaffold per a «{}»", p));
            assert!(r.contains("@file: project.godot"), "{}: {}", p, r);
            assert!(r.contains("@file: main.tscn"), "{}", p);
            assert!(r.contains("@file: main.gd"), "{}", p);
            assert!(r.contains("gl_compatibility"), "{}", p);
            assert!(!r.contains("random.randint"), "{}: Pygame no és el defecte", p);
        }
    }

    /// Les mencions EXPLÍCITES de motor/llenguatge no s'intercepten MAI:
    /// el camí local (LLM o plantilla) rep la petició tal qual.
    #[test]
    fn joc_explicit_no_s_intercepta_mai() {
        for p in [
            "fes un joc en pygame", "fes un joc en python",
            "crea un joc en Unreal", "fes un joc en Bevy",
            "fes un joc en Godot", "make a game in python",
            "fes un joc web", "haz un juego en pygame",
            "fes un joc de consola", "crea un joc en C#",
            "fes un joc d'endevinar",
        ] {
            assert!(scaffold_joc_generic(p).is_none(), "interceptat: «{}»", p);
        }
        // El determinista de tota la vida continua respectant el Python
        // explícit (endevinament) i el Godot explícit.
        let r = starter("fes un joc en python").unwrap();
        assert!(r.contains("random.randint"), "{}", r);
        let r = starter("fes un joc en godot").unwrap();
        assert!(r.contains("@file: project.godot"), "{}", r);
    }

    /// El scaffold del camí amb LLM respecta l'invariant del marcador
    /// «PREGUNTA / TASCA:» i no dispara amb preguntes, informes ni nudges.
    #[test]
    fn joc_generic_nomes_la_pregunta_real() {
        // Preguntes d'informació: no són peticions de crear.
        assert!(scaffold_joc_generic("què és un joc?").is_none());
        assert!(scaffold_joc_generic("how to make a game").is_none());
        // Context amb «pygame/python» davant del marcador: només mana la
        // pregunta de darrere.
        let prompt = format!(
            "{}\n\n---\nPREGUNTA / TASCA:\nfes un joc",
            "pygame python ".repeat(400)
        );
        assert!(scaffold_joc_generic(&prompt).is_some());
        // A l'inrevés: el context parla de joc però la pregunta és innocent.
        let invers = format!(
            "{}\n\n---\nPREGUNTA / TASCA:\nquè fa aquest projecte?",
            "joc godot ".repeat(400)
        );
        assert!(scaffold_joc_generic(&invers).is_none());
        // Informes del bucle d'eines i recordatoris: no són l'usuari.
        assert!(scaffold_joc_generic("RUN|RESULTAT| fitxers escrits: joc/main.py (crea)").is_none());
        assert!(scaffold_joc_generic("NoOrbit: l'usuari ha demanat crear un joc…").is_none());
    }

    #[test]
    fn joc_generic_godot_i_endevinament_nomes_explicit() {
        // «fes un joc» genèric → Godot 4 REAL, MAI l'endevinament Python.
        for p in ["fes un joc", "crea un joc", "vull un videojoc", "fes un joc 2d"] {
            let r = starter(p).unwrap_or_else(|| panic!("cap plantilla per a «{}»", p));
            assert!(r.contains("@file: project.godot"), "{}: {}", p, r);
            assert!(r.contains("@file: main.tscn"), "{}", p);
            assert!(r.contains("@file: main.gd"), "{}", p);
            assert!(!r.contains("random.randint"), "{}: l'endevinament no és el defecte", p);
        }
        // La Jam s'ofereix quan no s'ha demanat cap motor concret.
        assert!(starter("fes un joc").unwrap().contains("Jam"));
        assert!(!starter("fes un joc en godot").unwrap().contains("**Jam**"));
        // Endevinament NOMÉS quan es demana explícitament.
        let r = starter("fes un joc d'endevinar").unwrap();
        assert!(r.contains("random.randint"), "{}", r);
        let r = starter("vull un joc en python").unwrap();
        assert!(r.contains("random.randint"), "{}", r);
        // Els motors explícits continuen guanyant les seves peticions.
        assert!(starter("crea un joc amb defold").unwrap().contains("@file: game.project"));
        assert!(starter("vull un joc bevy").unwrap().contains("@file: Cargo.toml"));
        assert!(starter("fes un joc en unreal").unwrap().contains(".uproject"));
        assert!(starter("fes un joc en panda3d").unwrap().contains("ShowBase"));
    }

    #[test]
    fn pagina_php_senzilla_es_el_snippet_php() {
        let r = starter("fes una pàgina php").unwrap();
        assert!(r.contains("```php"), "{}", r);
        assert!(r.contains("htmlspecialchars"), "{}", r);
        assert!(r.contains(".php"), "{}", r);
        // I l'HTML continua guanyant quan es demana html.
        let r = starter("crea una pàgina html").unwrap();
        assert!(r.contains("```html"), "{}", r);
    }

    /// Els patrons Unreal ESPECÍFICS (escena, il·luminació, importació)
    /// guanyen al patró genèric quan la petició és concreta, i el `.uproject`
    /// continua guanyant quan es demana un «projecte» o un «joc».
    #[test]
    fn patrons_unreal_especifics_guanyen_el_generic() {
        let r = starter("fes una escena unreal").unwrap();
        assert!(r.contains("@file: unreal/escena_"), "{}", r);
        assert!(r.contains("set_static_mesh"), "{}", r);

        // «importació» i «il·luminació» porten accent: els tokens es
        // normalitzen, i el «·» fa de separador («il·luminació» → «luminacio»).
        let r = starter("fes una importació d'un fbx a unreal").unwrap();
        assert!(r.contains("@file: unreal/importa_"), "{}", r);
        assert!(r.contains("import_assets_automated"), "{}", r);

        let r = starter("fes la il·luminació de l'escena d'unreal").unwrap();
        assert!(r.contains("@file: unreal/iluminacio_"), "{}", r);
        assert!(r.contains("build_light_maps"), "{}", r);

        // «projecte» i «joc» segueixen guanyant el .uproject.
        assert!(starter("fes un joc en unreal").unwrap().contains(".uproject"));
        assert!(starter("crea un projecte unreal").unwrap().contains(".uproject"));
        // El patró genèric es queda per a «script» i «actor».
        assert!(starter("fes un script unreal").unwrap().contains("@file: unreal/"));

        // Cap patró pot usar les APIs deprecades (fallen en silenci).
        for p in [
            "fes una escena unreal",
            "fes una importació d'un fbx a unreal",
            "fes la il·luminació de l'escena d'unreal",
        ] {
            let r = starter(p).unwrap();
            assert!(!r.contains("EditorLevelLibrary"), "{}: {}", p, r);
            assert!(!r.contains("EditorAssetLibrary"), "{}: {}", p, r);
        }
    }
}
