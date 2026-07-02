//! Option B: register the bundled audio-processing object (APO) ourselves
//! instead of relying on EqualizerAPO's installer. Everything here must run in
//! the elevated helper (see `run_engine_setup_elevated` in lib.rs).
//!
//! ─────────────────────────────────────────────────────────────────────────
//! TWO THINGS YOU MUST SUPPLY for this to actually work (isolated below so they
//! are easy to fill in from your own setup):
//!
//!   1. The APO DLL itself — ship it next to the exe at `engine/<ENGINE_DLL_NAME>`
//!      (obtain it from an EqualizerAPO install; you may rename it). The depot
//!      build script copies `src-tauri/engine-bundle/` into the depot.
//!
//!   2. The EXACT registry values your RegShot capture shows the configurator
//!      writing — `APO_CLSID` and `FX_VALUES`. The defaults below are the
//!      best-known guess and very likely need correcting from your capture;
//!      modern Windows usually needs several composite-effect values, not one.
//!
//! This code is COMPILE-verified only. It writes detailed per-step results to
//! `%APPDATA%\QuickEqualizer\engine_setup.log` so you can see exactly which
//! step fails when you run it elevated on a real machine.
//! ─────────────────────────────────────────────────────────────────────────

use std::ffi::c_void;
use std::fs;
use std::path::PathBuf;
use std::ptr;

use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{CloseHandle, ERROR_SUCCESS, HANDLE, LUID};
use windows::Win32::Security::Authorization::{
    SetEntriesInAclW, SetNamedSecurityInfoW, SetSecurityInfo, EXPLICIT_ACCESS_W,
    NO_MULTIPLE_TRUSTEE, SET_ACCESS, SE_FILE_OBJECT, SE_OBJECT_TYPE, SE_REGISTRY_KEY,
    TRUSTEE_IS_GROUP, TRUSTEE_IS_SID, TRUSTEE_W,
};
use windows::Win32::Security::{
    AdjustTokenPrivileges, CreateWellKnownSid, LookupPrivilegeValueW, ACL,
    DACL_SECURITY_INFORMATION, LUID_AND_ATTRIBUTES, OWNER_SECURITY_INFORMATION, PSID,
    SECURITY_MAX_SID_SIZE, SE_PRIVILEGE_ENABLED, SE_RESTORE_NAME, SE_TAKE_OWNERSHIP_NAME,
    SUB_CONTAINERS_AND_OBJECTS_INHERIT, TOKEN_ADJUST_PRIVILEGES, TOKEN_PRIVILEGES, TOKEN_QUERY,
    WinBuiltinAdministratorsSid, WinBuiltinUsersSid, WELL_KNOWN_SID_TYPE,
};
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegOpenKeyExW, RegSetValueExW, HKEY, HKEY_LOCAL_MACHINE,
    KEY_ALL_ACCESS, KEY_SET_VALUE, REG_DWORD, REG_OPTION_NON_VOLATILE, REG_SAM_FLAGS, REG_SZ,
};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

// ── Things to verify/replace from your RegShot capture ──────────────────────

/// CLSID the device's FxProperties must point at (the registered APO). VERIFY.
const APO_CLSID: &str = "{E4D6F3E0-1AF2-48D1-8739-67B81AB5B9D0}";

/// (value_name, value_data) pairs written under the device's FxProperties key.
/// VERIFY/REPLACE from RegShot — modern Windows typically needs several
/// composite-effect entries (Stream/Mode/Endpoint), not just this one.
const FX_VALUES: &[(&str, &str)] = &[
    ("{d04e05a6-594b-4fb6-a80d-01af5eed7d1d},1", APO_CLSID),
    // Examples to fill in from your capture:
    // ("{d04e05a6-594b-4fb6-a80d-01af5eed7d1d},5", APO_CLSID),
    // ("{d04e05a6-594b-4fb6-a80d-01af5eed7d1d},6", APO_CLSID),
    // ("{d04e05a6-594b-4fb6-a80d-01af5eed7d1d},7", APO_CLSID),
];

/// Filename of the bundled APO DLL (shipped at `<exe dir>/engine/<this>`).
const ENGINE_DLL_NAME: &str = "EqualizerAPO.dll";

/// Where EqualizerAPO's APO reads its config path from. VERIFY key/value name.
const CONFIG_PATH_KEY: &str = "SOFTWARE\\EqualizerAPO";
const CONFIG_PATH_VALUE: &str = "ConfigPath";

// ── Stable paths ────────────────────────────────────────────────────────────

