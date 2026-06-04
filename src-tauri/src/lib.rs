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
    RegOpenKeyExW,
    RegCreateKeyExW,
    RegSetValueExW,
    RegCloseKey,
    HKEY_LOCAL_MACHINE,
    KEY_SET_VALUE,
    REG_SZ,
    REG_OPTION_NON_VOLATILE,
};
use windows::Win32::UI::Shell::PropertiesSystem::PROPERTYKEY;
use windows::core::PCWSTR;

fn get_config_path() -> PathBuf {
    let exe_path = std::env::current_exe().unwrap_or_default();
    let app_dir = exe_path.parent().unwrap_or(std::path::Path::new("."));
    app_dir.join("config").join("config.txt")
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
fn configure_device(
    app: tauri::AppHandle,
    device_id: String,
    device_name: String
) -> Result<(), String> {
    // Get the directory where our app is installed
    let exe_path = std::env::current_exe().map_err(|e| e.to_string())?;
    let app_dir = exe_path.parent().ok_or("Can't find app directory")?;
    let dll_path = app_dir.join("EqualizerAPO.dll");

    if !dll_path.exists() {
        return Err("EqualizerAPO.dll not found next to app".to_string());
    }

    // Write config directory
    let config_dir = app_dir.join("config");
    fs::create_dir_all(&config_dir).map_err(|e| e.to_string())?;

    let config_path = config_dir.join("config.txt");
    if !config_path.exists() {
        fs::write(&config_path, "Preamp: 0 dB\n").map_err(|e| e.to_string())?;
    }

    let device_list = config_dir.join("deviceList.txt");
    fs::write(&device_list, format!("{}\n", device_name))
        .map_err(|e| e.to_string())?;

    unsafe {
        let dll_path_str = format!("{}\0", dll_path.to_string_lossy());
        let dll_path_wide: Vec<u16> = dll_path_str.encode_utf16().collect();

        // Register CLSID for EqualizerAPO — this is what Windows needs to load it
        let clsid_path: Vec<u16> =
            "SOFTWARE\\Classes\\CLSID\\{62dc1a93-ae24-464c-a43e-452f824c4250}\\InprocServer32\0"
            .encode_utf16().collect();

        let mut hkey = windows::Win32::System::Registry::HKEY::default();
        let mut disposition = 0u32;
        RegCreateKeyExW(
            HKEY_LOCAL_MACHINE,
            PCWSTR(clsid_path.as_ptr()),
            0, None,
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            None, &mut hkey, Some(&mut disposition)
        ).map_err(|e| e.to_string())?;

        // Set DLL path
        let dll_bytes = std::slice::from_raw_parts(
            dll_path_wide.as_ptr() as *const u8,
            dll_path_wide.len() * 2
        );
        RegSetValueExW(hkey, PCWSTR(std::ptr::null()), 0, REG_SZ, Some(dll_bytes)).ok();

        // Set threading model
        let threading_name: Vec<u16> = "ThreadingModel\0".encode_utf16().collect();
        let threading_val: Vec<u16> = "Both\0".encode_utf16().collect();
        let threading_bytes = std::slice::from_raw_parts(
            threading_val.as_ptr() as *const u8,
            threading_val.len() * 2
        );
        RegSetValueExW(hkey, PCWSTR(threading_name.as_ptr()), 0, REG_SZ, Some(threading_bytes)).ok();
        RegCloseKey(hkey).ok();

        // Also register EACD2258 wrapper CLSID pointing to same DLL
        let wrapper_path: Vec<u16> =
            "SOFTWARE\\Classes\\CLSID\\{EACD2258-FCAC-4FF4-B36D-419E924A6D79}\\InprocServer32\0"
            .encode_utf16().collect();

        let mut hkey2 = windows::Win32::System::Registry::HKEY::default();
        let mut disposition2 = 0u32;
        RegCreateKeyExW(
            HKEY_LOCAL_MACHINE,
            PCWSTR(wrapper_path.as_ptr()),
            0, None,
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            None, &mut hkey2, Some(&mut disposition2)
        ).ok();
        RegSetValueExW(hkey2, PCWSTR(std::ptr::null()), 0, REG_SZ, Some(dll_bytes)).ok();
        RegSetValueExW(hkey2, PCWSTR(threading_name.as_ptr()), 0, REG_SZ, Some(threading_bytes)).ok();
        RegCloseKey(hkey2).ok();

        // Write FxProperties for the selected device
        let fx_path: Vec<u16> = format!(
            "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\MMDevices\\Audio\\Render\\{}\\FxProperties\0",
            device_id
        ).encode_utf16().collect();

        let mut hkey3 = windows::Win32::System::Registry::HKEY::default();
        if RegOpenKeyExW(
            HKEY_LOCAL_MACHINE,
            PCWSTR(fx_path.as_ptr()),
            0, KEY_SET_VALUE,
            &mut hkey3
        ).is_ok() {
            let val_name: Vec<u16> = "{d04e05a6-594b-4fb6-a80d-01af5eed7d1d},5\0"
                .encode_utf16().collect();
            let apo_guid: Vec<u16> = "{62dc1a93-ae24-464c-a43e-452f824c4250}\0"
                .encode_utf16().collect();
            let apo_bytes = std::slice::from_raw_parts(
                apo_guid.as_ptr() as *const u8,
                apo_guid.len() * 2
            );
            RegSetValueExW(hkey3, PCWSTR(val_name.as_ptr()), 0, REG_SZ, Some(apo_bytes)).ok();
            RegCloseKey(hkey3).ok();
        }

        // Also write the EqualizerAPO config path to registry
        let apo_reg_path: Vec<u16> = "SOFTWARE\\EqualizerAPO\0".encode_utf16().collect();
        let mut hkey4 = windows::Win32::System::Registry::HKEY::default();
        let mut disposition4 = 0u32;
        RegCreateKeyExW(
            HKEY_LOCAL_MACHINE,
            PCWSTR(apo_reg_path.as_ptr()),
            0, None,
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            None, &mut hkey4, Some(&mut disposition4)
        ).ok();

        let install_path_str = format!("{}\0", app_dir.to_string_lossy());
        let install_path_wide: Vec<u16> = install_path_str.encode_utf16().collect();
        let install_bytes = std::slice::from_raw_parts(
            install_path_wide.as_ptr() as *const u8,
            install_path_wide.len() * 2
        );
        let install_name: Vec<u16> = "InstallPath\0".encode_utf16().collect();
        let config_path_str = format!("{}\0", config_dir.to_string_lossy());
        let config_path_wide: Vec<u16> = config_path_str.encode_utf16().collect();
        let config_bytes = std::slice::from_raw_parts(
            config_path_wide.as_ptr() as *const u8,
            config_path_wide.len() * 2
        );
        let config_name: Vec<u16> = "ConfigPath\0".encode_utf16().collect();
        RegSetValueExW(hkey4, PCWSTR(install_name.as_ptr()), 0, REG_SZ, Some(install_bytes)).ok();
        RegSetValueExW(hkey4, PCWSTR(config_name.as_ptr()), 0, REG_SZ, Some(config_bytes)).ok();
        RegCloseKey(hkey4).ok();
    }
        // Restart Windows Audio service to pick up new device registration
    std::process::Command::new("net")
        .args(["stop", "audiosrv"])
        .status()
        .ok();
    std::process::Command::new("net")
        .args(["start", "audiosrv"])
        .status()
        .ok();
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