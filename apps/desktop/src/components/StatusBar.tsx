import { useEffect } from "react";
import { Loader2, Folder, MemoryStick, Volume2, VolumeX, MousePointer2, GitBranch } from "lucide-react";
import { useT } from "../i18n";
import { actions } from "../actions";
import { useWorkspaceStore } from "../stores/workspaceStore";
import { useBlenderLiveStore } from "../stores/blenderLiveStore";
import { useUnrealStore } from "../stores/unrealStore";
import { useAIStore } from "../stores/aiStore";
import { useProviderStore } from "../stores/providerStore";
import { useUIStore } from "../stores/uiStore";
import { useAIActivity } from "../stores/activityStore";
import { useLspStore } from "../stores/lspStore";
import { useGitStatusStore } from "../stores/gitStatusStore";
import { useEditorStatusStore } from "../stores/editorStatusStore";
import { useSystemStatsStore, memPercent, formatBytes } from "../stores/systemStatsStore";
import BackgroundTaskBadge from "./BackgroundTaskBadge";
import AutonomousClock from "./AutonomousClock";
import AccountChip from "./Account/AccountChip";
import { ProblemsBadge } from "./Problems/ProblemsPanel";
import UpdateBadge from "./Update/UpdateBadge";

function Dot({ on }: { on: boolean }) {
  return <span className={"status-dot " + (on ? "on" : "off")} />;
}

