# -*- coding: utf-8 -*-
"""
NoOrbit — bootstrap per a Unreal Engine 5.

S'executa automàticament quan NoOrbit arrenca l'editor amb:
    -EnablePlugins=PythonScriptPlugin,RemoteControl -ExecCmds=python bootstrap.py

Intenta habilitar el servidor Remote Control API (HTTP, port 30010) perquè
NoOrbit pugui controlar l'editor. Si cap mètode funciona, mostra instruccions
per activar-lo manualment (Ventana > Virtual Production > Remote Control).
"""

import os

import unreal

# Port del servidor Remote Control. NoOrbit l'injecta per la variable
# d'entorn NOORBIT_UNREAL_RC_PORT (el port CONFIGURAT per l'usuari): així el
# client i l'editor coincideixen sempre. La font d'aquest valor és
# src-tauri/src/ports.rs.
PORT = int(os.environ.get("NOORBIT_UNREAL_RC_PORT", "30010"))
TAG = "NoOrbit/bootstrap"


def log(msg):
    unreal.log("[{}] {}".format(TAG, msg))


def warn(msg):
    unreal.log_warning("[{}] {}".format(TAG, msg))


def port_is_open(port, timeout=0.5):
    """Escolta algú al port? (comprovació REAL, no «he executat una ordre»)."""
    try:
        import socket
        s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        s.settimeout(timeout)
        try:
            return s.connect_ex(("127.0.0.1", port)) == 0
        finally:
            s.close()
    except Exception:
        return False


def try_console_commands():
    """Mètode 1: comandes de consola del plugin Remote Control (UE 5.x).

    `execute_console_command` NO llença quan la comanda no existeix o el
    plugin encara no està carregat: simplement no fa res. Per això, el
    resultat es CONFIRMA obrint el port: així el «Remote Control hauria
    d'estar escoltant» no és un èxit fals.
    """
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
    any_ran = False
    for cmd in cmds:
        try:
            unreal.SystemLibrary.execute_console_command(ctx, cmd)
            any_ran = True
            log("Executada: {}".format(cmd))
        except Exception:
            pass
    if not any_ran:
        return False
    # Confirmació real: el port ha d'escoltar de veritat.
    if port_is_open(PORT):
        return True
    log("Les comandes s'han executat, però el port {} encara no escolta.".format(PORT))
    return False


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
        log("Remote Control ESTÀ escoltant al port {}.".format(PORT))
    else:
        warn(
            "No s'ha pogut confirmar que Remote Control escolti al port {}.\n"
            "Activa'l manualment: Window > Virtual Production > Remote Control >\n"
            "'Enable Remote Control' (servidor HTTP al port {}).".format(PORT, PORT)
        )


main()
