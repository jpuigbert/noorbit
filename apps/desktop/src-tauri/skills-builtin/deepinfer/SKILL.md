---
name: DeepInfer (DeepSeek, motor local)
description: Integrar el motor d'inferència DEEPINFER de DeepSeek amb NoOrbit — servit via vLLM/OpenAI, API al núvol o detecció automàtica del procés.
---

# DeepInfer i NoOrbit

**DeepInfer** (github.com/deepseek-ai/DeepInfer, llicència Apache 2.0) és el motor
d'inferència de producció de DeepSeek, alliberat el 2026. Executa el model
**DeepSeek-V4.1-Flash** amb kernels optimitzats (FlashMLA, DeepGEMM, DeepEP),
paral·lelisme (Attention DP / MoE EP), verificació especulativa (DSpark),
mostreig determinista i **sortides estructurades** (regex, JSON Schema, EBNF).

## ⚠️ Requisits de maquinari (importants)

DeepInfer és per a servidors, NO per a un portàtil:

- **Linux** (macOS i Windows no són compatibles)
- GCC ≥ 13.0 i **Python 3.12**
- GPU **NVIDIA SM100** amb CUDA 13 **o** NPU **Huawei Ascend 950** amb CANN 9.2
- La versió pública és **inferència offline**: no inclou servidor HTTP, KV cache
  ni balanceig (això cal afegir-ho: normalment davant es posa vLLM/SGLang).

En un Mac, NoOrbit **no pot executar** DeepInfer: en aquest cas usa el model de
DeepSeek **al núvol** (opció 2) o `deepseek-r1` amb **Ollama local**.

## Opció 1 — Servidor GPU amb API OpenAI (recomanat per a equips)

1. En una màquina Linux amb GPU SM100 (o Ascend 950):
   ```bash
   git clone https://github.com/deepseek-ai/DeepInfer && cd DeepInfer
   python3.12 -m venv .venv && source .venv/bin/activate
   export DEEPINFER_ACCELERATOR=nvidia   # 'ascend' per a NPU
   ./build.sh
   python examples/offline_inference.py --ep 8   # prova offline
   # Serveix-la per HTTP OpenAI-compatible amb vLLM/SGLang al davant:
   vllm serve deepseek-ai/DeepSeek-V4.1-Flash-1017 --port 8000
   ```
2. A NoOrbit: **Proveïdors d'IA → «DeepInfer (servidor GPU)»** → edita i
   posa la URL del servidor (per defecte `http://127.0.0.1:8000/v1`) i el
   model. **Activa** l'entrada. No cal cap token: la API local no n'utilitza.
3. NoOrbit detecta el motor sol si és al mateix ordinador (procés
   `deepinfer` o `offline_inference.py`) — mira «Proveïdors → IAs locals».

## Opció 2 — Model de DeepSeek al núvol (sense GPU)

La mateixa IA (els pesos de DeepSeek), servida oficialment:

1. Crea un token a plataforma.deepseek.com.
2. A NoOrbit: **Proveïdors d'IA → «DeepSeek API (núvol)»** → enganxa el token
   i activa'l. Ja hi està creat per defecte amb `deepseek-chat`.

## Com fa servir NoOrbit les respostes

Cap canal nou: DeepInfer/núvol entra pel **proveïdor OpenAI-compatible** que
NoOrbit ja té, així que el xat, els especialistes, el treball en equip, les
skills amb «@file:» i les directrius «RUN|» funcionen igual.

- Vols JSON ESTRICATA? Demana al system prompt «respon NOMÉS JSON vàlid amb
  aquest esquema: …» — amb la API del vLLM la sortida surt conforme.
- Vols **zero consum de RAM al Mac**? Aquest proveïdor no en consumeix: tot
  corre a la màquina del servidor (o al núvol).

## Referència ràpida

- Repo: https://github.com/deepseek-ai/DeepInfer
- Paquet: `pip install deepinfer` (PyPI, Apache 2.0)
- Sortides estructurades: `so_args={"schema_type": "json", "schema": {...}}`
- Env útils: `DEEPINFER_HF_MODEL_DIR` (checkpoint local),
  `DEEPINFER_BATCH_INVARIANT=1` (resultats idèntics sigui quina sigui la mida
  de batch; només NVIDIA).
