import { create } from "zustand";

/// Comptes d'usuari DE NoOrbit (registre propi, independent de les IAs).
///
/// Filosofia (estructura a l'estil dels IDEs amb compte, p. ex. Qoder):
/// - Es pot REGISTRAR (compte nou amb nom, correu i contrasenya) i INICIAT
///   SESSIÓ en una finestra pròpia, sense bloquejar l'ús local de l'app.
/// - Els comptes viuen AL DISPOSITIU (localStorage) perquè NoOrbit és
///   local-first: cap dada surt del Mac si l'usuari no ho demana.
/// - Les contrasenyes NUNCA es desen en clar: es guarden com a resum
///   SHA-256 amb sal aleatòria (com un backend ho faria).
/// - La sessió activa persisteix entre arrencades.
/// - Cada compte té una llista buida de «connexions d'IA»: és el lloc on,
///   en un futur, s'enganxaran els serveis d'IA del propi compte (claus,
///   plans, quotes) amb la mateixa estructura, sense una reescriptura.

const LS_ACCOUNTS = "noorbit.auth.accounts";
const LS_SESSION = "noorbit.auth.session";

export interface NoOrbitAccount {
  id: string;
  name: string;
  email: string;
  /// resum SHA-256 de (salt + contrasenya), en hex. Mai la contrasenya neta.
  passHash: string;
  salt: string;
  createdAt: number;
  /// Connexions d'IA del compte (reservat per al futur: serveis remots que
  /// s'associaran a aquest compte per connectar IAs sense repetir claus).
  connections: AiConnection[];
}

export interface AiConnection {
  id: string;
  /// tipus de servei (p. ex. «ollama-cloud», «api-openai»); de moment no s'usa.
  kind: string;
  label: string;
  createdAt: number;
  /// Adreça del lloc (només per a connexions web).
  url?: string;
  /// Usuari del lloc (WordPress).
  username?: string;
  /// Contrasenya d'aplicació (WordPress). Desada al compte de l'usuari:
  /// és la seva credencial, mai al codi ni al servidor.
  secret?: string;
}

export interface ProfileUpdates {
  name?: string;
  email?: string;
}

interface AuthState {
  /** comptes creats en aquest dispositiu */
  accounts: NoOrbitAccount[];
  /** compte amb sessió iniciada (o null: «continuar sense compte») */
  user: NoOrbitAccount | null;
  signUp: (name: string, email: string, password: string) => Promise<NoOrbitAccount>;
  logIn: (email: string, password: string) => Promise<NoOrbitAccount>;
  logOut: () => void;
  /** inicia sessió amb un altre compte JA CREAT (sense tornar a posar la clau? no: demana contrasenya) */
  switchAccount: (email: string, password: string) => Promise<NoOrbitAccount>;
  updateProfile: (updates: ProfileUpdates, password?: string) => Promise<void>;
  deleteAccount: (id: string, password: string) => Promise<void>;
  /// Afegeix una CONNEXIÓ al compte (p. ex. el marketplace de temes
  /// WordPress: lloc, usuari i contrasenya d'aplicació). És la credencial
  /// de l'usuari, desada al seu compte local.
  addConnection: (conn: Omit<AiConnection, "id" | "createdAt">) => AiConnection;
  removeConnection: (id: string) => void;
}

function loadAccounts(): NoOrbitAccount[] {
  try {
    const raw = localStorage.getItem(LS_ACCOUNTS);
    if (!raw) return [];
    const arr = JSON.parse(raw) as unknown;
    if (!Array.isArray(arr)) return [];
    return arr.filter(
      (a): a is NoOrbitAccount =>
        !!a && typeof (a as NoOrbitAccount).id === "string" && typeof (a as NoOrbitAccount).email === "string"
    );
  } catch {
    return [];
  }
}