fn engine_install_dir() -> PathBuf {
    PathBuf::from(r"C:\Program Files\QuickEqualizer\engine")
}

/// User-writable config dir (the unelevated app writes config.txt here on every
/// EQ change, and the APO running in audiodg must be able to read it).
fn engine_config_dir() -> PathBuf {
    PathBuf::from(r"C:\ProgramData\QuickEqualizer\config")
}

/// Path the unelevated side writes EQ filters to. Public so lib.rs can use it.
pub fn config_txt_path() -> PathBuf {
    engine_config_dir().join("config.txt")
}

fn marker_path() -> PathBuf {
    let mut p = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    p.push("QuickEqualizer");
    let _ = fs::create_dir_all(&p);
    p.push("engine_registered");
    p
}

pub fn log_path() -> PathBuf {
    let mut p = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    p.push("QuickEqualizer");
    let _ = fs::create_dir_all(&p);
    p.push("engine_setup.log");
    p
}

/// Best-effort check that our engine is registered. Hardened later by reading
/// the actual FxProperties value back.
pub fn is_registered() -> bool {
    marker_path().exists() && engine_install_dir().join(ENGINE_DLL_NAME).exists()
}

/// Locates the folder holding the bundled engine DLLs. Prefers `engine/` next to
/// the exe (depot layout); falls back to `src-tauri/engine-bundle/` for dev runs,
/// which survives `cargo clean` (unlike anything under target/).
fn bundled_engine_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    let beside = dir.join("engine");
    if beside.is_dir() {
        return Some(beside);
    }
    // Dev fallback: target/debug (or release) -> ../../engine-bundle
    let dev = dir.join("..").join("..").join("engine-bundle");
    dev.is_dir().then_some(dev)
}

// ── Wide-string + privilege helpers ─────────────────────────────────────────

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

unsafe fn enable_privilege(name: PCWSTR) -> Result<(), String> {
    let mut token = HANDLE::default();
    OpenProcessToken(
        GetCurrentProcess(),
        TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY,
        &mut token,
    )
    .map_err(|e| format!("OpenProcessToken: {e}"))?;

    let mut luid = LUID::default();
    LookupPrivilegeValueW(PCWSTR::null(), name, &mut luid)
        .map_err(|e| format!("LookupPrivilegeValue: {e}"))?;

    let tp = TOKEN_PRIVILEGES {
        PrivilegeCount: 1,
        Privileges: [LUID_AND_ATTRIBUTES {
            Luid: luid,
            Attributes: SE_PRIVILEGE_ENABLED,
        }],
    };
    let res = AdjustTokenPrivileges(token, false, Some(&tp), 0, None, None);
    let _ = CloseHandle(token);
    res.map_err(|e| format!("AdjustTokenPrivileges: {e}"))
}

unsafe fn well_known_sid(kind: WELL_KNOWN_SID_TYPE, buf: &mut [u8]) -> Result<PSID, String> {
    let psid = PSID(buf.as_mut_ptr() as *mut c_void);
    let mut cb = buf.len() as u32;
    CreateWellKnownSid(kind, None, psid, &mut cb).map_err(|e| format!("CreateWellKnownSid: {e}"))?;
    Ok(psid)
}

/// Grant a well-known group full access to a securable object, taking ownership
/// first if requested (needed for the TrustedInstaller-owned MMDevices keys).
unsafe fn grant_full_access(
    object: &str,
    obj_type: SE_OBJECT_TYPE,
    sid_kind: WELL_KNOWN_SID_TYPE,
    access_mask: u32,
    take_ownership: bool,
) -> Result<(), String> {
    let mut sid_buf = vec![0u8; SECURITY_MAX_SID_SIZE as usize];
    let psid = well_known_sid(sid_kind, &mut sid_buf)?;
    let mut obj_w = wide(object);

    if take_ownership {
        let rc = SetNamedSecurityInfoW(
            PWSTR(obj_w.as_mut_ptr()),
            obj_type,
            OWNER_SECURITY_INFORMATION,
            psid,
            PSID::default(),
            None,
            None,
        );
        if rc != ERROR_SUCCESS {
            return Err(format!("take ownership failed (Win32 {})", rc.0));
        }
    }

    let ea = EXPLICIT_ACCESS_W {
        grfAccessPermissions: access_mask,
        grfAccessMode: SET_ACCESS,
        grfInheritance: SUB_CONTAINERS_AND_OBJECTS_INHERIT,
        Trustee: TRUSTEE_W {
            pMultipleTrustee: ptr::null_mut(),
            MultipleTrusteeOperation: NO_MULTIPLE_TRUSTEE,
            TrusteeForm: TRUSTEE_IS_SID,
            TrusteeType: TRUSTEE_IS_GROUP,
            ptstrName: PWSTR(psid.0 as *mut u16),
        },
    };

    let mut new_acl: *mut ACL = ptr::null_mut();
    let rc = SetEntriesInAclW(Some(&[ea]), None, &mut new_acl);
    if rc != ERROR_SUCCESS {
        return Err(format!("SetEntriesInAcl failed (Win32 {})", rc.0));
    }

    // Note: new_acl is leaked (LocalFree omitted); the elevated helper exits
    // immediately after, so this is harmless.
    let rc = SetNamedSecurityInfoW(
        PWSTR(obj_w.as_mut_ptr()),
        obj_type,
        DACL_SECURITY_INFORMATION,
        PSID::default(),
        PSID::default(),
        Some(new_acl as *const ACL),
        None,
    );
    if rc != ERROR_SUCCESS {
        return Err(format!("set DACL failed (Win32 {})", rc.0));
    }
    Ok(())
}

