import { FormEvent, type CSSProperties, type MouseEvent, useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getVersion } from "@tauri-apps/api/app";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import wordmarkLogo from "./assets/mylist-wordmark.svg";
import { applyExternalMessages, formatDateTime, formatMonth, localeOptions, setActiveLocale, t, type Locale } from "./i18n";
import "./App.css";

type Theme = "light" | "dark";
type InterfaceTransparency = 0 | 5 | 10 | 15 | 20 | 25 | 30;
type Status = "todo" | "completed";
type WindowMode = "mode-topmost" | "mode-normal" | "mode-desktop";
type McpStatus = "disabled" | "starting" | "online" | "stopping" | "error";
type DefaultCategoryKey = "personal" | "team" | "work" | "life" | "travel" | "finance" | "study" | "other";
type Category = { id: string; name: string; defaultKey: DefaultCategoryKey | null; nameOverride: string | null; colorId: string; color: string };
type BootstrapData = { deviceId: string; theme: Theme; locale: Locale; startupEnabled: boolean; mcpEnabled: boolean; interfaceTransparency: InterfaceTransparency; categories: Category[]; palette: Array<{ id: string; row: number; column: number; value: string }> };
type McpServiceSnapshot = { status: McpStatus; endpoint: string | null; aiConnected: boolean };
type Task = { id: string; title: string; note: string; categoryId: string; categoryName: string; categoryDefaultKey: DefaultCategoryKey | null; categoryNameOverride: string | null; categoryColor: string; status: Status; dueAtUtcMs: number | null; recurrenceJson: string | null; createdAtUtcMs: number; updatedAtUtcMs: number; completedAtUtcMs: number | null };
type VisualTaskExit = { key: string; task: Task; source: Status; index: number };
type McpDestructiveConfirmation = { token: string; operation: "delete_task" | "delete_category"; expiresAtUtcMs: number; preview: { task?: Task; category?: Category; taskCount?: number; targetCategoryId?: string | null } };
type McpTransferRequest = { operationId: string; operation: "export_plaintext" | "export_encrypted" | "import_merge" | "import_replace" };
type ImportPreview = { sessionId: string; sourceFileName: string; sourceDeviceId: string; exportedAtUtcMs: number; taskCount: number; categoryCount: number; paletteCount: number; newTasks: number; updatedTasks: number; keptTasks: number; newCategories: number; updatedCategories: number; keptCategories: number };
type ImportSelection = { kind: "preview" | "password"; sessionId: string; sourceFileName: string; operation: ImportOperation; preview: ImportPreview | null };
type ImportOperation = "merge" | "replace";
type EncryptedImportRequest = { sessionId: string; sourceFileName: string; operation: ImportOperation };
type ImportResult = { sourceFileName: string; newTasks: number; updatedTasks: number; keptTasks: number; newCategories: number; updatedCategories: number; keptCategories: number; snapshotCreated: boolean };
type Page = "home" | "create" | "view" | "edit" | "settings" | "mcp-install" | "ai-guide" | "help";
type SettingsSection = "general" | "categories" | "ai";
type TextControl = HTMLInputElement | HTMLTextAreaElement;
type TextContextMenuState = { control: TextControl; x: number; y: number; hasSelection: boolean; canEdit: boolean; canCopy: boolean; canPaste: boolean; canSelectAll: boolean };

const icon = (name: string) => `/icons/${name}`;
// All two-stage confirmation exits use the same cadence: collapse, then shrink away.
const CONFIRM_TRANSITION_MS = 200;
const TASK_STATUS_EXIT_MS = 600;
const COMPLETE_EXIT_CHAIN_MS = TASK_STATUS_EXIT_MS;

const defaultCategoryLabelKeys: Record<DefaultCategoryKey, Parameters<typeof t>[0]> = {
  personal: "category.default.personal",
  team: "category.default.team",
  work: "category.default.work",
  life: "category.default.life",
  travel: "category.default.travel",
  finance: "category.default.finance",
  study: "category.default.study",
  other: "category.default.other",
};

function categoryLabel(category: Pick<Category, "name" | "defaultKey" | "nameOverride">) {
  return category.nameOverride || (category.defaultKey ? t(defaultCategoryLabelKeys[category.defaultKey]) : category.name);
}

function taskCategoryLabel(task: Pick<Task, "categoryName" | "categoryDefaultKey" | "categoryNameOverride">) {
  return task.categoryNameOverride || (task.categoryDefaultKey ? t(defaultCategoryLabelKeys[task.categoryDefaultKey]) : task.categoryName);
}

function isTextControl(target: EventTarget | null): target is TextControl {
  if (target instanceof HTMLTextAreaElement) return true;
  if (!(target instanceof HTMLInputElement)) return false;
  return ["", "text", "search", "url", "tel", "email", "password"].includes(target.type);
}

function replaceTextControlValue(control: TextControl, value: string) {
  const prototype = control instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
  const setter = Object.getOwnPropertyDescriptor(prototype, "value")?.set;
  setter?.call(control, value);
  control.dispatchEvent(new Event("input", { bubbles: true }));
}

function TextContextMenu({ theme }: { theme: Theme }) {
  const [menu, setMenu] = useState<TextContextMenuState | null>(null);
  useEffect(() => {
    const dismiss = () => setMenu(null);
    const show = (event: globalThis.MouseEvent) => {
      event.preventDefault();
      if (!isTextControl(event.target)) { setMenu(null); return; }
      const control = event.target;
      const isPassword = control instanceof HTMLInputElement && control.type === "password";
      const start = control.selectionStart ?? 0;
      const end = control.selectionEnd ?? start;
      const canEdit = !control.readOnly && !control.disabled;
      const next = { control, x: Math.max(8, Math.min(event.clientX, window.innerWidth - 120)), y: Math.max(8, Math.min(event.clientY, window.innerHeight - 124)), hasSelection: end > start, canEdit, canCopy: !isPassword && end > start, canPaste: false, canSelectAll: control.value.length > 0 };
      setMenu(next);
      void invoke<string>("read_clipboard_text").then((text) => setMenu((current) => current?.control === control ? { ...current, canPaste: canEdit && text.length > 0 } : current)).catch(() => undefined);
    };
    document.addEventListener("contextmenu", show);
    document.addEventListener("pointerdown", dismiss);
    window.addEventListener("blur", dismiss);
    return () => { document.removeEventListener("contextmenu", show); document.removeEventListener("pointerdown", dismiss); window.removeEventListener("blur", dismiss); };
  }, []);

  if (!menu) return null;
  const close = () => setMenu(null);
  const selection = () => menu.control.value.slice(menu.control.selectionStart ?? 0, menu.control.selectionEnd ?? 0);
  const selectAll = () => { menu.control.focus(); menu.control.select(); close(); };
  const copy = async () => { await invoke("copy_text", { text: selection() }).catch(() => undefined); close(); };
  const cut = async () => { await invoke("copy_text", { text: selection() }).catch(() => undefined); const start = menu.control.selectionStart ?? 0; const end = menu.control.selectionEnd ?? start; replaceTextControlValue(menu.control, `${menu.control.value.slice(0, start)}${menu.control.value.slice(end)}`); menu.control.focus(); menu.control.setSelectionRange(start, start); close(); };
  const paste = async () => { try { const text = await invoke<string>("read_clipboard_text"); if (!text) return; const start = menu.control.selectionStart ?? 0; const end = menu.control.selectionEnd ?? start; replaceTextControlValue(menu.control, `${menu.control.value.slice(0, start)}${text}${menu.control.value.slice(end)}`); const cursor = start + text.length; menu.control.focus(); menu.control.setSelectionRange(cursor, cursor); } finally { close(); } };
  const style = { left: menu.x, top: menu.y } as CSSProperties;
  return createPortal(<div className="text-context-menu" data-theme={theme} role="menu" style={style} onPointerDown={(event) => event.stopPropagation()}>
    {menu.canEdit && <button role="menuitem" disabled={!menu.hasSelection} onClick={() => void cut()}>{t("context.cut")}</button>}
    <button role="menuitem" disabled={!menu.canCopy} onClick={() => void copy()}>{t("context.copy")}</button>
    {menu.canEdit && <button role="menuitem" disabled={!menu.canPaste} onClick={() => void paste()}>{t("context.paste")}</button>}
    <button role="menuitem" disabled={!menu.canSelectAll} onClick={selectAll}>{t("context.selectAll")}</button>
  </div>, document.body);
}

