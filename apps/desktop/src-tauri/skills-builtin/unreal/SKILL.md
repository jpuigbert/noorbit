---
name: Unreal Engine 5 (Python)
description: Crea i edita escenes a Unreal Engine 5 amb scripts Python provats — actors, llums, text 3D, materials, importació d'assets i construcció procedural.
---

# Com programar per Unreal Engine 5 amb NoOrbit

Quan es demani alguna cosa d'Unreal (UE5), escriu scripts Python amb **unreal**
que s'apliquin de veritat dins l'editor. Usa aquests codis similars com a base
i adapta noms, mides i colors.

**Plugins que calen** (NoOrbit els activa sol al projecte):
`PythonScriptPlugin` + `RemoteControl` (el MCP oficial només en UE 5.8+).

## 0. Els dos subsistemes que s'usen SEMPRE

```python
import unreal

actors = unreal.get_editor_subsystem(unreal.EditorActorSubsystem)   # actors
level = unreal.get_editor_subsystem(unreal.LevelEditorSubsystem)    # nivell
assets = unreal.get_editor_subsystem(unreal.EditorAssetSubsystem)   # assets
```

**PROHIBIT** `unreal.EditorLevelLibrary` i `unreal.EditorAssetLibrary`: estan
deprecades des de UE 5.0 i per la Remote Control API **no fan res en silenci**
(l'editor respon èxit igual). Usa sempre els subsistemes de dalt.

## 1. Crear un actor amb malla

```python
import unreal

actors = unreal.get_editor_subsystem(unreal.EditorActorSubsystem)

mesh = unreal.load_object(None, "/Engine/BasicShapes/Cube.Cube")
if mesh is None:
    raise RuntimeError("No s'ha trobat la malla del motor")

actor = actors.spawn_actor_from_class(unreal.StaticMeshActor, unreal.Vector(0, 0, 100))
if actor is None:
    raise RuntimeError("L'editor no ha creat l'actor")
actor.set_actor_label("NoOrbit_Cub")
actor.static_mesh_component.set_static_mesh(mesh)
actor.static_mesh_component.set_material(
    0, unreal.load_object(None, "/Engine/BasicShapes/BasicShapeMaterial.BasicShapeMaterial")
)
print("Creat NoOrbit_Cub")
```

## 2. Moure, girar i escalar un actor

```python
import unreal

actors = unreal.get_editor_subsystem(unreal.EditorActorSubsystem)

for a in actors.get_all_level_actors():
    if a.get_actor_label() == "NoOrbit_Cub":
        a.set_actor_location(unreal.Vector(200, 0, 100))
        a.set_actor_rotation(unreal.Rotator(0, 45, 0))
        a.set_actor_scale3d(unreal.Vector(2, 2, 2))
        print("Mogut NoOrbit_Cub")
        break
```

`unreal.Rotator(pitch, yaw, roll)` és l'angle en graus; `unreal.Rotation` és el
quaternió i no el que espera `set_actor_rotation`.

## 3. Llums (direccional, puntual, focus)

```python
import unreal

actors = unreal.get_editor_subsystem(unreal.EditorActorSubsystem)

sol = actors.spawn_actor_from_class(unreal.DirectionalLight, unreal.Vector(0, 0, 500))
sol.set_actor_label("NoOrbit_Sol")
sol.set_actor_rotation(unreal.Rotator(-45, 30, 0))

foc = actors.spawn_actor_from_class(unreal.PointLight, unreal.Vector(200, 0, 150))
foc.set_actor_label("NoOrbit_Foc")
foc.light_component.set_editor_property("light_color", unreal.LinearColor(1.0, 0.5, 0.0, 1.0))
foc.light_component.set_editor_property("intensity", 5000.0)

print("Creades llum solar i llum puntual")
```

## 4. Text visible al nivell (TextRenderActor)

```python
import unreal

actors = unreal.get_editor_subsystem(unreal.EditorActorSubsystem)

t = actors.spawn_actor_from_class(unreal.TextRenderActor, unreal.Vector(0, 0, 200))
t.set_actor_label("NoOrbit_Text")
t.text_render.set_editor_property("text", "Hola món")
t.text_render.set_editor_property("scale", unreal.Vector(3, 3, 3))
t.text_render.set_editor_property("color", unreal.LinearColor(1.0, 0.9, 0.2, 1.0))
print("Creat text 3D")
```

Un UWidget de HUD **no** es pot muntar sencer des de script: usa TextRenderActor.

## 5. Importar un model 3D (FBX, GLB, OBJ)

```python
import os
import unreal

src = os.path.abspath(r"/ruta/absoluta/al/model.fbx")
if not os.path.isfile(src):
    raise RuntimeError("No existeix el fitxer " + src)

dest = "/Game/Imports"
assets = unreal.get_editor_subsystem(unreal.EditorAssetSubsystem)
before = set(assets.list_assets(dest, True, True))

data = unreal.AutomatedAssetImportData()
data.set_editor_property("filenames", [src])
data.set_editor_property("destination_path", dest)
data.set_editor_property("replace_existing", True)
data.set_editor_property("group_with_root", False)

imported = list(unreal.AssetToolsHelpers.get_asset_tools().import_assets_automated(data) or [])

noms = [a.get_path_name() for a in imported if a is not None]
if not noms:
    # Reserva: comparem el contingut de la carpeta abans i després.
    after = set(assets.list_assets(dest, True, True))
    noms = [str(p.get_asset_path()) for p in after - before]
if not noms:
    raise RuntimeError("Unreal no ha importat res (format no suportat o fitxer invalid)")
print("Importats: " + ", ".join(noms))
```

`import_assets_automated` **retorna els assets importats**: és la via fiable.
`import_asset_tasks` retorna `None`, això sí que es pot verificar comparant
`list_assets` abans i després de la importació.

## 6. Crear un Blueprint

```python
import unreal

parent = unreal.load_class(None, "/Script/Engine.Actor")
if parent is None:
    raise RuntimeError("No s'ha trobat la classe /Script/Engine.Actor")

factory = unreal.BlueprintFactory()
factory.set_editor_property("ParentClass", parent)

bp = unreal.AssetToolsHelpers.get_asset_tools().create_asset(
    "BP_NoOrbit_Interactuable", "/Game/Blueprints", None, factory
)
if bp is None:
    raise RuntimeError("L'editor no ha pogut crear el Blueprint")

unreal.get_editor_subsystem(unreal.EditorAssetSubsystem).save_loaded_asset(bp)
print("Blueprint creat: " + bp.get_path_name())
```

El **grafo** d'un Blueprint no es pot editar des de Python (cal compilar C++/BP).

## 7. Material nou i color

```python
import unreal

assets = unreal.get_editor_subsystem(unreal.EditorAssetSubsystem)

factory = unreal.MaterialFactoryNew()
mat = unreal.AssetToolsHelpers.get_asset_tools().create_asset(
    "M_NoOrbit_Color", "/Game/Materials", unreal.Material, factory
)
if mat is None:
    raise RuntimeError("L'editor no ha pogut crear el material")

node = unreal.MaterialEditingLibrary.create_material_expression(
    mat, unreal.MaterialExpressionConstant3Vector, -200, 0
)
node.set_editor_property("constant", unreal.LinearColor(0.8, 0.2, 0.1, 1.0))
unreal.MaterialEditingLibrary.connect_material_property(
    node, "", unreal.MaterialProperty.MP_BASE_COLOR
)
unreal.MaterialEditingLibrary.recompile_material(mat)
assets.save_loaded_asset(mat)
print("Material M_NoOrbit_Color creat")
```

## 8. Reixilla procedural d'actors

```python
import unreal
import random

actors = unreal.get_editor_subsystem(unreal.EditorActorSubsystem)
mesh = unreal.load_object(None, "/Engine/BasicShapes/Cube.Cube")

files, columnes, espai = 5, 8, 200.0
for f in range(files):
    for c in range(columnes):
        x = c * espai - (columnes - 1) * espai / 2
        y = f * espai - (files - 1) * espai / 2
        a = actors.spawn_actor_from_class(unreal.StaticMeshActor, unreal.Vector(x, y, 50))
        a.set_actor_label("NoOrbit_Grid_%d_%d" % (f, c))
        a.static_mesh_component.set_static_mesh(mesh)
        s = random.uniform(0.5, 1.5)
        a.set_actor_scale3d(unreal.Vector(s, s, s))
print("Reixilla %dx%d creada" % (files, columnes))
```

## 9. Il·luminació construïda i nivell desat

```python
import unreal

quality = getattr(unreal.LightingBuildQuality, "QUALITY_PRODUCTION", None)
if quality is None:
    raise RuntimeError("Aquesta versió d'Unreal no coneix LightingBuildQuality")

level = unreal.get_editor_subsystem(unreal.LevelEditorSubsystem)
if not level.build_light_maps(quality, False):
    raise RuntimeError(
        "Unreal ha refusat construir els lightmaps. Si el projecte fa servir "
        "Lumen (il·luminació dinàmica) no cal construir-los: activa'l a "
        "Window > Platform > Lighting."
    )
level.save_current_level()
print("Il·luminació construïda i nivell desat")
```

## 10. Llistar el que hi ha (ABANS de moure res)

```python
import unreal

actors = unreal.get_editor_subsystem(unreal.EditorActorSubsystem)
tot = actors.get_all_level_actors()
print("Actors al nivell: %d" % len(tot))
for a in tot:
    print("  " + a.get_actor_label())

assets = unreal.get_editor_subsystem(unreal.EditorAssetSubsystem)
llista = assets.list_assets("/Game", True, True)
print("Assets a /Game: %d" % len(llista))
for p in llista[:20]:
    print("  " + str(p))
```

**Rutes reals, no endevinades**: per moure o duplicar un asset del projecte
(`/Game/Personatge/BP_Jugador`) primer llistat el que hi ha i fes servir la ruta
que has vist. Endevinar-la dona un error silenciós.

## 11. Esborrar un actor pel nom

```python
import unreal

actors = unreal.get_editor_subsystem(unreal.EditorActorSubsystem)
for a in actors.get_all_level_actors():
    if a.get_actor_label() == "NoOrbit_Temporal":
        actors.destroy_actor(a)
        print("Esborrat NoOrbit_Temporal")
        break
```

## Regles d'or

- Termina **sempre** amb un `print()` en català que digui què has fet: la resposta
  HTTP de la Remote Control API **no** conté la sortida; NoOrbit la llegeix del
  fitxer segellat que l'script escriu.
- Si `unreal.load_object(...)` o `load_class(...)` tornen `None`, **llança
  `RuntimeError`**: així l'error arriba al model i es pot corregir.
- No facis servir `execute_console_command` per il·luminació: usa
  `LevelEditorSubsystem.build_light_maps`.
- Les operacions llargues (build d'il·luminació, importació massiva) trigaran
  minuts: NoOrbit espera la resposta de l'editor.
- UN sol bloc ```python, sense prosa fora.
- Si una cosa **no** es pot fer des de script, digues-ho en una línia que
  comenci per «NOORBIT_IMPOSSIBLE: ».

## Fonts

Patrons curats a partir de:
- https://dev.epicgames.com/community/unreal-engine/snippets (comunitat d'Epic)
- https://dev.epicgames.com/documentation/unreal-engine/scripting-the-unreal-editor-using-python

Verificats contra l'API Python d'UE 5 (subsistemes moderns) i contrastats amb
els scripts que NoOrbit ja executa a l'editor pel pont Remote Control.
