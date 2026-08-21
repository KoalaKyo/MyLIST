import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import "./App.css";

type Theme = "light" | "dark";
type WindowMode = "mode-topmost" | "mode-normal" | "mode-desktop";
type BootstrapData = {
  deviceId: string;
  theme: Theme;
  categories: Array<{ id: string; name: string; colorId: string; color: string }>;
  palette: Array<{ id: string; row: number; column: number; value: string }>;
};

export default function App() {
  const [theme, setTheme] = useState<Theme>("light");
  const [mode, setMode] = useState<WindowMode>("mode-normal");
  const [themeMenuOpen, setThemeMenuOpen] = useState(false);
  const [bootstrap, setBootstrap] = useState<BootstrapData | null>(null);

  useEffect(() => {
    void invoke<WindowMode>("window_mode").then(setMode).catch(() => undefined);
    void invoke<BootstrapData>("load_bootstrap_data").then((data) => {
      setTheme(data.theme);
      setBootstrap(data);
    }).catch(() => undefined);
    let unlisten: (() => void) | undefined;
    void listen<WindowMode>("window-mode-changed", (event) => setMode(event.payload)).then((dispose) => { unlisten = dispose; });
    return () => unlisten?.();
  }, []);

  async function setWindowMode(next: WindowMode) {
    setMode(await invoke<WindowMode>("set_window_mode", { mode: next }));
  }
  async function setThemeSetting(next: Theme) {
    setTheme(next);
    setThemeMenuOpen(false);
    try {
      await invoke<string>("save_theme_setting", { theme: next });
    } catch {
      // Persistence errors will receive explicit UI feedback in the settings module.
    }
  }
  const cycleMode = () => void setWindowMode(mode === "mode-normal" ? "mode-topmost" : mode === "mode-topmost" ? "mode-desktop" : "mode-normal");

  return (
    <main className="app-shell" data-theme={theme}>
      <header className="app-titlebar" data-tauri-drag-region>
        <button className={`icon-control pin pin-${mode.replace("mode-", "")} ${mode === "mode-topmost" ? "is-active" : ""}`} aria-label="切换窗口模式" onClick={cycleMode}><img src="/icons/pin_24_regular.svg" alt="" /></button>
        <h1 data-tauri-drag-region>MyLIST</h1>
        <button className="icon-control close" aria-label="隐藏到托盘" onClick={() => void invoke("hide_to_tray")}><img src="/icons/dismiss_20_regular.svg" alt="" /></button>
      </header>
      <section className="stage-one-content">
        <p className="stage-label">阶段 1 · 工程基座</p>
        <div className="mode-tabs" role="tablist" aria-label="窗口模式">
          {([ ["mode-normal", "普通"], ["mode-topmost", "置顶"], ["mode-desktop", "桌面"] ] as const).map(([value, label]) => <button key={value} className={mode === value ? "selected" : ""} onClick={() => void setWindowMode(value)}>{label}</button>)}
        </div>
        <section className="foundation-card"><h2>本地数据基础已就绪</h2><p>SQLite 已初始化：设备标识、24 色调色板、默认七分类与设备主题设置均会在本机持久化。</p></section>
        <div className="theme-row"><span>外观</span><div className="theme-select"><button aria-haspopup="listbox" aria-expanded={themeMenuOpen} onClick={() => setThemeMenuOpen((open) => !open)}><span>{theme === "light" ? "明亮" : "黑暗"}</span><span className="select-chevron" aria-hidden="true" /></button>{themeMenuOpen && <div className="theme-menu" role="listbox">{([ ["light", "明亮"], ["dark", "黑暗"] ] as const).map(([value, label]) => <button key={value} className={theme === value ? "selected" : ""} role="option" aria-selected={theme === value} onClick={() => void setThemeSetting(value)}>{label}</button>)}</div>}</div></div>
        {bootstrap && <p className="data-status">本机数据已就绪 · {bootstrap.categories.length} 个默认分类 · {bootstrap.palette.length} 种颜色</p>}
      </section>
      <footer className="app-footer"><button className="icon-control" aria-label="设置"><img src="/icons/settings_24_regular.svg" alt="" /></button><span>基础框架</span></footer>
    </main>
  );
}
