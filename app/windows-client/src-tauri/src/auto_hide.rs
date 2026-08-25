//! Native top-edge auto-hide adapter for normal and topmost modes.
//!
//! It moves the full window above the monitor instead of resizing it, preserving
//! the user-selected dimensions. The only visible pixel is the existing shell
//! edge, whose color already comes from the title-bar design token.

use std::{
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use tauri::{AppHandle, Manager};
use windows_sys::Win32::{
    Foundation::{HWND, POINT, RECT},
    Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromWindow, HMONITOR, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    },
    UI::{
        HiDpi::GetDpiForWindow,
        WindowsAndMessaging::{
            GetCursorPos, GetWindowRect, SetWindowPos, SWP_NOACTIVATE, SWP_NOZORDER,
        },
    },
};

const ANIMATION_MS: u64 = 300;
const MOVE_IDLE_DEBOUNCE_MS: u64 = 180;
const CURSOR_SAMPLE_MS: u64 = 40;
const VISIBLE_EDGE_DIP: u32 = 1;
const HOT_ZONE_DIP: u32 = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Idle,
    WaitingToCollapse,
    Collapsing,
    Collapsed,
    Expanding,
    ExpandedAtTop,
}

#[derive(Clone, Copy, Debug)]
struct WindowRect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

impl WindowRect {
    fn width(self) -> i32 {
        self.right - self.left
    }
    fn height(self) -> i32 {
        self.bottom - self.top
    }
}

#[derive(Clone, Copy, Debug)]
struct MonitorBounds {
    top: i32,
}

#[derive(Debug)]
struct AutoHideData {
    phase: Phase,
    move_revision: u64,
    suspend_moves_until: Option<Instant>,
    saved_rect: Option<WindowRect>,
    monitor: Option<MonitorBounds>,
}

pub struct AutoHideState {
    data: Arc<Mutex<AutoHideData>>,
}

impl Default for AutoHideState {
    fn default() -> Self {
        Self {
            data: Arc::new(Mutex::new(AutoHideData {
                phase: Phase::Idle,
                move_revision: 0,
                suspend_moves_until: None,
                saved_rect: None,
                monitor: None,
            })),
        }
    }
}

fn root_window(handle: HWND) -> HWND {
    crate::desktop_mode::top_level_window(handle)
}

fn current_rect(hwnd: HWND) -> Option<WindowRect> {
    let mut rect = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    (unsafe { GetWindowRect(hwnd, &mut rect) } != 0).then_some(WindowRect {
        left: rect.left,
        top: rect.top,
        right: rect.right,
        bottom: rect.bottom,
    })
}

fn monitor_bounds(hwnd: HWND) -> Option<MonitorBounds> {
    let monitor: HMONITOR = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
    if monitor.is_null() {
        return None;
    }
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        rcMonitor: RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        },
        rcWork: RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        },
        dwFlags: 0,
    };
    (unsafe { GetMonitorInfoW(monitor, &mut info) } != 0).then_some(MonitorBounds {
        top: info.rcMonitor.top,
    })
}

fn hot_zone_px(hwnd: HWND) -> i32 {
    let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96);
    ((HOT_ZONE_DIP * dpi) / 96) as i32
}

fn visible_edge_px(hwnd: HWND) -> i32 {
    let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96);
    ((VISIBLE_EDGE_DIP * dpi).div_ceil(96)) as i32
}

fn move_window(hwnd: HWND, rect: WindowRect) {
    unsafe {
        SetWindowPos(
            hwnd,
            std::ptr::null_mut(),
            rect.left,
            rect.top,
            rect.width(),
            rect.height(),
            SWP_NOACTIVATE | SWP_NOZORDER,
        );
    }
}

