fn main() {
    // The Windows executable icon is compiled into the PE resource during this
    // build step. Keep Cargo aware of it so updating the product mark cannot
    // leave Task Manager, Explorer, or the taskbar on a stale Tauri icon.
    println!("cargo:rerun-if-changed=icons/icon.ico");
    tauri_build::build()
}
