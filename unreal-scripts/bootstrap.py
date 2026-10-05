# -*- coding: utf-8 -*-
"""
NoOrbit — bootstrap per a Unreal Engine 5.

S'executa automàticament quan NoOrbit arrenca l'editor amb:
    -EnablePlugins=PythonScriptPlugin,RemoteControl -ExecCmds=python bootstrap.py

Intenta habilitar el servidor Remote Control API (HTTP, port 30010) perquè
NoOrbit pugui controlar l'editor. Si cap mètode funciona, mostra instruccions
per activar-lo manualment (Ventana > Virtual Production > Remote Control).
"""

import unreal

PORT = 30010
TAG = "NoOrbit/bootstrap"


def log(msg):
    unreal.log("[{}] {}".format(TAG, msg))


def warn(msg):
    unreal.log_warning("[{}] {}".format(TAG, msg))


def try_console_commands():
    """Mètode 1: comandes de consola del plugin Remote Control (UE 5.x)."""
    ctx = None
    if hasattr(unreal, "EditorLevelLibrary"):
        try:
            ctx = unreal.EditorLevelLibrary.get_editor_world()
        except Exception:
            ctx = None
    cmds = [
        "RemoteControl.EnableServer",
        "RemoteControl.Enable",
        "rc.EnableServer 1",
        "RemoteControl.SetHttpPort {}".format(PORT),
    ]
    any_ok = False
    for cmd in cmds:
        try:
            unreal.SystemLibrary.execute_console_command(ctx, cmd)
            any_ok = True
            log("Executada: {}".format(cmd))
        except Exception:
            pass
    return any_ok


def try_remote_controller_subsystem():
    """Mètode 2: subsistema RemoteController si l'API Python està disponible."""
    cls = getattr(unreal, "RemoteController", None)
    if cls is None:
        return False
    try:
        sub = unreal.get_editor_subsystem(cls)
    except Exception:
        return False
    if sub is None:
        return False
    for fn in ("startup", "enable", "start_server"):
        method = getattr(sub, fn, None)
        if callable(method):
            try:
                method()
                log("Servidor arrencat via RemoteController.{}()".format(fn))
                return True
            except Exception:
                continue
    return False


def main():
    log("Iniciant…")

    # Les APIs Remote Control responen mentre la UI està ocupada; en
    # arrencada, l'editor pot trigar a tenir el plugin llest.
    ok = try_console_commands()
    if not ok:
        ok = try_remote_controller_subsystem()

    if ok:
        log("Remote Control hauria d'estar escoltant al port {}".format(PORT))
    else:
        warn(
            "No s'ha pogut habilitar Remote Control automàticament.\n"
            "Activa'l manualment: Window > Virtual Production > Remote Control >\n"
            "'Enable Remote Control' (servidor HTTP al port {}).".format(PORT)
        )


main()