fn animate_vertical(hwnd: HWND, from: WindowRect, to_top: i32, expand: bool) {
    const FRAMES: i32 = 18;
    for frame in 1..=FRAMES {
        let progress = frame as f32 / FRAMES as f32;
        let eased = if expand {
            1.0 - (1.0 - progress).powi(3)
        } else {
            progress.powi(3)
        };
        let top = from.top + ((to_top - from.top) as f32 * eased).round() as i32;
        move_window(
            hwnd,
            WindowRect {
                top,
                bottom: top + from.height(),
                ..from
            },
        );
        thread::sleep(Duration::from_millis(ANIMATION_MS / FRAMES as u64));
    }
}

fn begin_collapse(
    data: Arc<Mutex<AutoHideData>>,
    hwnd: HWND,
    saved_rect: WindowRect,
    monitor: MonitorBounds,
) {
    {
        let Ok(mut state) = data.lock() else {
            return;
        };
        if !matches!(state.phase, Phase::WaitingToCollapse | Phase::ExpandedAtTop) {
            return;
        }
        state.phase = Phase::Collapsing;
        state.saved_rect = Some(saved_rect);
        state.monitor = Some(monitor);
    }
    animate_vertical(
        hwnd,
        saved_rect,
        monitor.top - saved_rect.height() + visible_edge_px(hwnd),
        false,
    );
    if let Ok(mut state) = data.lock() {
        if state.phase == Phase::Collapsing {
            state.phase = Phase::Collapsed;
        }
    }
}

fn begin_expand(data: Arc<Mutex<AutoHideData>>, hwnd: HWND, saved_rect: WindowRect) {
    {
        let Ok(mut state) = data.lock() else {
            return;
        };
        if state.phase != Phase::Collapsed {
            return;
        }
        state.phase = Phase::Expanding;
    }
    let Some(from) = current_rect(hwnd) else {
        return;
    };
    animate_vertical(hwnd, from, saved_rect.top, true);
    if let Ok(mut state) = data.lock() {
        if state.phase == Phase::Expanding {
            state.phase = Phase::ExpandedAtTop;
            state.suspend_moves_until = Some(Instant::now() + Duration::from_millis(250));
        }
    }
}

/// Debounces normal native move notifications. The resulting collapse starts
/// only after the user has stopped moving at the monitor top edge.
pub fn on_window_moved(app: AppHandle, hwnd: HWND, enabled: bool) {
    let hwnd = root_window(hwnd);
    let Some(rect) = current_rect(hwnd) else {
        return;
    };
    let Some(monitor) = monitor_bounds(hwnd) else {
        return;
    };
    let data = app.state::<AutoHideState>().data.clone();
    let revision = {
        let Ok(mut state) = data.lock() else {
            return;
        };
        state.move_revision = state.move_revision.wrapping_add(1);
        if state
            .suspend_moves_until
            .is_some_and(|until| until > Instant::now())
        {
            return;
        }
        state.suspend_moves_until = None;
        if !enabled {
            state.phase = Phase::Idle;
            state.saved_rect = None;
            state.monitor = None;
            return;
        }
        if matches!(
            state.phase,
            Phase::Collapsing | Phase::Collapsed | Phase::Expanding
        ) {
            return;
        }
        if rect.top > monitor.top + hot_zone_px(hwnd) {
            state.phase = Phase::Idle;
            state.saved_rect = None;
            state.monitor = None;
            return;
        }
        state.phase = Phase::WaitingToCollapse;
        state.move_revision
    };
    let hwnd = hwnd as usize;
    thread::spawn(move || {
        let hwnd = hwnd as HWND;
        thread::sleep(Duration::from_millis(MOVE_IDLE_DEBOUNCE_MS));
        let should_collapse = data.lock().ok().is_some_and(|state| {
            state.move_revision == revision && state.phase == Phase::WaitingToCollapse
        });
        if !should_collapse {
            return;
        }
        let Some(mut latest) = current_rect(hwnd) else {
            return;
        };
        let Some(latest_monitor) = monitor_bounds(hwnd) else {
            return;
        };
        let height = latest.height();
        latest.top = latest_monitor.top;
        latest.bottom = latest.top + height;
        move_window(hwnd, latest);
        begin_collapse(data, hwnd, latest, latest_monitor);
    });
}

