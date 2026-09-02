//! Local MCP bridge lifecycle foundation.
//!
//! The service owns the private named-pipe transport. Protocol handling lives
//! in `mcp_bridge`; this module manages lifecycle and session credentials.

use std::{
    sync::{atomic::{AtomicUsize, Ordering}, mpsc, Arc, Mutex},
    thread,
    time::Duration,
};

use rand::{rngs::OsRng, RngCore};
use serde::Serialize;

pub type McpRequestHandler = Arc<dyn Fn(&str) -> String + Send + Sync + 'static>;
pub type McpConnectionListener = Arc<dyn Fn(bool) + Send + Sync + 'static>;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum McpServiceStatus {
    Disabled,
    Starting,
    Online,
    Stopping,
    Error,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServiceSnapshot {
    pub status: McpServiceStatus,
    pub endpoint: Option<String>,
    pub ai_connected: bool,
}

struct Inner {
    status: McpServiceStatus,
    endpoint: Option<String>,
    stop_sender: Option<mpsc::Sender<()>>,
    session_secret: Option<Vec<u8>>,
    generation: u64,
    ready_clients: Arc<AtomicUsize>,
}

impl Default for Inner {
    fn default() -> Self {
        Self {
            status: McpServiceStatus::Disabled,
            endpoint: None,
            stop_sender: None,
            session_secret: None,
            generation: 0,
            ready_clients: Arc::new(AtomicUsize::new(0)),
        }
    }
}

/// Thread-safe process state managed by Tauri. No secret is exposed in the
/// snapshot; the session secret is retained in memory only for future stages.
#[derive(Default)]
pub struct McpServiceState {
    inner: Mutex<Inner>,
}

impl McpServiceState {
    pub fn snapshot(&self) -> McpServiceSnapshot {
        let inner = self.inner.lock().expect("MCP state mutex poisoned");
        McpServiceSnapshot {
            status: inner.status,
            endpoint: inner.endpoint.clone(),
            ai_connected: inner.ready_clients.load(Ordering::Acquire) > 0,
        }
    }

    /// Reports Online only after a real named-pipe JSON-RPC round trip.
    /// This verifies the same transport and MCP handler used by Codex instead
    /// of merely checking that the listener thread was created.
    pub fn verified_snapshot(&self) -> McpServiceSnapshot {
        let snapshot = self.snapshot();
        if snapshot.status == McpServiceStatus::Online && !probe_service() {
            return McpServiceSnapshot {
                status: McpServiceStatus::Error,
                endpoint: snapshot.endpoint,
                ai_connected: false,
            };
        }
        snapshot
    }

    #[allow(dead_code)]
    pub fn start(&self) -> Result<McpServiceSnapshot, String> {
        self.start_with_handler(None, None)
    }

