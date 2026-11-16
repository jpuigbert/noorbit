# Crates de l'Autonomous Jam

Aquests crates són el **nucli de l'agent autònom** de NoOrbit. Són
independents del framework (no depenen de Tauri), de manera que es poden
provar i reutilitzar fora de l'app.

```
                    ┌──────────────────────────────┐
   Tauri (pare)     │  supervisor (guardian)       │  ← RLIMIT_AS, swap, RSS
                    │  └─ spawn ─► fill (agent)    │  ← process group, killpg
                    └──────────────────────────────┘
                                      │  stdin/stdout (PING/COMPACT/STOP)
                    ┌─────────────────▼────────────┐
   Fill (agent)     │  planner → jam → git         │
                    │  worker → verifier → reporter│
                    └────────────────────────────┘
```

## Crates

| Crate | Què fa |
|---|---|
| `noorbit-guardian` | Supervisor de memòria: `RLIMIT_AS`, process group, `killpg`, mètriques de swap/RAM disponible, reinici limitat i `give-up`. Inclou el `ChildRuntime` del fill (protocol PING/COMPACT/STOP). |
| `noorbit-jam` | El bucle autònom: plan → tasca → worker → apply → verify → commit, amb pressupost de memòria per tasca (`MemoryGuard`), qualitat degradable, fases i temps, i `ProgressSink` per a la UI. |
| `noorbit-planner` | Brief → `Plan` (JSON estricte, validació, reintents amb feedback). |
| `noorbit-git` | `Checkpointer` amb el `git` CLI (reset/commit/tag, timeout, `kill_on_drop`). |
| `noorbit-worktree` | `Checkpointer` en un `git worktree` aïllat (no toca la feina de l'usuari). |
| `noorbit-reporter` | Informe final en Markdown (`REPORT.md`), escriptura atòmica, resum del guardian. |
| `noorbit-llm-openai` | Client per a APIs compatibles amb OpenAI (Ollama, LM Studio, Groq, Azure…): retry amb backoff, `Retry-After`, detecció de context excedit i de resposta truncada. |
| `noorbit-signal` | SIGTERM/SIGINT → cancel·lació ordenada (cleanup net). |
| `noorbit-memguard` | Mesura la RAM real (jam + fills) i cancel·la la tasca si s'excedeix el pressupost. |
| `noorbit-godot` | Worker/verificador per a Godot 4 (boot check i tests). |
| `noorbit-blender` | Worker/verificador per a Blender 4 (scripts `bpy`, export GLB). |
| `noorbit-blender-import` | Converteix `.glb` en `.tscn` de Godot 4. |
| `noorbit-lessons` | Memòria de lliçons entre intents (millora progressiva). |
| `noorbit-persistence` | Estat de la Jam a `state.json` per poder reprendre-la. |

## Contractes que cal respectar

1. **Retornar = «he acabat».** Quan el bucle retorna, el fill fa `exit(0)` i el
   supervisor **no** reinicia. Si el worker falla, es reintenta; no es retorna.
2. **Temps de resposta a STOP < 800 ms.** `apply_diff` i `verify` han de ser
   cancel·lables (`select!` amb cancel·lació).
3. **Paths versionats només.** El worker ha d'escriure dins del repo: `reset_hard`
   no neteja `/tmp`.
4. **`git apply` amb `kill_on_drop` + timeout.** Sinó el procés queda penjat.
5. **Cap subprocessos orfes.** `process_group(0)` + `kill_on_drop` a tot.
6. **El reporter escriu en <500 ms** (el watchdog del fill no espera).
7. **Diff materialitzat** (`Diff::materialized`): el worker ja ha escrit els
   fitxers, el jam no aplica cap patch.

## Comandes

```bash
cd crates
cargo build          # compila tots els crates
cargo test           # tests
cargo check          # verificació ràpida
```

Els crates no formen part del `Cargo.toml` de l'app Tauri (que és un package
aparte, a `apps/desktop/src-tauri`): es consumeixen per *path*.