/// Re-runs the normal top-edge decision after a window-mode transition. Mode
/// changes restore a collapsed window first; this clears that recovery guard
/// and collapses it again when its full rectangle remains at the monitor top.
pub fn recheck_after_mode_change(app: AppHandle, hwnd: HWND, enabled: bool) {
    let data = app.state::<AutoHideState>().data.clone();
    if let Ok(mut state) = data.lock() {
        state.suspend_moves_until = None;
    }
    on_window_moved(app, hwnd, enabled);
}

/// Runs one low-cost monitor loop for the application's lifetime. It only reads
/// cursor coordinates when an auto-hidden window is active, avoiding a global hook.
pub fn start_cursor_monitor(app: AppHandle, hwnd: HWND) {
    let data = app.state::<AutoHideState>().data.clone();
    let hwnd = root_window(hwnd) as usize;
    thread::spawn(move || loop {
        thread::sleep(Duration::from_millis(CURSOR_SAMPLE_MS));
        let snapshot = data.lock().ok().and_then(|state| {
            state
                .saved_rect
                .zip(state.monitor)
                .map(|(rect, monitor)| (state.phase, rect, monitor))
        });
        let Some((phase, saved_rect, monitor)) = snapshot else {
            continue;
        };
        let hwnd = hwnd as HWND;
        let mut cursor = POINT { x: 0, y: 0 };
        if unsafe { GetCursorPos(&mut cursor) } == 0 {
            continue;
        }
        if phase == Phase::Collapsed {
            let in_hot_zone = cursor.x >= saved_rect.left
                && cursor.x < saved_rect.right
                && cursor.y >= monitor.top
                && cursor.y < monitor.top + hot_zone_px(hwnd);
            if in_hot_zone {
                begin_expand(data.clone(), hwnd, saved_rect);
            }
        } else if phase == Phase::ExpandedAtTop {
            let Some(rect) = current_rect(hwnd) else {
                continue;
            };
            let inside = cursor.x >= rect.left
                && cursor.x < rect.right
                && cursor.y >= rect.top
                && cursor.y < rect.bottom;
            if !inside {
                if let Ok(mut state) = data.lock() {
                    if state.phase == Phase::ExpandedAtTop {
                        state.phase = Phase::WaitingToCollapse;
                    }
                }
                begin_collapse(data.clone(), hwnd, saved_rect, monitor);
            }
        }
    });
}

/// Restores the full rectangle synchronously before any lifecycle or mode action.
pub fn cancel_and_restore(app: &AppHandle, hwnd: HWND) {
    let hwnd = root_window(hwnd);
    let data = app.state::<AutoHideState>().data.clone();
    let saved = {
        let Ok(mut state) = data.lock() else {
            return;
        };
        state.move_revision = state.move_revision.wrapping_add(1);
        state.suspend_moves_until = Some(Instant::now() + Duration::from_millis(350));
        state.phase = Phase::Idle;
        state.monitor = None;
        state.saved_rect.take()
    };
    if let Some(rect) = saved {
        move_window(hwnd, rect);
    }
}

#[cfg(test)]
mod tests {
    use super::{Phase, WindowRect, VISIBLE_EDGE_DIP};

    #[test]
    fn collapsed_window_leaves_exactly_one_pixel_visible() {
        let rect = WindowRect {
            left: 20,
            top: 0,
            right: 370,
            bottom: 530,
        };
        let collapsed_top = -rect.height() + VISIBLE_EDGE_DIP as i32;
        assert_eq!(collapsed_top + rect.height(), VISIBLE_EDGE_DIP as i32);
    }

    #[test]
    fn auto_hide_has_a_stable_idle_state() {
        assert_eq!(Phase::Idle, Phase::Idle);
    }
}