/// Take ownership of a registry key and grant Administrators full control using
/// handle-based APIs. SeTakeOwnership (enabled earlier) lets us open the key for
/// WRITE_OWNER even when the DACL denies us. The string-path SetNamedSecurityInfo
/// variant fails on these MMDevices keys (Win32 2), so we go handle-based.
unsafe fn take_regkey(subkey: &str) -> Result<(), String> {
    const WRITE_OWNER: u32 = 0x0008_0000;
    const WRITE_DAC: u32 = 0x0004_0000;
    const READ_CONTROL: u32 = 0x0002_0000;

    let mut sid_buf = vec![0u8; SECURITY_MAX_SID_SIZE as usize];
    let psid = well_known_sid(WinBuiltinAdministratorsSid, &mut sid_buf)?;

    // 1) Open for WRITE_OWNER (allowed via SeTakeOwnership) and set owner=Admins.
    let mut hkey = HKEY::default();
    let rc = RegOpenKeyExW(
        HKEY_LOCAL_MACHINE,
        PCWSTR(wide(subkey).as_ptr()),
        0,
        REG_SAM_FLAGS(WRITE_OWNER),
        &mut hkey,
    );
    if rc != ERROR_SUCCESS {
        return Err(format!("open for WRITE_OWNER failed (Win32 {})", rc.0));
    }
    let rc = SetSecurityInfo(
        HANDLE(hkey.0),
        SE_REGISTRY_KEY,
        OWNER_SECURITY_INFORMATION,
        psid,
        PSID::default(),
        None,
        None,
    );
    let _ = RegCloseKey(hkey);
    if rc != ERROR_SUCCESS {
        return Err(format!("set owner failed (Win32 {})", rc.0));
    }

    // 2) Now owner: open for WRITE_DAC and grant admins full control (inheritable).
    let mut hkey = HKEY::default();
    let rc = RegOpenKeyExW(
        HKEY_LOCAL_MACHINE,
        PCWSTR(wide(subkey).as_ptr()),
        0,
        REG_SAM_FLAGS(WRITE_DAC | READ_CONTROL),
        &mut hkey,
    );
    if rc != ERROR_SUCCESS {
        return Err(format!("open for WRITE_DAC failed (Win32 {})", rc.0));
    }
    let ea = EXPLICIT_ACCESS_W {
        grfAccessPermissions: KEY_ALL_ACCESS.0,
        grfAccessMode: SET_ACCESS,
        grfInheritance: SUB_CONTAINERS_AND_OBJECTS_INHERIT,
        Trustee: TRUSTEE_W {
            pMultipleTrustee: ptr::null_mut(),
            MultipleTrusteeOperation: NO_MULTIPLE_TRUSTEE,
            TrusteeForm: TRUSTEE_IS_SID,
            TrusteeType: TRUSTEE_IS_GROUP,
            ptstrName: PWSTR(psid.0 as *mut u16),
        },
    };
    let mut new_acl: *mut ACL = ptr::null_mut();
    let rc2 = SetEntriesInAclW(Some(&[ea]), None, &mut new_acl);
    if rc2 != ERROR_SUCCESS {
        let _ = RegCloseKey(hkey);
        return Err(format!("SetEntriesInAcl failed (Win32 {})", rc2.0));
    }
    // new_acl is leaked (LocalFree omitted); the helper exits right after.
    let rc = SetSecurityInfo(
        HANDLE(hkey.0),
        SE_REGISTRY_KEY,
        DACL_SECURITY_INFORMATION,
        PSID::default(),
        PSID::default(),
        Some(new_acl as *const ACL),
        None,
    );
    let _ = RegCloseKey(hkey);
    if rc != ERROR_SUCCESS {
        return Err(format!("set DACL failed (Win32 {})", rc.0));
    }
    Ok(())
}

