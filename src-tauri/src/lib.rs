use tauri::Manager;

/// Start the embedded patent-hub web server in a background thread.
/// The server runs on localhost:3000 and the WebView loads from it.
fn start_embedded_server() {
    std::thread::spawn(|| {
        // Find the patent-hub executable next to this app
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.to_path_buf()))
            .unwrap_or_else(|| std::env::current_dir().unwrap());

        // Look for patent-hub binary
        let server_exe = if cfg!(windows) {
            exe_dir.join("patent-hub.exe")
        } else {
            exe_dir.join("patent-hub")
        };

        if server_exe.exists() {
            // Launch as subprocess
            let _ = std::process::Command::new(&server_exe)
                .current_dir(&exe_dir)
                .spawn();
        } else {
            eprintln!("[Patent Hub APP] Server binary not found at {:?}", server_exe);
            eprintln!("[Patent Hub APP] Please ensure patent-hub binary is in the same directory");
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Start the web server before creating the window
    start_embedded_server();

    // Give the server a moment to start
    std::thread::sleep(std::time::Duration::from_millis(1500));

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            // On mobile, the window is created automatically
            // On desktop, we can customize it
            #[cfg(desktop)]
            {
                let _window = app.get_webview_window("main")
                    .expect("main window not found");
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Patent Hub");
}
