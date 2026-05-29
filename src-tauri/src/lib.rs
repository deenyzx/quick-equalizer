use std::fs;
use std::path::PathBuf;
use tauri::Manager;
use window_vibrancy::apply_acrylic;

#[tauri::command]
fn write_eq_config(config: String) -> Result<(), String> {
    let path = get_config_path();
    fs::write(&path, config).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_apo_device() -> Result<String, String> {
    let apo_dir = PathBuf::from(r"C:\Program Files\EqualizerAPO");
    if !apo_dir.exists() {
        return Err("APO not found".to_string());
    }
    let config_path = apo_dir.join("config").join("config.txt");
    if !config_path.exists() {
        return Err("APO config not found".to_string());
    }
    let device_path = apo_dir.join("config").join("deviceList.txt");
    if device_path.exists() {
        if let Ok(content) = fs::read_to_string(&device_path) {
            if let Some(line) = content.lines().find(|l| !l.trim().is_empty() && !l.starts_with('#')) {
                return Ok(line.trim().to_string());
            }
        }
    }
    Ok("Headphones (Realtek HD Audio)".to_string())
}

fn get_config_path() -> PathBuf {
    PathBuf::from(r"C:\Program Files\EqualizerAPO\config\config.txt")
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            if let Some(window) = app.get_webview_window("main") {
                #[cfg(target_os = "windows")]let _ = apply_acrylic(&window, Some((0, 0, 0, 100)));
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            write_eq_config,
            get_apo_device
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}