    pub fn start_with_handler(
        &self,
        handler: Option<McpRequestHandler>,
        connection_listener: Option<McpConnectionListener>,
    ) -> Result<McpServiceSnapshot, String> {
        let (endpoint, stop_receiver, ready_receiver, ready_sender, generation, ready_clients) = {
            let mut inner = self
                .inner
                .lock()
                .map_err(|_| "MCP 服务状态不可用".to_string())?;
            if matches!(
                inner.status,
                McpServiceStatus::Starting | McpServiceStatus::Online
            ) {
                return Ok(McpServiceSnapshot {
                    status: inner.status,
                    endpoint: inner.endpoint.clone(),
                    ai_connected: inner.ready_clients.load(Ordering::Acquire) > 0,
                });
            }
            if let Some(sender) = inner.stop_sender.take() {
                let _ = sender.send(());
            }
            inner.generation = inner.generation.wrapping_add(1);
            let generation = inner.generation;
            let endpoint = new_endpoint();
            let session_secret = new_session_secret();
            let (stop_sender, stop_receiver) = mpsc::channel();
            let (ready_sender, ready_receiver) = mpsc::channel();
            inner.status = McpServiceStatus::Starting;
            inner.endpoint = Some(endpoint.clone());
            inner.stop_sender = Some(stop_sender);
            inner.session_secret = Some(session_secret);
            inner.ready_clients = Arc::new(AtomicUsize::new(0));
            (
                endpoint,
                stop_receiver,
                ready_receiver,
                ready_sender,
                generation,
                inner.ready_clients.clone(),
            )
        };

        // The worker owns the named-pipe handle and waits for the stop signal.
        // A null security descriptor uses the creating process token's default
        // DACL, so the endpoint is accessible only to the current user.
        let spawn_result = thread::Builder::new()
            .name("mylist-mcp-bridge".into())
            .spawn(move || run_pipe(endpoint, stop_receiver, ready_sender, handler, ready_clients, connection_listener));
        if spawn_result.is_err() {
            let mut inner = self
                .inner
                .lock()
                .map_err(|_| "MCP 服务状态不可用".to_string())?;
            if inner.generation == generation {
                inner.status = McpServiceStatus::Error;
                inner.endpoint = None;
                inner.stop_sender = None;
                inner.session_secret = None;
            }
            return Err("MCP 服务启动失败".into());
        }

        let ready = ready_receiver
            .recv_timeout(Duration::from_secs(1))
            .unwrap_or(false);
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| "MCP 服务状态不可用".to_string())?;
        if inner.generation != generation {
            return Ok(McpServiceSnapshot {
                status: inner.status,
                endpoint: inner.endpoint.clone(),
                ai_connected: inner.ready_clients.load(Ordering::Acquire) > 0,
            });
        }
        if ready {
            inner.status = McpServiceStatus::Online;
            Ok(McpServiceSnapshot {
                status: inner.status,
                endpoint: inner.endpoint.clone(),
                ai_connected: inner.ready_clients.load(Ordering::Acquire) > 0,
            })
        } else {
            inner.status = McpServiceStatus::Error;
            inner.endpoint = None;
            inner.stop_sender = None;
            inner.session_secret = None;
            Err("MCP 服务启动失败".into())
        }
    }

    pub fn stop(&self) -> McpServiceSnapshot {
        let mut inner = self.inner.lock().expect("MCP state mutex poisoned");
        if let Some(sender) = inner.stop_sender.take() {
            inner.status = McpServiceStatus::Stopping;
            let _ = sender.send(());
        }
        inner.generation = inner.generation.wrapping_add(1);
        inner.status = McpServiceStatus::Disabled;
        inner.endpoint = None;
        inner.session_secret = None;
        inner.ready_clients.store(0, Ordering::Release);
        McpServiceSnapshot {
            status: inner.status,
            endpoint: None,
            ai_connected: false,
        }
    }

    pub fn shutdown(&self) {
        let _ = self.stop();
    }
}

#[cfg(target_os = "windows")]
fn probe_service() -> bool {
    use std::iter::once;
    use windows_sys::Win32::{
        Foundation::{CloseHandle, GetLastError, ERROR_PIPE_BUSY, INVALID_HANDLE_VALUE},
        Storage::FileSystem::{
            CreateFileW, ReadFile, WriteFile, FILE_ATTRIBUTE_NORMAL, FILE_GENERIC_READ,
            FILE_GENERIC_WRITE, OPEN_EXISTING,
        },
        System::Pipes::WaitNamedPipeW,
    };

    let wide = r"\\.\pipe\MyLIST-MCP"
        .encode_utf16()
        .chain(once(0))
        .collect::<Vec<_>>();
    let mut handle = INVALID_HANDLE_VALUE;
    for _ in 0..4 {
        handle = unsafe {
            CreateFileW(
                wide.as_ptr(), FILE_GENERIC_READ | FILE_GENERIC_WRITE, 0,
                std::ptr::null(), OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL,
                std::ptr::null_mut(),
            )
        };
        if handle != INVALID_HANDLE_VALUE { break; }
        if unsafe { GetLastError() } != ERROR_PIPE_BUSY
            || unsafe { WaitNamedPipeW(wide.as_ptr(), 500) } == 0 { break; }
    }
    if handle == INVALID_HANDLE_VALUE { return false; }

    let request = b"{\"jsonrpc\":\"2.0\",\"id\":987654,\"method\":\"tools/list\",\"params\":{}}\n";
    let mut written = 0_u32;
    let write_ok = unsafe {
        WriteFile(handle, request.as_ptr().cast(), request.len() as u32, &mut written, std::ptr::null_mut())
    };
    let mut buffer = vec![0_u8; 64 * 1024];
    let mut read = 0_u32;
    let read_ok = write_ok != 0 && written == request.len() as u32 && unsafe {
        ReadFile(handle, buffer.as_mut_ptr().cast(), buffer.len() as u32, &mut read, std::ptr::null_mut())
    } != 0;
    unsafe { CloseHandle(handle) };
    if !read_ok || read == 0 { return false; }
    serde_json::from_slice::<serde_json::Value>(&buffer[..read as usize])
        .ok()
        .and_then(|value| value.get("result")?.get("tools")?.as_array().map(|tools| !tools.is_empty()))
        .unwrap_or(false)
}

