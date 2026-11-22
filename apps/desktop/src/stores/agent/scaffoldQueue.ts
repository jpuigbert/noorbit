/// Cua de generació FITXER A FITXER del scaffold. Un model dèbil no pot
/// escriure 20 fitxers d'un cop, però sí un de cada vegada: la cua es
/// processa en LOTS successius (MAX_PER_LOT) fins acabar, amb progrés
/// visible i cancel·lació possible entre fitxers.
///
/// REGLES DE FIABILITAT (Fase 1):
///  • PROHIBIT escriure placeholders «(Pendent: el model no ha retornat
///    contingut per a aquest fitxer)»: si un fitxer surt buit (o és un
///    placeholder), es REGENERA un cop; si torna a fallar, queda SENSE
///    escriure i es reporta a `failed` (el cridaner ho informa al xat).
///  • La cancel·lació (l'usuari prem «Atura») talla la cua entre fitxers:
///    els que queden es retornen a `pending`, en ordre, per reprendre'ls.

import { invoke } from "@tauri-apps/api/core";
import { esContingutPlaceholder, extractSingleFileContent } from "./fileBlocks";

/// Fitxers per lot: la mida que un model local petit pot pair sense perdre
/// el fil. La continuació és AUTOMÀTICA: lots successius fins buidar la cua.
export const MAX_PER_LOT = 8;

export interface ScaffoldQueueOpts {
  /// Descripció del projecte (context de cada fitxer).
  projectText: string;
  /// Rutes del manifest, en ordre de creació.
  paths: string[];
  /// System prompt de generació d'un sol fitxer (FILE_SYSTEM).
  system: string;
  provider?: string | null;
  model?: string | null;
  lotSize?: number;
  /// L'usuari ha premut «Atura» (o el torn ja no és vigent).
  isCancelled: () => boolean;
  /// Progrés visible al xat (pas de la línia de temps).
  onLot?: (lot: number, lots: number, done: number, total: number) => void;
  onFile?: (rel: string) => void;
  onRetry?: (rel: string) => void;
  onSkip?: (rel: string) => void;
  /// Fitxer generat amb contingut real (per marcar la tasca com a feta).
  onDone?: (rel: string) => void;
}

export interface ScaffoldQueueResult {
  /// Fitxers amb contingut REAL (mai buit, mai placeholder).
  generated: { path: string; content: string }[];
  /// Fitxers que el model no ha pogut generar ni amb el reintent: NO
  /// s'escriuen a disc (el cridaner ho ha de dir clarament al xat).
  failed: string[];
  /// Cua restant per cancel·lació (en ordre original).
  pending: string[];
}

export async function generateFileQueue(opts: ScaffoldQueueOpts): Promise<ScaffoldQueueResult> {
  const lotSize = Math.max(1, opts.lotSize ?? MAX_PER_LOT);
  const queue = [...opts.paths];
  const total = queue.length;
  const lots = Math.ceil(total / lotSize);
  const generated: { path: string; content: string }[] = [];
  const failed: string[] = [];
  let done = 0;
  let lot = 0;

  const ask = (rel: string): Promise<string> =>
    invoke<string>("ai_chat_plain", {
      prompt:
        `Projecte: ${opts.projectText}\n\nEscriu NOMÉS el fitxer «${rel}» amb el seu ` +
        `contingut COMPLET i executable. Cap text fora del codi.`,
      system: opts.system,
      provider: opts.provider ?? undefined,
      model: opts.model ?? undefined,
    }).catch(() => "");

  while (queue.length > 0) {
    if (opts.isCancelled()) break;
    lot++;
    opts.onLot?.(lot, lots, done, total);
    const batch = queue.splice(0, lotSize);
    for (let i = 0; i < batch.length; i++) {
      if (opts.isCancelled()) {
        // La resta del lot torna a la cua EN ORDRE per reprendre-la.
        queue.unshift(...batch.slice(i));
        break;
      }
      const rel = batch[i];
      opts.onFile?.(rel);
      let content = extractSingleFileContent(await ask(rel));
      if (!content.trim() || esContingutPlaceholder(content)) {
        // Un sol reintent: si el model torna a fallar, el fitxer queda
        // SENSE escriure (mai un placeholder a disc).
        opts.onRetry?.(rel);
        content = extractSingleFileContent(await ask(rel));
      }
      if (!content.trim() || esContingutPlaceholder(content)) {
        failed.push(rel);
        opts.onSkip?.(rel);
        continue;
      }
      generated.push({ path: rel, content });
      done++;
      opts.onDone?.(rel);
    }
  }
  return { generated, failed, pending: queue };
}