// ── Registry write helpers ──────────────────────────────────────────────────

unsafe fn open_or_create(subkey: &str) -> Result<HKEY, String> {
    let mut hkey = HKEY::default();
    let rc = RegCreateKeyExW(
        HKEY_LOCAL_MACHINE,
        PCWSTR(wide(subkey).as_ptr()),
        0,
        PCWSTR::null(),
        REG_OPTION_NON_VOLATILE,
        KEY_SET_VALUE,
        None,
        &mut hkey,
        None,
    );
    if rc != ERROR_SUCCESS {
        return Err(format!("open/create '{subkey}' failed (Win32 {})", rc.0));
    }
    Ok(hkey)
}

unsafe fn set_sz(hkey: HKEY, name: &str, value: &str) -> Result<(), String> {
    let data = wide(value);
    let bytes =
        std::slice::from_raw_parts(data.as_ptr() as *const u8, std::mem::size_of_val(&data[..]));
    let rc = RegSetValueExW(hkey, PCWSTR(wide(name).as_ptr()), 0, REG_SZ, Some(bytes));
    if rc != ERROR_SUCCESS {
        return Err(format!("set '{name}' failed (Win32 {})", rc.0));
    }
    Ok(())
}

unsafe fn set_dword(hkey: HKEY, name: &str, value: u32) -> Result<(), String> {
    let bytes = value.to_le_bytes();
    let rc = RegSetValueExW(hkey, PCWSTR(wide(name).as_ptr()), 0, REG_DWORD, Some(&bytes));
    if rc != ERROR_SUCCESS {
        return Err(format!("set '{name}' failed (Win32 {})", rc.0));
    }
    Ok(())
}

// ── Individual setup steps ──────────────────────────────────────────────────

/// Copy the whole bundled engine folder (the APO DLL + its dependency DLLs) into
/// a stable install dir. We copy every file because audiodg loads the APO by full
/// path and resolves its dependencies from that same directory.
fn install_engine_files() -> Result<PathBuf, String> {
    let src_dir = bundled_engine_dir().ok_or_else(|| {
        "bundled engine folder not found (expected engine/ next to the exe, or \
         src-tauri/engine-bundle/ in dev)"
            .to_string()
    })?;
    let dst_dir = engine_install_dir();
    fs::create_dir_all(&dst_dir).map_err(|e| format!("create install dir: {e}"))?;

    let mut copied = 0;
    for entry in fs::read_dir(&src_dir).map_err(|e| format!("read bundle: {e}"))? {
        let entry = entry.map_err(|e| e.to_string())?;
        if entry.path().is_file() {
            fs::copy(entry.path(), dst_dir.join(entry.file_name()))
                .map_err(|e| format!("copy {:?}: {e}", entry.file_name()))?;
            copied += 1;
        }
    }
    if copied == 0 {
        return Err("bundle folder is empty".to_string());
    }
    let dll = dst_dir.join(ENGINE_DLL_NAME);
    if !dll.exists() {
        return Err(format!("{ENGINE_DLL_NAME} missing from bundle"));
    }
    Ok(dll)
}

/// Registers the APO's COM class by writing the CLSID keys directly. We do NOT
/// use regsvr32: the APO imports audioeng.dll, which only loads inside
/// audiodg.exe, so LoadLibrary (and thus regsvr32/DllRegisterServer) fails in a
/// normal process. EqualizerAPO's own installer writes these keys directly too.
///
/// NOTE: this writes the standard in-proc COM server registration. If your
/// RegShot capture shows additional CLSID / APO-metadata keys, add them here.
fn register_apo_com(dll_path: &str) -> Result<(), String> {
    unsafe {
        let clsid_key = format!("SOFTWARE\\Classes\\CLSID\\{APO_CLSID}");
        let hk = open_or_create(&clsid_key)?;
        let _ = set_sz(hk, "", "QuickEqualizer Audio Engine");
        let _ = RegCloseKey(hk);

        let inproc = format!("{clsid_key}\\InprocServer32");
        let hk = open_or_create(&inproc)?;
        let r1 = set_sz(hk, "", dll_path);
        let r2 = set_sz(hk, "ThreadingModel", "Both");
        let _ = RegCloseKey(hk);
        r1.and(r2)
    }
}