export default function App() {
  const [theme, setTheme] = useState<Theme>("light");
  const [locale, setLocale] = useState<Locale>("zh-CN");
  const [mode, setMode] = useState<WindowMode>("mode-normal");
  const [bootstrap, setBootstrap] = useState<BootstrapData | null>(null);
  const [mcpStatus, setMcpStatus] = useState<McpStatus>("disabled");
  const [tasks, setTasks] = useState<Record<Status, Task[]>>({ todo: [], completed: [] });
  const [visualTaskExits, setVisualTaskExits] = useState<VisualTaskExit[]>([]);
  const [status, setStatus] = useState<Status>("todo");
  const [page, setPage] = useState<Page>("home");
  const [settingsSection, setSettingsSection] = useState<SettingsSection>("general");
  const [selectedTask, setSelectedTask] = useState<Task | null>(null);
  const [notice, setNotice] = useState("");
  const [importPreview, setImportPreview] = useState<ImportPreview | null>(null);
  const [encryptedImport, setEncryptedImport] = useState<EncryptedImportRequest | null>(null);
  const [importOperation, setImportOperation] = useState<ImportOperation>("merge");
  const [exportDialogOpen, setExportDialogOpen] = useState(false);
  const [todoDeleteConfirmation, setTodoDeleteConfirmation] = useState<Task | null>(null);
  const [mcpConfirmation, setMcpConfirmation] = useState<McpDestructiveConfirmation | null>(null);
  const [mcpTransfer, setMcpTransfer] = useState<McpTransferRequest | null>(null);
  const noticeTimerRef = useRef<number | null>(null);
  const visualExitSequenceRef = useRef(0);
  const visualExitTimersRef = useRef<Map<string, number>>(new Map());

  const categories = bootstrap?.categories ?? [];
  const visibleTasks = tasks[status];
  const renderedTasks: Array<{ task: Task; visualExit?: VisualTaskExit }> = visibleTasks.map((task) => ({ task }));
  visualTaskExits.filter((exit) => exit.source === status).sort((a, b) => a.index - b.index).forEach((visualExit) => {
    renderedTasks.splice(Math.min(visualExit.index, renderedTasks.length), 0, { task: visualExit.task, visualExit });
  });
  const formMode = page === "edit" ? t("task.edit") : t("task.add");

  function currentNativeLabels() {
    return {
      openMain: t("tray.openMain"),
      showDesktop: t("tray.showDesktop"),
      topmost: t("tray.topmost"),
      normal: t("tray.normal"),
      desktop: t("tray.desktop"),
      quit: t("tray.quit"),
      plaintextFile: t("file.plaintext"),
      encryptedFile: t("file.encrypted"),
    };
  }

  async function syncNativeLabels() {
    await invoke("sync_native_labels", { labels: currentNativeLabels() });
  }

  async function loadLocaleMessages(next: string) {
    const messages = await invoke<Record<string, string>>("load_external_locale", { locale: next });
    applyExternalMessages(next, messages);
  }

  async function refreshBootstrap() {
    try {
      const data = await invoke<BootstrapData>("load_bootstrap_data");
      setTheme(data.theme);
      await loadLocaleMessages(data.locale);
      setActiveLocale(data.locale);
      setLocale(data.locale);
      setBootstrap(data);
      const service = await invoke<McpServiceSnapshot>("mcp_status");
      setMcpStatus(service.status);
      await syncNativeLabels();
    } catch (error) { showError(error); }
  }

  function applyDesktopDisplayScale(scale: number) {
    void getCurrentWebview().setZoom(Math.max(0.5, Math.min(scale, 2))).catch(() => undefined);
  }

  useEffect(() => {
    void invoke<WindowMode>("window_mode").then(setMode).catch(() => undefined);
    void refreshBootstrap();
    void (async () => { await invoke("settle_due_recurrences").catch(() => undefined); await refreshTasks(); })();
    let unlisten: (() => void) | undefined;
    void listen<WindowMode>("window-mode-changed", (event) => setMode(event.payload)).then((dispose) => { unlisten = dispose; });
    let unlistenData: (() => void) | undefined;
    void listen("mylist-data-changed", () => { void refreshTasks(); }).then((dispose) => { unlistenData = dispose; });
    let unlistenMcpConfirmation: (() => void) | undefined;
    void listen<McpDestructiveConfirmation>("mylist-mcp-confirmation-requested", (event) => setMcpConfirmation(event.payload)).then((dispose) => { unlistenMcpConfirmation = dispose; });
    let unlistenMcpTransfer: (() => void) | undefined;
    void listen<McpTransferRequest>("mylist-mcp-transfer-requested", (event) => { void beginMcpTransfer(event.payload); }).then((dispose) => { unlistenMcpTransfer = dispose; });
    let unlistenDesktopScale: (() => void) | undefined;
    void listen<number>("desktop-display-scale-changed", (event) => applyDesktopDisplayScale(event.payload)).then((dispose) => { unlistenDesktopScale = dispose; });
    return () => { unlisten?.(); unlistenData?.(); unlistenMcpConfirmation?.(); unlistenMcpTransfer?.(); unlistenDesktopScale?.(); };
  }, []);

  useEffect(() => {
    if (!bootstrap?.mcpEnabled) return;
    const timer = window.setTimeout(() => { void verifyMcpStatus(true); }, 10000);
    return () => window.clearTimeout(timer);
  }, [bootstrap?.mcpEnabled]);

  useEffect(() => {
    if (mode === "mode-desktop") {
      void invoke<number>("desktop_display_scale").then(applyDesktopDisplayScale).catch(() => undefined);
    } else {
      applyDesktopDisplayScale(1);
    }
  }, [mode]);

  async function refreshTasks() {
    try {
      const [todo, completed] = await Promise.all([invoke<Task[]>("list_tasks", { status: "todo" }), invoke<Task[]>("list_tasks", { status: "completed" })]);
      setTasks({ todo, completed });
    } catch (error) { showError(error); }
  }

  useEffect(() => () => {
    if (noticeTimerRef.current) window.clearTimeout(noticeTimerRef.current);
    visualExitTimersRef.current.forEach((timer) => window.clearTimeout(timer));
    visualExitTimersRef.current.clear();
  }, []);
  function showNotice(message: string) { if (noticeTimerRef.current) window.clearTimeout(noticeTimerRef.current); setNotice(message); noticeTimerRef.current = window.setTimeout(() => { setNotice(""); noticeTimerRef.current = null; }, 3000); }
  function showError(error: unknown) {
    const message = error instanceof Error ? error.message : String(error);
    showNotice(message === "mcp_start_failed" ? t("notice.mcpFailed") : /unique constraint failed: categories\.name|已存在同名分类/i.test(message) ? t("notice.sameCategory") : t("error.operationFailed"));
  }
  async function setWindowMode(next: WindowMode) { try { const changed = await invoke<WindowMode>("set_window_mode", { mode: next }); setMode(changed); showNotice(changed === "mode-topmost" ? t("mode.topmost") : changed === "mode-desktop" ? t("mode.desktop") : t("mode.normal")); } catch (error) { showError(error); } }
  async function setThemeSetting(next: Theme) { try { const saved = await invoke<Theme>("save_theme_setting", { theme: next }); setTheme(saved); setBootstrap((current) => current ? { ...current, theme: saved } : current); showNotice(saved === "dark" ? t("notice.themeDark") : t("notice.themeLight")); } catch (error) { showError(error); } }
  async function setInterfaceTransparency(next: InterfaceTransparency) { try { const saved = await invoke<InterfaceTransparency>("save_interface_transparency_setting", { transparency: next }); setBootstrap((current) => current ? { ...current, interfaceTransparency: saved } : current); } catch (error) { showError(error); } }
  async function setLocaleSetting(next: Locale) { try { const saved = await invoke<Locale>("save_locale_setting", { locale: next }); await loadLocaleMessages(saved); setActiveLocale(saved); setLocale(saved); setBootstrap((current) => current ? { ...current, locale: saved } : current); await syncNativeLabels(); } catch (error) { showError(error); } }
  async function setStartupSetting(enabled: boolean) { try { const saved = await invoke<boolean>("set_startup_enabled", { enabled }); setBootstrap((current) => current ? { ...current, startupEnabled: saved } : current); showNotice(saved ? t("notice.startupOn") : t("notice.startupOff")); } catch { showNotice(t("notice.startupFailed")); } }
  async function verifyMcpStatus(retryOnError: boolean) {
    try {
      const snapshot = await invoke<McpServiceSnapshot>("mcp_status");
      setMcpStatus(snapshot.status);
      if (snapshot.status === "error" && retryOnError) {
        window.setTimeout(() => { void verifyMcpStatus(false); }, 2000);
      }
    } catch {
      setMcpStatus("error");
      if (retryOnError) window.setTimeout(() => { void verifyMcpStatus(false); }, 2000);
    }
  }
  async function setMcpSetting(enabled: boolean) {
    setMcpStatus(enabled ? "starting" : "stopping");
    try {
      const snapshot = await invoke<McpServiceSnapshot>("set_mcp_enabled", { enabled });
      setMcpStatus(snapshot.status);
      setBootstrap((current) => current ? { ...current, mcpEnabled: enabled } : current);
      showNotice(enabled ? t("notice.mcpEnabled") : t("notice.mcpDisabled"));
    } catch (error) {
      setMcpStatus(enabled ? "disabled" : "online");
      showError(error);
    }
  }

  useEffect(() => {
    const due = tasks.todo.filter((task) => task.recurrenceJson && task.dueAtUtcMs).map((task) => task.dueAtUtcMs as number).sort((a, b) => a - b)[0];
    if (!due) return;
    const delay = Math.max(0, Math.min(due - Date.now(), 2_147_000_000));
    const timer = window.setTimeout(() => { void invoke("settle_due_recurrences").then(refreshTasks).catch(() => undefined); }, delay);
    return () => window.clearTimeout(timer);
  }, [tasks.todo]);
  async function copyText(text: string, successNotice = t("notice.copied")) { try { await invoke("copy_text", { text }); showNotice(successNotice); } catch { showNotice(t("notice.copyFailed")); } }
  async function openMcpInstall() { try { await invoke<string>("mcp_install_prompt", { locale }); setPage("mcp-install"); } catch (error) { showError(error); } }
  async function copyMcpInstallPrompt() { try { const prompt = await invoke<string>("mcp_install_prompt", { locale }); await copyText(prompt); } catch (error) { showError(error); } }
  async function createCategory() { try { await invoke<Category>("create_category", { input: { name: t("category.newDefault") } }); await refreshBootstrap(); showNotice(t("notice.categoryAdded")); } catch (error) { showError(error); } }
  async function updateCategory(input: { id: string; name: string; colorId: string }) { try { await invoke<Category>("update_category", { input }); await Promise.all([refreshBootstrap(), refreshTasks()]); showNotice(t("notice.categorySaved")); return true; } catch (error) { showError(error); return false; } }
  async function deleteCategory(id: string, targetCategoryId?: string) { try { await invoke("delete_category", { id, targetCategoryId: targetCategoryId ?? null }); await Promise.all([refreshBootstrap(), refreshTasks()]); showNotice(t("notice.categoryDeleted")); return true; } catch (error) { showError(error); return false; } }
  async function restoreDefaultCategories() { try { await invoke("restore_default_categories"); await Promise.all([refreshBootstrap(), refreshTasks()]); showNotice(t("notice.defaultsRestored")); return true; } catch (error) { showError(error); return false; } }
  async function exportPlaintextData() { try { const path = await invoke<string | null>("export_plaintext_snapshot"); if (path) showNotice(t("notice.exported")); return Boolean(path); } catch (error) { showError(error); return false; } }
  async function exportEncryptedData(password: string) { try { const path = await invoke<string | null>("export_encrypted_snapshot", { password }); if (path) showNotice(t("notice.exportedEncrypted")); return Boolean(path); } catch (error) { showError(error); return false; } }
  async function previewPlaintextImport(operation: ImportOperation) { try { setImportPreview(null); setEncryptedImport(null); setImportOperation(operation); const selection = await invoke<ImportSelection | null>("preview_plaintext_import", { operation }); if (!selection) return; if (selection.kind === "password") { setEncryptedImport({ sessionId: selection.sessionId, sourceFileName: selection.sourceFileName, operation }); return; } if (selection.preview) setImportPreview(selection.preview); } catch (error) { showError(error); } }
  async function previewEncryptedImport(sessionId: string, password: string, operation: ImportOperation) { try { const preview = mcpTransfer?.operationId === sessionId ? await invoke<ImportPreview>("mcp_preview_pending_encrypted_import", { operationId: sessionId, password }) : await invoke<ImportPreview>("preview_pending_encrypted_import", { sessionId, password }); setImportOperation(operation); setEncryptedImport(null); setImportPreview(preview); return true; } catch (error) { showError(error); return false; } }
  async function applyPlaintextImport(sessionId: string, operation: ImportOperation) { try { const result = mcpTransfer?.operationId === sessionId ? await invoke<ImportResult>("mcp_apply_import", { operationId: sessionId }) : await invoke<ImportResult>("apply_pending_plaintext_import", { sessionId, operation }); await Promise.all([refreshBootstrap(), refreshTasks()]); setImportPreview(null); if (mcpTransfer?.operationId === sessionId) setMcpTransfer(null); showNotice(operation === "replace" ? t("data.importCompleteReplace", { count: result.newTasks }) : t("data.importCompleteMerge", { newCount: result.newTasks, updatedCount: result.updatedTasks })); return true; } catch (error) { showError(error); return false; } }
  async function beginMcpTransfer(request: McpTransferRequest) {
    setMcpTransfer(request);
    if (request.operation.startsWith("export_")) { setExportDialogOpen(true); return; }
    const operation: ImportOperation = request.operation === "import_replace" ? "replace" : "merge";
    try {
      setImportPreview(null); setEncryptedImport(null); setImportOperation(operation);
      const selection = await invoke<ImportSelection | null>("mcp_preview_import", { operationId: request.operationId });
      if (!selection) { setMcpTransfer(null); return; }
      if (selection.kind === "password") { setEncryptedImport({ sessionId: selection.sessionId, sourceFileName: selection.sourceFileName, operation }); return; }
      if (selection.preview) setImportPreview(selection.preview);
    } catch (error) { showError(error); setMcpTransfer(null); }
  }
  async function cancelMcpTransfer() { const active = mcpTransfer; setMcpTransfer(null); setImportPreview(null); setEncryptedImport(null); setExportDialogOpen(false); if (active) { try { await invoke("cancel_mcp_transfer", { operationId: active.operationId }); } catch { /* The client may have already completed or expired the operation. */ } } }
  async function exportMcpData(password?: string) { const active = mcpTransfer; if (!active) return false; try { await invoke("mcp_export_snapshot", { operationId: active.operationId, password: password ?? null }); setMcpTransfer(null); showNotice(password ? t("notice.exportedEncrypted") : t("notice.exported")); return true; } catch (error) { showError(error); return false; } }
  const cycleMode = () => void setWindowMode(mode === "mode-normal" ? "mode-topmost" : mode === "mode-topmost" ? "mode-desktop" : "mode-normal");
  function finishVisualTaskExit(key: string) {
    const timer = visualExitTimersRef.current.get(key);
    if (timer) window.clearTimeout(timer);
    visualExitTimersRef.current.delete(key);
    setVisualTaskExits((current) => current.filter((exit) => exit.key !== key));
  }
  function retainTaskForCompleteExitChain(task: Task) {
    const key = `${task.id}:${++visualExitSequenceRef.current}`;
    const index = Math.max(0, tasks[task.status].findIndex((item) => item.id === task.id));
    setVisualTaskExits((current) => [...current.filter((exit) => exit.task.id !== task.id), { key, task, source: task.status, index }]);
    const timer = window.setTimeout(() => finishVisualTaskExit(key), COMPLETE_EXIT_CHAIN_MS);
    visualExitTimersRef.current.set(key, timer);
    return key;
  }
  function switchTaskTab(next: Status) {
    if (next !== status) {
      visualExitTimersRef.current.forEach((timer) => window.clearTimeout(timer));
      visualExitTimersRef.current.clear();
      setVisualTaskExits([]);
    }
    setStatus(next);
  }
  async function setTaskStatus(task: Task, next: Status): Promise<boolean> {
    const visualExitKey = retainTaskForCompleteExitChain(task);
    const movedTask: Task = { ...task, status: next, completedAtUtcMs: next === "completed" ? Date.now() : null, updatedAtUtcMs: Date.now() };
    setTasks((current) => ({
      ...current,
      [task.status]: current[task.status].filter((item) => item.id !== task.id),
      [next]: [movedTask, ...current[next].filter((item) => item.id !== task.id)],
    }));
    showNotice(next === "completed" ? t("notice.taskCompleted") : t("notice.taskMoved"));
    try {
      await invoke<Task>("set_task_status", { id: task.id, status: next });
      await refreshTasks();
      return true;
    } catch (error) {
      finishVisualTaskExit(visualExitKey);
      await refreshTasks();
      showError(error);
      return false;
    }
  }
  async function removeTask(task: Task, animate = false): Promise<boolean> {
    const visualExitKey = animate ? retainTaskForCompleteExitChain(task) : null;
    setTasks((current) => ({ ...current, [task.status]: current[task.status].filter((item) => item.id !== task.id) }));
    setPage("home");
    showNotice(t("notice.taskDeleted"));
    try {
      await invoke("delete_task", { id: task.id });
      await refreshTasks();
      return true;
    } catch (error) {
      if (visualExitKey) finishVisualTaskExit(visualExitKey);
      await refreshTasks();
      showError(error);
      return false;
    }
  }
  async function approveMcpConfirmation(token: string) { try { await invoke("approve_mcp_confirmation", { token }); setMcpConfirmation(null); } catch (error) { showError(error); } }
  async function rejectMcpConfirmation(token: string) { try { await invoke("reject_mcp_confirmation", { token }); } catch (error) { showError(error); } finally { setMcpConfirmation(null); } }
  function openTask(task: Task) { setSelectedTask(task); setPage("view"); }

  return <main className="app-shell" data-theme={theme} lang={locale} onPointerDownCapture={mode === "mode-desktop" ? () => void invoke("refresh_window_surface") : undefined} style={{ "--interface-transparency": `${bootstrap?.interfaceTransparency ?? 5}%` } as CSSProperties}>
    {page === "home" ? <>
    <Header mode={mode} onCycle={cycleMode} onModePress={() => void invoke("refresh_window_surface")} onHide={() => void invoke("hide_to_tray")} />
      <section className="task-page">
        <div className="mode-tabs task-tabs" role="tablist" aria-label={t("task.status")}>
          <button className={status === "todo" ? "selected" : ""} onClick={() => switchTaskTab("todo")}><span className="tab-count">{tasks.todo.length}</span>{t("task.todo")}</button>
          <button className={status === "completed" ? "selected" : ""} onClick={() => switchTaskTab("completed")}><span className="tab-count">{tasks.completed.length}</span>{t("task.completed")}</button>
        </div>
        <TaskList>
          {renderedTasks.map(({ task }) => <TaskRow key={`${status}:${task.id}`} task={task} theme={theme} onOpen={() => openTask(task)} onEdit={() => { setSelectedTask(task); setPage("edit"); }} onCopy={() => void copyText(task.note ? `${task.title}\n${task.note}` : task.title, task.note ? t("notice.copiedTitleAndNote") : t("notice.copiedTitle"))} onStatus={() => setTaskStatus(task, task.status === "todo" ? "completed" : "todo")} onDelete={() => task.status === "todo" ? setTodoDeleteConfirmation(task) : void removeTask(task, true)} />)}
          {!renderedTasks.length && <div className="empty-state"><span className="empty-state-icon" aria-hidden="true" /><p>{status === "todo" ? t("task.emptyTodo") : t("task.emptyCompleted")}</p><span>{status === "todo" ? t("task.emptyTodoHelp") : t("task.emptyCompletedHelp")}</span></div>}
        </TaskList>
      </section>
      <footer className="app-footer"><button className="icon-control" aria-label={t("app.settings")} onClick={() => setPage("settings")}><img src={icon("settings_24_regular.svg")} alt="" /></button><button className="add-control" aria-label={t("task.add")} onClick={() => { setSelectedTask(null); setPage("create"); }}><img src={icon("add_24_regular.svg")} alt="" /></button><button className="resize-grip" aria-label={t("app.resize")} onMouseDown={startResize}><span>{Array.from({ length: 6 }, (_, index) => <i key={index} />)}</span></button></footer>
    </> : page === "settings" ? <Settings section={settingsSection} onSectionChange={setSettingsSection} theme={theme} locale={locale} interfaceTransparency={bootstrap?.interfaceTransparency ?? 5} startupEnabled={bootstrap?.startupEnabled ?? true} mcpEnabled={bootstrap?.mcpEnabled ?? true} mcpStatus={mcpStatus} categories={categories} palette={bootstrap?.palette ?? []} categoryUsage={Object.fromEntries(categories.map((category) => [category.id, tasks.todo.filter((task) => task.categoryId === category.id).length + tasks.completed.filter((task) => task.categoryId === category.id).length]))} onThemeChange={setThemeSetting} onTransparencyChange={setInterfaceTransparency} onLocaleChange={setLocaleSetting} onStartupChange={setStartupSetting} onMcpChange={setMcpSetting} onOpenMcpInstall={openMcpInstall} onOpenAiGuide={() => setPage("ai-guide")} onCreateCategory={createCategory} onUpdateCategory={updateCategory} onDeleteCategory={deleteCategory} onRestoreDefaults={restoreDefaultCategories} onOpenExport={() => setExportDialogOpen(true)} onPreviewImport={previewPlaintextImport} onBack={() => setPage("home")} /> : page === "mcp-install" ? <McpInstallPage locale={locale} onBack={() => setPage("settings")} onCopy={copyMcpInstallPrompt} /> : page === "ai-guide" ? <AiUsageGuide onBack={() => setPage("settings")} /> : page === "help" ? <HelpPage onBack={() => setPage("settings")} /> : page === "view" && selectedTask ? <TaskView task={selectedTask} onBack={() => setPage("home")} onEdit={() => setPage("edit")} onDelete={() => void removeTask(selectedTask)} onCopy={(text, successNotice) => void copyText(text, successNotice)} /> : <TaskForm title={formMode} task={page === "edit" ? selectedTask : null} categories={categories} theme={theme} onBack={() => setPage(selectedTask ? "view" : "home")} onSave={async (input) => { try { const { recurrence, ...taskInput } = input; const editing = page === "edit" && selectedTask; const saved = await (editing ? invoke<Task>("update_task", { input: { ...taskInput, id: selectedTask.id } }) : invoke<Task>("create_task", { input: taskInput })); await invoke("save_task_recurrence", { id: saved.id, recurrence }); await refreshTasks(); setSelectedTask(null); setStatus("todo"); setPage("home"); showNotice(editing ? t("notice.taskSaved") : t("notice.taskAdded")); } catch (error) { showError(error); } }} />}
    {notice && <div className="toast" role="status">{notice}</div>}
    {importPreview && <ImportPreviewDialog preview={importPreview} operation={importOperation} theme={theme} onApply={applyPlaintextImport} onClose={() => { if (mcpTransfer?.operationId === importPreview.sessionId) void cancelMcpTransfer(); else setImportPreview(null); }} />}
    {encryptedImport && <EncryptedImportPasswordDialog theme={theme} request={encryptedImport} onClose={() => { if (mcpTransfer?.operationId === encryptedImport.sessionId) void cancelMcpTransfer(); else setEncryptedImport(null); }} onPreview={previewEncryptedImport} />}
    {exportDialogOpen && <ExportDataDialog theme={theme} lockedEncrypted={mcpTransfer ? mcpTransfer.operation === "export_encrypted" : undefined} onClose={() => { if (mcpTransfer) void cancelMcpTransfer(); else setExportDialogOpen(false); }} onExportPlaintext={mcpTransfer ? () => exportMcpData() : exportPlaintextData} onExportEncrypted={mcpTransfer ? exportMcpData : exportEncryptedData} />}
    {todoDeleteConfirmation && <TodoTaskDeleteDialog theme={theme} task={todoDeleteConfirmation} onCancel={() => setTodoDeleteConfirmation(null)} onConfirm={async () => { const task = todoDeleteConfirmation; if (!task) return false; setTodoDeleteConfirmation(null); void removeTask(task); return true; }} />}
    {mcpConfirmation && <McpDestructiveConfirmationDialog theme={theme} confirmation={mcpConfirmation} onApprove={approveMcpConfirmation} onReject={rejectMcpConfirmation} />}
    <TextContextMenu theme={theme} />
  </main>;
}

