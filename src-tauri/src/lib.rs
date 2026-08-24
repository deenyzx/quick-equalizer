use std::fs;
use std::path::PathBuf;
use tauri::Manager;
use window_vibrancy::{apply_acrylic, apply_blur};
use windows::Win32::Media::Audio::{
    eConsole, eRender, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator, DEVICE_STATE_ACTIVE,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED,
};
use windows::Win32::UI::Shell::PropertiesSystem::PROPERTYKEY;

mod engine;

fn get_presets_path() -> PathBuf {
    let mut path = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    path.push("QuickEqualizer");
    fs::create_dir_all(&path).ok();
    path.push("presets.json");
    path
}

fn get_device_info_path() -> PathBuf {
    let mut path = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    path.push("QuickEqualizer");
    fs::create_dir_all(&path).ok();
    path.push("device.json");
    path
}

/// (device_id, device_name) the user selected, if any.
fn read_device_info() -> Option<(String, String)> {
    let raw = fs::read_to_string(get_device_info_path()).ok()?;
    let v: serde_json::Value = serde_json::from_str(&raw).ok()?;
    let id = v.get("id")?.as_str()?.to_string();
    let name = v.get("name")?.as_str()?.to_string();
    Some((id, name))
}

fn get_hidden_builtins_path() -> PathBuf {
    let mut path = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    path.push("QuickEqualizer");
    fs::create_dir_all(&path).ok();
    path.push("hidden_builtins.json");
    path
}

/// Reads the human-readable friendly name (PKEY_Device_FriendlyName) of an audio endpoint.
unsafe fn device_friendly_name(device: &IMMDevice) -> Option<String> {
    let props = device
        .OpenPropertyStore(windows::Win32::System::Com::STGM(0))
        .ok()?;
    let key = PROPERTYKEY {
        fmtid: windows::core::GUID::from_u128(0xa45c254e_df1c_4efd_8020_67d146a850e0),
        pid: 14,
    };
    let val = props.GetValue(&key).ok()?;
    use windows::Win32::System::Com::StructuredStorage::PropVariantToStringAlloc;
    let name_pwstr = PropVariantToStringAlloc(&val).ok()?;
    name_pwstr.to_string().ok()
}

#[tauri::command]
fn write_eq_config(config: String) -> Result<(), String> {
    let path = engine::config_txt_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    fs::write(path, config).map_err(|e| e.to_string())
}

#[tauri::command]
fn is_apo_installed() -> bool {
    engine::is_registered()
}

#[tauri::command]
fn is_device_configured() -> bool {
    get_device_info_path().exists()
}

/// Returns the device name the user previously selected (persisted locally),
/// or an empty string if none has been configured yet.
#[tauri::command]
fn get_configured_device() -> Result<String, String> {
    Ok(read_device_info().map(|(_, name)| name).unwrap_or_default())
}

/// Returns the friendly name of the current default playback device,
/// read live from the Windows audio API (no hardcoded fallback).
#[tauri::command]
fn get_default_device_name() -> Result<String, String> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(|e| e.to_string())?;
        let device = enumerator
            .GetDefaultAudioEndpoint(eRender, eConsole)
            .map_err(|e| e.to_string())?;
        device_friendly_name(&device).ok_or_else(|| "Could not read device name".to_string())
    }
}

#[tauri::command]
fn get_audio_devices() -> Result<Vec<serde_json::Value>, String> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);

        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(|e| e.to_string())?;

        let collection = enumerator
            .EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)
            .map_err(|e| e.to_string())?;

        let count = collection.GetCount().map_err(|e| e.to_string())?;
        let mut devices = Vec::new();

        for i in 0..count {
            if let Ok(device) = collection.Item(i) {
                if let Ok(id_pwstr) = device.GetId() {
                    let id_str = id_pwstr.to_string().unwrap_or_default();
                    if let Some(name) = device_friendly_name(&device) {
                        devices.push(serde_json::json!({
                            "id": id_str,
                            "name": name
                        }));
                    }
                }
            }
        }

        Ok(devices)
    }
}