#[cfg(not(target_os = "windows"))]
fn probe_service() -> bool { true }

fn new_endpoint() -> String {
    // The endpoint name is deliberately stable.  An MCP Bridge launched by
    // Codex must know where to connect, and Windows named-pipe discovery is
    // not reliably available to normal PowerShell sessions.  Access is still
    // limited by the pipe's current-user ACL; the random in-memory session
    // secret remains reserved for the authenticated Bridge handshake.
    r"\\.\pipe\MyLIST-MCP".to_string()
}

fn new_session_secret() -> Vec<u8> {
    let mut secret = vec![0_u8; 32];
    OsRng.fill_bytes(&mut secret);
    secret
}

#[cfg(all(target_os = "windows", not(test)))]
fn run_pipe(
    endpoint: String,
    stop_receiver: mpsc::Receiver<()>,
    ready_sender: mpsc::Sender<bool>,
    handler: Option<McpRequestHandler>,
    ready_clients: Arc<AtomicUsize>,
    connection_listener: Option<McpConnectionListener>,
) {
    use std::iter::once;
    use std::sync::atomic::{AtomicBool, Ordering};
    use windows_sys::Win32::{
        Foundation::{CloseHandle, GetLastError, INVALID_HANDLE_VALUE},
        Storage::FileSystem::PIPE_ACCESS_DUPLEX,
        System::Pipes::{
            ConnectNamedPipe, CreateNamedPipeW, PIPE_NOWAIT, PIPE_READMODE_MESSAGE,
            PIPE_TYPE_MESSAGE,
        },
    };

    let wide = endpoint.encode_utf16().chain(once(0)).collect::<Vec<_>>();
    let stopping = Arc::new(AtomicBool::new(false));
    let mut first_instance = true;

    loop {
        if stop_receiver.try_recv().is_ok() {
            stopping.store(true, Ordering::Release);
            break;
        }
        let handle = unsafe {
            CreateNamedPipeW(
                wide.as_ptr(),
                PIPE_ACCESS_DUPLEX,
                PIPE_TYPE_MESSAGE | PIPE_READMODE_MESSAGE | PIPE_NOWAIT,
                255,
                4096,
                4096,
                0,
                std::ptr::null(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            if first_instance {
                let _ = ready_sender.send(false);
            }
            stopping.store(true, Ordering::Release);
            return;
        }
        if first_instance {
            let _ = ready_sender.send(true);
            first_instance = false;
        }

        let connected = loop {
            if stop_receiver.try_recv().is_ok() {
                stopping.store(true, Ordering::Release);
                unsafe { CloseHandle(handle) };
                return;
            }
            let result = unsafe { ConnectNamedPipe(handle, std::ptr::null_mut()) };
            if result != 0 || unsafe { GetLastError() } == 535 {
                break true;
            }
            match unsafe { GetLastError() } {
                536 => thread::sleep(Duration::from_millis(25)),
                _ => break false,
            }
        };
        if !connected {
            unsafe { CloseHandle(handle) };
            continue;
        }

        let client_handle = handle as usize;
        let client_handler = handler.clone();
        let client_stopping = stopping.clone();
        let client_ready_clients = ready_clients.clone();
        let client_connection_listener = connection_listener.clone();
        let _ = thread::Builder::new()
            .name("mylist-mcp-client".into())
            .spawn(move || run_pipe_client(client_handle, client_stopping, client_handler, client_ready_clients, client_connection_listener));
        // Immediately create the next pipe instance so other Codex tasks can
        // connect while this session remains open.
    }
}

#[cfg(all(target_os = "windows", not(test)))]
fn run_pipe_client(
    raw_handle: usize,
    stopping: Arc<std::sync::atomic::AtomicBool>,
    handler: Option<McpRequestHandler>,
    ready_clients: Arc<AtomicUsize>,
    connection_listener: Option<McpConnectionListener>,
) {
    use std::sync::atomic::Ordering;
    use windows_sys::Win32::{
        Foundation::CloseHandle,
        Storage::FileSystem::{ReadFile, WriteFile},
        System::Pipes::{DisconnectNamedPipe, PeekNamedPipe},
    };

    let handle = raw_handle as windows_sys::Win32::Foundation::HANDLE;
    let mut buffer = vec![0_u8; 64 * 1024];
    let mut session_ready = false;
    while !stopping.load(Ordering::Acquire) {
        let mut available = 0_u32;
        let peeked = unsafe {
            PeekNamedPipe(
                handle,
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                &mut available,
                std::ptr::null_mut(),
            )
        };
        if peeked == 0 {
            break;
        }
        if available == 0 {
            thread::sleep(Duration::from_millis(10));
            continue;
        }
        let mut read = 0_u32;
        let read_ok = unsafe {
            ReadFile(
                handle,
                buffer.as_mut_ptr().cast(),
                buffer.len() as u32,
                &mut read,
                std::ptr::null_mut(),
            )
        };
        if read_ok == 0 || read == 0 {
            break;
        }
        let request = String::from_utf8_lossy(&buffer[..read as usize]);
        // A client can flush several JSON-RPC lines before the next pipe
        // read. Process every complete line so notifications never consume a
        // following request (for example `notifications/initialized` followed
        // by `tools/list`).
        for line in request.lines().filter(|line| !line.trim().is_empty()) {
            if !session_ready && serde_json::from_str::<serde_json::Value>(line)
                .ok()
                .and_then(|value| value.get("method").and_then(|method| method.as_str()).map(str::to_owned))
                .as_deref() == Some("notifications/initialized")
            {
                session_ready = true;
                if ready_clients.fetch_add(1, Ordering::AcqRel) == 0 {
                    if let Some(listener) = connection_listener.as_ref() { listener(true); }
                }
            }
            let response = handler
                .as_ref()
                .map(|callback| callback(line))
                .unwrap_or_else(|| crate::mcp_bridge::handle_request(None, line));
            if response.is_empty() {
                continue;
            }
            let response = format!("{response}\n");
            let mut written = 0_u32;
            unsafe {
                WriteFile(
                    handle,
                    response.as_ptr().cast(),
                    response.len() as u32,
                    &mut written,
                    std::ptr::null_mut(),
                );
            }
        }
        // Keep the connection alive for the duration of an MCP session. A
        // client may send initialize, tools/list and multiple tools/call
        // requests over the same named pipe connection. The next iteration
        // waits for another message, and the disconnect path above handles
        // clients that close the pipe.
    }
    if session_ready {
        let previous = ready_clients.fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| Some(count.saturating_sub(1))).unwrap_or(0);
        if previous == 1 {
            if let Some(listener) = connection_listener.as_ref() { listener(false); }
        }
    }
    unsafe { DisconnectNamedPipe(handle); CloseHandle(handle); }
}

#[cfg(all(not(target_os = "windows"), not(test)))]
fn run_pipe(
    _endpoint: String,
    stop_receiver: mpsc::Receiver<()>,
    ready_sender: mpsc::Sender<bool>,
    _handler: Option<McpRequestHandler>,
    _ready_clients: Arc<AtomicUsize>,
    _connection_listener: Option<McpConnectionListener>,
) {
    let _ = ready_sender.send(true);
    let _ = stop_receiver.recv();
}

#[cfg(test)]
fn run_pipe(
    _endpoint: String,
    stop_receiver: mpsc::Receiver<()>,
    ready_sender: mpsc::Sender<bool>,
    _handler: Option<McpRequestHandler>,
    _ready_clients: Arc<AtomicUsize>,
    _connection_listener: Option<McpConnectionListener>,
) {
    let _ = ready_sender.send(true);
    let _ = stop_receiver.recv();
}

#[cfg(test)]
mod tests {
    use super::{McpServiceState, McpServiceStatus};

    #[test]
    fn service_starts_with_stable_endpoint_and_stops_cleanly() {
        let state = McpServiceState::default();
        let started = state.start().expect("service should start");
        assert_eq!(started.status, McpServiceStatus::Online);
        assert_eq!(started.endpoint.as_deref(), Some(r"\\.\pipe\MyLIST-MCP"));
        let stopped = state.stop();
        assert_eq!(stopped.status, McpServiceStatus::Disabled);
        assert!(stopped.endpoint.is_none());
    }

    #[test]
    fn starting_twice_is_idempotent() {
        let state = McpServiceState::default();
        let first = state.start().unwrap();
        let second = state.start().unwrap();
        assert_eq!(first.endpoint, second.endpoint);
        state.stop();
    }
}