function Header({ mode, onCycle, onModePress, onHide }: { mode: WindowMode; onCycle: () => void; onModePress: () => void; onHide: () => void }) {
  const pinIcon = mode === "mode-topmost" ? "pin_topmost_24_regular.svg" : mode === "mode-desktop" ? "pin_desktop_24_regular.svg" : "pin_normal_24_regular.svg";
  return <header className="app-titlebar" onMouseDown={startWindowDrag} onDoubleClick={preventTitlebarDoubleClick}><button className={`icon-control pin pin-${mode.replace("mode-", "")} ${mode === "mode-topmost" ? "is-active" : ""}`} aria-label={t("app.windowMode")} onPointerDown={onModePress} onClick={onCycle}><img src={icon(pinIcon)} alt="" /></button><h1 className="wordmark-title"><span className="wordmark-mark" role="img" aria-label="MyLIST" style={{ "--wordmark-image": `url("${wordmarkLogo}")` } as CSSProperties} /></h1><button className="icon-control close" aria-label={t("app.hideToTray")} onClick={onHide}><img src={icon("dismiss_20_regular.svg")} alt="" /></button></header>;
}

function Settings({ section, onSectionChange, theme, locale, interfaceTransparency, startupEnabled, mcpEnabled, mcpStatus, categories, palette, categoryUsage, onThemeChange, onTransparencyChange, onLocaleChange, onStartupChange, onMcpChange, onOpenMcpInstall, onOpenAiGuide, onCreateCategory, onUpdateCategory, onDeleteCategory, onRestoreDefaults, onOpenExport, onPreviewImport, onBack }: { section: SettingsSection; onSectionChange: (section: SettingsSection) => void; theme: Theme; locale: Locale; interfaceTransparency: InterfaceTransparency; startupEnabled: boolean; mcpEnabled: boolean; mcpStatus: McpStatus; categories: Category[]; palette: BootstrapData["palette"]; categoryUsage: Record<string, number>; onThemeChange: (theme: Theme) => void; onTransparencyChange: (value: InterfaceTransparency) => void; onLocaleChange: (locale: Locale) => void; onStartupChange: (enabled: boolean) => void; onMcpChange: (enabled: boolean) => void; onOpenMcpInstall: () => void; onOpenAiGuide: () => void; onCreateCategory: () => void; onUpdateCategory: (input: { id: string; name: string; colorId: string }) => Promise<boolean>; onDeleteCategory: (id: string, targetCategoryId?: string) => Promise<boolean>; onRestoreDefaults: () => Promise<boolean>; onOpenExport: () => void; onPreviewImport: (operation: ImportOperation) => Promise<void>; onBack: () => void }) {
  const [productVersion, setProductVersion] = useState("");
  useEffect(() => {
    void getVersion().then(setProductVersion).catch(() => setProductVersion(""));
  }, []);
  return <section className="sheet settings-sheet">
    <SheetHeader title={t("app.settings")} onBack={onBack} />
    <nav className="mode-tabs settings-tabs" role="tablist" aria-label={t("settings.tabs")}>
      <button className={section === "general" ? "selected" : ""} onClick={() => onSectionChange("general")}>{t("settings.general")}</button>
      <button className={section === "categories" ? "selected" : ""} onClick={() => onSectionChange("categories")}>{t("settings.categories")}</button>
      <button className={section === "ai" ? "selected" : ""} onClick={() => onSectionChange("ai")}>{t("settings.aiConnection")}</button>
    </nav>
    <div className="settings-content">
      {section === "general" ? <div className="settings-block">
        <div className="settings-appearance-row"><span>{t("settings.appearance")}</span><ThemeModeToggle value={theme} onChange={onThemeChange} /></div>
        <div className="settings-language-row"><span>{t("settings.interfaceTransparency")}</span><TransparencyDropdown value={interfaceTransparency} theme={theme} onChange={onTransparencyChange} /></div>
        <div className="settings-language-row"><span>{t("settings.language")}</span><LanguageDropdown value={locale} theme={theme} onChange={onLocaleChange} /></div>
        <div className="settings-toggle-row"><span>{t("settings.startup")}</span><button type="button" className={`settings-toggle ${startupEnabled ? "enabled" : ""}`} role="switch" aria-checked={startupEnabled} aria-label={t("settings.startup")} onClick={() => onStartupChange(!startupEnabled)}><i /></button></div>
        <div className="settings-version-row"><span>{t("settings.version")}</span><span className="settings-version-value">{productVersion ? `V${productVersion}` : "—"}</span></div>
        <div className="settings-divider" role="separator" />
        <DataSettings onOpenExport={onOpenExport} onPreviewImport={onPreviewImport} />
      </div> : section === "categories" ? <CategorySettings theme={theme} categories={categories} palette={palette} categoryUsage={categoryUsage} onCreate={onCreateCategory} onUpdate={onUpdateCategory} onDelete={onDeleteCategory} onRestoreDefaults={onRestoreDefaults} /> : <AiConnectionSettings mcpEnabled={mcpEnabled} mcpStatus={mcpStatus} onMcpChange={onMcpChange} onOpenMcpInstall={onOpenMcpInstall} onOpenAiGuide={onOpenAiGuide} />}
    </div>
  </section>;
}

function DataSettings({ onOpenExport, onPreviewImport }: { onOpenExport: () => void; onPreviewImport: (operation: ImportOperation) => Promise<void> }) {
  return <div className="data-settings"><div className="data-export-row"><span>{t("data.export")}</span><button className="button secondary data-export-button" onClick={onOpenExport}>{t("data.exportAction")}</button></div><div className="data-export-row"><span>{t("data.importMerge")}</span><button className="button secondary data-export-button" onClick={() => void onPreviewImport("merge")}>{t("data.importAction")}</button></div><div className="data-export-row"><span>{t("data.importReplace")}</span><button className="button secondary data-export-button" onClick={() => void onPreviewImport("replace")}>{t("data.importAction")}</button></div></div>;
}

function ExportDataDialog({ theme, lockedEncrypted, onClose, onExportPlaintext, onExportEncrypted }: { theme: Theme; lockedEncrypted?: boolean; onClose: () => void; onExportPlaintext: () => Promise<boolean>; onExportEncrypted: (password: string) => Promise<boolean> }) {
  const [encrypted, setEncrypted] = useState(lockedEncrypted ?? true);
  const [password, setPassword] = useState("");
  const [confirmation, setConfirmation] = useState("");
  const [exporting, setExporting] = useState(false);
  const passwordError = encrypted && password.length > 0 && (password.length < 6 || password.length > 32) ? t("data.passwordLength") : encrypted && confirmation.length > 0 && password !== confirmation ? t("data.passwordMismatch") : "";
  const canExport = !exporting && (!encrypted || (password.length >= 6 && password.length <= 32 && password === confirmation));
  return createPortal(<div className="dialog-backdrop" onMouseDown={exporting ? undefined : onClose}><section className="export-data-dialog" data-theme={theme} role="dialog" aria-modal="true" aria-label={t("data.export")} onMouseDown={(event) => event.stopPropagation()}><h2>{t("data.exportDialogTitle")}</h2><p>{t("data.exportPalette")}</p><div className="export-format-toggle"><button className={!encrypted ? "selected" : ""} disabled={lockedEncrypted !== undefined} onClick={() => setEncrypted(false)}>{t("data.plaintext")}</button><button className={encrypted ? "selected" : ""} disabled={lockedEncrypted !== undefined} onClick={() => setEncrypted(true)}>{t("data.encrypted")}</button></div>{encrypted ? <div className="export-password-fields"><label>{t("data.password")}<input type="password" value={password} autoFocus maxLength={32} onChange={(event) => setPassword(event.target.value)} /></label><label>{t("data.confirmPassword")}<input type="password" value={confirmation} maxLength={32} onChange={(event) => setConfirmation(event.target.value)} /></label>{passwordError && <span role="alert">{passwordError}</span>}</div> : <p className="export-warning">{t("data.plaintextWarning")}</p>}<footer className="dialog-actions"><button className="button secondary" disabled={exporting} onClick={onClose}>{t("data.cancel")}</button><button className="button primary" disabled={!canExport} onClick={async () => { setExporting(true); const done = encrypted ? await onExportEncrypted(password) : await onExportPlaintext(); if (done) onClose(); else setExporting(false); }}>{exporting ? t("data.exporting") : t("data.exportAction")}</button></footer></section></div>, document.body);
}

