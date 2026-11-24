import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";

export interface AndroidStatus {
  /** SDK usable per COMPILAR (abans exigia l'emulador: era el defecte). */
  installed: boolean;
  compile_ok: boolean;
  /** Paquet «emulator» present (només cal per a AVDs). */
  emulator_ok: boolean;
  /** «cmdline-tools» present: es pot actualitzar l'SDK sol. */
  cmdline_ok: boolean;
  sdk_path: string | null;
  emulator_bin: string | null;
  adb_bin: string | null;
  sdkmanager_bin: string | null;
  /** Carpeta triada per l'usuari (buida = ruta estàndard del sistema). */
  sdk_dir_configured: string;
  api: number;
  /** Un clic d'instal·lació disponible en este SO (POSIX). */
  one_click_supported: boolean;
  hint: string | null;
}
export interface IosStatus {
  installed: boolean;
  simctl: string | null;
  developer_dir: string | null;
  simulator_app: string | null;
  supported: boolean;
  hint: string | null;
}
export interface MobileStatus {
  android: AndroidStatus;
  ios: IosStatus;
  os: string;
}
export interface AndroidAvd {
  name: string;
  pretty: string | null;
  running: boolean;
}
export interface IosDevice {
  udid: string;
  name: string;
  state: string;
  is_available: boolean;
}
export interface IosRuntimeGroup {
  runtime: string;
  build: string;
  devices: IosDevice[];
}

type Platform = "android" | "ios";

interface MobileState {
  status: MobileStatus | null;
  avds: AndroidAvd[];
  iosGroups: IosRuntimeGroup[];
  loading: boolean;
  error: string | null;

  /** Instal·lació sota demanda de l'SDK d'Android. */
  sdkBusy: boolean;
  sdkMsg: string | null;
  /** Marcador que indica que el projecte obert és d'Android (si n'hi ha). */
  projectMarker: string | null;

  /** Screenshot en viu (base64 PNG) i interval de refresc. */
  liveShot: string | null;
  livePlatform: Platform | null;
  liveTarget: string | null; // AVD nom o UDID iOS
  liveTimer: ReturnType<typeof setInterval> | null;
  liveError: string | null;

  refreshStatus: () => Promise<void>;
  refreshAndroid: () => Promise<void>;
  refreshIos: () => Promise<void>;

  startAvd: (name: string) => Promise<void>;
  stopAvd: (name: string) => Promise<void>;
  bootIos: (udid: string) => Promise<void>;
  shutdownIos: (udid: string) => Promise<void>;
  openIos: (udid: string) => Promise<void>;

  startLive: (platform: Platform, target?: string) => Promise<void>;
  stopLive: () => void;

  /** Baixa l'SDK de compilació (o en canvia la carpeta) — destí LLIURE. */
  installSdk: (dest?: string) => Promise<void>;
  setSdkDir: (path: string) => Promise<void>;
  /** Demana si el projecte indicat és d'Android (per oferir-ho, no per global). */
  checkProject: (root: string | null) => Promise<void>;
}