export default function StatusBar() {
  const { t } = useT();
  const root = useWorkspaceStore((s) => s.root);
  const blender = useBlenderLiveStore((s) => s.connected);
  const unreal = useUnrealStore((s) => s.connected);
  const ollama = useAIStore((s) => s.ollamaRunning);
  const setShowSetupCheck = useUIStore((s) => s.setShowSetupCheck);
  const funSound = useUIStore((s) => s.funSound);
  const setFunSound = useUIStore((s) => s.setFunSound);
  const funCursor = useUIStore((s) => s.funCursor);
  const setFunCursor = useUIStore((s) => s.setFunCursor);
  // Mode professional: els commutadors lúdics de la barra s'amaguen
  // (les preferències es conserven i tornen en desactivar-lo).
  const professionalMode = useUIStore((s) => s.professionalMode);
  const activity = useAIActivity();

  // RAM en temps real: arrenca el mostreig global (idempotent) i llig.
  const mem = useSystemStatsStore((s) => s.mem);
  useEffect(() => {
    useSystemStatsStore.getState().start();
  }, []);
  const ramPct = memPercent(mem);

  useEffect(() => {
    useAIStore.getState().checkOllama();
    const id = setInterval(() => useAIStore.getState().checkOllama(), 15000);
    return () => clearInterval(id);
  }, []);

  // Indicador LSP (Fase 3c): estat honest de la sessió persistent. Consulta
  // barata (lsp_status NO arrenca servidors) cada 5 s i en canviar de projecte.
  const lspServer = useLspStore((s) => s.server);
  const lspReady = useLspStore((s) => s.ready);
  useEffect(() => {
    void useLspStore.getState().refresh(root);
    const id = setInterval(
      () => void useLspStore.getState().refresh(useWorkspaceStore.getState().root),
      5000
    );
    return () => clearInterval(id);
  }, [root]);

  const folderName = root ? root.split(/[/\\]/).pop() : null;

  // Branca de Git (Fase 4): polling lleuger de git_status cada 10 s mentre
  // hi ha projecte; el GitPanel publica el seu refresh al mateix store.
  const gitRepo = useGitStatusStore((s) => s.status?.repo ?? false);
  const gitBranch = useGitStatusStore((s) => s.status?.branch ?? "");
  const gitChanges = useGitStatusStore((s) => s.status?.changes.length ?? 0);
  useEffect(() => {
    void useGitStatusStore.getState().refresh(root);
    const id = setInterval(
      () => void useGitStatusStore.getState().refresh(useWorkspaceStore.getState().root),
      10000
    );
    return () => clearInterval(id);
  }, [root]);

  // Posició del cursor i llenguatge del fitxer actiu (els alimenta
  // EditorPane amb onDidChangeCursorPosition de Monaco).
  const curLine = useEditorStatusStore((s) => s.line);
  const curCol = useEditorStatusStore((s) => s.column);
  const language = useEditorStatusStore((s) => s.language);

  // Salut de la IA local (3 estils): instal·lada i amb model = a punt;
  // instal·lada però sense model = hi ha un pas pendent; no instal·lada.
  const ollamaInstalled = useAIStore((s) => s.ollamaInstalled);
  const models = useAIStore((s) => s.models);
  // IA EN LÍNIA activa: Ollama és opcional i NoOrbit funciona igual. La
  // barra no ha d'avisar de res en aquest cas.
  const cloudActive = useProviderStore((s) => s.active !== "ollama" && s.active !== "");
  const aiOk = cloudActive || (ollama && ollamaInstalled && models.length > 0);
  const aiLabel = cloudActive
    ? "IA: al núvol"
    : !ollamaInstalled
      ? "IA: no instal·lada"
      : !ollama
        ? "IA: aturada"
        : models.length === 0
          ? "IA: sense model"
          : `IA: ${models.length} model${models.length === 1 ? "" : "s"}`;
  const aiTitle = cloudActive
    ? "Estàs fent servir una IA en línia: Ollama no és necessari. Fes clic per veure l'entorn."
    : aiOk
      ? "La IA local està instal·lada i en marxa."
      : "Fes clic per posar la IA local en marxa (instal·lar, engegar o baixar model).";

  return (
    <div className="statusbar">
      {activity.working ? (
        <span className="status-item working">
          <Loader2 size={12} className="spin" /> {t(activity.labelKey)}
          {activity.detail && <span className="working-detail"> · {activity.detail}</span>}
        </span>
      ) : (
        <span className="status-item">{t("status.ready")}</span>
      )}
      <span className="status-item">
        <Folder size={12} /> {t("status.workspace")}: {folderName ?? t("status.noWorkspace")}
      </span>
      {/* Branca actual: clic → panell Git (pestanya de primer nivell). */}
      {gitRepo && gitBranch && (
        <button
          className="status-item as-btn"
          title={t("status.branchHint")}
          onClick={() => actions["git.open"]()}
        >
          <GitBranch size={12} /> {gitBranch}
          {gitChanges > 0 && <span className="status-git-count">{gitChanges}</span>}
        </button>
      )}
      <ProblemsBadge />
      <UpdateBadge />
      <div className="spacer" />
      {/* Editor: posició del cursor (clic → «Ves a la línia»), llenguatge
          del fitxer actiu i codificació. Només amb un fitxer obert. */}
      {language && (
        <>
          <button
            className="status-item as-btn"
            title={t("status.positionHint")}
            onClick={() => actions["goto.line"]()}
          >
            {t("status.position", { line: curLine, col: curCol })}
          </button>
          <span className="status-item" title={t("status.languageHint")}>
            {language}
          </span>
          <span className="status-item" title={t("status.encodingHint")}>
            UTF-8
          </span>
        </>
      )}
      <AccountChip />
      <BackgroundTaskBadge />
      <AutonomousClock />
      {/* Commutadors ràpids de la diversió: so de tasca acabada i cursor
          disfressat. Un clic i canvia l'estat (es desa entre sessions).
          Amb el mode professional actiu no es mostren. */}
      {!professionalMode && (
        <>
          <button
            className={"status-item status-fun-btn" + (funSound ? "" : " off")}
            title={t(funSound ? "status.funSoundOn" : "status.funSoundOff")}
            onClick={() => setFunSound(!funSound)}
          >
            {funSound ? <Volume2 size={12} /> : <VolumeX size={12} />}
          </button>
          <button
            className={"status-item status-fun-btn" + (funCursor ? "" : " off")}
            title={t(funCursor ? "status.funCursorOn" : "status.funCursorOff")}
            onClick={() => setFunCursor(!funCursor)}
          >
            <MousePointer2 size={12} />
          </button>
        </>
      )}
      {ramPct !== null && (
        <span
          className="status-item"
          title={`RAM usada ${mem ? formatBytes(mem.used) : ""} de ${mem ? formatBytes(mem.total) : ""}`}
        >
          <MemoryStick size={12} /> {t("status.ram")}: {ramPct}%
        </span>
      )}
      <span className="status-item status-ai" onClick={() => setShowSetupCheck(true)} title={aiTitle}>
        <Dot on={aiOk} /> {aiLabel}
      </span>
      <span className="status-item">
        <Dot on={blender} /> {t("status.blender")}
      </span>
      <span className="status-item">
        <Dot on={unreal} /> {t("status.unreal")}
      </span>
      <span className="status-item" title={t("status.lspHint")}>
        <Dot on={lspReady && !!lspServer} /> LSP: {lspServer ?? t("status.lspNone")}
      </span>
    </div>
  );
}
