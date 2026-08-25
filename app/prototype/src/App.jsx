import { useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";

const initialTypes = [
  { id: "work", name: "工作", color: "#8CB9FF" },
  { id: "personal", name: "个人", color: "#FF9390" },
  { id: "team", name: "团队", color: "#D294F4" },
  { id: "life", name: "生活", color: "#77D6AC" },
  { id: "finance", name: "财务", color: "#FFE36A" },
  { id: "study", name: "学习", color: "#7DD8FF" },
  { id: "travel", name: "出行", color: "#FFB774" },
];

const initialPalette = [
  { id: "color-01", name: "色彩 01", value: "#D6E5FF" },
  { id: "color-02", name: "色彩 02", value: "#D6F1FF" },
  { id: "color-03", name: "色彩 03", value: "#D3F3E2" },
  { id: "color-04", name: "色彩 04", value: "#FFDCDB" },
  { id: "color-05", name: "色彩 05", value: "#FFECDB" },
  { id: "color-06", name: "色彩 06", value: "#FFF5CC" },
  { id: "color-07", name: "色彩 07", value: "#FBDBFF" },
  { id: "color-08", name: "色彩 08", value: "#FFDBEA" },
  { id: "color-09", name: "色彩 09", value: "#8CB9FF" },
  { id: "color-10", name: "色彩 10", value: "#7DD8FF" },
  { id: "color-11", name: "色彩 11", value: "#77D6AC" },
  { id: "color-12", name: "色彩 12", value: "#FF9390" },
  { id: "color-13", name: "色彩 13", value: "#FFB774" },
  { id: "color-14", name: "色彩 14", value: "#FFE36A" },
  { id: "color-15", name: "色彩 15", value: "#D294F4" },
  { id: "color-16", name: "色彩 16", value: "#FF93C6" },
  { id: "color-17", name: "色彩 17", value: "#1A4FBC" },
  { id: "color-18", name: "色彩 18", value: "#007DBB" },
  { id: "color-19", name: "色彩 19", value: "#2E7D52" },
  { id: "color-20", name: "色彩 20", value: "#A9282A" },
  { id: "color-21", name: "色彩 21", value: "#B95612" },
  { id: "color-22", name: "色彩 22", value: "#B58E00" },
  { id: "color-23", name: "色彩 23", value: "#6E219E" },
  { id: "color-24", name: "色彩 24", value: "#A72E6E" },
];

const initialTasks = [
  {
    id: "task-server-report",
    title: "提交服务器巡检报告",
    note: "整理本周异常节点与处理结论。",
    typeId: "work",
    color: "#8CB9FF",
    due: "2026-08-20T15:00",
    timeLabel: "已逾期",
    tone: "danger",
    reminder: "到期时提醒",
    status: "todo",
  },
  {
    id: "task-long-title",
    title: "完成季度服务器巡检并整理异常记录与后续处理方案",
    note: "用于验证任务标题在胶囊动画和窄窗口下的省略号与 Tooltip。",
    typeId: "work",
    color: "#8CB9FF",
    due: "2026-08-22T15:30",
    timeLabel: "周六 15:30",
    tone: "accent",
    reminder: "提前 10 分钟",
    status: "todo",
  },
  {
    id: "task-friday-agenda",
    title: "确认周五会议议程",
    note: "确认参会人和演示材料。",
    typeId: "work",
    color: "#8CB9FF",
    due: "2026-08-20T16:45",
    timeLabel: "45分钟",
    tone: "accent",
    reminder: "提前 10 分钟",
    status: "todo",
  },
  {
    id: "task-physical",
    title: "预约年度体检",
    note: "优先选择上午时间。",
    typeId: "life",
    color: "#77D6AC",
    due: "2026-08-21T10:00",
    timeLabel: "明天 10:00",
    tone: "success",
    reminder: "提前 1 小时",
    status: "todo",
  },
  {
    id: "task-desk",
    title: "整理书桌",
    note: "",
    typeId: "life",
    color: "#77D6AC",
    due: "",
    timeLabel: "",
    tone: "neutral",
    reminder: "不提醒",
    status: "todo",
  },
  {
    id: "task-backup",
    title: "备份项目文档",
    note: "已完成本周归档。",
    typeId: "work",
    color: "#8CB9FF",
    due: "2026-08-19T18:00",
    timeLabel: "昨天完成",
    tone: "neutral",
    reminder: "不提醒",
    status: "done",
    completedAt: "昨天 17:42",
  },
  {
    id: "task-bill",
    title: "缴纳水电费",
    note: "",
    typeId: "life",
    color: "#77D6AC",
    due: "2026-08-18T20:00",
    timeLabel: "周二完成",
    tone: "neutral",
    reminder: "不提醒",
    status: "done",
    completedAt: "周二 19:08",
  },
  {
    id: "task-update",
    title: "更新设备清单",
    note: "",
    typeId: "work",
    color: "#8CB9FF",
    due: "",
    timeLabel: "周一完成",
    tone: "neutral",
    reminder: "不提醒",
    status: "done",
    completedAt: "周一 11:30",
  },
];

const blankDraft = {
  title: "",
  note: "",
  typeId: "work",
  colorMode: "inherit",
  color: "#8CB9FF",
  due: "",
  reminder: "不提醒",
};

function Icon({ name, size = 20, className = "" }) {
  return (
    <span
      aria-hidden="true"
      className={`fluent-icon ${className}`}
      style={{
        "--icon-url": `url(/icons/${name}.svg)`,
        "--icon-size": `${size}px`,
      }}
    />
  );
}

function formatNewTaskTime(due) {
  if (!due) return { timeLabel: "", tone: "neutral" };
  const date = new Date(due);
  const minutes = Math.ceil((date.getTime() - Date.now()) / 60000);
  if (minutes <= 0) return { timeLabel: "已逾期", tone: "danger" };
  if (minutes < 60) return { timeLabel: `${minutes}分钟`, tone: "accent" };
  const hours = Math.floor(minutes / 60);
  const remainder = minutes % 60;
  return { timeLabel: remainder ? `${hours}小时${remainder}分钟` : `${hours}小时`, tone: "accent" };
}

function FloatingPortal({ open, anchorRef, onDismiss, children, className = "", role, ariaLabel, matchWidth = false, align = "left", theme = "light" }) {
  const layerRef = useRef(null);
  const [position, setPosition] = useState({ left: 8, top: 8, width: 0 });

  useEffect(() => {
    if (!open) return undefined;
    let frame = 0;
    const updatePosition = () => {
      const anchor = anchorRef?.current;
      if (!anchor) return;
      const rect = anchor.getBoundingClientRect();
      const layerWidth = matchWidth ? rect.width : (layerRef.current?.getBoundingClientRect().width || 0);
      const width = Math.max(0, Math.min(layerWidth, window.innerWidth - 16));
      const rawLeft = align === "right" ? rect.right - width : rect.left;
      const left = Math.max(8, Math.min(rawLeft, window.innerWidth - width - 8));
      const layerHeight = layerRef.current?.getBoundingClientRect().height || 0;
      const below = rect.bottom + 4;
      const top = below + layerHeight > window.innerHeight - 8
        ? Math.max(8, rect.top - layerHeight - 4)
        : below;
      setPosition({ left, top, width });
    };
    const schedule = () => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(updatePosition);
    };
    const handlePointerDown = (event) => {
      if (!anchorRef?.current?.contains(event.target) && !layerRef.current?.contains(event.target)) onDismiss?.();
    };
    schedule();
    window.addEventListener("resize", schedule);
    window.addEventListener("scroll", schedule, true);
    document.addEventListener("pointerdown", handlePointerDown);
    return () => {
      cancelAnimationFrame(frame);
      window.removeEventListener("resize", schedule);
      window.removeEventListener("scroll", schedule, true);
      document.removeEventListener("pointerdown", handlePointerDown);
    };
  }, [open, anchorRef, onDismiss, matchWidth, align]);

  if (!open) return null;
  return createPortal(
    <div
      ref={layerRef}
      className={`portal-layer ${theme === "dark" ? "theme-dark" : ""} ${className}`}
      role={role}
      aria-label={ariaLabel}
      style={{ left: position.left, top: position.top, ...(matchWidth ? { width: position.width } : {}) }}
    >
      {children}
    </div>,
    document.body,
  );
}