function HelpPage({ onBack }: { onBack: () => void }) {
  return <section className="sheet"><SheetHeader title={t("help.title")} onBack={onBack} /><article className="help-page"><p>{t("help.basics")}</p><p>{t("help.repeat")}</p><p>{t("help.ai")}</p></article></section>;
}

function AiConnectionSettings({ mcpEnabled, mcpStatus, onMcpChange, onOpenMcpInstall, onOpenAiGuide }: { mcpEnabled: boolean; mcpStatus: McpStatus; onMcpChange: (enabled: boolean) => void; onOpenMcpInstall: () => void; onOpenAiGuide: () => void }) {
  const [aiConnected, setAiConnected] = useState(false);
  const [displayStatus, setDisplayStatus] = useState(mcpStatus);
  useEffect(() => setDisplayStatus(mcpStatus), [mcpStatus]);
  useEffect(() => {
    if (!mcpEnabled) { setAiConnected(false); return; }
    let retryTimer: number | undefined;
    const refresh = async (retryOnError: boolean) => {
      try {
        const snapshot = await invoke<McpServiceSnapshot>("mcp_status");
        setDisplayStatus(snapshot.status);
        setAiConnected(snapshot.status === "online" && snapshot.aiConnected);
        if (snapshot.status === "error" && retryOnError) {
          retryTimer = window.setTimeout(() => { void refresh(false); }, 2000);
        }
      } catch {
        setDisplayStatus("error");
        setAiConnected(false);
        if (retryOnError) retryTimer = window.setTimeout(() => { void refresh(false); }, 2000);
      }
    };
    void refresh(true);
    let unlisten: (() => void) | undefined;
    void listen<boolean>("mylist-mcp-ai-connection-changed", (event) => setAiConnected(event.payload))
      .then((dispose) => { unlisten = dispose; });
    return () => { unlisten?.(); if (retryTimer) window.clearTimeout(retryTimer); };
  }, [mcpEnabled]);
  const statusText = displayStatus === "starting" ? t("settings.mcpStatusStarting") : displayStatus === "online" ? t(aiConnected ? "settings.aiConnected" : "settings.waitingAiConnection") : displayStatus === "stopping" ? t("settings.mcpStatusStopping") : displayStatus === "error" ? t("settings.mcpStatusError") : t("settings.mcpStatusDisabled");
  return <div className="ai-connection-settings">
    <div className="settings-toggle-row"><span>{t("settings.installMcpSkill")}</span><button className="button secondary settings-action-button" onClick={onOpenMcpInstall}>{t("settings.connect")}</button></div>
    <div className="settings-mcp-row"><span className="settings-mcp-label">{t("settings.mcpService")}</span><div className="settings-mcp-control"><span className={`mcp-status mcp-status-${displayStatus}`}>{statusText}</span><button type="button" className={`settings-toggle ${mcpEnabled ? "enabled" : ""}`} role="switch" aria-checked={mcpEnabled} aria-label={t("settings.mcpService")} disabled={mcpStatus === "starting" || mcpStatus === "stopping"} onClick={() => onMcpChange(!mcpEnabled)}><i /></button></div></div>
    <div className="settings-toggle-row"><span>{t("settings.aiUsageTips")}</span><button className="button secondary settings-action-button" onClick={onOpenAiGuide}>{t("settings.view")}</button></div>
    <p>{t("settings.aiConnectionDescription")}</p>
  </div>;
}

function AiUsageGuide({ onBack }: { onBack: () => void }) {
  return <section className="sheet ai-guide-sheet">
    <SheetHeader title={t("aiGuide.title")} onBack={onBack} />
    <article className="ai-guide-page">
      <p className="ai-guide-intro">{t("aiGuide.intro")}</p>
      <section className="ai-guide-rule"><span className="ai-guide-index">1</span><div><h2>{t("aiGuide.triggerTitle")}</h2><p>{t("aiGuide.triggerBody")}</p></div></section>
      <section className="ai-guide-rule"><span className="ai-guide-index">2</span><div><h2>{t("aiGuide.ruleTitle")}</h2><p>{t("aiGuide.ruleBody")}</p></div></section>
      <div className="ai-guide-examples"><h2>{t("aiGuide.examplesTitle")}</h2><code>{t("aiGuide.exampleOnce")}</code><code>{t("aiGuide.exampleRepeat")}</code><code>{t("aiGuide.exampleOther")}</code></div>
      <p className="ai-guide-note">{t("aiGuide.note")}</p>
    </article>
  </section>;
}

function McpInstallPage({ locale, onBack, onCopy }: { locale: Locale; onBack: () => void; onCopy: () => Promise<void> }) {
  const [prompt, setPrompt] = useState("");
  useEffect(() => { void invoke<string>("mcp_install_prompt", { locale }).then(setPrompt).catch(() => setPrompt("")); }, [locale]);
  return <section className="sheet mcp-install-sheet"><SheetHeader title={t("settings.installMcpTitle")} onBack={onBack} /><div className="mcp-install-content"><p className="mcp-install-step">{t("settings.installMcpHint")}</p><textarea readOnly value={prompt} aria-label={t("settings.installMcpTitle")} /><button className="button primary" onClick={() => void onCopy()}>{t("settings.copy")}</button><div className="mcp-install-guide"><p>{t("settings.installMcpNote")}</p><p>{t("settings.installMcpGuideNote")}</p><p>{t("settings.installMcpGuideMcp")}</p><p>{t("settings.installMcpGuideSkill")}</p><p>{t("settings.installMcpGuideOnce")}</p><p>{t("settings.installMcpGuideRelocation")}</p></div></div></section>;
}

function ImportPreviewDialog({ preview, operation, theme, onApply, onClose }: { preview: ImportPreview; operation: ImportOperation; theme: Theme; onApply: (sessionId: string, operation: ImportOperation) => Promise<boolean>; onClose: () => void }) {
  const [importing, setImporting] = useState(false);
  const [replaceConfirmation, setReplaceConfirmation] = useState(false);
  const overwrite = operation === "replace";
  const title = overwrite ? t("data.replacePreview") : t("data.mergePreview");
  return createPortal(<div className="dialog-backdrop" onMouseDown={importing ? undefined : onClose}><section className="import-preview-dialog" data-theme={theme} role="dialog" aria-modal="true" aria-label={title} onMouseDown={(event) => event.stopPropagation()}><h2>{title}</h2><p>{preview.sourceFileName}</p><dl><div><dt>{t("data.tasks")}</dt><dd>{t("data.importCountTasks", { count: preview.taskCount })}</dd></div><div><dt>{t("data.categories")}</dt><dd>{t("data.importCountCategories", { count: preview.categoryCount })}</dd></div><div><dt>{t("data.palette")}</dt><dd>{t("data.importCountPalette", { count: preview.paletteCount })}</dd></div>{!overwrite && <><div><dt>{t("data.taskChanges")}</dt><dd>{t("data.changeSummary", { newCount: preview.newTasks, updatedCount: preview.updatedTasks, keptCount: preview.keptTasks })}</dd></div><div><dt>{t("data.categoryChanges")}</dt><dd>{t("data.changeSummary", { newCount: preview.newCategories, updatedCount: preview.updatedCategories, keptCount: preview.keptCategories })}</dd></div></>}</dl><p className={`import-preview-note ${overwrite ? "replace-warning" : ""}`}>{overwrite ? replaceConfirmation ? t("data.replaceConfirmNote") : t("data.replaceNote") : t("data.mergeNote")}</p><footer className="dialog-actions"><button className="button secondary" disabled={importing} onClick={onClose}>{t("data.cancel")}</button><button className="button primary" disabled={importing} onClick={async () => { if (overwrite && !replaceConfirmation) { setReplaceConfirmation(true); return; } setImporting(true); const done = await onApply(preview.sessionId, operation); if (!done) setImporting(false); }}>{importing ? t("data.importing") : t("data.confirmImport")}</button></footer></section></div>, document.body);
}

function EncryptedImportPasswordDialog({ theme, request, onClose, onPreview }: { theme: Theme; request: EncryptedImportRequest; onClose: () => void; onPreview: (sessionId: string, password: string, operation: ImportOperation) => Promise<boolean> }) {
  const [password, setPassword] = useState("");
  const [unlocking, setUnlocking] = useState(false);
  const valid = password.length >= 6 && password.length <= 32;
  return createPortal(<div className="dialog-backdrop" onMouseDown={unlocking ? undefined : onClose}><section className="encrypted-import-dialog" data-theme={theme} role="dialog" aria-modal="true" aria-label={t("dialog.importPassword")} onMouseDown={(event) => event.stopPropagation()}><h2>{t("data.encryptedFile")}</h2><p>{request.sourceFileName}</p><label>{t("data.importPassword")}<input type="password" value={password} autoFocus maxLength={32} onChange={(event) => setPassword(event.target.value)} /></label>{password.length > 0 && !valid && <span role="alert">{t("data.passwordLength")}</span>}<p className="import-preview-note">{t("data.passwordOnly")}</p><footer className="dialog-actions"><button className="button secondary" disabled={unlocking} onClick={onClose}>{t("data.cancel")}</button><button className="button primary" disabled={!valid || unlocking} onClick={async () => { setUnlocking(true); const done = await onPreview(request.sessionId, password, request.operation); if (!done) setUnlocking(false); }}>{unlocking ? t("data.verifying") : t("data.continue")}</button></footer></section></div>, document.body);
}

function CategorySettings({ theme, categories, palette, categoryUsage, onCreate, onUpdate, onDelete, onRestoreDefaults }: { theme: Theme; categories: Category[]; palette: BootstrapData["palette"]; categoryUsage: Record<string, number>; onCreate: () => void; onUpdate: (input: { id: string; name: string; colorId: string }) => Promise<boolean>; onDelete: (id: string, targetCategoryId?: string) => Promise<boolean>; onRestoreDefaults: () => Promise<boolean> }) {
  const [editingId, setEditingId] = useState<string | null>(null);
  const [draftName, setDraftName] = useState("");
  const [deleting, setDeleting] = useState<Category | null>(null);
  const [deleteConfirmingId, setDeleteConfirmingId] = useState<string | null>(null);
  const [deleteCollapsingId, setDeleteCollapsingId] = useState<string | null>(null);
  const [deleteFadingId, setDeleteFadingId] = useState<string | null>(null);
  const [deleteTransitionLocked, setDeleteTransitionLocked] = useState(false);
  const [restoreDialogOpen, setRestoreDialogOpen] = useState(false);
  const editingRowRef = useRef<HTMLDivElement>(null);
  const deleteTimerRef = useRef<number | null>(null);

  useEffect(() => {
    if (!editingId) return;
    const dismiss = (event: PointerEvent) => {
      if (!editingRowRef.current?.contains(event.target as Node)) setEditingId(null);
    };
    window.addEventListener("pointerdown", dismiss);
    return () => window.removeEventListener("pointerdown", dismiss);
  }, [editingId]);
  useEffect(() => {
    if (!deleteConfirmingId) return;
    const dismiss = (event: PointerEvent) => {
      if (!(event.target as HTMLElement).closest(".category-delete")) collapseDeleteConfirmation();
    };
    window.addEventListener("pointerdown", dismiss);
    return () => window.removeEventListener("pointerdown", dismiss);
  }, [deleteConfirmingId]);
  useEffect(() => () => { if (deleteTimerRef.current) window.clearTimeout(deleteTimerRef.current); }, []);

  function beginEdit(category: Category) {
    setDeleting(null);
    setEditingId(category.id);
    setDraftName(categoryLabel(category));
  }
  async function saveEdit() {
    if (!editingId) return;
    const category = categories.find((item) => item.id === editingId);
    if (category && await onUpdate({ id: editingId, name: draftName, colorId: category.colorId })) setEditingId(null);
  }
  async function saveColor(category: Category, colorId: string) {
    setEditingId(null);
    await onUpdate({ id: category.id, name: category.name, colorId });
  }
  async function remove(targetCategoryId?: string) {
    if (!deleting) return;
    if (await onDelete(deleting.id, targetCategoryId)) setDeleting(null);
  }
  async function handleDelete(category: Category) {
    if (deleteTransitionLocked) return;
    if (deleteConfirmingId === category.id) {
      setDeleteConfirmingId(null);
      if ((categoryUsage[category.id] ?? 0) > 0) setDeleting(category);
      else await onDelete(category.id);
      return;
    }
    if (deleteConfirmingId) {
      const previousId = deleteConfirmingId;
      setDeleteTransitionLocked(true);
      setDeleteCollapsingId(previousId);
      setDeleteConfirmingId(category.id);
      if (deleteTimerRef.current) window.clearTimeout(deleteTimerRef.current);
      deleteTimerRef.current = window.setTimeout(() => {
        setDeleteCollapsingId(null);
        setDeleteTransitionLocked(false);
      }, CONFIRM_TRANSITION_MS);
      return;
    }
    setEditingId(null);
    setDeleteTransitionLocked(true);
    setDeleteFadingId(null);
    setDeleteConfirmingId(category.id);
    if (deleteTimerRef.current) window.clearTimeout(deleteTimerRef.current);
    deleteTimerRef.current = window.setTimeout(() => setDeleteTransitionLocked(false), CONFIRM_TRANSITION_MS);
  }

  function collapseDeleteConfirmation() {
    if (!deleteConfirmingId || deleteTransitionLocked) return;
    const id = deleteConfirmingId;
    setDeleteTransitionLocked(true);
    setDeleteConfirmingId(null);
    setDeleteCollapsingId(id);
    if (deleteTimerRef.current) window.clearTimeout(deleteTimerRef.current);
    deleteTimerRef.current = window.setTimeout(() => {
      setDeleteCollapsingId(null);
      setDeleteFadingId(id);
      deleteTimerRef.current = window.setTimeout(() => {
        setDeleteFadingId(null);
        setDeleteTransitionLocked(false);
      }, CONFIRM_TRANSITION_MS);
    }, CONFIRM_TRANSITION_MS);
  }

  return <div className="category-settings">
    <div className="category-list" aria-label={t("settings.categoryList")}>
      {categories.map((category) => {
        const editing = editingId === category.id;
        const deleteConfirming = deleteConfirmingId === category.id;
        const deleteCollapsing = deleteCollapsingId === category.id;
        const deleteFading = deleteFadingId === category.id;
        return <div ref={editing ? editingRowRef : undefined} className={`category-row ${editing ? "editing" : ""} ${deleteConfirming ? "delete-confirming" : ""} ${deleteCollapsing ? "delete-collapsing" : ""} ${deleteFading ? "delete-fading" : ""}`} key={category.id} onMouseLeave={() => collapseDeleteConfirmation()} onClick={() => { if (!editing) beginEdit(category); }}>
          <ColorPicker theme={theme} palette={palette} selectedId={category.colorId} onSelect={(colorId) => void saveColor(category, colorId)} />
          {editing ? <input className="category-name-input" aria-label={t("settings.categoryName")} autoFocus value={draftName} maxLength={30} onChange={(event) => setDraftName(event.target.value)} onKeyDown={(event) => { if (event.key === "Enter") void saveEdit(); if (event.key === "Escape") setEditingId(null); }} /> : <span className="category-name">{categoryLabel(category)}</span>}
          <div className="category-actions">
            {editing ? <button className="button primary category-save" onClick={(event) => { event.stopPropagation(); void saveEdit(); }}>{t("settings.saveCategory")}</button> : <CategoryDeleteButton categoryName={categoryLabel(category)} confirming={deleteConfirming} collapsing={deleteCollapsing} fading={deleteFading} locked={deleteTransitionLocked} onClick={() => void handleDelete(category)} />}
          </div>
        </div>;
      })}
    </div>
    <div className="category-footer-actions"><button className="category-add" onClick={onCreate}><img src={icon("add_24_regular.svg")} alt="" />{t("settings.addCategory")}</button><button className="category-restore" title={t("settings.restoreDefaults")} aria-label={t("settings.restoreDefaults")} onClick={() => setRestoreDialogOpen(true)}><img src={icon("arrow_clockwise_20_regular.svg")} alt="" /></button></div>
    {deleting && <CategoryDeleteDialog theme={theme} category={deleting} usageCount={categoryUsage[deleting.id] ?? 0} targets={categories.filter((item) => item.id !== deleting.id)} onCancel={() => setDeleting(null)} onDelete={remove} />}
    {restoreDialogOpen && <RestoreDefaultCategoriesDialog theme={theme} onCancel={() => setRestoreDialogOpen(false)} onRestore={async () => { if (await onRestoreDefaults()) setRestoreDialogOpen(false); }} />}
  </div>;
}

