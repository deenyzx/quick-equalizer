use std::fs;
use std::path::PathBuf;
use tauri::Manager;
use window_vibrancy::apply_mica;

fn get_config_path() -> PathBuf {
    PathBuf::from(r"C:\Program Files\EqualizerAPO\config\config.txt")
}

fn get_presets_path() -> PathBuf {
    let mut path = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    path.push("QuickEqualizer");
    fs::create_dir_all(&path).ok();
    path.push("presets.json");
    path
}

#[tauri::command]
fn write_eq_config(config: String) -> Result<(), String> {
    fs::write(get_config_path(), config).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_apo_device() -> Result<String, String> {
    let apo_dir = PathBuf::from(r"C:\Program Files\EqualizerAPO");
    if !apo_dir.exists() {
        return Err("APO not found".to_string());
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

#[tauri::command]
fn load_presets() -> Result<String, String> {
    let path = get_presets_path();
    if path.exists() {
        fs::read_to_string(&path).map_err(|e| e.to_string())
    } else {
        Ok("{}".to_string())
    }
}

#[tauri::command]
fn save_presets(json: String) -> Result<(), String> {
    fs::write(get_presets_path(), json).map_err(|e| e.to_string())
}

#[tauri::command]
fn close_window(window: tauri::WebviewWindow) {
    window.close().unwrap();
}

#[tauri::command]
fn minimize_window(window: tauri::WebviewWindow) {
    window.minimize().unwrap();
}

#[tauri::command]
fn maximize_window(window: tauri::WebviewWindow) {
    window.maximize().unwrap();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let window = app.get_webview_window("main").unwrap();
            #[cfg(target_os = "windows")]
            let _ = apply_mica(&window, None);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            write_eq_config,
            get_apo_device,
            load_presets,
            save_presets,
            close_window,
            minimize_window,
            maximize_window
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}