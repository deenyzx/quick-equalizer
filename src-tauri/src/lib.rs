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
use windows::Win32::System::Registry::{
    RegCloseKey, RegOpenKeyExW, RegSetValueExW, HKEY_LOCAL_MACHINE, KEY_SET_VALUE, REG_SZ,
};
use windows::Win32::UI::Shell::PropertiesSystem::PROPERTYKEY;
use windows::core::PCWSTR;

const APO_DIR: &str = r"C:\Program Files\EqualizerAPO";

fn get_config_path() -> PathBuf {
    PathBuf::from(APO_DIR).join("config").join("config.txt")
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
    // No-op (but not an error) when EqualizerAPO isn't installed, so the UI
    // can keep working as a configurator without crashing.
    if !PathBuf::from(APO_DIR).exists() {
        return Ok(());
    }
    fs::write(get_config_path(), config).map_err(|e| e.to_string())
}

#[tauri::command]
fn is_apo_installed() -> bool {
    PathBuf::from(APO_DIR).exists()
}

#[tauri::command]
fn is_device_configured() -> bool {
    get_setup_flag_path().exists()
}

/// Returns the device name the user previously selected (persisted locally),
/// or an empty string if none has been configured yet.
#[tauri::command]
fn get_configured_device() -> Result<String, String> {
    let p = get_setup_flag_path();
    if p.exists() {
        fs::read_to_string(&p)
            .map(|s| s.trim().to_string())
            .map_err(|e| e.to_string())
    } else {
        Ok(String::new())
    }
}

/// Returns the friendly name of the current default playback device,
/// read live from the Windows audio API (no hardcoded fallback).
#[tauri::command]
fn get_default_device_name() -> Result<String, String> {
    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED).ok();
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
        CoInitializeEx(None, COINIT_MULTITHREADED).ok();

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

#[tauri::command]
fn configure_device(device_id: String, device_name: String) -> Result<(), String> {
    let apo_config_dir = PathBuf::from(APO_DIR).join("config");

    // Only touch EqualizerAPO's files / registry when it's actually installed.
    // Without this guard, a fresh machine (no APO) would hard-fail here and the
    // setup modal would get stuck with no feedback.
    if apo_config_dir.exists() {
        let apo_config = apo_config_dir.join("config.txt");
        if !apo_config.exists() {
            let _ = fs::write(&apo_config, "Preamp: 0 dB\n");
        }

        let device_list = apo_config_dir.join("deviceList.txt");
        let _ = fs::write(&device_list, format!("{}\n", device_name));

        // Best-effort APO activation via the registry. NOTE (issue #1): these
        // exact keys still need verification via RegShot against APO's
        // DeviceSelector on a clean install; treated as non-fatal for now.
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
    }

    // Always remember the user's selection locally, even if APO isn't installed
    // yet, so the choice survives restarts and can be applied once APO appears.
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

/// Entry point for the privileged setup helper. Runs in an elevated copy of the
/// app (spawned by `run_engine_setup`), performs the work that requires admin
/// rights, then exits.
///
/// NOTE: the real audio-engine registration (architecture A or B) is not
/// implemented yet — this is the scaffold that proves the elevation path works
/// end to end. For now it just confirms it is running elevated and leaves a log.
fn run_engine_setup_elevated() -> i32 {
    let mut log_dir = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    log_dir.push("QuickEqualizer");
    let _ = fs::create_dir_all(&log_dir);
    let log = log_dir.join("engine_setup.log");

    // Creating a directory under Program Files only succeeds when elevated, so
    // it doubles as a proof-of-privilege probe.
    let elevated = fs::create_dir_all(r"C:\Program Files\QuickEqualizer\engine").is_ok();

    let _ = fs::write(
        &log,
        format!(
            "engine setup ran; elevated={}\nTODO: register audio engine here (architecture A or B)\n",
            elevated
        ),
    );

    if elevated {
        0
    } else {
        1
    }
}

/// Relaunches the app elevated (one UAC prompt, branded as this app) to run the
/// setup helper, waits for it, and reports whether it succeeded. Shared by both
/// installation strategies; the helper body is filled in later.
#[tauri::command]
fn run_engine_setup() -> Result<(), String> {
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
                exe_str, ENGINE_SETUP_ARG
            ),
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map_err(|e| e.to_string())?;

    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "Setup didn't complete (exit {})",
            status.code().unwrap_or(-1)
        ))
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // When launched as the elevated helper, do that work and exit before any UI.
    if std::env::args().any(|a| a == ENGINE_SETUP_ARG) {
        std::process::exit(run_engine_setup_elevated());
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
            run_engine_setup
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
