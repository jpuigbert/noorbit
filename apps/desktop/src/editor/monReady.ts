/// Càrrega ÀJUS de Monaco (configuració local + workers).
///
/// Abans (v0.68/0.69), `monacoSetup` s'importava estàticament des de
/// `main.tsx`: el chunk vendor-monaco (3,9 MB + 6 MB de workers TS) es
/// descarregava SENGUER a l'arrencada de l'app, encara que no obrires cap
/// fitxer de codi. Ara els que munten un editor/diff fan:
///
///   const monReady = useMonacoReady();
///   if (!monReady) return <Placeholder />;   // ← primer frame, instantani
///
/// i el hook importa `monacoSetup` la primera vegada. Com que
/// `loader.config({ monaco })` s'aplica quan encara NO hi ha cap `<Editor>`
/// muntat, Monaco mai baixa del CDN (el requisit que ens obligava a tenir-lo
/// a dalt de tot).
import { useEffect, useState } from "react";

let done = false;
let loading: Promise<void> | null = null;
const subs = new Set<(v: boolean) => void>();

function load(): Promise<void> {
  if (done) return Promise.resolve();
  loading ??= import("./monacoSetup").then(() => {
    done = true;
    for (const s of subs) s(true);
    subs.clear();
  });
  return loading;
}

/// Retorna «cert» quan Monaco ja està configurat i pot muntar-se.
export function useMonacoReady(): boolean {
  const [ready, setReady] = useState(done);
  useEffect(() => {
    if (done) {
      setReady(true);
      return;
    }
    subs.add(setReady);
    void load();
    return () => {
      subs.delete(setReady);
    };
  }, []);
  return ready;
}
