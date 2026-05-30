use std::fs;
use std::path::PathBuf;
use tauri::Manager;
use window_vibrancy::apply_mica;
use windows::Win32::Media::Audio::{
    eRender, IMMDeviceEnumerator, MMDeviceEnumerator, DEVICE_STATE_ACTIVE,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED,
};
use windows::Win32::System::Registry::{
    RegOpenKeyExW, RegSetValueExW, RegCloseKey, HKEY_LOCAL_MACHINE, KEY_SET_VALUE, REG_SZ,
};
use windows::Win32::UI::Shell::PropertiesSystem::PROPERTYKEY;
use windows::core::PCWSTR;

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

fn get_setup_flag_path() -> PathBuf {
    let mut path = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    path.push("QuickEqualizer");
    fs::create_dir_all(&path).ok();
    path.push("device_configured");
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
fn is_device_configured() -> bool {
    get_setup_flag_path().exists()
}

#[tauri::command]
fn get_audio_devices() -> Result<Vec<serde_json::Value>, String> {
    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED).ok();

        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                .map_err(|e| e.to_string())?;

        let collection = enumerator
            .EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)
            .map_err(|e| e.to_string())?;

        let count = collection.GetCount().map_err(|e| e.to_string())?;
        let mut devices = Vec::new();

        for i in 0..count {
            if let Ok(device) = collection.Item(i) {
                if let Ok(id_pwstr) = device.GetId() {
                    let id_str = id_pwstr.to_string().unwrap_or_default();

                    if let Ok(props) = device.OpenPropertyStore(
                    windows::Win32::System::Com::STGM(0),
                    ){
                        let key = PROPERTYKEY {
                            fmtid: windows::core::GUID::from_u128(
                                0xa45c254e_df1c_4efd_8020_67d146a850e0,
                            ),
                            pid: 14,
                        };
                        if let Ok(val) = props.GetValue(&key) {
                            use windows::Win32::System::Com::StructuredStorage::PropVariantToStringAlloc;
                            if let Ok(name_pwstr) = PropVariantToStringAlloc(&val) {
                                let name = name_pwstr.to_string().unwrap_or_else(|_| "Unknown Device".to_string());
                                devices.push(serde_json::json!({
                                    "id": id_str,
                                    "name": name
                                }));
                            }
                        }
                    }
                }
            }
        }

        Ok(devices)
    }
}

#[tauri::command]
fn configure_device(device_id: String, device_name: String) -> Result<(), String> {
    let apo_config = PathBuf::from(r"C:\Program Files\EqualizerAPO\config\config.txt");
    if !apo_config.exists() {
        fs::write(&apo_config, "Preamp: 0 dB\n").map_err(|e| e.to_string())?;
    }

    let device_list = PathBuf::from(r"C:\Program Files\EqualizerAPO\config\deviceList.txt");
    fs::write(&device_list, format!("{}\n", device_name)).map_err(|e| e.to_string())?;

    unsafe {
        let key_path: Vec<u16> = format!(
            "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\MMDevices\\Audio\\Render\\{}\\FxProperties",
            device_id
        )
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();

        let mut hkey = windows::Win32::System::Registry::HKEY::default();
        let result = RegOpenKeyExW(
            HKEY_LOCAL_MACHINE,
            PCWSTR(key_path.as_ptr()),
            0,
            KEY_SET_VALUE,
            &mut hkey,
        );

        if result.is_ok() {
            let value_name: Vec<u16> = "{d04e05a6-594b-4fb6-a80d-01af5eed7d1d},1\0"
                .encode_utf16()
                .collect();
            let value_data: Vec<u16> = "{E4D6F3E0-1AF2-48D1-8739-67B81AB5B9D0}\0"
                .encode_utf16()
                .collect();
            let bytes = std::slice::from_raw_parts(
                value_data.as_ptr() as *const u8,
                value_data.len() * 2,
            );
            RegSetValueExW(hkey, PCWSTR(value_name.as_ptr()), 0, REG_SZ, Some(bytes)).ok();
            RegCloseKey(hkey).ok();
        }
    }

    fs::write(get_setup_flag_path(), &device_name).map_err(|e| e.to_string())?;
    Ok(())
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
            is_device_configured,
            get_audio_devices,
            configure_device,
            load_presets,
            save_presets,
            close_window,
            minimize_window,
            maximize_window
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}