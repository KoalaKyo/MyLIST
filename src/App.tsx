import { FormEvent, type CSSProperties, type MouseEvent, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import "./App.css";

type Theme = "light" | "dark";
type Status = "todo" | "completed";
type WindowMode = "mode-topmost" | "mode-normal" | "mode-desktop";
type Category = { id: string; name: string; colorId: string; color: string };
type BootstrapData = { deviceId: string; theme: Theme; categories: Category[]; palette: Array<{ id: string; row: number; column: number; value: string }> };
type Task = { id: string; title: string; note: string; categoryId: string; categoryName: string; categoryColor: string; status: Status; dueAtUtcMs: number | null; createdAtUtcMs: number; updatedAtUtcMs: number; completedAtUtcMs: number | null };
type Page = "home" | "create" | "view" | "edit";

const icon = (name: string) => `/icons/${name}`;

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
  const cycleMode = () => void setWindowMode(mode === "mode-normal" ? "mode-topmost" : mode === "mode-topmost" ? "mode-desktop" : "mode-normal");
  async function setTaskStatus(task: Task, next: Status) { try { await invoke<Task>("set_task_status", { id: task.id, status: next }); await refreshTasks(); setPage("home"); showNotice(next === "completed" ? "事项已完成" : "已移入待办"); } catch (error) { showError(error); } }
  async function removeTask(task: Task) { if (!window.confirm(`删除“${task.title}”？`)) return; try { await invoke("delete_task", { id: task.id }); await refreshTasks(); setPage("home"); showNotice("事项已删除"); } catch (error) { showError(error); } }
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
          {visibleTasks.map((task) => <TaskRow key={task.id} task={task} onOpen={() => openTask(task)} onStatus={() => void setTaskStatus(task, task.status === "todo" ? "completed" : "todo")} onDelete={() => void removeTask(task)} />)}
          {!visibleTasks.length && <div className="empty-state"><img src={icon("tag_20_regular.svg")} alt="" /><p>{status === "todo" ? "还没有待办事项" : "还没有已完成事项"}</p><span>{status === "todo" ? "点击下方加号添加一条" : "完成事项后会显示在这里"}</span></div>}
        </TaskList>
      </section>
      <footer className="app-footer"><button className="icon-control" aria-label="设置" onClick={() => showNotice("设置将在阶段 5 提供")}><img src={icon("settings_24_regular.svg")} alt="" /></button><button className="add-control" aria-label="添加事项" onClick={() => { setSelectedTask(null); setPage("create"); }}><img src={icon("add_24_regular.svg")} alt="" /></button><button className="resize-grip" aria-label="拖动调整窗口大小" onMouseDown={startResize}><span>{Array.from({ length: 6 }, (_, index) => <i key={index} />)}</span></button></footer>
    </> : page === "view" && selectedTask ? <TaskView task={selectedTask} onBack={() => setPage("home")} onEdit={() => setPage("edit")} onStatus={() => void setTaskStatus(selectedTask, selectedTask.status === "todo" ? "completed" : "todo")} onDelete={() => void removeTask(selectedTask)} /> : <TaskForm title={formMode} task={page === "edit" ? selectedTask : null} categories={categories} onBack={() => setPage(selectedTask ? "view" : "home")} onSave={async (input) => { try { const editing = page === "edit" && selectedTask; await (editing ? invoke<Task>("update_task", { input: { ...input, id: selectedTask.id } }) : invoke<Task>("create_task", { input })); await refreshTasks(); setSelectedTask(null); setStatus("todo"); setPage("home"); showNotice(editing ? "事项已保存" : "事项已添加"); } catch (error) { showError(error); } }} />}
    {notice && <div className="toast" role="status">{notice}</div>}
  </main>;
}

function Header({ mode, onCycle, onHide }: { mode: WindowMode; onCycle: () => void; onHide: () => void }) {
  return <header className="app-titlebar" data-tauri-drag-region onMouseDown={startWindowDrag}><button className={`icon-control pin pin-${mode.replace("mode-", "")} ${mode === "mode-topmost" ? "is-active" : ""}`} aria-label="切换窗口模式" onClick={onCycle}><img src={icon("pin_24_regular.svg")} alt="" /></button><h1 data-tauri-drag-region>MyLIST</h1><button className="icon-control close" aria-label="隐藏到托盘" onClick={onHide}><img src={icon("dismiss_20_regular.svg")} alt="" /></button></header>;
}

function TaskRow({ task, onOpen, onStatus, onDelete }: { task: Task; onOpen: () => void; onStatus: () => void; onDelete: () => void }) {
  const actionIcon = task.status === "completed" ? "checkmark_20_regular.svg" : null;
  return <article className={`task-row task-row-${task.status}`}>
    <button className="task-status" aria-label={task.status === "todo" ? "完成事项" : "移入待办"} onClick={onStatus}>
      <span className="category-dot" style={{ "--category-color": task.categoryColor } as CSSProperties} />
      <span className="task-status-action" aria-hidden="true">{actionIcon && <img src={icon(actionIcon)} alt="" />}</span>
    </button>
    <button className="task-main" onClick={onOpen}><span className="task-title">{task.title}</span></button>
    <span className="task-time" aria-label={task.dueAtUtcMs ? "截止时间" : undefined}>{formatTaskTime(task.dueAtUtcMs)}</span>
    <button className="task-delete" aria-label="删除事项" onClick={onDelete}><img src={icon("delete_20_regular.svg")} alt="" /></button>
  </article>;
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
  return <div className="task-list-wrap"><div ref={listRef} className="task-list" aria-live="polite" onScroll={syncMetrics}>{children}</div>{maxScroll > 0 && <div className="task-scroll-track" aria-hidden="true"><button className="task-scroll-thumb" type="button" onMouseDown={beginThumbDrag} style={{ height: thumbHeight, transform: `translateY(${thumbOffset}px)` }} /></div>}</div>;
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
