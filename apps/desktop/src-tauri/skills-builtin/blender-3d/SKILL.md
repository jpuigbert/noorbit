---
name: Blender 3D (bpy)
description: Creat i edita escenes 3D a Blender — scripts bpy provats per crear objectes, materials, llums, càmeres, animació i modificadors.
---

# Com programar per Blender amb NoOrbit

Quan es demani 3D, escriu scripts Python amb **bpy** que s'apliquin de veritat.
Usa aquests codis similars com a base i adapta noms, mides i colors.

1. **Crear una forma**:
   ```python
   import bpy
   bpy.ops.mesh.primitive_uv_sphere_add(radius=1.5, location=(0, 0, 0))
   ob = bpy.context.view_layer.objects.active; ob.name = "NoOrbit_Esfera"
   print("Creat NoOrbit_Esfera")
   ```
2. **Material amb color**:
   ```python
   import bpy
   mat = bpy.data.materials.new("NoOrbit_Mat"); mat.use_nodes = True
   bsdf = mat.node_tree.nodes["Principled BSDF"]
   bsdf.inputs["Base Color"].default_value = (0.8, 0.2, 0.1, 1.0)
   bsdf.inputs["Metallic"].default_value = 0.6
   ob = bpy.context.view_layer.objects.active
   ob.material_slots.clear(); ob.material_slot_add()
   ob.material_slots[0].material = mat
   ```
3. **Editar un objecte JA existent** (moure/escalar/girar):
   ```python
   import bpy
   if "NoOrbit_Cub" in bpy.data.objects:
       ob = bpy.data.objects["NoOrbit_Cub"]
       ob.location = (1.0, 0.0, 0.5)
       ob.scale = (1.5, 1.5, 1.0)
       ob.rotation_euler = (0.0, 0.0, 0.3)
       print("Deformat NoOrbit_Cub")
   ```
4. **Llum i càmera**:
   ```python
   import bpy
   bpy.ops.object.light_add(type='SUN', location=(5, -5, 10))
   lam = bpy.context.view_layer.objects.active; lam.data.energy = 3.0
   bpy.ops.object.camera_add(location=(7, -6, 5), rotation=(1.1, 0.0, 0.8))
   bpy.context.scene.camera = bpy.context.view_layer.objects.active
   print("Afegida llum i càmera")
   ```
5. **Animar** (dues keyframes de `location`):
   ```python
   import bpy
   ob = bpy.data.objects["NoOrbit_Esfera"]
   ob.location = (0, 0, 0); ob.keyframe_insert(data_path="location", frame=1)
   ob.location = (0, 0, 3); ob.keyframe_insert(data_path="location", frame=60)
   print("Animat l'esfera (1..60)")
   ```
6. **Esborrar per nom** (només si se't demana):
   ```python
   import bpy
   if "NoOrbit_Esfera" in bpy.data.objects:
       bpy.data.objects.remove(bpy.data.objects["NoOrbit_Esfera"], do_unlink=True)
       print("Esborrat NoOrbit_Esfera")
   ```
7. **Modificadors** (bevel + subdivision):
   ```python
   import bpy
   ob = bpy.data.objects["NoOrbit_Cub"]
   if "Bevel" not in ob.modifiers:
       ob.modifiers.new("Bevel", "BEVEL"); ob.modifiers["Bevel"].width = 0.05
       ob.modifiers.new("Subsurf", "SUBSURF"); ob.modifiers["Subsurf"].levels = 2
   print("Aplicats modificadors a NoOrbit_Cub")
   ```

**Regles d'or**:
- Després d'un operator, agafa l'objecte amb `bpy.context.view_layer.objects.active`.
- Comprova sempre que noms existeixen (`if "Nom" in bpy.data.objects`) abans d'usar-los.
- Termina sempre amb un `print()` en català que digui què has fet.
- UN sol bloc ```python, sense text fora.
