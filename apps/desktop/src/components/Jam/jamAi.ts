/// Tria d'IA del panell «Autonomous Jam» — lògica PURA (sense efectes).
///
/// El desplegable codifica l'opció triada amb un prefixe perquè un sol
/// `<select>` pugui oferir tres famílies d'IA (model local d'Ollama,
/// proveïdor en línia i IA integrada) sense confondre'ls. Aquesta funció
/// la tradueix al (proveïdor, model) que espera el backend.

import type { ProviderPublic } from "../../stores/providerStore";

/// Model local d'Ollama: `local|<nom del model>`.
export const LOCAL = "local|";
/// Proveïdor en línia: `prov|<id del proveïdor>`.
export const PROV = "prov|";
/// IA integrada al binari: `nat|noorbit_native`.
export const NATIVE = "nat|";
/// IA integrada al binari («noorbit_native»): sense descàrregues ni claus.
export const NATIVE_ID = "noorbit_native";

export interface AiTria {
  /// `null` = «deixa-ho al backend» (Ollama → proveïdor en línia → IA
  /// integrada, segons el que estigui disponible).
  provider: string | null;
  model: string | null;
}

/// Resol la tria d'IA en (proveïdor, model) per als arguments del backend.
export function resolAi(tria: string, providers: ProviderPublic[]): AiTria {
  if (tria.startsWith(LOCAL)) {
    const model = tria.slice(LOCAL.length);
    return { provider: "ollama", model: model || null };
  }
  if (tria.startsWith(NATIVE)) {
    const nat = providers.find((p) => p.kind === "native");
    return { provider: NATIVE_ID, model: nat?.model ?? null };
  }
  if (tria.startsWith(PROV)) {
    const p = providers.find((x) => x.id === tria.slice(PROV.length));
    return { provider: p?.id ?? null, model: p?.model ?? null };
  }
  return { provider: null, model: null };
}