fn install_and_register_dll() -> Result<(), String> {
    let dll = install_engine_files()?;
    register_apo_com(&dll.to_string_lossy())
}

fn set_disable_protected_audio() -> Result<(), String> {
    unsafe {
        let hkey = open_or_create("SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Audio")?;
        let r = set_dword(hkey, "DisableProtectedAudioDG", 1);
        let _ = RegCloseKey(hkey);
        r
    }
}

fn set_config_path() -> Result<(), String> {
    let cfg = engine_config_dir();
    fs::create_dir_all(&cfg).map_err(|e| format!("create config dir: {e}"))?;

    // Let the unelevated app write config.txt here at runtime.
    const FILE_ALL_ACCESS: u32 = 0x001F_01FF;
    unsafe {
        grant_full_access(
            &cfg.to_string_lossy(),
            SE_FILE_OBJECT,
            WinBuiltinUsersSid,
            FILE_ALL_ACCESS,
            false,
        )?;
    }

    let initial = config_txt_path();
    if !initial.exists() {
        let _ = fs::write(&initial, "Preamp: 0 dB\n");
    }

    unsafe {
        let hkey = open_or_create(CONFIG_PATH_KEY)?;
        let r = set_sz(hkey, CONFIG_PATH_VALUE, &cfg.to_string_lossy());
        let _ = RegCloseKey(hkey);
        r
    }
}

fn write_fx_values(device_id: &str) -> Result<(), String> {
    let render = format!(
        "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\MMDevices\\Audio\\Render\\{device_id}"
    );
    let fx_subkey = format!("{render}\\FxProperties");

    unsafe {
        // The whole tree is owned by TrustedInstaller. Take ownership of the
        // parent Render\{id} key (always present for a connected device) and grant
        // admins with inheritance, so we can create/modify FxProperties under it.
        take_regkey(&render).map_err(|e| format!("{e} [key: {render}]"))?;

        // If FxProperties already exists but is locked, take it over too. It won't
        // exist yet on many devices, so failure here is non-fatal.
        let _ = take_regkey(&fx_subkey);

        let hkey =
            open_or_create(&fx_subkey).map_err(|e| format!("{e} [key: {fx_subkey}]"))?;
        let mut err = None;
        for (name, value) in FX_VALUES {
            if let Err(e) = set_sz(hkey, name, value) {
                err = Some(format!("{e} [value: {name}]"));
                break;
            }
        }
        let _ = RegCloseKey(hkey);
        if let Some(e) = err {
            return Err(e);
        }
    }
    Ok(())
}

// ── Orchestration (called by the elevated helper) ───────────────────────────

/// Runs every setup step for `device_id`, writing a detailed report to the log.
/// Returns a process exit code: 0 if the critical steps succeeded.
pub fn register_for_device(device_id: &str) -> i32 {
    // Enabling these is required to take ownership of the locked registry keys.
    let mut report = Vec::new();
    let mut critical_ok = true;

    macro_rules! step {
        ($label:expr, $expr:expr, $critical:expr) => {
            match $expr {
                Ok(()) => report.push(format!("OK   {}", $label)),
                Err(e) => {
                    report.push(format!("FAIL {}: {}", $label, e));
                    if $critical {
                        critical_ok = false;
                    }
                }
            }
        };
    }

    unsafe {
        step!(
            "enable SeTakeOwnership",
            enable_privilege(SE_TAKE_OWNERSHIP_NAME),
            false
        );
        step!("enable SeRestore", enable_privilege(SE_RESTORE_NAME), false);
    }

    step!("install + register APO DLL", install_and_register_dll(), true);
    step!(
        "set DisableProtectedAudioDG",
        set_disable_protected_audio(),
        true
    );
    step!("set config path + permissions", set_config_path(), true);
    report.push(format!(
        "target: HKLM\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\MMDevices\\Audio\\Render\\{device_id}\\FxProperties"
    ));
    step!(
        "write device FxProperties",
        write_fx_values(device_id),
        true
    );

    report.push(if critical_ok {
        "\nRESULT: success — a reboot (or device disable/enable) is required for\nthe effect to load.".to_string()
    } else {
        "\nRESULT: incomplete — see failures above.".to_string()
    });

    let _ = fs::write(log_path(), report.join("\n"));

    if critical_ok {
        let _ = fs::write(marker_path(), "1");
        0
    } else {
        1
    }
}