function persistAccounts(accounts: NoOrbitAccount[]) {
  try {
    localStorage.setItem(LS_ACCOUNTS, JSON.stringify(accounts));
  } catch {
    /* sense persistència */
  }
}

function persistSession(id: string | null) {
  try {
    if (id) localStorage.setItem(LS_SESSION, id);
    else localStorage.removeItem(LS_SESSION);
  } catch {
    /* sense persistència */
  }
}

function newId(): string {
  return `u-${Date.now()}-${Math.floor(Math.random() * 1e6)}`;
}

/// Resum SHA-256 de text, en hexadecimal. Fa servir WebCrypto (subtle),
/// disponible al webview de Tauri.
async function sha256Hex(text: string): Promise<string> {
  const data = new TextEncoder().encode(text);
  const digest = await crypto.subtle.digest("SHA-256", data);
  return Array.from(new Uint8Array(digest))
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}

function randomSalt(): string {
  const buf = new Uint8Array(16);
  crypto.getRandomValues(buf);
  return Array.from(buf)
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}

/// Coincidència tolerant: correu sense espais i en minúscula.
function normEmail(email: string): string {
  return email.trim().toLowerCase();
}

/// Missatge d'error amable (i18n per clau) a partir del motiu.
export type AuthErrorKey =
  | "account.errors.emailFormat"
  | "account.errors.emailTaken"
  | "account.errors.passShort"
  | "account.errors.nameEmpty"
  | "account.errors.noAccount"
  | "account.errors.wrongPass"
  | "account.errors.samePass"
  | "account.errors.sameEmail";

export const authErrors = {
  emailFormat: "account.errors.emailFormat" as AuthErrorKey,
  emailTaken: "account.errors.emailTaken" as AuthErrorKey,
  passShort: "account.errors.passShort" as AuthErrorKey,
  nameEmpty: "account.errors.nameEmpty" as AuthErrorKey,
  noAccount: "account.errors.noAccount" as AuthErrorKey,
  wrongPass: "account.errors.wrongPass" as AuthErrorKey,
  samePass: "account.errors.samePass" as AuthErrorKey,
  sameEmail: "account.errors.sameEmail" as AuthErrorKey,
};

