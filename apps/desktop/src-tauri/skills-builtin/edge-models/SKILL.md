---
name: edge-models
description: Models lliures a l'edge (cost 0) amb NoOrbit: GGUF locals amb llama.cpp, i el projecte «smolmaxxing» (hyperionaidev/smolmaxxing) per fer córrer models de 260K a 16B en dispositius amb ~1 GB de RAM (TV box, iPad vell). Inclou dreceres per a totes les IA.
---

# Models lliures a l'edge (cost 0) + smolmaxxing

## Què és
- **Models lliures (cost 0)**: models GGUF de pesos oberts (Qwen, Llama, Mistral, Gemma,
  DeepSeek, SmolLM, Bonsai…) que es baixen una vegada i es fan córrer **localment** amb
  llama.cpp. No surten de la màquina i no costen res per token.
- **smolmaxxing** (hyperionaidev/smolmaxxing): pedaç de llama.cpp que fa córrer models
  de 260K a 16B paràmetres en dispositius amb ~1 GB de RAM (Xiaomi TV Box S, iPad Air
  2013), amb kernels per xip, paginació d'experts MoE des de flash i streaming de pesos.
  Llicència MIT. Els models necessiten el build pedaçat (el llama.cpp d'estoc no els
  carrega).

## Com fer-ho funcionar a NoOrbit
1. **Baixa un GGUF** (IA ▸ Hugging Bay, o manualment) a la carpeta de models.
   - Models petits recomanats: Qwen3.5-0.8B/2B, SmolLM3 3B, Gemma 3 4B, Bonsai 1.7B (1-bit).
   - Models grans en MoE per a edge: Ling-mini-2.0 (16B), Moon-Beam-8B-A1B.
2. **Motor local** (IA ▸ Gestor de models ▸ llama.cpp): s'instal·la amb un clic i engega
   `llama-server` en segon pla; es registra automàticament com a proveïdor sense clau.
3. **Per a smolmaxxing**: compila el build pedaçat (zig + cmake + ninja) segons
   `docs/tv-box-setup.md` del repo, i apunta NoOrbit al binari pedaçat via
   IA ▸ Servidor d'IA remot (URL local) o canviant la ruta del motor.
4. **Tria el model** al xat i conversa: tot és local, cost 0.

## Dreceres (shortcuts) per IA
- **Local (Ollama/llama.cpp)**: proves ràpides, tot privat. Ideal per a edge.
- **Núvol**: si cal més qualitat, connecta un token (OpenAI, Claude, Gemini…).
- **Hugging Bay**: cerca i baixa GGUF amb SHA-256 verificat; tria disc extern.
- **IA de web**: consulta IAs web i materialitza el codi al projecte.
- **Imatges**: ComfyUI local o via API; nudge «IMG|».
- **3D**: panells Blender / Unreal.

## Compatibilitat
Qualsevol IA connectada a NoOrbit pot aprofitar els resultats. Els models edge (smolmaxxing)
requereixen el build pedaçat i, en alguns casos, dispositius ARM amb dotprod.

## Referència
- smolmaxxing: https://github.com/hyperionaidev/smolmaxxing
- Hugging Face: https://huggingface.co/hyperionaidev
