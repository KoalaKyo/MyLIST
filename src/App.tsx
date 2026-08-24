import { FormEvent, type CSSProperties, type MouseEvent, useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import "./App.css";

type Theme = "light" | "dark";
type Status = "todo" | "completed";
type WindowMode = "mode-topmost" | "mode-normal" | "mode-desktop";
type Category = { id: string; name: string; colorId: string; color: string };
type BootstrapData = { deviceId: string; theme: Theme; categories: Category[]; palette: Array<{ id: string; row: number; column: number; value: string }> };
type Task = { id: string; title: string; note: string; categoryId: string; categoryName: string; categoryColor: string; status: Status; dueAtUtcMs: number | null; createdAtUtcMs: number; updatedAtUtcMs: number; completedAtUtcMs: number | null };
type Page = "home" | "create" | "view" | "edit" | "settings";

const icon = (name: string) => `/icons/${name}`;
const CONFIRM_TRANSITION_MS = 300;
const TASK_STATUS_EXIT_MS = 600;

export default function App() {
  const [theme, setTheme] = useState<Theme>("light");
  const [mode, setMode] = useState<WindowMode>("mode-normal");
  const [bootstrap, setBootstrap] = useState<BootstrapData | null>(null);
  const [tasks, setTasks] = useState<Record<Status, Task[]>>({ todo: [], completed: [] });
  const [status, setStatus] = useState<Status>("todo");
  const [page, setPage] = useState<Page>("home");
  const [selectedTask, setSelectedTask] = useState<Task | null>(null);
  const [notice, setNotice] = useState("");

  const categories = bootstrap?.categories ?? [];
  const visibleTasks = tasks[status];
  const formMode = page === "edit" ? "编辑事项" : "添加事项";

  useEffect(() => {
    void invoke<WindowMode>("window_mode").then(setMode).catch(() => undefined);
    void invoke<BootstrapData>("load_bootstrap_data").then((data) => { setTheme(data.theme); setBootstrap(data); }).catch(showError);
    void refreshTasks();
    let unlisten: (() => void) | undefined;
    void listen<WindowMode>("window-mode-changed", (event) => setMode(event.payload)).then((dispose) => { unlisten = dispose; });
    return () => unlisten?.();
  }, []);

  async function refreshTasks() {
    try {
      const [todo, completed] = await Promise.all([invoke<Task[]>("list_tasks", { status: "todo" }), invoke<Task[]>("list_tasks", { status: "completed" })]);
      setTasks({ todo, completed });
    } catch (error) { showError(error); }
  }

  function showError(error: unknown) { setNotice(error instanceof Error ? error.message : String(error)); }
  function showNotice(message: string) { setNotice(message); window.setTimeout(() => setNotice(""), 2400); }
  async function setWindowMode(next: WindowMode) { try { setMode(await invoke<WindowMode>("set_window_mode", { mode: next })); } catch (error) { showError(error); } }
  async function setThemeSetting(next: Theme) { try { const saved = await invoke<Theme>("save_theme_setting", { theme: next }); setTheme(saved); setBootstrap((current) => current ? { ...current, theme: saved } : current); showNotice(saved === "dark" ? "已切换为黑暗模式" : "已切换为明亮模式"); } catch (error) { showError(error); } }
  const cycleMode = () => void setWindowMode(mode === "mode-normal" ? "mode-topmost" : mode === "mode-topmost" ? "mode-desktop" : "mode-normal");
  async function setTaskStatus(task: Task, next: Status): Promise<boolean> { try { await invoke<Task>("set_task_status", { id: task.id, status: next }); await refreshTasks(); setPage("home"); showNotice(next === "completed" ? "事项已完成" : "已移入待办"); return true; } catch (error) { showError(error); return false; } }
  async function removeTask(task: Task) { try { await invoke("delete_task", { id: task.id }); await refreshTasks(); setPage("home"); showNotice("事项已删除"); } catch (error) { showError(error); } }
  function openTask(task: Task) { setSelectedTask(task); setPage("view"); }

  return <main className="app-shell" data-theme={theme}>
    {page === "home" ? <>
      <Header mode={mode} onCycle={cycleMode} onHide={() => void invoke("hide_to_tray")} />
      <section className="task-page">
        <div className="mode-tabs task-tabs" role="tablist" aria-label="事项状态">
          <button className={status === "todo" ? "selected" : ""} onClick={() => setStatus("todo")}><span className="tab-count">{tasks.todo.length}</span>待办</button>
          <button className={status === "completed" ? "selected" : ""} onClick={() => setStatus("completed")}><span className="tab-count">{tasks.completed.length}</span>已完成</button>
        </div>
        <TaskList>
          {visibleTasks.map((task) => <TaskRow key={task.id} task={task} theme={theme} onOpen={() => openTask(task)} onEdit={() => { setSelectedTask(task); setPage("edit"); }} onStatus={() => setTaskStatus(task, task.status === "todo" ? "completed" : "todo")} onDelete={() => void removeTask(task)} />)}
          {!visibleTasks.length && <div className="empty-state"><img src={icon("tag_20_regular.svg")} alt="" /><p>{status === "todo" ? "还没有待办事项" : "还没有已完成事项"}</p><span>{status === "todo" ? "点击下方加号添加一条" : "完成事项后会显示在这里"}</span></div>}
        </TaskList>
      </section>
      <footer className="app-footer"><button className="icon-control" aria-label="设置" onClick={() => setPage("settings")}><img src={icon("settings_24_regular.svg")} alt="" /></button><button className="add-control" aria-label="添加事项" onClick={() => { setSelectedTask(null); setPage("create"); }}><img src={icon("add_24_regular.svg")} alt="" /></button><button className="resize-grip" aria-label="拖动调整窗口大小" onMouseDown={startResize}><span>{Array.from({ length: 6 }, (_, index) => <i key={index} />)}</span></button></footer>
    </> : page === "settings" ? <Settings theme={theme} onThemeChange={setThemeSetting} onBack={() => setPage("home")} /> : page === "view" && selectedTask ? <TaskView task={selectedTask} onBack={() => setPage("home")} onEdit={() => setPage("edit")} onStatus={() => void setTaskStatus(selectedTask, selectedTask.status === "todo" ? "completed" : "todo")} onDelete={() => void removeTask(selectedTask)} /> : <TaskForm title={formMode} task={page === "edit" ? selectedTask : null} categories={categories} onBack={() => setPage(selectedTask ? "view" : "home")} onSave={async (input) => { try { const editing = page === "edit" && selectedTask; await (editing ? invoke<Task>("update_task", { input: { ...input, id: selectedTask.id } }) : invoke<Task>("create_task", { input })); await refreshTasks(); setSelectedTask(null); setStatus("todo"); setPage("home"); showNotice(editing ? "事项已保存" : "事项已添加"); } catch (error) { showError(error); } }} />}
    {notice && <div className="toast" role="status">{notice}</div>}
  </main>;
}

function Header({ mode, onCycle, onHide }: { mode: WindowMode; onCycle: () => void; onHide: () => void }) {
  return <header className="app-titlebar" data-tauri-drag-region onMouseDown={startWindowDrag}><button className={`icon-control pin pin-${mode.replace("mode-", "")} ${mode === "mode-topmost" ? "is-active" : ""}`} aria-label="切换窗口模式" onClick={onCycle}><img src={icon("pin_24_regular.svg")} alt="" /></button><h1 data-tauri-drag-region>MyLIST</h1><button className="icon-control close" aria-label="隐藏到托盘" onClick={onHide}><img src={icon("dismiss_20_regular.svg")} alt="" /></button></header>;
}

function Settings({ theme, onThemeChange, onBack }: { theme: Theme; onThemeChange: (theme: Theme) => void; onBack: () => void }) {
  const [section, setSection] = useState<"general" | "categories" | "data">("general");
  return <section className="sheet settings-sheet">
    <SheetHeader title="设置" onBack={onBack} />
    <nav className="mode-tabs settings-tabs" role="tablist" aria-label="设置分类">
      <button className={section === "general" ? "selected" : ""} onClick={() => setSection("general")}>常规</button>
      <button className={section === "categories" ? "selected" : ""} onClick={() => setSection("categories")}>分类</button>
      <button className={section === "data" ? "selected" : ""} onClick={() => setSection("data")}>数据</button>
    </nav>
    <div className="settings-content">
      {section === "general" ? <div className="settings-block">
        <label htmlFor="theme-setting">外观</label>
        <ThemeDropdown value={theme} onChange={onThemeChange} />
      </div> : <div className="settings-placeholder">{section === "categories" ? "分类管理将在本阶段下一模块提供" : "数据导入导出将在阶段 8 提供"}</div>}
    </div>
  </section>;
}

function ThemeDropdown({ value, onChange }: { value: Theme; onChange: (theme: Theme) => void }) {
  const [open, setOpen] = useState(false);
  const [position, setPosition] = useState({ left: 0, top: 0, width: 0 });
  const triggerRef = useRef<HTMLButtonElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const themeLabel = value === "dark" ? "黑暗" : "明亮";
  useEffect(() => {
    if (!open || !triggerRef.current) return;
    const updatePosition = () => {
      const rect = triggerRef.current!.getBoundingClientRect();
      const menuHeight = 72;
      setPosition({ left: rect.left, top: rect.bottom + menuHeight + 5 > window.innerHeight ? Math.max(8, rect.top - menuHeight - 5) : rect.bottom + 5, width: rect.width });
    };
    const dismiss = (event: PointerEvent) => {
      const target = event.target as Node;
      if (!triggerRef.current?.contains(target) && !menuRef.current?.contains(target)) setOpen(false);
    };
    updatePosition();
    window.addEventListener("resize", updatePosition);
    window.addEventListener("pointerdown", dismiss);
    return () => { window.removeEventListener("resize", updatePosition); window.removeEventListener("pointerdown", dismiss); };
  }, [open]);
  return <>
    <button ref={triggerRef} id="theme-setting" className={`settings-theme-trigger ${open ? "open" : ""}`} aria-haspopup="listbox" aria-expanded={open} onClick={() => setOpen((current) => !current)}>{themeLabel}<img src={icon("chevron_right_20_regular.svg")} alt="" /></button>
    {open && createPortal(<div ref={menuRef} className="settings-theme-menu" data-theme={value} role="listbox" aria-label="外观" style={{ left: position.left, top: position.top, width: position.width }}>
      {(["light", "dark"] as Theme[]).map((option) => <button key={option} role="option" aria-selected={value === option} className={value === option ? "selected" : ""} onClick={() => { onChange(option); setOpen(false); }}>{option === "light" ? "明亮" : "黑暗"}</button>)}
    </div>, document.body)}
  </>;
}

function TaskRow({ task, theme, onOpen, onEdit, onStatus, onDelete }: { task: Task; theme: Theme; onOpen: () => void; onEdit: () => void; onStatus: () => Promise<boolean>; onDelete: () => void }) {
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
  const timerRef = useRef<number | null>(null);
  const [deleteConfirmWidth, setDeleteConfirmWidth] = useState(64);
  const completed = task.status === "completed";

  useEffect(() => () => { if (timerRef.current) window.clearTimeout(timerRef.current); }, []);
  useEffect(() => {
    const labelWidth = deleteLabelRef.current?.scrollWidth ?? 0;
    if (labelWidth) setDeleteConfirmWidth(Math.ceil(16 + 6 + labelWidth + 24));
  }, []);
  useEffect(() => { setConfirming(null); setCollapsing(null); setTransitionLocked(false); setMenuOpen(false); setStatusExiting(false); }, [task.status]);
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
      if (statusRef.current?.contains(target) || deleteRef.current?.contains(target)) return;
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
      setTransitionLocked(false);
      after?.();
    }, CONFIRM_TRANSITION_MS);
  }
  function switchConfirmation(next: "status" | "delete") {
    if (!confirming || transitionLocked) return;
    setTransitionLocked(true);
    setCollapseAfterTransition(false);
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
    setConfirming(next);
    if (timerRef.current) window.clearTimeout(timerRef.current);
    timerRef.current = window.setTimeout(() => { setCollapsing(null); setTransitionLocked(false); }, CONFIRM_TRANSITION_MS);
  }
  function closeConfirmation() { if (confirming) lockTransition(null); }
  function handleStatus() {
    if (transitionLocked) return;
    setMenuOpen(false);
    if (confirming === "status") {
      setTransitionLocked(true);
      setConfirming(null);
      setCollapsing(null);
      setStatusExiting(true);
      if (timerRef.current) window.clearTimeout(timerRef.current);
      timerRef.current = window.setTimeout(() => {
        void onStatus().then((changed) => {
          if (!changed) { setStatusExiting(false); setTransitionLocked(false); }
        });
      }, TASK_STATUS_EXIT_MS);
      return;
    }
    if (confirming === "delete") { switchConfirmation("status"); return; }
    lockTransition("status");
  }
  function handleDelete() {
    if (transitionLocked) return;
    if (confirming === "delete") { onDelete(); return; }
    if (confirming === "status") { switchConfirmation("delete"); return; }
    lockTransition("delete");
  }
  function handleMenuToggle() {
    if (transitionLocked) return;
    if (confirming) {
      collapseConfirmation(() => setMenuOpen(true));
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
  const statusLabel = completed ? "移入待办" : "已完成";
  const statusIcon = completed ? "arrow_left_20_regular.svg" : "checkmark_20_regular.svg";

  return <article className={`task-row task-row-${task.status} ${confirming === "status" ? "status-confirming" : ""} ${confirming === "delete" ? "delete-confirming" : ""} ${collapsing === "status" ? "status-collapsing" : ""} ${collapsing === "delete" ? "delete-collapsing" : ""} ${statusExiting ? `status-exiting status-exiting-${completed ? "left" : "right"}` : ""}`} style={{ "--delete-confirm-width": `${deleteConfirmWidth}px` } as CSSProperties} onMouseLeave={() => { setTooltip(null); if (statusExiting) return; if (transitionLocked) setCollapseAfterTransition(true); else closeConfirmation(); }}>
    <button ref={statusRef} className="task-status" aria-label={confirming === "status" ? `确认${statusLabel}` : (completed ? "移入待办" : "完成事项")} aria-disabled={transitionLocked} onClick={handleStatus}>
      <span className="category-dot" style={{ "--category-color": task.categoryColor } as CSSProperties} />
      <span className="task-status-action" aria-hidden="true"><img src={icon(statusIcon)} alt="" /></span>
      <span className="task-status-confirm-icon" aria-hidden="true"><img src={icon(statusIcon)} alt="" /></span>
      <span className="task-status-confirm-text">{statusLabel}</span>
    </button>
    <button className="task-main" onClick={onOpen} onMouseEnter={showTooltip} onMouseLeave={() => setTooltip(null)}><span ref={titleRef} className="task-title">{task.title}</span></button>
    <span className="task-time" aria-label={task.dueAtUtcMs ? "截止时间" : undefined}>{formatTaskTime(task.dueAtUtcMs)}</span>
    {completed ? <button ref={deleteRef} className="task-delete" aria-label={confirming === "delete" ? "确认删除事项" : "删除事项"} aria-disabled={transitionLocked} onClick={handleDelete}><img src={icon("delete_20_regular.svg")} alt="" /><span ref={deleteLabelRef}>删除</span></button> : <><button ref={menuRef} className={`task-menu-trigger ${menuOpen ? "open" : ""}`} aria-label="更多操作" aria-expanded={menuOpen} onClick={handleMenuToggle}><span>•••</span></button>{menuOpen && <TaskMenu anchor={menuRef.current} theme={theme} onEdit={() => { setMenuOpen(false); onEdit(); }} onDelete={() => { setMenuOpen(false); onDelete(); }} onDismiss={() => setMenuOpen(false)} />}</>}
    {tooltip && <TaskTitleTooltip x={tooltip.x} y={tooltip.y} title={task.title} theme={theme} />}
  </article>;
}

function TaskMenu({ anchor, theme, onEdit, onDelete, onDismiss }: { anchor: HTMLButtonElement | null; theme: Theme; onEdit: () => void; onDelete: () => void; onDismiss: () => void }) {
  const [position, setPosition] = useState({ left: 0, top: 0 });
  useEffect(() => {
    if (!anchor) return;
    const update = () => {
      const rect = anchor.getBoundingClientRect();
      const menuHeight = 68;
      setPosition({ left: Math.max(8, rect.right - 96), top: rect.bottom + menuHeight + 5 > window.innerHeight ? Math.max(8, rect.top - menuHeight - 5) : rect.bottom + 5 });
    };
    update(); window.addEventListener("resize", update); window.addEventListener("scroll", update, true);
    const onPointer = (event: PointerEvent) => { if (!anchor.contains(event.target as Node)) onDismiss(); };
    window.addEventListener("pointerdown", onPointer);
    return () => { window.removeEventListener("resize", update); window.removeEventListener("scroll", update, true); window.removeEventListener("pointerdown", onPointer); };
  }, [anchor, onDismiss]);
  return createPortal(<div className="task-floating-menu" data-theme={theme} style={position} role="menu" onPointerDown={(event) => event.stopPropagation()}><button type="button" role="menuitem" onClick={onEdit}>编辑</button><button type="button" role="menuitem" onClick={onDelete}>删除</button></div>, document.body);
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
  if (difference < 0) return "已逾期";
  const minutes = Math.max(1, Math.ceil(difference / 60_000));
  if (minutes < 60) return `${minutes}分钟`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}小时`;
  const date = new Date(dueAtUtcMs);
  return `${date.getMonth() + 1}/${date.getDate()}`;
}

function TaskForm({ title, task, categories, onBack, onSave }: { title: string; task: Task | null; categories: Category[]; onBack: () => void; onSave: (input: { title: string; note: string; categoryId: string }) => void }) {
  const [draft, setDraft] = useState(() => ({ title: task?.title ?? "", note: task?.note ?? "", categoryId: task?.categoryId ?? categories[0]?.id ?? "" }));
  useEffect(() => setDraft({ title: task?.title ?? "", note: task?.note ?? "", categoryId: task?.categoryId ?? categories[0]?.id ?? "" }), [task, categories]);
  function submit(event: FormEvent) { event.preventDefault(); onSave(draft); }
  return <section className="sheet"><SheetHeader title={title} onBack={onBack} /><form className="task-form" onSubmit={submit}><label>标题<input autoFocus value={draft.title} maxLength={200} placeholder="要完成什么？" onChange={(event) => setDraft({ ...draft, title: event.target.value })} /></label><label>备注<textarea value={draft.note} maxLength={2000} placeholder="补充说明（可选）" onChange={(event) => setDraft({ ...draft, note: event.target.value })} /></label><label>类型<select value={draft.categoryId} onChange={(event) => setDraft({ ...draft, categoryId: event.target.value })}>{categories.map((category) => <option key={category.id} value={category.id}>{category.name}</option>)}</select></label><p className="form-help">截止时间与提醒将在阶段 6 接入。</p><footer className="sheet-footer"><button type="button" className="button secondary" onClick={onBack}>返回</button><button type="submit" className="button primary">保存事项</button></footer></form></section>;
}

function TaskView({ task, onBack, onEdit, onStatus, onDelete }: { task: Task; onBack: () => void; onEdit: () => void; onStatus: () => void; onDelete: () => void }) {
  return <section className="sheet"><SheetHeader title="查看事项" onBack={onBack} /><div className="task-view"><label>标题<div className="read-value">{task.title}</div></label><label>备注<div className="read-value note">{task.note || "无备注"}</div></label><label>类型<div className="read-value category-value"><span className="category-dot" style={{ "--category-color": task.categoryColor } as CSSProperties} />{task.categoryName}</div></label><p className="form-help">当前状态：{task.status === "todo" ? "待办" : "已完成"}</p><footer className="sheet-footer"><button className="button danger" onClick={onDelete}>删除</button><button className="button secondary" onClick={onStatus}>{task.status === "todo" ? "完成" : "恢复"}</button><button className="button primary" onClick={onEdit}>编辑</button></footer></div></section>;
}

function SheetHeader({ title, onBack }: { title: string; onBack: () => void }) { return <header className="sheet-header" data-tauri-drag-region onMouseDown={startWindowDrag}><button className="icon-control back-button" aria-label="返回" onClick={onBack}><img src={icon("chevron_right_20_regular.svg")} alt="" /></button><h1 data-tauri-drag-region>{title}</h1></header>; }

function startWindowDrag(event: MouseEvent<HTMLElement>) {
  if (event.button !== 0 || (event.target as HTMLElement).closest("button")) return;
  event.preventDefault();
  void invoke("start_window_drag").catch(() => undefined);
}

function startResize(event: MouseEvent<HTMLButtonElement>) {
  if (event.button !== 0) return;
  event.preventDefault();
  void invoke("start_window_resize").catch(() => undefined);
}