function CategoryDeleteButton({ categoryName, confirming, collapsing, fading, locked, onClick }: { categoryName: string; confirming: boolean; collapsing: boolean; fading: boolean; locked: boolean; onClick: () => void }) {
  const label = t("task.delete");
  const labelRef = useRef<HTMLSpanElement>(null);
  const [width, setWidth] = useState(70);
  useEffect(() => {
    const measured = labelRef.current?.scrollWidth ?? 0;
    if (measured) setWidth(Math.ceil(24 + 16 + 6 + measured));
  }, [label]);
  return <button className={`icon-control category-action category-delete ${confirming ? "expanded" : ""} ${collapsing ? "collapsing" : ""} ${fading ? "fading" : ""}`} style={{ "--category-delete-width": `${width}px` } as CSSProperties} aria-label={confirming ? t("settings.confirmDeleteCategory", { name: categoryName }) : `${label}${categoryName}`} aria-disabled={locked} onClick={(event) => { event.stopPropagation(); onClick(); }}><img className="delete-base-icon" src={icon("delete_20_regular.svg")} alt="" /><img className="delete-overlay-icon" src={icon("delete_20_regular.svg")} alt="" /><span ref={labelRef}>{label}</span></button>;
}

function ColorPicker({ theme, palette, selectedId, onSelect }: { theme: Theme; palette: BootstrapData["palette"]; selectedId: string; onSelect: (id: string) => void }) {
  const [open, setOpen] = useState(false);
  const [position, setPosition] = useState({ left: 0, top: 0 });
  const triggerRef = useRef<HTMLButtonElement>(null);
  const popoverRef = useRef<HTMLDivElement>(null);
  const selected = palette.find((color) => color.id === selectedId) ?? palette[0];
  useEffect(() => {
    if (!open || !triggerRef.current) return;
    const update = () => {
      const rect = triggerRef.current!.getBoundingClientRect();
      const popoverHeight = 90;
      const below = rect.bottom + popoverHeight + 5;
      setPosition({
        left: Math.max(8, Math.min(rect.left, window.innerWidth - 236)),
        top: below <= window.innerHeight ? rect.bottom + 5 : Math.max(8, rect.top - popoverHeight - 5),
      });
    };
    const dismiss = (event: PointerEvent) => { const target = event.target as Node; if (!triggerRef.current?.contains(target) && !popoverRef.current?.contains(target)) setOpen(false); };
    update(); window.addEventListener("resize", update); window.addEventListener("pointerdown", dismiss);
    return () => { window.removeEventListener("resize", update); window.removeEventListener("pointerdown", dismiss); };
  }, [open]);
  if (!selected) return null;
  return <>
    <button ref={triggerRef} className="color-picker-trigger" aria-label={t("settings.categoryColor")} aria-expanded={open} onClick={(event) => { event.stopPropagation(); setOpen((value) => !value); }}><span className="category-dot category-editor-dot" style={{ "--category-color": selected.value } as CSSProperties} /></button>
    {open && createPortal(<div ref={popoverRef} className="color-picker-popover" data-theme={theme} role="listbox" aria-label={t("settings.categoryColorList")} style={position}>{palette.map((color) => <button key={color.id} role="option" aria-selected={color.id === selectedId} className={color.id === selectedId ? "selected" : ""} onClick={() => { onSelect(color.id); setOpen(false); }}><span className="category-dot category-editor-dot" style={{ "--category-color": color.value } as CSSProperties} /></button>)}</div>, document.body)}
  </>;
}

function TodoTaskDeleteDialog({ theme, task, onCancel, onConfirm }: { theme: Theme; task: Task; onCancel: () => void; onConfirm: () => Promise<boolean> }) {
  const [deleting, setDeleting] = useState(false);
  return createPortal(<div className="category-dialog-backdrop" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget && !deleting) onCancel(); }}><section className="category-delete-dialog todo-task-delete-dialog" data-theme={theme} role="dialog" aria-modal="true" aria-labelledby="todo-task-delete-title">
  <h2 id="todo-task-delete-title">{t("dialog.deleteTaskTitle")}</h2>
    <p>{t("dialog.deleteTaskMessage", { name: task.title })}</p>
    <footer className="dialog-actions"><button className="button secondary" disabled={deleting} onClick={onCancel}>{t("data.cancel")}</button><button className="button primary" disabled={deleting} onClick={async () => { setDeleting(true); if (!await onConfirm()) setDeleting(false); }}>{deleting ? t("task.deleting") : t("task.delete")}</button></footer>
  </section></div>, document.body);
}

function CategoryDeleteDialog({ theme, category, usageCount, targets, onCancel, onDelete }: { theme: Theme; category: Category; usageCount: number; targets: Category[]; onCancel: () => void; onDelete: (targetCategoryId?: string) => void }) {
  const [targetId, setTargetId] = useState(targets[0]?.id ?? "");
  return createPortal(<div className="category-dialog-backdrop" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget) onCancel(); }}><section className="category-delete-dialog" data-theme={theme} role="dialog" aria-modal="true" aria-labelledby="category-delete-title">
    <h2 id="category-delete-title">{t("dialog.deleteCategoryTitle", { name: categoryLabel(category) })}</h2>
    {usageCount > 0 ? <><p>{t("dialog.categoryDeleteReferenced", { count: usageCount })}</p><select value={targetId} onChange={(event) => setTargetId(event.target.value)}>{targets.map((target) => <option key={target.id} value={target.id}>{categoryLabel(target)}</option>)}</select></> : <p>{t("dialog.categoryDeleteMessage")}</p>}
    <footer className="dialog-actions"><button className="button secondary" onClick={onCancel}>{t("data.cancel")}</button><button className="button danger" onClick={() => onDelete(usageCount > 0 ? targetId : undefined)}>{t("task.delete")}</button></footer>
  </section></div>, document.body);
}

function RestoreDefaultCategoriesDialog({ theme, onCancel, onRestore }: { theme: Theme; onCancel: () => void; onRestore: () => Promise<void> }) {
  const [restoring, setRestoring] = useState(false);
  return createPortal(<div className="category-dialog-backdrop" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget && !restoring) onCancel(); }}><section className="category-delete-dialog restore-default-dialog" data-theme={theme} role="dialog" aria-modal="true" aria-labelledby="restore-default-title">
    <h2 id="restore-default-title">{t("dialog.restoreTitle")}</h2>
    <p>{t("dialog.restoreMessage")}</p>
    <footer className="dialog-actions"><button className="button secondary" disabled={restoring} onClick={onCancel}>{t("data.cancel")}</button><button className="button primary restore-confirm-button" disabled={restoring} onClick={async () => { setRestoring(true); await onRestore(); }}>{t("dialog.restoreConfirm")}</button></footer>
  </section></div>, document.body);
}

function ThemeModeToggle({ value, onChange }: { value: Theme; onChange: (theme: Theme) => void }) {
  return <div className="theme-mode-toggle" role="group" aria-label={t("settings.appearance")}>
    <button type="button" className={value === "light" ? "selected" : ""} aria-label={t("settings.themeLight")} aria-pressed={value === "light"} onClick={() => onChange("light")}><img src={icon("weather_sunny_20_regular.svg")} alt="" /></button>
    <button type="button" className={value === "dark" ? "selected" : ""} aria-label={t("settings.themeDark")} aria-pressed={value === "dark"} onClick={() => onChange("dark")}><img src={icon("weather_moon_20_regular.svg")} alt="" /></button>
  </div>;
}

function CategoryDropdown({ value, categories, theme, onChange }: { value: string; categories: Category[]; theme: Theme; onChange: (categoryId: string) => void }) {
  const [open, setOpen] = useState(false);
  const [position, setPosition] = useState({ left: 0, top: 0, width: 0, maxHeight: 0 });
  const triggerRef = useRef<HTMLButtonElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const selected = categories.find((category) => category.id === value) ?? categories[0];
  useEffect(() => {
    if (!open || !triggerRef.current) return;
    const updatePosition = () => {
      const rect = triggerRef.current!.getBoundingClientRect();
      const viewportPadding = 8;
      const desiredHeight = Math.min(224, Math.max(40, categories.length * 32 + 8));
      const availableBelow = window.innerHeight - rect.bottom - viewportPadding;
      const availableAbove = rect.top - viewportPadding;
      const placeAbove = availableBelow < Math.min(desiredHeight, 96) && availableAbove > availableBelow;
      const maxHeight = Math.max(40, Math.min(desiredHeight, placeAbove ? availableAbove - 5 : availableBelow - 5));
      setPosition({ left: Math.max(viewportPadding, Math.min(rect.left, window.innerWidth - rect.width - viewportPadding)), top: placeAbove ? Math.max(viewportPadding, rect.top - maxHeight - 5) : rect.bottom + 5, width: rect.width, maxHeight });
    };
    const dismiss = (event: PointerEvent) => {
      const target = event.target as Node;
      if (!triggerRef.current?.contains(target) && !menuRef.current?.contains(target)) setOpen(false);
    };
    updatePosition();
    window.addEventListener("resize", updatePosition);
    window.addEventListener("pointerdown", dismiss);
    return () => { window.removeEventListener("resize", updatePosition); window.removeEventListener("pointerdown", dismiss); };
  }, [open, categories.length]);
  return <>
    <button ref={triggerRef} type="button" className={`task-category-trigger ${open ? "open" : ""}`} aria-haspopup="listbox" aria-expanded={open} onClick={() => setOpen((current) => !current)}>
      <span className="category-dot" style={{ "--category-color": selected?.color ?? "#8caeff" } as CSSProperties} />
      <span>{selected ? categoryLabel(selected) : t("form.selectCategory")}</span><img src={icon("chevron_right_20_regular.svg")} alt="" />
    </button>
    {open && createPortal(<div ref={menuRef} className="task-category-menu" data-theme={theme} role="listbox" aria-label={t("form.type")} style={{ left: position.left, top: position.top, width: position.width, maxHeight: position.maxHeight }}>
      {categories.map((category) => <button key={category.id} type="button" role="option" aria-selected={value === category.id} className={value === category.id ? "selected" : ""} onClick={() => { onChange(category.id); setOpen(false); }}><span className="category-dot" style={{ "--category-color": category.color } as CSSProperties} />{categoryLabel(category)}</button>)}
    </div>, document.body)}
  </>;
}

