//! stdio-to-named-pipe adapter used by MCP clients such as Codex.
//!
//! It is launched as `windows-client.exe --mcp-bridge`; no Tauri window or
//! SQLite connection is created in this mode. The desktop app remains the sole
//! owner of data and its local MCP service remains the sole pipe server.

#[cfg(target_os = "windows")]
pub fn run() -> std::io::Result<()> {
    use std::{
        io::{self, BufRead, BufReader, Write},
        iter::once,
    };
    use windows_sys::Win32::{
        Foundation::{CloseHandle, GetLastError, ERROR_FILE_NOT_FOUND, ERROR_PIPE_BUSY, INVALID_HANDLE_VALUE},
        Storage::FileSystem::{
            CreateFileW, ReadFile, WriteFile, FILE_ATTRIBUTE_NORMAL, FILE_GENERIC_READ,
            FILE_GENERIC_WRITE, OPEN_EXISTING,
        },
        System::Pipes::WaitNamedPipeW,
    };

    let endpoint = r"\\.\pipe\MyLIST-MCP";
    let wide = endpoint.encode_utf16().chain(once(0)).collect::<Vec<_>>();
    let mut handle = INVALID_HANDLE_VALUE;
    let mut last_error = ERROR_FILE_NOT_FOUND;
    for _ in 0..4 {
        handle = unsafe {
            CreateFileW(
                wide.as_ptr(),
                FILE_GENERIC_READ | FILE_GENERIC_WRITE,
                0,
                std::ptr::null(),
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                std::ptr::null_mut(),
            )
        };
        if handle != INVALID_HANDLE_VALUE {
            break;
        }
        last_error = unsafe { GetLastError() };
        if last_error != ERROR_PIPE_BUSY || unsafe { WaitNamedPipeW(wide.as_ptr(), 1500) } == 0 {
            break;
        }
    }
    if handle == INVALID_HANDLE_VALUE {
        let detail = if last_error == ERROR_PIPE_BUSY {
            "MyLIST local MCP service is busy. Try again in a moment."
        } else {
            "MyLIST local MCP service is offline. Start MyLIST and enable the local MCP service."
        };
        return Err(io::Error::new(
            io::ErrorKind::ConnectionRefused,
            detail,
        ));
    }

    let stdin = io::stdin();
    let mut stdout = io::stdout().lock();
    let mut input = BufReader::new(stdin.lock());
    let mut line = String::new();
    let mut buffer = vec![0_u8; 64 * 1024];
    while input.read_line(&mut line)? > 0 {
        if line.trim().is_empty() {
            line.clear();
            continue;
        }
        // JSON-RPC notifications (for example `notifications/initialized`)
        // deliberately have no `id` and therefore no response. Waiting for a
        // pipe response here blocks the stdio bridge forever and prevents the
        // following tools/list or tools/call request from reaching MyLIST.
        let expects_response = serde_json::from_str::<serde_json::Value>(&line)
            .map(|value| value.get("id").is_some())
            // Keep malformed input on the response path so the server can
            // return its JSON-RPC parse error.
            .unwrap_or(true);
        let bytes = line.as_bytes();
        let mut written = 0_u32;
        let write_ok = unsafe {
            WriteFile(
                handle,
                bytes.as_ptr().cast(),
                bytes.len() as u32,
                &mut written,
                std::ptr::null_mut(),
            )
        };
        if write_ok == 0 || written != bytes.len() as u32 {
            unsafe { CloseHandle(handle) };
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "MyLIST local MCP service disconnected.",
            ));
        }
        if !expects_response {
            line.clear();
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
        if read_ok == 0 {
            unsafe { CloseHandle(handle) };
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "MyLIST local MCP service disconnected.",
            ));
        }
        if read > 0 {
            stdout.write_all(&buffer[..read as usize])?;
            if !buffer[..read as usize].ends_with(b"\n") {
                stdout.write_all(b"\n")?;
            }
            stdout.flush()?;
        }
        line.clear();
    }
    unsafe { CloseHandle(handle) };
    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn run() -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "MyLIST MCP Bridge is available on Windows only.",
    ))
}