function DropdownSelect({ value, options, onChange, ariaLabel, className = "", theme = "light" }) {
  const [open, setOpen] = useState(false);
  const triggerRef = useRef(null);
  const selected = options.find((option) => option.value === value) || options[0];

  return (
    <div className={`dropdown-select ${className} ${open ? "is-open" : ""}`}>
      <button
        ref={triggerRef}
        type="button"
        className="dropdown-trigger"
        aria-label={ariaLabel}
        aria-haspopup="listbox"
        aria-expanded={open}
        onClick={() => setOpen((current) => !current)}
      >
        <span>{selected?.label || "请选择"}</span>
        <Icon name="chevron_right_20_regular" size={15} className="dropdown-chevron" />
      </button>
      <FloatingPortal open={open} anchorRef={triggerRef} onDismiss={() => setOpen(false)} matchWidth theme={theme} className="dropdown-menu" role="listbox" ariaLabel={ariaLabel}>
        <>
          {options.map((option) => (
            <button
              type="button"
              role="option"
              aria-selected={option.value === value}
              className={option.value === value ? "selected" : ""}
              key={option.value}
              onClick={() => { onChange(option.value); setOpen(false); }}
            >
              {option.label}
            </button>
          ))}
        </>
      </FloatingPortal>
    </div>
  );
}

function parseLocalDateTime(value) {
  if (!value) return new Date();
  const [datePart, timePart = "00:00"] = value.split("T");
  const [year, month, day] = datePart.split("-").map(Number);
  const [hour, minute] = timePart.split(":").map(Number);
  return new Date(year, (month || 1) - 1, day || 1, hour || 0, minute || 0);
}