function LanguageDropdown({ value, theme, onChange }: { value: Locale; theme: Theme; onChange: (locale: Locale) => void }) {
  const [open, setOpen] = useState(false);
  const [position, setPosition] = useState({ left: 0, top: 0, width: 0, maxHeight: 0 });
  const triggerRef = useRef<HTMLButtonElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const selected = localeOptions.find((option) => option.locale === value) ?? localeOptions[0];
  useEffect(() => {
    if (!open || !triggerRef.current) return;
    const update = () => {
      const rect = triggerRef.current!.getBoundingClientRect();
      const padding = 8;
      const desired = localeOptions.length * 30 + 8;
      const below = window.innerHeight - rect.bottom - padding;
      const above = rect.top - padding;
      const placeAbove = below < desired && above > below;
      const maxHeight = Math.max(40, Math.min(desired, (placeAbove ? above : below) - 5));
      setPosition({ left: Math.max(padding, Math.min(rect.left, window.innerWidth - rect.width - padding)), top: placeAbove ? Math.max(padding, rect.top - maxHeight - 5) : rect.bottom + 5, width: rect.width, maxHeight });
    };
    const dismiss = (event: PointerEvent) => { const target = event.target as Node; if (!triggerRef.current?.contains(target) && !menuRef.current?.contains(target)) setOpen(false); };
    update(); window.addEventListener("resize", update); window.addEventListener("pointerdown", dismiss);
    return () => { window.removeEventListener("resize", update); window.removeEventListener("pointerdown", dismiss); };
  }, [open]);
  return <><button ref={triggerRef} type="button" className={`task-category-trigger language-trigger ${open ? "open" : ""}`} aria-haspopup="listbox" aria-expanded={open} onClick={() => setOpen((current) => !current)}><span>{selected.name}</span><img src={icon("chevron_right_20_regular.svg")} alt="" /></button>{open && createPortal(<div ref={menuRef} className="task-category-menu language-menu" data-theme={theme} role="listbox" aria-label={t("settings.language")} style={{ left: position.left, top: position.top, width: position.width, maxHeight: position.maxHeight }}>{localeOptions.map((option) => <button key={option.locale} type="button" role="option" aria-selected={value === option.locale} className={value === option.locale ? "selected" : ""} onClick={() => { onChange(option.locale); setOpen(false); }}>{option.name}</button>)}</div>, document.body)}</>;
}

function TransparencyDropdown({ value, theme, onChange }: { value: InterfaceTransparency; theme: Theme; onChange: (value: InterfaceTransparency) => void }) {
  const values: InterfaceTransparency[] = [0, 5, 10, 15, 20, 25, 30];
  const [open, setOpen] = useState(false);
  const [position, setPosition] = useState({ left: 0, top: 0, width: 0, maxHeight: 0 });
  const triggerRef = useRef<HTMLButtonElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open || !triggerRef.current) return;
    const update = () => { const rect = triggerRef.current!.getBoundingClientRect(); const padding = 8; const desired = Math.min(224, values.length * 32 + 8); const below = window.innerHeight - rect.bottom - padding; const above = rect.top - padding; const placeAbove = below < desired && above > below; const maxHeight = Math.max(40, Math.min(desired, (placeAbove ? above : below) - 5)); setPosition({ left: Math.max(padding, Math.min(rect.left, window.innerWidth - rect.width - padding)), top: placeAbove ? Math.max(padding, rect.top - maxHeight - 5) : rect.bottom + 5, width: rect.width, maxHeight }); };
    const dismiss = (event: PointerEvent) => { const target = event.target as Node; if (!triggerRef.current?.contains(target) && !menuRef.current?.contains(target)) setOpen(false); };
    update(); window.addEventListener("resize", update); window.addEventListener("pointerdown", dismiss);
    return () => { window.removeEventListener("resize", update); window.removeEventListener("pointerdown", dismiss); };
  }, [open]);
  return <><button ref={triggerRef} type="button" className={`task-category-trigger language-trigger ${open ? "open" : ""}`} aria-haspopup="listbox" aria-expanded={open} aria-label={t("settings.interfaceTransparency")} onClick={() => setOpen((current) => !current)}><span>{value}%</span><img src={icon("chevron_right_20_regular.svg")} alt="" /></button>{open && createPortal(<div ref={menuRef} className="task-category-menu language-menu" data-theme={theme} role="listbox" aria-label={t("settings.interfaceTransparency")} style={{ left: position.left, top: position.top, width: position.width, maxHeight: position.maxHeight }}>{values.map((option) => <button key={option} type="button" role="option" aria-selected={value === option} className={value === option ? "selected" : ""} onClick={() => { onChange(option); setOpen(false); }}>{option}%</button>)}</div>, document.body)}</>;
}

function TaskRow({ task, theme, onOpen, onEdit, onCopy, onStatus, onDelete }: { task: Task; theme: Theme; onOpen: () => void; onEdit: () => void; onCopy: () => void; onStatus: () => Promise<boolean>; onDelete: () => void }) {
  const [confirming, setConfirming] = useState<"status" | "delete" | null>(null);
  const [transitionLocked, setTransitionLocked] = useState(false);
  const [collapseAfterTransition, setCollapseAfterTransition] = useState(false);
  const [collapsing, setCollapsing] = useState<"status" | "delete" | null>(null);
  const [menuOpen, setMenuOpen] = useState(false);
  const [tooltip, setTooltip] = useState<{ x: number; y: number } | null>(null);
  const [statusExiting, setStatusExiting] = useState(false);
  const statusRef = useRef<HTMLButtonElement>(null);
  const deleteRef = useRef<HTMLButtonElement>(null);
  const menuRef = useRef<HTMLButtonElement>(null);
  const titleRef = useRef<HTMLSpanElement>(null);
  const deleteLabelRef = useRef<HTMLSpanElement>(null);
  const statusLabelRef = useRef<HTMLSpanElement>(null);
  const timerRef = useRef<number | null>(null);
  const [deleteConfirmWidth, setDeleteConfirmWidth] = useState(64);
  const [statusConfirmWidth, setStatusConfirmWidth] = useState(72);
  const [statusFading, setStatusFading] = useState(false);
  const [deleteFading, setDeleteFading] = useState(false);
  const completed = task.status === "completed";
  const statusLabel = completed ? t("task.moveToTodo") : t("task.complete");
  const statusIcon = completed ? "arrow_left_20_regular.svg" : "checkmark_20_regular.svg";
  const deleteLabel = t("task.delete");

  useEffect(() => () => { if (timerRef.current) window.clearTimeout(timerRef.current); }, []);
  useEffect(() => {
    const labelWidth = deleteLabelRef.current?.scrollWidth ?? 0;
    if (labelWidth) setDeleteConfirmWidth(Math.ceil(16 + 6 + labelWidth + 24));
    const statusLabelWidth = statusLabelRef.current?.scrollWidth ?? 0;
    if (statusLabelWidth) setStatusConfirmWidth(Math.max(72, Math.ceil(8 + 14 + 6 + statusLabelWidth + 12)));
  }, [deleteLabel, statusLabel]);
  useEffect(() => { setConfirming(null); setCollapsing(null); setStatusFading(false); setDeleteFading(false); setTransitionLocked(false); setMenuOpen(false); setStatusExiting(false); }, [task.status]);
  useEffect(() => {
    if (!transitionLocked && collapseAfterTransition) {
      setCollapseAfterTransition(false);
      closeConfirmation();
    }
  }, [collapseAfterTransition, transitionLocked]);
  useEffect(() => {
    if (!confirming) return;
    const closeFromOutside = (event: PointerEvent) => {
      const target = event.target as Node;
      if (statusRef.current?.contains(target) || deleteRef.current?.contains(target) || menuRef.current?.contains(target)) return;
      closeConfirmation();
    };
    window.addEventListener("pointerdown", closeFromOutside);
    return () => window.removeEventListener("pointerdown", closeFromOutside);
  }, [confirming]);

  function collapseConfirmation(after?: () => void) {
    if (!confirming || transitionLocked) return;
    setTransitionLocked(true);
    setCollapsing(confirming);
    setConfirming(null);
    if (timerRef.current) window.clearTimeout(timerRef.current);
    timerRef.current = window.setTimeout(() => {
      setCollapsing(null);
      if (confirming === "status") {
        setStatusFading(true);
        timerRef.current = window.setTimeout(() => {
          setStatusFading(false);
          setTransitionLocked(false);
          after?.();
        }, CONFIRM_TRANSITION_MS);
      } else if (confirming === "delete") {
        setDeleteFading(true);
        timerRef.current = window.setTimeout(() => {
          setDeleteFading(false);
          setTransitionLocked(false);
          after?.();
        }, CONFIRM_TRANSITION_MS);
      } else {
        setTransitionLocked(false);
        after?.();
      }
    }, CONFIRM_TRANSITION_MS);
  }
  function switchConfirmation(next: "status" | "delete") {
    if (!confirming || transitionLocked) return;
    setTransitionLocked(true);
    setCollapseAfterTransition(false);
    setStatusFading(false);
    setDeleteFading(false);
    setCollapsing(confirming);
    setConfirming(next);
    if (timerRef.current) window.clearTimeout(timerRef.current);
    timerRef.current = window.setTimeout(() => {
      setCollapsing(null);
      setTransitionLocked(false);
    }, CONFIRM_TRANSITION_MS);
  }
  function lockTransition(next: "status" | "delete" | null) {
    if (transitionLocked) return;
    if (next === null && confirming) {
      collapseConfirmation();
      return;
    }
    setTransitionLocked(true);
    setCollapsing(null);
    setStatusFading(false);
    setDeleteFading(false);
    setConfirming(next);
    if (timerRef.current) window.clearTimeout(timerRef.current);
    timerRef.current = window.setTimeout(() => { setCollapsing(null); setTransitionLocked(false); }, CONFIRM_TRANSITION_MS);
  }
  function closeConfirmation() { if (confirming) lockTransition(null); }
  function handleStatus() {
    if (transitionLocked) return;
    setMenuOpen(false);
    if (confirming === "status") {
      void onStatus().then((changed) => {
        if (!changed) { setStatusExiting(false); setTransitionLocked(false); }
      });
      setStatusExiting(true);
      collapseConfirmation();
      return;
    }
    if (confirming === "delete") { switchConfirmation("status"); return; }
    lockTransition("status");
  }
  function handleDelete() {
    if (transitionLocked) return;
    if (confirming === "delete") { onDelete(); setStatusExiting(true); collapseConfirmation(); return; }
    if (confirming === "status") { switchConfirmation("delete"); return; }
    lockTransition("delete");
  }
  function handleMenuToggle() {
    if (transitionLocked) return;
    if (confirming) {
      // Start both motions together: the confirmation contracts while the
      // contextual menu enters, rather than serializing two 300ms transitions.
      setMenuOpen(true);
      collapseConfirmation();
      return;
    }
    setMenuOpen((open) => !open);
  }
  function showTooltip() {
    const title = titleRef.current;
    if (!title || title.scrollWidth <= title.clientWidth) return;
    const rect = title.getBoundingClientRect();
    setTooltip({ x: rect.left, y: rect.bottom + 6 });
  }
  return <article className={`task-row task-row-${task.status} ${confirming === "status" ? "status-confirming" : ""} ${confirming === "delete" ? "delete-confirming" : ""} ${collapsing === "status" ? "status-collapsing" : ""} ${collapsing === "delete" ? "delete-collapsing" : ""} ${statusFading ? "status-fading" : ""} ${deleteFading ? "delete-fading" : ""} ${statusExiting ? `status-exiting status-exiting-${completed ? "left" : "right"}` : ""}`} style={{ "--delete-confirm-width": `${deleteConfirmWidth}px`, "--status-confirm-width": `${statusConfirmWidth}px` } as CSSProperties} onMouseLeave={() => { setTooltip(null); if (statusExiting) return; if (transitionLocked) setCollapseAfterTransition(true); else closeConfirmation(); }}>
    <button ref={statusRef} className="task-status" aria-label={confirming === "status" ? `${t("task.confirm")}${statusLabel}` : (completed ? t("task.moveToTodo") : t("task.completeAction"))} aria-disabled={transitionLocked} onClick={handleStatus}>
      <span className="category-dot" style={{ "--category-color": task.categoryColor } as CSSProperties} />
      <span className="task-status-action" aria-hidden="true"><img src={icon(statusIcon)} alt="" /></span>
      <span className="task-status-confirm-icon" aria-hidden="true"><img src={icon(statusIcon)} alt="" /></span>
      <span ref={statusLabelRef} className="task-status-confirm-text">{statusLabel}</span>
    </button>
    <button className="task-main" onClick={onOpen} onMouseEnter={showTooltip} onMouseLeave={() => setTooltip(null)}><span ref={titleRef} className="task-title">{task.title}</span></button>
    <span className={`task-time${!completed && task.dueAtUtcMs !== null && task.dueAtUtcMs < Date.now() ? " overdue" : ""}`} aria-label={!completed && task.dueAtUtcMs ? t("calendar.due") : undefined}>{completed ? "" : formatTaskTime(task.dueAtUtcMs)}</span>
    {completed ? <button ref={deleteRef} className="task-delete" aria-label={confirming === "delete" ? t("task.confirmDelete") : t("task.deleteTask")} aria-disabled={transitionLocked} onClick={handleDelete}><img className="delete-base-icon" src={icon("delete_20_regular.svg")} alt="" /><img className="delete-overlay-icon" src={icon("delete_20_regular.svg")} alt="" /><span ref={deleteLabelRef}>{deleteLabel}</span></button> : <><button ref={menuRef} className={`task-menu-trigger ${menuOpen ? "open" : ""}`} aria-label={t("task.more")} aria-expanded={menuOpen} onClick={handleMenuToggle}><span>•••</span></button>{menuOpen && <TaskMenu anchor={menuRef.current} theme={theme} onEdit={() => { setMenuOpen(false); onEdit(); }} onCopy={() => { setMenuOpen(false); onCopy(); }} onDelete={() => { setMenuOpen(false); onDelete(); }} onDismiss={() => setMenuOpen(false)} />}</>}
    {tooltip && <TaskTitleTooltip x={tooltip.x} y={tooltip.y} title={task.title} theme={theme} />}
  </article>;
}

