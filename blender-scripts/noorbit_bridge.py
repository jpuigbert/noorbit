# ==============================================================================
# NoOrbit Bridge — add-on / script servidor per a Blender
# ------------------------------------------------------------------------------
# Obre un servidor TCP a 127.0.0.1:9876 que parla el protocol JSON de NOORBIT:
#   -> {"key":"noorbit","command":"execute","argument":{"code":"..."}}
#   <- {"exec":"<sortida capturada>","final":true}
#
# Es pot usar de dues maneres:
#   1) Com a add-on instal·lable (apareix a Preferències > Add-ons i s'inicia sol
#      en activar-lo). Panell a la barra lateral del View3D > pestanya NoOrbit.
#   2) Com a script d'arrencada: blender -P noorbit_bridge.py  (l'inicia NoOrbit
#      amb el botó «Arrenca Blender»). En aquest cas el servidor s'obre de
#      seguida, sense instal·lar res.
#
# L'execució del codi es fa SEMPRE al fil principal de Blender (via un temporitzador
# bpy.app.timers), perquè bpy.ops no és segur des d'un fil secundari.
# ==============================================================================

bl_info = {
    "name": "NoOrbit Bridge",
    "author": "NoOrbit",
    "version": (0, 5, 0),
    "blender": (4, 0, 0),
    "location": "View3D > Barra lateral > NoOrbit",
    "description": "Servidor de sòcol per controlar Blender des de NoOrbit",
    "category": "System",
}

import bpy
import json
import socket
import threading
import queue
import io
import traceback
from contextlib import redirect_stdout
from bpy.props import IntProperty, StringProperty

AUTH_KEY = "noorbit"
DEFAULT_PORT = 9876

_state = {
    "server": None,
    "thread": None,
    "port": DEFAULT_PORT,
    "running": False,
    "timer_on": False,
}
_requests = queue.Queue()
_results = {}


# -----------------------------------------------------------------------------
# Execució segura (fil principal)
# -----------------------------------------------------------------------------
def _run_code(code):
    ns = {"bpy": bpy, "__name__": "__main__"}
    buf = io.StringIO()
    try:
        with redirect_stdout(buf):
            exec(code, ns)
        return buf.getvalue()
    except Exception as e:  # noqa: BLE001
        return "Error: " + "".join(traceback.format_exception_only(type(e), e)).strip()


def _worker_timer():
    # S'executa al fil principal: atén una petició pendent si n'hi ha.
    if not _state["running"]:
        _state["timer_on"] = False
        return None
    try:
        rid, code, ev = _requests.get_nowait()
    except queue.Empty:
        return 0.05
    _results[rid] = _run_code(code)
    ev.set()
    return 0.05


def _ensure_timer():
    if _state["timer_on"]:
        return
    try:
        bpy.app.timers.register(_worker_timer, first_interval=0.1)
        _state["timer_on"] = True
    except Exception:  # noqa: BLE001
        _state["timer_on"] = False


# -----------------------------------------------------------------------------
# Atenció de connexions (fil secundari)
# -----------------------------------------------------------------------------
def _send(fobj, obj):
    fobj.write((json.dumps(obj) + "\n").encode("utf-8"))
    fobj.flush()


def _handle(conn):
    conn.settimeout(60)
    fobj = conn.makefile("rwb")
    try:
        for line in fobj:
            line = line.strip()
            if not line:
                continue
            try:
                msg = json.loads(line.decode("utf-8"))
            except Exception:  # noqa: BLE001
                _send(fobj, {"exec": "Error: JSON invàlid", "final": True})
                break
            if msg.get("key") != AUTH_KEY:
                _send(fobj, {"exec": "Error: clau no vàlida", "final": True})
                break
            command = msg.get("command", "execute")
            arg = msg.get("argument") or {}
            if command == "execute":
                code = arg.get("code", "")
                rid = "%s-%s" % (threading.get_ident(), id(code))
                ev = threading.Event()
                _requests.put((rid, code, ev))
                ev.wait(timeout=55)
                out = _results.pop(rid, "Error: sense resultat")
                _send(fobj, {"exec": out, "final": True})
            else:
                _send(fobj, {"exec": "Error: comanda desconeguda", "final": True})
            # Una petició per connexió (coincideix amb el client de NoOrbit).
            break
    finally:
        try:
            fobj.close()
        except Exception:  # noqa: BLE001
            pass
        try:
            conn.close()
        except Exception:  # noqa: BLE001
            pass