function formatLocalDateTime(date) {
  const pad = (number) => String(number).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}T${pad(date.getHours())}:${pad(date.getMinutes())}`;
}

function DateTimePicker({ value, onChange, ariaLabel, theme }) {
  const [open, setOpen] = useState(false);
  const [viewDate, setViewDate] = useState(() => parseLocalDateTime(value));
  const [selectedDate, setSelectedDate] = useState(() => value ? parseLocalDateTime(value) : null);
  const [hour, setHour] = useState(() => value ? parseLocalDateTime(value).getHours() : 18);
  const [minute, setMinute] = useState(() => value ? parseLocalDateTime(value).getMinutes() : 0);
  const [popoverPosition, setPopoverPosition] = useState({ left: 12, top: 12 });
  const containerRef = useRef(null);
  const triggerRef = useRef(null);
  const popoverRef = useRef(null);

  useEffect(() => {
    if (!open) return undefined;
    const updatePosition = () => {
      const rect = triggerRef.current?.getBoundingClientRect();
      if (!rect) return;
      const width = Math.min(336, window.innerWidth - 24);
      const left = Math.max(12, Math.min(rect.right - width, window.innerWidth - width - 12));
      const top = rect.bottom + 6;
      setPopoverPosition({ left, top });
    };
    const handlePointerDown = (event) => {
      if (!containerRef.current?.contains(event.target) && !popoverRef.current?.contains(event.target)) setOpen(false);
    };
    updatePosition();
    window.addEventListener("resize", updatePosition);
    window.addEventListener("scroll", updatePosition, true);
    document.addEventListener("pointerdown", handlePointerDown);
    return () => {
      window.removeEventListener("resize", updatePosition);
      window.removeEventListener("scroll", updatePosition, true);
      document.removeEventListener("pointerdown", handlePointerDown);
    };
  }, [open]);

  useEffect(() => {
    if (!value) {
      setSelectedDate(null);
      setViewDate(new Date());
      setHour(18);
      setMinute(0);
      return;
    }
    const next = parseLocalDateTime(value);
    setSelectedDate(next);
    setViewDate(next);
    setHour(next.getHours());
    setMinute(next.getMinutes());
  }, [value]);

  const days = useMemo(() => {
    const first = new Date(viewDate.getFullYear(), viewDate.getMonth(), 1);
    const start = new Date(viewDate.getFullYear(), viewDate.getMonth(), 1 - first.getDay());
    return Array.from({ length: 42 }, (_, index) => {
      const date = new Date(start);
      date.setDate(start.getDate() + index);
      return date;
    });
  }, [viewDate]);

  const commit = (date, nextHour = hour, nextMinute = minute) => {
    const next = new Date(date.getFullYear(), date.getMonth(), date.getDate(), nextHour, nextMinute);
    setSelectedDate(next);
    setViewDate(next);
    setHour(nextHour);
    setMinute(nextMinute);
    onChange(formatLocalDateTime(next));
  };

  const changeTime = (kind, delta) => {
    const current = kind === "hour" ? hour : minute;
    const max = kind === "hour" ? 23 : 59;
    const next = (current + delta + max + 1) % (max + 1);
    const nextHour = kind === "hour" ? next : hour;
    const nextMinute = kind === "minute" ? next : minute;
    if (selectedDate) commit(selectedDate, nextHour, nextMinute);
    else {
      setHour(nextHour);
      setMinute(nextMinute);
    }
  };

  const displayValue = value
    ? (() => {
      const date = parseLocalDateTime(value);
      return `${date.getFullYear()}年${String(date.getMonth() + 1).padStart(2, "0")}月${String(date.getDate()).padStart(2, "0")}日 ${String(date.getHours()).padStart(2, "0")}:${String(date.getMinutes()).padStart(2, "0")}`;
    })()
    : "年 / 月 / 日 --:--";

  const monthLabel = `${viewDate.getFullYear()}年${String(viewDate.getMonth() + 1).padStart(2, "0")}月`;
  const isSameDay = (left, right) => left && right && left.toDateString() === right.toDateString();
  const renderTimeColumn = (kind, label, value, max) => (
    <div className="time-column" onWheel={(event) => { event.preventDefault(); changeTime(kind, event.deltaY > 0 ? 1 : -1); }}>
      <span className="time-label">{label}</span>
      <button type="button" className="time-step-button" aria-label={`${label}加一`} onClick={() => changeTime(kind, 1)}><Icon name="chevron_right_20_regular" size={15} className="chevron-up" /></button>
      <div className="time-values" aria-live="polite">
        {[-2, -1, 0, 1, 2, 3].map((offset) => {
          const next = (value + offset + max + 1) % (max + 1);
          return <span key={`${kind}-${offset}`} className={`time-value ${offset === 0 ? "selected" : ""}`}>{String(next).padStart(2, "0")}</span>;
        })}
      </div>
      <button type="button" className="time-step-button" aria-label={`${label}减一`} onClick={() => changeTime(kind, -1)}><Icon name="chevron_right_20_regular" size={15} className="chevron-down" /></button>
    </div>
  );

  const popover = open && createPortal(
    <div ref={popoverRef} className={`datetime-popover datetime-popover-portal ${theme === "dark" ? "theme-dark" : ""}`} role="dialog" aria-label="选择截止时间" style={{ left: popoverPosition.left, top: popoverPosition.top }}>
      <section className="date-panel">
        <header className="date-panel-header">
          <strong>{monthLabel}</strong>
          <div>
            <button type="button" className="date-nav-button" aria-label="上个月" onClick={() => setViewDate(new Date(viewDate.getFullYear(), viewDate.getMonth() - 1, 1))}>
              <Icon name="chevron_right_20_regular" size={16} className="date-chevron previous" />
            </button>
            <button type="button" className="date-nav-button" aria-label="下个月" onClick={() => setViewDate(new Date(viewDate.getFullYear(), viewDate.getMonth() + 1, 1))}>
              <Icon name="chevron_right_20_regular" size={16} />
            </button>
          </div>
        </header>
        <div className="calendar-weekdays">{["日", "一", "二", "三", "四", "五", "六"].map((day) => <span key={day}>{day}</span>)}</div>
        <div className="calendar-grid">
          {days.map((date) => {
            const inMonth = date.getMonth() === viewDate.getMonth();
            return <button type="button" key={date.toISOString()} className={`${inMonth ? "" : "outside-month"} ${isSameDay(date, selectedDate) ? "selected" : ""}`} onClick={() => commit(date)}>{date.getDate()}</button>;
          })}
        </div>
        <footer className="date-panel-footer">
          <button type="button" onClick={() => { setSelectedDate(null); onChange(""); setOpen(false); }}>清除</button>
          <button type="button" onClick={() => commit(new Date())}>今天</button>
        </footer>
      </section>
      <section className="time-panel" aria-label="选择时间">
        {renderTimeColumn("hour", "时", hour, 23)}
        {renderTimeColumn("minute", "分", minute, 59)}
      </section>
    </div>,
    document.body,
  );

  return (
    <div ref={containerRef} className={`datetime-picker ${open ? "is-open" : ""}`}>
      <button ref={triggerRef} type="button" className="datetime-trigger" aria-label={ariaLabel} aria-haspopup="dialog" aria-expanded={open} onClick={() => setOpen((current) => !current)}>
        <span>{displayValue}</span>
        <Icon name="clock_20_regular" size={16} />
      </button>
      {popover}
    </div>
  );
}

function TaskRow({ task, type, fallbackTypeName, onComplete, onView, onEdit, onDelete, onFinalizeDelete, onRestore, theme = "light" }) {
  const isDone = task.status === "done";
  const displayColor = type?.color || task.color;
  const [menuOpen, setMenuOpen] = useState(false);
  const [deleteConfirm, setDeleteConfirm] = useState(false);
  const [deleteAnimating, setDeleteAnimating] = useState(false);
  const [completeConfirm, setCompleteConfirm] = useState(false);
  const [completeAnimating, setCompleteAnimating] = useState(false);
  const [titleTooltipOpen, setTitleTooltipOpen] = useState(false);
  const menuButtonRef = useRef(null);
  const completeButtonRef = useRef(null);
  const taskMainRef = useRef(null);
  const deleteTimerRef = useRef(null);
  const completeTimerRef = useRef(null);

  const collapseComplete = () => {
    if (!completeConfirm) return;
    window.clearTimeout(completeTimerRef.current);
    setCompleteAnimating(true);
    completeTimerRef.current = window.setTimeout(() => {
      setCompleteConfirm(false);
      setCompleteAnimating(false);
    }, 300);
  };

  useEffect(() => () => {
    window.clearTimeout(deleteTimerRef.current);
    window.clearTimeout(completeTimerRef.current);
  }, []);

  useEffect(() => {
    setCompleteConfirm(false);
    setCompleteAnimating(false);
  }, [isDone]);

  useEffect(() => {
    if (!completeConfirm) return undefined;
    const closeOnOutsidePointer = (event) => {
      if (completeButtonRef.current?.contains(event.target)) return;
      collapseComplete();
    };
    document.addEventListener("pointerdown", closeOnOutsidePointer);
    return () => document.removeEventListener("pointerdown", closeOnOutsidePointer);
  }, [completeConfirm]);

  const closeMenu = () => setMenuOpen(false);
  const handleDeleteClick = () => {
    if (deleteAnimating) return;
    window.clearTimeout(completeTimerRef.current);
    setCompleteConfirm(false);
    setCompleteAnimating(false);
    if (deleteConfirm) {
      onFinalizeDelete(task.id);
      return;
    }
    setDeleteConfirm(true);
    setDeleteAnimating(true);
    deleteTimerRef.current = window.setTimeout(() => setDeleteAnimating(false), 300);
  };

  const handleCompleteClick = () => {
    if (completeAnimating) return;
    window.clearTimeout(deleteTimerRef.current);
    setDeleteConfirm(false);
    setDeleteAnimating(false);
    if (completeConfirm) {
      if (isDone) onRestore(task.id);
      else onComplete(task.id);
      return;
    }
    setCompleteConfirm(true);
    setCompleteAnimating(true);
    completeTimerRef.current = window.setTimeout(() => setCompleteAnimating(false), 300);
  };

  const handleRowMouseLeave = () => {
    window.clearTimeout(deleteTimerRef.current);
    if (isDone) {
      setDeleteConfirm(false);
      setDeleteAnimating(false);
    }
    collapseComplete();
    setTitleTooltipOpen(false);
  };

  const showTitleTooltip = () => {
    const element = taskMainRef.current;
    const title = element?.querySelector(".task-title");
    if (title && title.scrollWidth > title.clientWidth) setTitleTooltipOpen(true);
  };

  return (
    <article className={`task-row ${isDone ? "is-done" : ""} ${menuOpen ? "menu-open" : ""} ${completeConfirm ? "complete-open" : ""} ${isDone && deleteConfirm ? "delete-open" : ""}`} style={{ "--task-color": displayColor }} onMouseLeave={handleRowMouseLeave}>
      <button
        ref={completeButtonRef}
        className={`complete-button ${completeConfirm ? "is-complete-confirming" : ""} ${completeAnimating ? "is-complete-animating" : ""}`}
        type="button"
        aria-label={completeConfirm ? `确认${isDone ? "移入待办" : "完成任务"}：${task.title}` : (isDone ? `恢复任务：${task.title}` : `完成任务：${task.title}`)}
        aria-disabled={completeAnimating}
        onClick={handleCompleteClick}
      >
        {completeConfirm ? (
          <span className="complete-confirm-label">{isDone ? "移入待办" : "已完成"}</span>
        ) : (
          <>
            <span className="category-color-ring task-color-ring">
              <span className="swatch" style={{ background: displayColor }} />
            </span>
            {isDone && <Icon name="checkmark_20_regular" size={17} />}
          </>
        )}
      </button>

      <button
        ref={taskMainRef}
        className="task-main"
        type="button"
        onClick={() => onView(task)}
        onMouseEnter={showTitleTooltip}
        onMouseLeave={() => setTitleTooltipOpen(false)}
        onFocus={showTitleTooltip}
        onBlur={() => setTitleTooltipOpen(false)}
      >
        <span className="task-title">{task.title}</span>
      </button>
      <FloatingPortal open={titleTooltipOpen} anchorRef={taskMainRef} onDismiss={() => setTitleTooltipOpen(false)} theme={theme} className="task-title-tooltip" role="tooltip" ariaLabel={`完整标题：${task.title}`}>
        <span>{task.title}</span>
      </FloatingPortal>

      <div className={`task-time ${task.tone}`}>
        {task.reminder !== "不提醒" && !isDone && <Icon name="alert_24_regular" size={18} />}
        <span>{isDone ? task.completedAt : task.timeLabel}</span>
      </div>

      <div className={`row-actions ${isDone ? "row-actions-single" : ""} ${menuOpen ? "is-open" : ""}`}>
        {isDone ? (
          <button type="button" className={`icon-button tiny danger-button delete-confirm-button ${deleteConfirm ? "is-confirming" : ""} ${deleteAnimating ? "is-animating" : ""}`} aria-label={`删除任务：${task.title}`} aria-disabled={deleteAnimating} onClick={handleDeleteClick}>
            <Icon name="delete_20_regular" size={17} />
            {deleteConfirm && <span className="delete-confirm-label">删除</span>}
          </button>
        ) : (
          <>
            <button
              ref={menuButtonRef}
              type="button"
              className={`icon-button tiny row-menu-button ${menuOpen ? "active" : ""}`}
              aria-label={`更多操作：${task.title}`}
              aria-haspopup="menu"
              aria-expanded={menuOpen}
              aria-pressed={menuOpen}
              onClick={() => setMenuOpen((open) => !open)}
            >
              <Icon name="more_horizontal_20_regular" size={17} />
            </button>
            <FloatingPortal open={menuOpen} anchorRef={menuButtonRef} onDismiss={closeMenu} align="right" theme={theme} className="task-menu" role="menu" ariaLabel={`任务操作：${task.title}`}>
                <button type="button" role="menuitem" onClick={() => { closeMenu(); onEdit(task); }}>
                  <Icon name="edit_20_regular" size={16} />
                  编辑
                </button>
                <button type="button" role="menuitem" className="danger-menu-item" onClick={() => { closeMenu(); onDelete(task); }}>
                  <Icon name="delete_20_regular" size={16} />
                  删除
                </button>
            </FloatingPortal>
          </>
        )}
      </div>
    </article>
  );
}

function TaskEditor({ draft, setDraft, types, theme, mode, onClose, onSave, onEdit, error }) {
  const readOnly = mode === "view";
  const currentType = types.find((item) => item.id === draft.typeId) || types[0];
  const activeColor = currentType?.color || draft.color;
  const dueLabel = draft.due
    ? (() => {
      const date = parseLocalDateTime(draft.due);
      return `${date.getFullYear()}年${String(date.getMonth() + 1).padStart(2, "0")}月${String(date.getDate()).padStart(2, "0")}日 ${String(date.getHours()).padStart(2, "0")}:${String(date.getMinutes()).padStart(2, "0")}`;
    })()
    : "年 / 月 / 日 --:--";

  return (
    <section className={`editor-sheet ${readOnly ? "is-read-only" : ""}`} aria-label={mode === "add" ? "添加事项" : mode === "edit" ? "编辑事项" : "查看事项"}>
      <header className="sheet-header">
        <button type="button" className="icon-button settings-back-button sheet-back-button" aria-label="返回" title="返回" onClick={onClose}>
          <Icon name="chevron_right_20_regular" size={20} className="back-chevron" />
        </button>
        <div>
          <h2>{mode === "add" ? "添加事项" : mode === "edit" ? "编辑事项" : "查看事项"}</h2>
        </div>
      </header>

      <div className="sheet-scroll">
        <label className="field full-field">
          <span>标题</span>
          <input
            autoFocus={!readOnly}
            readOnly={readOnly}
            value={draft.title}
            maxLength={200}
            placeholder="要完成什么？"
            onChange={(event) => setDraft({ ...draft, title: event.target.value })}
          />
          {error && <small className="field-error">{error}</small>}
        </label>

        <label className="field full-field">
          <span>备注</span>
          <textarea
            readOnly={readOnly}
            value={draft.note}
            maxLength={2000}
            rows={2}
            placeholder="补充说明（可选）"
            onChange={(event) => setDraft({ ...draft, note: event.target.value })}
          />
        </label>

        <div className="field-grid">
          <label className="field">
            <span>类型</span>
            {readOnly ? (
              <div className="read-only-control"><span className="category-color-ring task-color-ring"><span className="swatch" style={{ background: activeColor }} /></span><span>{currentType?.name || "未分类"}</span></div>
            ) : (
              <DropdownSelect
                value={draft.typeId}
                ariaLabel="任务类型"
                options={types.map((item) => ({ value: item.id, label: item.name }))}
                theme={theme}
                onChange={(nextTypeId) => {
                  const nextType = types.find((item) => item.id === nextTypeId) || types[0];
                  setDraft({ ...draft, typeId: nextType.id, colorMode: "inherit", color: nextType.color });
                }}
              />
            )}
          </label>

          <label className="field">
            <span>截止时间</span>
            {readOnly ? <div className="read-only-control"><span>{dueLabel}</span></div> : <DateTimePicker value={draft.due} onChange={(nextDue) => setDraft({ ...draft, due: nextDue })} ariaLabel="截止时间" theme={theme} />}
          </label>
        </div>

        <label className="field full-field">
          <span>提醒</span>
          {readOnly ? <div className="read-only-control"><span>{draft.reminder}</span></div> : (
            <DropdownSelect
              value={draft.reminder}
              ariaLabel="提醒"
              options={["不提醒", "到期时提醒", "提前 10 分钟", "提前 1 小时", "提前 1 天"].map((item) => ({ value: item, label: item }))}
              theme={theme}
              onChange={(nextReminder) => setDraft({ ...draft, reminder: nextReminder })}
            />
          )}
        </label>
      </div>

      <footer className="sheet-actions">
        {readOnly ? (
          <>
            <button type="button" className="secondary-button" onClick={onEdit}>编辑</button>
            <button type="button" className="primary-button" onClick={onClose}>返回</button>
          </>
        ) : (
          <>
            <button type="button" className="secondary-button" onClick={onClose}>取消</button>
            <button type="button" className="primary-button" onClick={() => onSave(activeColor)}>保存事项</button>
          </>
        )}
      </footer>
    </section>
  );
}

function Toggle({ checked, onChange, label }) {
  return (
    <button type="button" className={`toggle ${checked ? "on" : ""}`} role="switch" aria-checked={checked} aria-label={label} onClick={() => onChange(!checked)}>
      <span />
    </button>
  );
}

function SettingsDrawer({
  onClose,
  onChange,
  theme,
  effectiveTheme = "light",
  startup,
  types,
  palette,
  onExport,
  onImport,
  setToast,
  onResizeStart,
}) {
  const [section, setSection] = useState("general");
  const [editingTypeId, setEditingTypeId] = useState(null);
  const [colorOpenId, setColorOpenId] = useState(null);
  const [draftTheme, setDraftTheme] = useState(theme);
  const [draftStartup, setDraftStartup] = useState(startup);
  const [draftTypes, setDraftTypes] = useState(types.map((item) => ({ ...item })));
  const [draftPalette, setDraftPalette] = useState(palette.map((item) => ({ ...item })));
  const colorButtonRefs = useRef({});

  useEffect(() => {
    onChange({ theme: draftTheme, startup: draftStartup, types: draftTypes, palette: draftPalette });
  }, [draftTheme, draftStartup, draftTypes, draftPalette]);

  const addType = () => {
    const usedColors = new Set(draftTypes.map((item) => item.color));
    const nextColor = draftPalette.find((item) => !usedColors.has(item.value))?.value || draftPalette[draftTypes.length % draftPalette.length].value;
    const nextId = `type-${Date.now()}`;
    setDraftTypes([...draftTypes, { id: nextId, name: "新分类", color: nextColor }]);
    setEditingTypeId(nextId);
    setColorOpenId(null);
    setToast("已添加分类");
  };

  const saveType = (id) => {
    const current = draftTypes.find((item) => item.id === id);
    if (!current?.name.trim()) {
      setToast("名称不能为空");
      return;
    }
    if (draftTypes.some((item) => item.id !== id && item.name.trim().toLowerCase() === current.name.trim().toLowerCase())) {
      setToast("分类已存在");
      return;
    }
    setDraftTypes(draftTypes.map((item) => item.id === id ? { ...item, name: item.name.trim() } : item));
    setEditingTypeId(null);
    setColorOpenId(null);
  };

  return (
    <section className="settings-drawer" aria-label="设置">
      <header className="sheet-header settings-header">
        <button type="button" className="icon-button settings-back-button" aria-label="返回主界面" title="返回主界面" onClick={onClose}>
          <Icon name="chevron_right_20_regular" size={22} className="back-chevron" />
        </button>
        <h2>设置</h2>
      </header>

      <nav className="main-tabs settings-tabs" aria-label="设置分类">
        {[
          ["general", "常规"],
          ["types", "分类"],
          ["data", "数据"],
        ].map(([id, label]) => (
          <button key={id} type="button" className={section === id ? "active" : ""} onClick={() => setSection(id)}>{label}</button>
        ))}
      </nav>

      <div className="settings-content">
        {section === "general" && (
          <>
            <div className="setting-block">
              <h3>外观</h3>
              <DropdownSelect
                className="theme-select"
                ariaLabel="主题模式"
                value={draftTheme}
                options={[{ value: "system", label: "跟随系统" }, { value: "light", label: "明亮" }, { value: "dark", label: "黑暗" }]}
                onChange={setDraftTheme}
                theme={effectiveTheme}
              />
            </div>
            <div className="setting-row">
              <div><strong>随 Windows 启动</strong><small>登录后自动在桌面显示</small></div>
              <Toggle checked={draftStartup} onChange={setDraftStartup} label="随 Windows 启动" />
            </div>
            <div className="setting-row">
              <div><strong>通知权限</strong><small className="status-ok">已允许 · Windows 通知</small></div>
              <button type="button" className="text-button" onClick={() => setToast("已打开系统设置")}>系统设置</button>
            </div>
          </>
        )}

        {section === "types" && (
          <div className="category-block">
            <div className="category-list" role="list" aria-label="任务分类">
              {draftTypes.map((item) => {
                const isEditing = editingTypeId === item.id;
                const isColorOpen = colorOpenId === item.id;
                return (
                  <div className={`category-row ${isEditing ? "is-editing" : ""}`} key={item.id} role="listitem">
                    <div className="category-main">
                      <button
                        type="button"
                        ref={(node) => { colorButtonRefs.current[item.id] = node; }}
                        className={`category-color-trigger ${isColorOpen ? "active" : ""}`}
                        aria-label={`${item.name}颜色`}
                        aria-haspopup="listbox"
                        aria-expanded={isColorOpen}
                        disabled={!isEditing}
                        onClick={() => setColorOpenId(isColorOpen ? null : item.id)}
                      >
                        <span className="category-color-ring">
                          <span className="swatch" style={{ background: item.color }} />
                        </span>
                      </button>
                      {isEditing ? (
                        <input
                          autoFocus
                          aria-label={`${item.name}名称`}
                          value={item.name}
                          maxLength={20}
                          onChange={(event) => setDraftTypes(draftTypes.map((type) => type.id === item.id ? { ...type, name: event.target.value } : type))}
                          onKeyDown={(event) => event.key === "Enter" && saveType(item.id)}
                        />
                      ) : <span className="category-name">{item.name}</span>}
                      <FloatingPortal
                        open={isColorOpen}
                        anchorRef={{ current: colorButtonRefs.current[item.id] }}
                        onDismiss={() => setColorOpenId(null)}
                        theme={effectiveTheme}
                        className="category-color-popover"
                        role="listbox"
                        ariaLabel="选择分类颜色"
                      >
                          {draftPalette.map((color, index) => (
                            <button
                              type="button"
                              role="option"
                              aria-label={`颜色 ${index + 1}`}
                              aria-selected={item.color === color.value}
                              className={item.color === color.value ? "selected" : ""}
                              key={color.id}
                              style={{ "--palette-color": color.value }}
                              onClick={() => {
                                setDraftTypes(draftTypes.map((type) => type.id === item.id ? { ...type, color: color.value } : type));
                                setColorOpenId(null);
                              }}
                            />
                          ))}
                      </FloatingPortal>
                    </div>
                    <div className="category-actions">
                      {item.system ? <span className="category-system-label">系统</span> : isEditing ? (
                        <button type="button" className="small-primary category-save" onClick={() => saveType(item.id)}>保存</button>
                      ) : (
                        <>
                          <button type="button" className="icon-button tiny" aria-label={`编辑分类${item.name}`} onClick={() => { setEditingTypeId(item.id); setColorOpenId(null); }}>
                            <Icon name="edit_20_regular" size={16} />
                          </button>
                          <button type="button" className="icon-button tiny danger-button" aria-label={`删除分类${item.name}`} onClick={() => setDraftTypes(draftTypes.filter((type) => type.id !== item.id))}>
                            <Icon name="delete_20_regular" size={16} />
                          </button>
                        </>
                      )}
                    </div>
                  </div>
                );
              })}
            </div>
            <button type="button" className="add-category-button" onClick={addType}>
              <Icon name="add_24_regular" size={18} />
              添加分类
            </button>
          </div>
        )}

        {section === "data" && (
          <>
            <button type="button" className="data-action" onClick={onExport}>
              <Icon name="document_arrow_down_24_regular" size={22} />
              <span><strong>导出待办文件</strong><small>支持密码加密或明文 JSON</small></span>
              <Icon name="chevron_right_20_regular" size={18} />
            </button>
            <button type="button" className="data-action" onClick={onImport}>
              <Icon name="document_arrow_up_24_regular" size={22} />
              <span><strong>导入并自动合并</strong><small>按唯一 ID 去重，以最后编辑为准</small></span>
              <Icon name="chevron_right_20_regular" size={18} />
            </button>
          </>
        )}
      </div>
      <button
        type="button"
        className="resize-grip settings-resize-grip"
        aria-label="拖动调整窗口大小"
        title="拖动调整窗口大小"
        onPointerDown={onResizeStart}
      >
        <i /><i /><i /><i /><i /><i />
      </button>
    </section>
  );
}

function Dialog({ dialog, setDialog, onConfirmDelete, setToast }) {
  const [exportMode, setExportMode] = useState("encrypted");
  const [password, setPassword] = useState("");
  const [importStep, setImportStep] = useState("pick");

  if (!dialog) return null;

  const close = () => setDialog(null);

  return (
    <div className="dialog-backdrop" role="presentation" onMouseDown={(event) => event.target === event.currentTarget && close()}>
      <section className="dialog" role="dialog" aria-modal="true" aria-label={dialog.type === "delete" ? "删除任务" : dialog.type === "export" ? "导出数据" : "导入数据"}>
        <button type="button" className="icon-button dialog-close" aria-label="关闭" onClick={close}><Icon name="dismiss_20_regular" size={18} /></button>

        {dialog.type === "delete" && (
          <>
            <div className="dialog-icon danger"><Icon name="delete_20_regular" size={24} /></div>
            <h2>删除这个事项？</h2>
            <p>“{dialog.task.title}”将从列表中移除。此操作会保留内部删除标记，用于未来合并。</p>
            <div className="dialog-actions"><button type="button" className="secondary-button" onClick={close}>取消</button><button type="button" className="destructive-button" onClick={() => { onConfirmDelete(dialog.task.id); close(); }}>删除</button></div>
          </>
        )}

        {dialog.type === "export" && (
          <>
            <div className="dialog-icon"><Icon name="document_arrow_down_24_regular" size={24} /></div>
            <h2>导出待办文件</h2>
            <p>将任务、类型和颜色打包，方便在另一台电脑继续使用。</p>
            <label className={`choice-row ${exportMode === "encrypted" ? "selected" : ""}`}><input type="radio" name="exportMode" checked={exportMode === "encrypted"} onChange={() => setExportMode("encrypted")} /><span><strong>密码加密文件</strong><small>推荐 · .dtodo</small></span></label>
            <label className={`choice-row ${exportMode === "plain" ? "selected" : ""}`}><input type="radio" name="exportMode" checked={exportMode === "plain"} onChange={() => setExportMode("plain")} /><span><strong>明文 JSON</strong><small>内容可直接读取 · .dtodo.json</small></span></label>
            {exportMode === "encrypted" && <label className="field full-field dialog-field"><span>导出密码</span><input type="password" value={password} placeholder="至少 10 个字符" onChange={(event) => setPassword(event.target.value)} /></label>}
            <div className="dialog-actions"><button type="button" className="secondary-button" onClick={close}>取消</button><button type="button" className="primary-button" onClick={() => { if (exportMode === "encrypted" && password.length < 10) { setToast("密码至少10位"); return; } setToast(exportMode === "encrypted" ? "已导出加密文件" : "已导出明文文件"); close(); }}>导出</button></div>
          </>
        )}

        {dialog.type === "import" && importStep === "pick" && (
          <>
            <div className="dialog-icon"><Icon name="document_arrow_up_24_regular" size={24} /></div>
            <h2>导入并自动合并</h2>
            <p>原型会模拟文件校验和合并预览，不会读取真实文件。</p>
            <button type="button" className="file-drop" onClick={() => setImportStep("preview")}><Icon name="document_arrow_up_24_regular" size={28} /><strong>选择待办文件</strong><small>.dtodo 或 .dtodo.json</small></button>
            <div className="dialog-actions"><button type="button" className="secondary-button" onClick={close}>取消</button></div>
          </>
        )}

        {dialog.type === "import" && importStep === "preview" && (
          <>
            <span className="eyebrow">合并预览</span>
            <h2>文件校验通过</h2>
            <p>来源：公司电脑 · 2026-08-20 15:48</p>
            <div className="merge-grid">
              <div><strong>3</strong><span>新增</span></div><div><strong>2</strong><span>更新</span></div><div><strong>8</strong><span>保持本机</span></div><div><strong>1</strong><span>删除</span></div><div><strong>6</strong><span>跳过重复</span></div>
            </div>
            <div className="snapshot-note">导入前会自动创建本机快照。</div>
            <div className="dialog-actions"><button type="button" className="secondary-button" onClick={close}>取消</button><button type="button" className="primary-button" onClick={() => { setImportStep("done"); }}>确认合并</button></div>
          </>
        )}

        {dialog.type === "import" && importStep === "done" && (
          <>
            <div className="dialog-icon success"><Icon name="checkmark_20_regular" size={24} /></div>
            <h2>合并完成</h2>
            <p>已新增 3 条、更新 2 条、删除 1 条，6 条相同事项已跳过。</p>
            <div className="dialog-actions single"><button type="button" className="primary-button" onClick={() => { setToast("已合并数据"); close(); }}>完成</button></div>
          </>
        )}
      </section>
    </div>
  );
}

export function App() {
  const [tasks, setTasks] = useState(initialTasks);
  const [types, setTypes] = useState(initialTypes);
  const [palette, setPalette] = useState(initialPalette);
  const [activeTab, setActiveTab] = useState("todo");
  const [topmost, setTopmost] = useState(false);
  const [theme, setTheme] = useState("light");
  const [systemDark, setSystemDark] = useState(false);
  const [startup, setStartup] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [editor, setEditor] = useState(null);
  const [draft, setDraft] = useState(blankDraft);
  const [editorError, setEditorError] = useState("");
  const [dialog, setDialog] = useState(null);
  const [toast, setToast] = useState("");
  const [widgetSize, setWidgetSize] = useState({ width: 360, height: 520 });
  const [isResizing, setIsResizing] = useState(false);
  const resizeStartRef = useRef(null);

  useEffect(() => {
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const update = () => setSystemDark(media.matches);
    update();
    media.addEventListener?.("change", update);
    return () => media.removeEventListener?.("change", update);
  }, []);

  useEffect(() => {
    if (!toast) return undefined;
    const timer = window.setTimeout(() => setToast(""), 2600);
    return () => window.clearTimeout(timer);
  }, [toast]);

  useEffect(() => {
    const onPointerMove = (event) => {
      const start = resizeStartRef.current;
      if (!start) return;

      const maxWidth = Math.max(320, window.innerWidth - 24);
      const maxHeight = Math.max(360, window.innerHeight - 24);
      setWidgetSize({
        width: Math.min(maxWidth, Math.max(320, start.width + event.clientX - start.x)),
        height: Math.min(maxHeight, Math.max(360, start.height + event.clientY - start.y)),
      });
    };

    const stopResize = () => {
      resizeStartRef.current = null;
      setIsResizing(false);
    };

    window.addEventListener("pointermove", onPointerMove);
    window.addEventListener("pointerup", stopResize);
    window.addEventListener("pointercancel", stopResize);
    return () => {
      window.removeEventListener("pointermove", onPointerMove);
      window.removeEventListener("pointerup", stopResize);
      window.removeEventListener("pointercancel", stopResize);
    };
  }, []);

  const effectiveTheme = theme === "system" ? (systemDark ? "dark" : "light") : theme;
  const todoTasks = useMemo(() => tasks.filter((task) => task.status === "todo"), [tasks]);
  const doneTasks = useMemo(() => tasks.filter((task) => task.status === "done"), [tasks]);
  const visibleTasks = activeTab === "todo" ? todoTasks : doneTasks;

  const openAdd = () => {
    setDraft({ ...blankDraft });
    setEditorError("");
    setEditor({ mode: "add" });
  };

  const startResize = (event) => {
    event.preventDefault();
    resizeStartRef.current = {
      x: event.clientX,
      y: event.clientY,
      width: widgetSize.width,
      height: widgetSize.height,
    };
    setIsResizing(true);
  };

  const openTaskEditor = (task, mode = "edit") => {
    const taskType = types.find((type) => type.id === task.typeId) || types[0];
    setDraft({
      title: task.title,
      note: task.note,
      typeId: taskType?.id || "work",
      colorMode: task.colorMode || "inherit",
      color: task.color,
      due: task.due,
      reminder: task.reminder,
    });
    setEditorError("");
    setEditor({ mode, id: task.id });
  };

  const openEdit = (task) => openTaskEditor(task, "edit");
  const openView = (task) => openTaskEditor(task, "view");

  const saveTask = (activeColor) => {
    const title = draft.title.trim();
    if (!title) {
      setEditorError("请输入任务标题");
      return;
    }
    const time = formatNewTaskTime(draft.due);
    if (editor.mode === "add") {
      setTasks([
        ...tasks,
        {
          id: `task-${Date.now()}`,
          title,
          note: draft.note.trim(),
          typeId: draft.typeId,
          color: activeColor,
          colorMode: draft.colorMode,
          due: draft.due,
          reminder: draft.reminder,
          status: "todo",
          ...time,
        },
      ]);
      setActiveTab("todo");
      setToast("已添加事项");
    } else {
      setTasks(tasks.map((task) => task.id === editor.id ? { ...task, title, note: draft.note.trim(), typeId: draft.typeId, color: activeColor, colorMode: draft.colorMode, due: draft.due, reminder: draft.reminder, ...time } : task));
      setToast("已更新事项");
    }
    setEditor(null);
  };

  const completeTask = (id) => {
    setTasks(tasks.map((task) => task.id === id ? { ...task, status: "done", completedAt: "刚刚", reminder: "不提醒" } : task));
    setToast("已完成事项");
  };

  const restoreTask = (id) => {
    setTasks(tasks.map((task) => task.id === id ? { ...task, status: "todo", completedAt: undefined } : task));
    setActiveTab("todo");
    setToast("已恢复事项");
  };

  const deleteTask = (id, silent = false) => {
    setTasks(tasks.filter((task) => task.id !== id));
    if (!silent) setToast("已删除事项");
  };

  return (
    <main className="prototype-stage" data-theme={effectiveTheme}>
      <section
        className={`todo-widget ${effectiveTheme === "dark" ? "theme-dark" : ""} ${topmost ? "is-topmost" : ""} ${isResizing ? "is-resizing" : ""}`}
        data-theme={effectiveTheme}
        style={{ "--widget-width": `${widgetSize.width}px`, "--widget-height": `${widgetSize.height}px` }}
        aria-label="MyLIST 原型"
      >
        <header className="widget-header">
          <button type="button" className={`icon-button header-pin-button ${topmost ? "active" : ""}`} aria-label={topmost ? "取消置顶" : "置顶"} title={topmost ? "取消置顶" : "置顶"} onClick={() => { setTopmost(!topmost); setToast(topmost ? "已取消置顶" : "已置顶模式"); }}>
            <Icon name={topmost ? "pin_24_filled" : "pin_24_regular"} size={22} />
          </button>
          <h1>MyLIST</h1>
          <div className="header-actions">
            <button type="button" className="icon-button window-close-button" aria-label="关闭窗口" title="关闭" onClick={() => setToast("已最小化到托盘")}>
              <Icon name="dismiss_20_regular" size={22} />
            </button>
          </div>
        </header>

        <div className="list-toolbar">
          <nav className="main-tabs" aria-label="任务列表">
            <button type="button" className={activeTab === "todo" ? "active" : ""} onClick={() => setActiveTab("todo")}><span>{todoTasks.length}</span> 待办</button>
            <button type="button" className={activeTab === "done" ? "active" : ""} onClick={() => setActiveTab("done")}><span>{doneTasks.length}</span> 已完成</button>
          </nav>
        </div>

        <section className="task-list" aria-live="polite">
          {visibleTasks.length ? visibleTasks.map((task) => (
            <TaskRow
              key={task.id}
              task={task}
              type={types.find((type) => type.id === task.typeId)}
              fallbackTypeName={types[0]?.name || "工作"}
              onComplete={completeTask}
              onView={openView}
              onEdit={openEdit}
              onDelete={(selected) => setDialog({ type: "delete", task: selected })}
              onFinalizeDelete={(id) => deleteTask(id, true)}
              onRestore={restoreTask}
              theme={effectiveTheme}
            />
          )) : (
            <div className="empty-state">
              <div className="empty-check"><Icon name="checkmark_20_regular" size={24} /></div>
              <strong>{activeTab === "todo" ? "今天没有待办" : "还没有已完成事项"}</strong>
              <span>{activeTab === "todo" ? "可以给自己留一点空白。" : "完成的事项会出现在这里。"}</span>
            </div>
          )}
        </section>

        <footer className="widget-footer">
          <button type="button" className="icon-button footer-settings" aria-label="设置" title="设置" onClick={() => setSettingsOpen(true)}>
            <Icon name="settings_24_regular" size={20} />
          </button>
          <button
            type="button"
            className="resize-grip"
            aria-label="拖动调整窗口大小"
            title="拖动调整窗口大小"
            onPointerDown={startResize}
          >
            <i /><i /><i /><i /><i /><i />
          </button>
        </footer>

        <button type="button" className="floating-add-button" aria-label="添加新事项" title="添加新事项" onClick={openAdd}>
          <Icon name="add_24_regular" size={22} />
        </button>

        {editor && (
          <TaskEditor
            draft={draft}
            setDraft={setDraft}
            types={types}
            theme={effectiveTheme}
            mode={editor.mode}
            error={editorError}
            onClose={() => setEditor(null)}
            onEdit={() => {
              const selected = tasks.find((task) => task.id === editor.id);
              if (selected) openEdit(selected);
            }}
            onSave={saveTask}
          />
        )}

        {settingsOpen && (
          <SettingsDrawer
            onClose={() => setSettingsOpen(false)}
            onChange={({ theme: nextTheme, startup: nextStartup, types: nextTypes, palette: nextPalette }) => {
              setTheme(nextTheme);
              setStartup(nextStartup);
              setTypes(nextTypes);
              setPalette(nextPalette);
            }}
            theme={theme}
            effectiveTheme={effectiveTheme}
            startup={startup}
            types={types}
            palette={palette}
            setToast={setToast}
            onExport={() => setDialog({ type: "export" })}
            onImport={() => setDialog({ type: "import" })}
            onResizeStart={startResize}
          />
        )}

        <Dialog dialog={dialog} setDialog={setDialog} onConfirmDelete={deleteTask} setToast={setToast} />
        {toast && <div className="toast" role="status">{toast}</div>}
      </section>
    </main>
  );
}
