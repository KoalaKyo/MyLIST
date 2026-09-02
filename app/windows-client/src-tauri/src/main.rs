// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if std::env::args().any(|argument| argument == "--mcp-bridge") {
        if let Err(error) = windows_client_lib::run_mcp_bridge() {
            eprintln!("MyLIST MCP Bridge: {error}");
            std::process::exit(1);
        }
        return;
    }
    windows_client_lib::run()
}