def _serve(port):
    srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    try:
        srv.bind(("127.0.0.1", port))
    except OSError as e:
        print("NoOrbit Bridge: no s'ha pogut obrir el port %d: %s" % (port, e))
        _state["running"] = False
        return
    srv.listen(5)
    srv.settimeout(1.0)
    _state["server"] = srv
    while _state["running"]:
        try:
            conn, _addr = srv.accept()
        except socket.timeout:
            continue
        except OSError:
            break
        threading.Thread(target=_handle, args=(conn,), daemon=True).start()
    try:
        srv.close()
    except Exception:  # noqa: BLE001
        pass


# -----------------------------------------------------------------------------
# Control d'inici/aturada
# -----------------------------------------------------------------------------
def start_server(port=DEFAULT_PORT):
    if _state["running"]:
        return True
    _state["port"] = port
    _state["running"] = True
    _ensure_timer()
    _state["thread"] = threading.Thread(target=_serve, args=(port,), daemon=True)
    _state["thread"].start()
    print("NoOrbit Bridge escoltant a 127.0.0.1:%d" % port)
    return True


def stop_server():
    _state["running"] = False
    srv = _state.get("server")
    if srv is not None:
        try:
            srv.close()
        except Exception:  # noqa: BLE001
            pass
    _state["server"] = None
    try:
        if _state["timer_on"]:
            bpy.app.timers.unregister(_worker_timer)
            _state["timer_on"] = False
    except Exception:  # noqa: BLE001
        pass
    print("NoOrbit Bridge aturat")


# -----------------------------------------------------------------------------
# Operadors i panell (UI de l'add-on)
# -----------------------------------------------------------------------------
class NOORBIT_OT_start(bpy.types.Operator):
    bl_idname = "noorbit.start_server"
    bl_label = "Inicia el servidor NoOrbit"
    bl_description = "Obre el port 9876 perquè NoOrbit es pugui conectar"

    def execute(self, context):
        start_server(int(context.scene.noorbit_port))
        return {"FINISHED"}


class NOORBIT_OT_stop(bpy.types.Operator):
    bl_idname = "noorbit.stop_server"
    bl_label = "Atura el servidor NoOrbit"

    def execute(self, context):
        stop_server()
        return {"FINISHED"}


class NOORBIT_PT_panel(bpy.types.Panel):
    bl_label = "NoOrbit"
    bl_idname = "NOORBIT_PT_panel"
    bl_space_type = "VIEW_3D"
    bl_region_type = "UI"
    bl_category = "NoOrbit"

    def draw(self, context):
        lay = self.layout
        st = context.scene.noorbit_status
        running = _state["running"]
        row = lay.row(align=True)
        row.alert = not running
        row.operator("noorbit.start_server", text="Inicia", icon="MESH_ICOSPHERE")
        row.operator("noorbit.stop_server", text="Atura", icon="CANCEL")
        lay.prop(context.scene, "noorbit_port")
        lay.label(text=("Estat: escoltant" if running else "Estat: aturat"))
        if st:
            box = lay.box()
            box.label(text=st)


def _status_update():
    # Petit informe a la UI (nombre d'objectes) per saber que l'add-on viu.
    try:
        n = len(bpy.context.scene.objects)
        bpy.context.scene.noorbit_status = "Escena: %d objectes" % n
    except Exception:  # noqa: BLE001
        bpy.context.scene.noorbit_status = ""
    return 2.0


classes = (NOORBIT_OT_start, NOORBIT_OT_stop, NOORBIT_PT_panel)


def register():
    for c in classes:
        bpy.utils.register_class(c)
    if not hasattr(bpy.types.Scene, "noorbit_port"):
        bpy.types.Scene.noorbit_port = IntProperty(
            name="Port", default=DEFAULT_PORT, min=1024, max=65535
        )
        bpy.types.Scene.noorbit_status = StringProperty(name="Estat", default="")
    # En activar l'add-on, arrenca el servidor automàticament.
    start_server(DEFAULT_PORT)


def unregister():
    stop_server()
    for c in reversed(classes):
        bpy.utils.unregister_class(c)


# Quan s'executa com a script d'arrencada (blender -P noorbit_bridge.py),
# obri el servidor de seguida sense instal·lar l'add-on.
if __name__ == "__main__":
    start_server(DEFAULT_PORT)