/// Saves the user's device selection (id + name) locally. The actual privileged
/// registration happens later in the elevated helper, which reads this back.
#[tauri::command]
fn configure_device(device_id: String, device_name: String) -> Result<(), String> {
    let info = serde_json::json!({ "id": device_id, "name": device_name });
    fs::write(get_device_info_path(), info.to_string()).map_err(|e| e.to_string())
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

/// Built-in presets the user has deleted. Persisted so they don't reappear on
/// restart (issue #4).
#[tauri::command]
fn load_hidden_builtins() -> Result<String, String> {
    let path = get_hidden_builtins_path();
    if path.exists() {
        fs::read_to_string(&path).map_err(|e| e.to_string())
    } else {
        Ok("[]".to_string())
    }
}

#[tauri::command]
fn save_hidden_builtins(json: String) -> Result<(), String> {
    fs::write(get_hidden_builtins_path(), json).map_err(|e| e.to_string())
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

const ENGINE_SETUP_ARG: &str = "--engine-setup";
const ENGINE_UNINSTALL_ARG: &str = "--engine-uninstall";
const ENGINE_COLLECT_LOGS_ARG: &str = "--engine-collect-logs";

/// Entry point for the privileged setup helper — runs in an elevated copy of the
/// app (spawned by `run_engine_setup`), registers the audio engine, then exits.
fn run_engine_setup_elevated() -> i32 {
    match read_device_info() {
        Some((id, _name)) => engine::register_for_device(&id),
        None => {
            let _ = fs::write(
                engine::log_path(),
                "No device selected — pick a device before running setup.",
            );
            2
        }
    }
}

/// Returns the last engine-setup report so the UI can show what happened.
#[tauri::command]
fn read_engine_log() -> Result<String, String> {
    let p = engine::log_path();
    if p.exists() {
        fs::read_to_string(&p).map_err(|e| e.to_string())
    } else {
        Ok(String::new())
    }
}

/// Relaunches the app elevated (one UAC prompt, branded as this app) with the
/// given hidden flag, waits for it, and reports whether it succeeded.
fn elevate_self(arg: &str) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe_str = exe.to_string_lossy().replace('\'', "''");

    let status = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-WindowStyle",
            "Hidden",
            "-Command",
            &format!(
                "$p = Start-Process -FilePath '{}' -ArgumentList '{}' -Verb RunAs -PassThru -Wait; exit $p.ExitCode",
                exe_str, arg
            ),
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map_err(|e| e.to_string())?;

    if status.success() {
        Ok(())
    } else {
        Err(format!("Didn't complete (exit {})", status.code().unwrap_or(-1)))
    }
}

#[tauri::command]
fn run_engine_setup() -> Result<(), String> {
    elevate_self(ENGINE_SETUP_ARG)
}

#[tauri::command]
fn uninstall_engine() -> Result<(), String> {
    elevate_self(ENGINE_UNINSTALL_ARG)
}

/// Copies the audio engine's own log (written from inside the Windows audio
/// service) somewhere readable, then returns it. This is the ground truth for
/// whether the effect is actually being loaded.
#[tauri::command]
fn collect_engine_logs() -> Result<String, String> {
    let _ = elevate_self(ENGINE_COLLECT_LOGS_ARG);
    let p = engine::apo_trace_copy_path();
    if p.exists() {
        fs::read_to_string(&p).map_err(|e| e.to_string())
    } else {
        Err("Could not read the engine log.".to_string())
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // When launched as the elevated helper, do that work and exit before any UI.
    if std::env::args().any(|a| a == ENGINE_SETUP_ARG) {
        std::process::exit(run_engine_setup_elevated());
    }
    if std::env::args().any(|a| a == ENGINE_UNINSTALL_ARG) {
        let id = read_device_info().map(|(id, _)| id);
        std::process::exit(engine::unregister_for_device(id.as_deref()));
    }
    if std::env::args().any(|a| a == ENGINE_COLLECT_LOGS_ARG) {
        std::process::exit(engine::collect_logs());
    }

    // Prefer a fixed-version WebView2 runtime bundled next to the exe (under
    // `webview2/`) so the app works on machines without the system WebView2
    // runtime — common on fresh Windows 10. Falls back to the system runtime.
    #[cfg(target_os = "windows")]
    {
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                let wv = dir.join("webview2");
                if wv.exists() {
                    std::env::set_var("WEBVIEW2_BROWSER_EXECUTABLE_FOLDER", wv);
                }
            }
        }
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let window = app.get_webview_window("main").unwrap();
            // Acrylic gives a translucent, dark-tinted blur on BOTH Windows 10
            // (1809+) and Windows 11, unlike Mica which is Windows 11 only.
            // Fall back to a plain blur, then to the CSS background, if it fails.
            #[cfg(target_os = "windows")]
            {
                let tint = Some((18, 18, 22, 125));
                if apply_acrylic(&window, tint).is_err() {
                    let _ = apply_blur(&window, tint);
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            write_eq_config,
            is_apo_installed,
            is_device_configured,
            get_configured_device,
            get_default_device_name,
            get_audio_devices,
            configure_device,
            load_presets,
            save_presets,
            load_hidden_builtins,
            save_hidden_builtins,
            close_window,
            minimize_window,
            maximize_window,
            run_engine_setup,
            uninstall_engine,
            collect_engine_logs,
            read_engine_log
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