export const useAuthStore = create<AuthState>((set, get) => {
  const accounts0 = loadAccounts();
  let user0: NoOrbitAccount | null = null;
  try {
    const sid = localStorage.getItem(LS_SESSION);
    if (sid) user0 = accounts0.find((a) => a.id === sid) ?? null;
  } catch {
    /* sense sessió desada */
  }

  return {
    accounts: accounts0,
    user: user0,

    signUp: async (name, email, password) => {
      const clean = {
        name: name.trim(),
        email: normEmail(email),
        pass: password,
      };
      if (!clean.name) throw authErrors.nameEmpty;
      if (!/^[^\s@]+@[^\s@]+\.[^\s@]{2,}$/.test(clean.email)) throw authErrors.emailFormat;
      if (clean.pass.length < 6) throw authErrors.passShort;
      if (get().accounts.some((a) => a.email === clean.email)) throw authErrors.emailTaken;

      const salt = randomSalt();
      const account: NoOrbitAccount = {
        id: newId(),
        name: clean.name,
        email: clean.email,
        passHash: await sha256Hex(salt + clean.pass),
        salt,
        createdAt: Date.now(),
        connections: [],
      };
      const accounts = [...get().accounts, account];
      persistAccounts(accounts);
      persistSession(account.id);
      set({ accounts, user: account });
      return account;
    },

    logIn: async (email, password) => {
      const e = normEmail(email);
      const account = get().accounts.find((a) => a.email === e);
      if (!account) throw authErrors.noAccount;
      const hash = await sha256Hex(account.salt + password);
      if (hash !== account.passHash) throw authErrors.wrongPass;
      persistSession(account.id);
      set({ user: account });
      return account;
    },

    logOut: () => {
      persistSession(null);
      set({ user: null });
    },

    /// Casualitat freqüent: dos comptes al mateix Mac (personal + treball).
    /// La finestra d'entrada escolleix; la contrasenya sempre es demana.
    switchAccount: async (email, password) => get().logIn(email, password),

    updateProfile: async (updates, password) => {
      const user = get().user;
      if (!user) throw authErrors.noAccount;
      const next: NoOrbitAccount = { ...user, connections: [...user.connections] };
      if (updates.name !== undefined) {
        const clean = updates.name.trim();
        if (!clean) throw authErrors.nameEmpty;
        next.name = clean;
      }
      if (updates.email !== undefined) {
        const e = normEmail(updates.email);
        if (!/^[^\s@]+@[^\s@]+\.[^\s@]{2,}$/.test(e)) throw authErrors.emailFormat;
        if (get().accounts.some((a) => a.id !== user.id && a.email === e)) throw authErrors.emailTaken;
        next.email = e;
      }
      if (password !== undefined && password.length > 0) {
        if (password.length < 6) throw authErrors.passShort;
        // una contrasenya NOVA no pot coincidir amb l'actual
        const same = await sha256Hex(user.salt + password);
        if (same === user.passHash) throw authErrors.samePass;
        const salt = randomSalt();
        next.salt = salt;
        next.passHash = await sha256Hex(salt + password);
      }
      const accounts = get().accounts.map((a) => (a.id === user.id ? next : a));
      persistAccounts(accounts);
      set({ accounts, user: next });
    },

    deleteAccount: async (id, password) => {
      const account = get().accounts.find((a) => a.id === id);
      if (!account) throw authErrors.noAccount;
      const hash = await sha256Hex(account.salt + password);
      if (hash !== account.passHash) throw authErrors.wrongPass;
      const accounts = get().accounts.filter((a) => a.id !== id);
      persistAccounts(accounts);
      if (get().user?.id === id) {
        persistSession(null);
        set({ accounts, user: null });
      } else {
        set({ accounts });
      }
    },

    /// Connexió al marketplace (temes WordPress) desada al compte de l'usuari.
    addConnection: (conn) => {
      const user = get().user;
      if (!user) throw authErrors.noAccount;
      const c: AiConnection = {
        ...conn,
        id: newId(),
        createdAt: Date.now(),
      };
      const accounts = get().accounts.map((a) =>
        a.id === user.id ? { ...a, connections: [...a.connections, c] } : a
      );
      persistAccounts(accounts);
      set({
        accounts,
        user: accounts.find((a) => a.id === user.id) ?? null,
      });
      return c;
    },

    removeConnection: (id) => {
      const user = get().user;
      if (!user) return;
      const accounts = get().accounts.map((a) =>
        a.id === user.id
          ? { ...a, connections: a.connections.filter((c) => c.id !== id) }
          : a
      );
      persistAccounts(accounts);
      set({
        accounts,
        user: accounts.find((a) => a.id === user.id) ?? null,
      });
    },
  };
});

/// Inici de sessió d'un compte sense reescriure el nom (auxiliar per a la UI).
export async function accountExists(email: string): Promise<boolean> {
  const e = normEmail(email);
  return useAuthStore.getState().accounts.some((a) => a.email === e);
}

/// Avatar de la primera lletra del nom (com els IDEs amb compte: cercle de
/// color amb la inicial a la barra inferior).
export function initialOf(name: string): string {
  return name.trim().charAt(0).toUpperCase() || "?";
}

/// Color determinista de l'avatar (de l'id del compte) per distingir comptes.
const AVATAR_HUES = [212, 152, 268, 8, 46, 190, 330, 108];
export function avatarColor(id: string): string {
  let sum = 0;
  for (let i = 0; i < id.length; i++) sum = (sum * 31 + id.charCodeAt(i)) >>> 0;
  const hue = AVATAR_HUES[sum % AVATAR_HUES.length];
  return `hsl(${hue} 45% 42%)`;
}