function TaskMenu({ anchor, theme, onEdit, onCopy, onDelete, onDismiss }: { anchor: HTMLButtonElement | null; theme: Theme; onEdit: () => void; onCopy: () => void; onDelete: () => void; onDismiss: () => void }) {
  const [position, setPosition] = useState<{ right: number; top: number } | null>(null);
  useEffect(() => {
    if (!anchor) return;
    const update = () => {
      const rect = anchor.getBoundingClientRect();
      const menuHeight = 98;
      setPosition({ right: Math.max(8, window.innerWidth - rect.right), top: rect.bottom + menuHeight + 5 > window.innerHeight ? Math.max(8, rect.top - menuHeight - 5) : rect.bottom + 5 });
    };
    update(); window.addEventListener("resize", update); window.addEventListener("scroll", update, true);
    const onPointer = (event: PointerEvent) => { if (!anchor.contains(event.target as Node)) onDismiss(); };
    window.addEventListener("pointerdown", onPointer);
    return () => { window.removeEventListener("resize", update); window.removeEventListener("scroll", update, true); window.removeEventListener("pointerdown", onPointer); };
  }, [anchor, onDismiss]);
  if (!position) return null;
  return createPortal(<div className="task-floating-menu" data-theme={theme} style={position} role="menu" onPointerDown={(event) => event.stopPropagation()}><button type="button" role="menuitem" onClick={onEdit}>{t("task.edit")}</button><button type="button" role="menuitem" onClick={onCopy}>{t("task.copyTitle")}</button><button type="button" role="menuitem" onClick={onDelete}>{t("task.deleteTask")}</button></div>, document.body);
}

function TaskTitleTooltip({ x, y, title, theme }: { x: number; y: number; title: string; theme: Theme }) {
  return createPortal(<div className="task-title-tooltip" data-theme={theme} style={{ left: x, top: y }} role="tooltip">{title}</div>, document.body);
}

function TaskList({ children }: { children: React.ReactNode }) {
  const listRef = useRef<HTMLDivElement>(null);
  const [metrics, setMetrics] = useState({ scrollTop: 0, clientHeight: 0, scrollHeight: 0 });
  const syncMetrics = () => {
    const element = listRef.current;
    if (!element) return;
    setMetrics({ scrollTop: element.scrollTop, clientHeight: element.clientHeight, scrollHeight: element.scrollHeight });
  };
  useEffect(() => {
    syncMetrics();
    const element = listRef.current;
    if (!element) return;
    const observer = new ResizeObserver(syncMetrics);
    observer.observe(element);
    return () => observer.disconnect();
  }, [children]);
  const maxScroll = Math.max(0, metrics.scrollHeight - metrics.clientHeight);
  const thumbHeight = metrics.clientHeight > 0 ? Math.max(36, Math.round((metrics.clientHeight * metrics.clientHeight) / Math.max(metrics.scrollHeight, 1))) : 0;
  const trackLength = Math.max(0, metrics.clientHeight - thumbHeight);
  const thumbOffset = maxScroll > 0 ? Math.round((metrics.scrollTop / maxScroll) * trackLength) : 0;
  const beginThumbDrag = (event: MouseEvent<HTMLButtonElement>) => {
    const element = listRef.current;
    if (!element || maxScroll === 0) return;
    event.preventDefault();
    const startY = event.clientY;
    const startTop = element.scrollTop;
    const onMove = (moveEvent: globalThis.MouseEvent) => {
      element.scrollTop = Math.max(0, Math.min(maxScroll, startTop + ((moveEvent.clientY - startY) / Math.max(trackLength, 1)) * maxScroll));
    };
    const onUp = () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
  };
  const scrollable = maxScroll > 0;
  return <div className="task-list-wrap"><div ref={listRef} className={`task-list${scrollable ? " has-scrollbar" : ""}`} aria-live="polite" onScroll={syncMetrics}>{children}</div>{scrollable && <div className="task-scroll-track" aria-hidden="true"><button className="task-scroll-thumb" type="button" onMouseDown={beginThumbDrag} style={{ height: thumbHeight, transform: `translateY(${thumbOffset}px)` }} /></div>}</div>;
}

function formatTaskTime(dueAtUtcMs: number | null) {
  if (!dueAtUtcMs) return "";
  const difference = dueAtUtcMs - Date.now();
  const isOverdue = difference < 0;
  const prefix = isOverdue ? "-" : "";
  const elapsed = Math.abs(difference);
  if (elapsed < 3_600_000) return `${prefix}${t("time.minutes", { value: Math.max(1, Math.ceil(elapsed / 60_000)) })}`;
  if (elapsed < 86_400_000) return `${prefix}${t("time.hours", { value: Math.max(1, Math.ceil(elapsed / 3_600_000)) })}`;
  const dayCount = Math.max(1, Math.ceil(elapsed / 86_400_000));
  if (isOverdue) return t("time.overdueDays", { value: dayCount });
  if (dayCount <= 7) return t("time.days", { value: dayCount });
  const date = new Date(dueAtUtcMs);
  if (elapsed > 365 * 86_400_000) return `${prefix}${date.getFullYear()}/${date.getMonth() + 1}/${date.getDate()}`;
  return `${prefix}${date.getMonth() + 1}/${date.getDate()}`;
}

function formatDueAt(value: number | null) {
  return formatDateTime(value);
}

function dueTimestamp(date: Date, hour: number, minute: number) {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate(), hour, minute, 0, 0).getTime();
}

function DateTimePicker({ value, theme, onChange }: { value: number | null; theme: Theme; onChange: (value: number | null) => void }) {
  const triggerRef = useRef<HTMLButtonElement>(null);
  const popoverRef = useRef<HTMLDivElement>(null);
  const [open, setOpen] = useState(false);
  const [position, setPosition] = useState({ top: 8, left: 8 });
  const defaultDate = () => {
    const tomorrow = new Date();
    tomorrow.setDate(tomorrow.getDate() + 1);
    tomorrow.setHours(18, 0, 0, 0);
    return tomorrow;
  };
  // 空截止时间打开时，以明天 18:00 作为选择器内的默认选中值；
  // 在用户真正选择日期或时间前，表单字段本身仍保持为空。
  const selected = value ? new Date(value) : defaultDate();
  const [monthCursor, setMonthCursor] = useState(() => value ? new Date(value) : defaultDate());
  const hour = selected?.getHours() ?? 18;
  const minute = selected?.getMinutes() ?? 0;
  const placePopover = () => {
    const rect = triggerRef.current?.getBoundingClientRect();
    if (!rect) return;
    const width = 336;
    const height = 292;
    const left = Math.max(8, Math.min(window.innerWidth - width - 8, rect.right - width));
    const top = rect.bottom + 6 + height <= window.innerHeight ? rect.bottom + 6 : Math.max(8, rect.top - height - 6);
    setPosition({ top, left });
  };
  useEffect(() => {
    if (!open) return;
    placePopover();
    const dismiss = (event: PointerEvent) => {
      const target = event.target as Node;
      if (!triggerRef.current?.contains(target) && !popoverRef.current?.contains(target)) setOpen(false);
    };
    window.addEventListener("pointerdown", dismiss);
    window.addEventListener("resize", placePopover);
    return () => { window.removeEventListener("pointerdown", dismiss); window.removeEventListener("resize", placePopover); };
  }, [open]);
  const setPart = (part: "hour" | "minute", delta: number) => {
    const base = selected ?? new Date(monthCursor.getFullYear(), monthCursor.getMonth(), monthCursor.getDate(), 18, 0);
    onChange(dueTimestamp(base, part === "hour" ? (hour + delta + 24) % 24 : hour, part === "minute" ? (minute + delta + 60) % 60 : minute));
  };
  const year = monthCursor.getFullYear();
  const month = monthCursor.getMonth();
  const start = new Date(year, month, 1 - new Date(year, month, 1).getDay());
  // 从真实的首格日期逐日递增，不能重新套用当前月份；否则上月日期会被误算进下个月。
  const days = Array.from({ length: 42 }, (_, index) => {
    const day = new Date(start);
    day.setDate(start.getDate() + index);
    return day;
  });
  const selectDay = (day: Date) => { onChange(dueTimestamp(day, hour, minute)); setMonthCursor(day); };
  const sameDay = (day: Date) => day.toDateString() === selected.toDateString();
  const isToday = (day: Date) => day.toDateString() === new Date().toDateString();
  const weekdayCatalog = t("calendar.weekdays");
  // Use an explicit delimiter so locales can provide multi-character abbreviations
  // (for example `Mon.`) while still accepting legacy single-character catalogs.
  const weekdays = weekdayCatalog.includes("|") ? weekdayCatalog.split("|") : Array.from(weekdayCatalog);
  return <div className={`date-time-picker${open ? " is-open" : ""}`}><button ref={triggerRef} type="button" className="date-time-trigger" aria-label={t("calendar.due")} aria-haspopup="dialog" aria-expanded={open} onClick={() => { setMonthCursor(value ? new Date(value) : defaultDate()); setOpen((current) => !current); }}><span>{formatDueAt(value)}</span><img src={icon("clock_20_regular.svg")} alt="" /></button>{open && createPortal(<div ref={popoverRef} className="date-time-popover" data-theme={theme} role="dialog" aria-label={t("calendar.selectDue")} style={position}><section className="calendar-panel"><header className="calendar-panel-header"><strong>{formatMonth(year, month)}</strong><div><button type="button" aria-label={t("calendar.previousMonth")} onClick={() => setMonthCursor(new Date(year, month - 1, 1))}><img className="previous" src={icon("chevron_right_20_regular.svg")} alt="" /></button><button type="button" aria-label={t("calendar.nextMonth")} onClick={() => setMonthCursor(new Date(year, month + 1, 1))}><img src={icon("chevron_right_20_regular.svg")} alt="" /></button></div></header><div className="calendar-weekdays">{weekdays.map((label, index) => <span key={`${index}-${label}`}>{label}</span>)}</div><div className="calendar-days">{days.map((day) => <button type="button" key={day.toISOString()} className={`${isToday(day) ? " today" : ""}${sameDay(day) ? " selected" : ""}`} onClick={() => selectDay(day)}>{day.getDate()}</button>)}</div><footer className="calendar-panel-footer"><button type="button" onClick={() => { const today = new Date(); onChange(dueTimestamp(today, hour, minute)); setMonthCursor(today); }}>{t("calendar.today")}</button><button type="button" onClick={() => { const tomorrow = defaultDate(); onChange(dueTimestamp(tomorrow, hour, minute)); setMonthCursor(tomorrow); }}>{t("calendar.tomorrow")}</button></footer></section><section className="time-panel"><TimeColumn label={t("calendar.hour")} value={hour} maximum={23} onStep={(delta) => setPart("hour", delta)} /><TimeColumn label={t("calendar.minute")} value={minute} maximum={59} onStep={(delta) => setPart("minute", delta)} /></section></div>, document.body)}</div>;
}

function TimeColumn({ label, value, maximum, onStep }: { label: string; value: number; maximum: number; onStep: (delta: number) => void }) {
  const [motion, setMotion] = useState<"up" | "down" | null>(null);
  const [displayValue, setDisplayValue] = useState(value);
  const motionTimer = useRef<number | null>(null);
  useEffect(() => () => { if (motionTimer.current) window.clearTimeout(motionTimer.current); }, []);
  useEffect(() => { if (!motion) setDisplayValue(value); }, [motion, value]);
  const step = (delta: number) => {
    if (motionTimer.current) window.clearTimeout(motionTimer.current);
    const nextValue = (value + delta + maximum + 1) % (maximum + 1);
    setMotion(delta > 0 ? "up" : "down");
    motionTimer.current = window.setTimeout(() => {
      setDisplayValue(nextValue);
      setMotion(null);
    }, 100);
    onStep(delta);
  };
  const isHour = label === t("calendar.hour");
  return <div className="time-column" onWheel={(event) => { event.preventDefault(); step(event.deltaY > 0 ? 1 : -1); }}><span className="time-label">{label}</span><button type="button" className="time-step-button up" aria-label={isHour ? t("calendar.hourUp") : t("calendar.minuteUp")} onClick={() => step(1)}><img src={icon("chevron_right_20_regular.svg")} alt="" /></button><div className="time-values"><div className={`time-values-track${motion ? ` moving-${motion}` : ""}`}>{[-2, -1, 0, 1, 2].map((offset) => <span key={offset} className="time-value">{String((value + offset + maximum + 1) % (maximum + 1)).padStart(2, "0")}</span>)}</div><span className="time-selected-overlay">{String(displayValue).padStart(2, "0")}</span></div><button type="button" className="time-step-button down" aria-label={isHour ? t("calendar.hourDown") : t("calendar.minuteDown")} onClick={() => step(-1)}><img src={icon("chevron_right_20_regular.svg")} alt="" /></button></div>;
}