export const useMobileStore = create<MobileState>((set, get) => ({
  status: null,
  avds: [],
  iosGroups: [],
  loading: false,
  error: null,
  liveShot: null,
  livePlatform: null,
  liveTarget: null,
  liveTimer: null,
  liveError: null,
  sdkBusy: false,
  sdkMsg: null,
  projectMarker: null,

  refreshStatus: async () => {
    set({ loading: true, error: null });
    try {
      const s = await invoke<MobileStatus>("mobile_status");
      set({ status: s });
    } catch (e) {
      set({ error: String(e) });
    } finally {
      set({ loading: false });
    }
  },

  refreshAndroid: async () => {
    try {
      // Sense paquet «emulator» NO hi ha AVDs: cal preguntar-ho abans, si no
      // el panell mostraria un error d'«emulator no disponible» com si
      // l'SDK no hi fora.
      const st = get().status;
      if (st && !st.android.emulator_ok) {
        set({ avds: [] });
        return;
      }
      const list = await invoke<AndroidAvd[]>("mobile_android_list_avds");
      set({ avds: list, error: null });
    } catch (e) {
      set({ avds: [], error: String(e) });
    }
  },

  refreshIos: async () => {
    try {
      const groups = await invoke<IosRuntimeGroup[]>("mobile_ios_list_devices");
      set({ iosGroups: groups, error: null });
    } catch (e) {
      set({ iosGroups: [], error: String(e) });
    }
  },

  startAvd: async (name) => {
    try {
      await invoke("mobile_android_start", { name });
      // Refresquem llistat: un dels AVD passarà a running=true.
      setTimeout(() => void get().refreshAndroid(), 1500);
    } catch (e) {
      set({ error: String(e) });
    }
  },

  stopAvd: async (name) => {
    try {
      await invoke("mobile_android_stop", { name });
      setTimeout(() => void get().refreshAndroid(), 800);
    } catch (e) {
      set({ error: String(e) });
    }
  },

  bootIos: async (udid) => {
    try {
      await invoke("mobile_ios_boot", { udid });
      await get().refreshIos();
    } catch (e) {
      set({ error: String(e) });
    }
  },

  shutdownIos: async (udid) => {
    try {
      await invoke("mobile_ios_shutdown", { udid });
      await get().refreshIos();
    } catch (e) {
      set({ error: String(e) });
    }
  },

  openIos: async (udid) => {
    try {
      await invoke("mobile_ios_open", { udid });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  // Encesa del «viewer» en viu: cada 2.5s, crida el screenshot corresponent.
  startLive: async (platform, target) => {
    get().stopLive();
    let useTarget = target ?? null;
    if (platform === "ios" && !useTarget) {
      // Agafa el primer Booted si no n'hem indicat cap.
      const booted = get().iosGroups.flatMap((g) => g.devices).find((d) => d.state === "Booted");
      useTarget = booted?.udid ?? null;
    }
    if (platform === "android" && !useTarget) {
      const running = get().avds.find((a) => a.running);
      useTarget = running?.name ?? null;
    }
    if (platform === "ios" && !useTarget) {
      set({ liveError: "Cap simulador iOS en marxa (prem «Engega» primer)." });
      return;
    }
    set({
      livePlatform: platform,
      liveTarget: useTarget,
      liveError: null,
    });
    // Primera captura immediata.
    const tick = async () => {
      try {
        const img =
          platform === "android"
            ? await invoke<string>("mobile_android_screenshot")
            : await invoke<string>("mobile_ios_screenshot", { udid: useTarget });
        set({ liveShot: img, liveError: null });
      } catch (e) {
        set({ liveError: String(e) });
      }
    };
    await tick();
    const timer = setInterval(() => void tick(), 2500);
    set({ liveTimer: timer });
  },

  stopLive: () => {
    const s = get();
    if (s.liveTimer) clearInterval(s.liveTimer);
    set({ liveTimer: null, liveShot: null, livePlatform: null, liveTarget: null });
  },

  installSdk: async (dest) => {
    set({ sdkBusy: true, sdkMsg: null, error: null });
    try {
      const msg = await invoke<string>("android_install_sdk", { dest: dest ?? null });
      set({ sdkMsg: msg });
      // El Terminal va a la seua lentura: refresquem l'estat una vegada i dejà
      // que siga l'usuari qui prema «Revisa» quan el script haja acabat.
      setTimeout(() => void get().refreshStatus(), 20_000);
    } catch (e) {
      set({ error: String(e) });
    } finally {
      set({ sdkBusy: false });
    }
  },

  setSdkDir: async (path) => {
    set({ sdkBusy: true, sdkMsg: null, error: null });
    try {
      set({ sdkMsg: await invoke<string>("android_set_sdk_dir", { path }) });
      await get().refreshStatus();
    } catch (e) {
      set({ error: String(e) });
    } finally {
      set({ sdkBusy: false });
    }
  },

  checkProject: async (root) => {
    if (!root) {
      set({ projectMarker: null });
      return;
    }
    try {
      set({ projectMarker: await invoke<string | null>("android_project_marker", { root }) });
    } catch {
      set({ projectMarker: null });
    }
  },
}));