function TaskForm({ title, task, categories, theme, onBack, onSave }: { title: string; task: Task | null; categories: Category[]; theme: Theme; onBack: () => void; onSave: (input: { title: string; note: string; categoryId: string; dueAtUtcMs: number | null; recurrence: { interval: number; unit: string; action: string; baseTitle: string } | null }) => void }) {
  const parse = (value: string | null | undefined) => { try { const r = value ? JSON.parse(value) : null; return r?.interval ? { enabled:true, interval:r.interval, unit:r.unit, action:r.action } : { enabled:false, interval:1, unit:"week", action:"create_new" }; } catch { return { enabled:false, interval:1, unit:"week", action:"create_new" }; } };
  const initial = () => ({ title: task?.title ?? "", note: task?.note ?? "", categoryId: task?.categoryId ?? categories[0]?.id ?? "", dueAtUtcMs: task?.dueAtUtcMs ?? null, ...parse(task?.recurrenceJson) });
  const [draft, setDraft] = useState(initial);
  useEffect(() => setDraft(initial()), [task, categories]);
  function submit(event: FormEvent) { event.preventDefault(); onSave({ title:draft.title, note:draft.note, categoryId:draft.categoryId, dueAtUtcMs:draft.dueAtUtcMs, recurrence:draft.enabled ? { interval:Math.max(1, Math.min(999, Number(draft.interval)||1)), unit:draft.unit, action:draft.action, baseTitle:draft.title } : null }); }
  const RepeatMark = () => <span className="repeat-mark" aria-hidden="true">{draft.enabled ? <svg viewBox="0 0 16 16"><path d="M3.5 8.25 6.5 11.25 12.5 4.75" /></svg> : <svg viewBox="0 0 16 16"><path d="M4.5 8h7" /></svg>}</span>;
  const units = [{ value:"day", label:t("form.day") }, { value:"week", label:t("form.week") }, { value:"month", label:t("form.month") }, { value:"year", label:t("form.year") }]; const actions = [{ value:"update_due", label:t("form.updateDue") }, { value:"create_new", label:t("form.createNewTask") }];
  return <section className="sheet sheet-with-footer"><SheetHeader title={title} onBack={onBack} /><form id="task-form" className="task-form" onSubmit={submit}><label>{t("form.title")}<input autoFocus value={draft.title} maxLength={200} placeholder={t("form.titlePlaceholder")} onChange={(event) => setDraft({ ...draft, title: event.target.value })} /></label><label>{t("form.note")}<textarea value={draft.note} maxLength={2000} placeholder={t("form.notePlaceholder")} onChange={(event) => setDraft({ ...draft, note: event.target.value })} /></label><label>{t("form.type")}<CategoryDropdown value={draft.categoryId} categories={categories} theme={theme} onChange={(categoryId) => setDraft({ ...draft, categoryId })} /></label><label>{t("form.due")}<div className="due-repeat-row"><DateTimePicker value={draft.dueAtUtcMs} theme={theme} onChange={(dueAtUtcMs) => setDraft({ ...draft, dueAtUtcMs, enabled:dueAtUtcMs ? draft.enabled : false })} /><button type="button" disabled={!draft.dueAtUtcMs} className={`repeat-toggle ${draft.enabled ? "enabled" : ""}`} onClick={() => setDraft({ ...draft, enabled:!draft.enabled })}><RepeatMark />{t("form.repeat")}</button></div></label>{draft.enabled && <div className="repeat-options"><label>{t("form.every")}<span className="repeat-inputs"><RepeatIntervalStepper value={Number(draft.interval) || 1} onChange={(interval) => setDraft({ ...draft, interval })} /><RepeatDropdown value={draft.unit} options={units} theme={theme} onChange={(unit) => setDraft({ ...draft, unit, action:unit === "day" ? "update_due" : "create_new" })} /></span></label><label>{t("form.afterDue")}<RepeatDropdown value={draft.action} options={actions} theme={theme} onChange={(action) => setDraft({ ...draft, action })} /></label></div>}<p className="form-help">{t("form.help")}</p></form><footer className="sheet-footer"><button type="button" className="button secondary" onClick={onBack}>{t("app.back")}</button><button type="submit" form="task-form" className="button primary">{t("task.save")}</button></footer></section>;
}

function RepeatIntervalStepper({ value, onChange }: { value: number; onChange: (value: number) => void }) {
  const update = (next: number) => onChange(Math.max(1, Math.min(999, next || 1)));
  const updateText = (text: string) => update(Number(text.replace(/\D/g, "").slice(0, 3)));
  return <span className="repeat-interval-stepper"><input type="text" inputMode="numeric" maxLength={3} role="spinbutton" aria-valuemin={1} aria-valuemax={999} aria-valuenow={value} value={value} onChange={(event) => updateText(event.target.value)} onKeyDown={(event) => { if (event.key === "ArrowUp") { event.preventDefault(); update(value + 1); } else if (event.key === "ArrowDown") { event.preventDefault(); update(value - 1); } }} /><span className="repeat-step-buttons"><button type="button" aria-label={`${t("form.every")} +`} onClick={() => update(value + 1)}><img src={icon("chevron_right_20_regular.svg")} alt="" /></button><button type="button" aria-label={`${t("form.every")} −`} onClick={() => update(value - 1)}><img src={icon("chevron_right_20_regular.svg")} alt="" /></button></span></span>;
}

function ViewDeleteButton({ onDelete }: { onDelete: () => void }) {
  const [state, setState] = useState<"idle" | "expanded" | "collapsing" | "fading">("idle");
  const [width, setWidth] = useState(76);
  const labelRef = useRef<HTMLSpanElement>(null);
  const timerRef = useRef<number | null>(null);
  const deleteLabel = t("task.delete");
  useEffect(() => {
    const labelWidth = labelRef.current?.scrollWidth ?? 0;
    if (labelWidth) setWidth(Math.ceil(13 + 16 + 6 + labelWidth + 13));
  }, [deleteLabel]);
  useEffect(() => () => { if (timerRef.current) window.clearTimeout(timerRef.current); }, []);
  const collapse = () => {
    if (state !== "expanded") return;
    setState("collapsing");
    timerRef.current = window.setTimeout(() => {
      setState("fading");
      timerRef.current = window.setTimeout(() => setState("idle"), CONFIRM_TRANSITION_MS);
    }, CONFIRM_TRANSITION_MS);
  };
  const activate = () => {
    if (state === "expanded") { onDelete(); return; }
    if (state === "idle") setState("expanded");
  };
  return <button type="button" className={`view-delete-confirm ${state}`} style={{ "--view-delete-width": `${width}px` } as CSSProperties} aria-label={state === "expanded" ? t("task.confirmDelete") : t("task.deleteTask")} onClick={activate} onMouseLeave={collapse}><img className="delete-base-icon" src={icon("delete_20_regular.svg")} alt="" /><img className="delete-overlay-icon" src={icon("delete_20_regular.svg")} alt="" /><span ref={labelRef}>{deleteLabel}</span></button>;
}

function RepeatDropdown({ value, options, theme, onChange }: { value: string; options: Array<{ value: string; label: string }>; theme: Theme; onChange: (value: string) => void }) {
  const [open, setOpen] = useState(false); const triggerRef = useRef<HTMLButtonElement>(null); const menuRef = useRef<HTMLDivElement>(null); const [position, setPosition] = useState({ left: 0, top: 0, width: 0 });
  const selected = options.find((item) => item.value === value) ?? options[0];
  useEffect(() => { if (!open || !triggerRef.current) return; const update = () => { const r = triggerRef.current!.getBoundingClientRect(); const height = options.length * 30 + 8; const below = window.innerHeight - r.bottom - 8; const above = r.top - 8; const top = below < height && above > below ? Math.max(8, r.top - height - 4) : r.bottom + 4; setPosition({ left:Math.max(8, Math.min(r.left, window.innerWidth - r.width - 8)), top, width:r.width }); }; const dismiss = (event: PointerEvent) => { const target = event.target as Node; if (!triggerRef.current?.contains(target) && !menuRef.current?.contains(target)) setOpen(false); }; update(); window.addEventListener("resize", update); window.addEventListener("pointerdown", dismiss); return () => { window.removeEventListener("resize", update); window.removeEventListener("pointerdown", dismiss); }; }, [open, options.length]);
  return <><button ref={triggerRef} type="button" className={`repeat-dropdown-trigger ${open ? "open" : ""}`} aria-haspopup="listbox" aria-expanded={open} onClick={() => setOpen((current) => !current)}><span>{selected.label}</span><img src={icon("chevron_right_20_regular.svg")} alt="" /></button>{open && createPortal(<div ref={menuRef} className="repeat-dropdown-menu" data-theme={theme} role="listbox" style={{ left:position.left, top:position.top, width:position.width }}>{options.map((option) => <button type="button" key={option.value} className={option.value === value ? "selected" : ""} role="option" aria-selected={option.value === value} onClick={() => { onChange(option.value); setOpen(false); }}>{option.label}</button>)}</div>, document.body)}</>;
}

function McpDestructiveConfirmationDialog({ theme, confirmation, onApprove, onReject }: { theme: Theme; confirmation: McpDestructiveConfirmation; onApprove: (token: string) => Promise<void>; onReject: (token: string) => Promise<void> }) {
  const [submitting, setSubmitting] = useState(false);
  const isTask = confirmation.operation === "delete_task";
  const subject = isTask ? confirmation.preview.task?.title ?? "" : confirmation.preview.category ? categoryLabel(confirmation.preview.category) : "";
  const message = isTask ? t("dialog.deleteTaskMessage", { name: subject }) : (confirmation.preview.taskCount ?? 0) > 0 ? t("dialog.categoryDeleteReferenced", { count: confirmation.preview.taskCount ?? 0 }) : t("dialog.categoryDeleteMessage");
  useEffect(() => {
    const delay = Math.max(0, confirmation.expiresAtUtcMs - Date.now());
    const timer = window.setTimeout(() => { void onReject(confirmation.token); }, delay);
    return () => window.clearTimeout(timer);
  }, [confirmation, onReject]);
  return createPortal(<div className="category-dialog-backdrop" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget && !submitting) void onReject(confirmation.token); }}><section className="category-delete-dialog todo-task-delete-dialog" data-theme={theme} role="dialog" aria-modal="true" aria-labelledby="mcp-delete-confirmation-title">
    <h2 id="mcp-delete-confirmation-title">{isTask ? t("dialog.deleteTaskTitle") : t("dialog.deleteCategoryTitle", { name: subject })}</h2>
    <p>{message}</p>
    <footer className="dialog-actions"><button className="button secondary" disabled={submitting} onClick={() => void onReject(confirmation.token)}>{t("data.cancel")}</button><button className="button primary" disabled={submitting} onClick={async () => { setSubmitting(true); await onApprove(confirmation.token); }}>{t("task.confirm")}</button></footer>
  </section></div>, document.body);
}

function FieldCopyButton({ label, onCopy }: { label: string; onCopy: () => void }) {
  return <button type="button" className="field-copy" aria-label={label} onClick={onCopy}><img src={icon("copy_20_regular.svg")} alt="" /></button>;
}

function TaskView({ task, onBack, onEdit, onDelete, onCopy }: { task: Task; onBack: () => void; onEdit: () => void; onDelete: () => void; onCopy: (text: string, successNotice: string) => void }) {
  const recurrence = (() => {
    try {
      const value = task.recurrenceJson ? JSON.parse(task.recurrenceJson) : null;
      return value?.interval && value?.unit && value?.action ? value as { interval: number; unit: string; action: string } : null;
    } catch {
      return null;
    }
  })();
  const unitLabel = recurrence?.unit === "day" ? t("form.day") : recurrence?.unit === "week" ? t("form.week") : recurrence?.unit === "month" ? t("form.month") : t("form.year");
  const actionLabel = recurrence?.action === "update_due" ? t("form.updateDue") : t("form.createNewTask");
  return <section className="sheet sheet-with-footer"><SheetHeader title={t("task.view")} onBack={onBack} /><div className="task-view"><label>{t("form.title")}<div className="read-value-wrap"><div className="read-value">{task.title}</div><FieldCopyButton label={t("task.copyTitle")} onCopy={() => onCopy(task.title, t("notice.copiedTitle"))} /></div></label><label>{t("form.note")}<div className="read-value-wrap note-wrap"><div className="read-value note">{task.note}</div>{task.note ? <FieldCopyButton label={t("task.copyNote")} onCopy={() => onCopy(task.note, t("notice.copiedNote"))} /> : null}</div></label><label>{t("form.type")}<div className="read-value category-value"><span className="category-dot" style={{ "--category-color": task.categoryColor } as CSSProperties} />{taskCategoryLabel(task)}</div></label><label>{t("form.due")}<div className="read-value">{formatDueAt(task.dueAtUtcMs)}</div></label>{recurrence ? <label>{t("form.repeat")}<div className="read-value">{t("form.every")}{recurrence.interval}{unitLabel} · {t("form.afterDue")}{actionLabel}</div></label> : null}<p className="form-help">{t("form.currentStatus", { status: task.status === "todo" ? t("task.todo") : t("task.completed") })}</p></div><footer className="sheet-footer"><ViewDeleteButton onDelete={onDelete} /><button className="button secondary" onClick={onEdit}>{t("task.edit")}</button><button className="button primary" onClick={onBack}>{t("app.back")}</button></footer></section>;
}

function SheetHeader({ title, onBack }: { title: string; onBack: () => void }) { return <header className="sheet-header" onMouseDown={startWindowDrag} onDoubleClick={preventTitlebarDoubleClick}><button className="icon-control back-button" aria-label={t("app.back")} onClick={onBack}><img src={icon("chevron_right_20_regular.svg")} alt="" /></button><h1>{title}</h1><button className="icon-control close" aria-label={t("app.hideToTray")} onClick={() => void invoke("hide_to_tray")}><img src={icon("dismiss_20_regular.svg")} alt="" /></button></header>; }

function preventTitlebarDoubleClick(event: MouseEvent<HTMLElement>) {
  event.preventDefault();
  event.stopPropagation();
}

function startWindowDrag(event: MouseEvent<HTMLElement>) {
  if (event.button !== 0 || event.detail > 1 || (event.target as HTMLElement).closest("button")) return;
  event.preventDefault();
  void invoke("start_window_drag").catch(() => undefined);
}

function startResize(event: MouseEvent<HTMLButtonElement>) {
  if (event.button !== 0) return;
  event.preventDefault();
  void invoke("start_window_resize").catch(() => undefined);
}
