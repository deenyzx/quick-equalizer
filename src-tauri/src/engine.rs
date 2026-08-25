//! Option B: register the bundled audio-processing object (APO) ourselves
//! instead of relying on EqualizerAPO's installer. Everything here must run in
//! the elevated helper (see `run_engine_setup_elevated` in lib.rs).
//!
//! ─────────────────────────────────────────────────────────────────────────
//! The only thing you must SUPPLY is the APO DLL + its dependency DLLs: put them
//! in `engine/` next to the exe (the depot build copies `src-tauri/engine-bundle/`
//! there; in dev they're read from `src-tauri/engine-bundle/`). All the CLSIDs
//! and FxProperties values below were taken directly from EqualizerAPO's source,
//! so no RegShot capture is needed.
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
    GetNamedSecurityInfoW, GetSecurityInfo, SetEntriesInAclW, SetNamedSecurityInfoW,
    SetSecurityInfo, EXPLICIT_ACCESS_W, NO_MULTIPLE_TRUSTEE, SET_ACCESS, SE_FILE_OBJECT,
    SE_OBJECT_TYPE, SE_REGISTRY_KEY, TRUSTEE_IS_GROUP, TRUSTEE_IS_SID, TRUSTEE_W,
};
use windows::Win32::Security::{
    AdjustTokenPrivileges, CreateWellKnownSid, LookupPrivilegeValueW, ACL,
    DACL_SECURITY_INFORMATION, LUID_AND_ATTRIBUTES, OWNER_SECURITY_INFORMATION,
    PSECURITY_DESCRIPTOR, PSID,
    SECURITY_MAX_SID_SIZE, SE_PRIVILEGE_ENABLED, SE_RESTORE_NAME, SE_TAKE_OWNERSHIP_NAME,
    SUB_CONTAINERS_AND_OBJECTS_INHERIT, TOKEN_ADJUST_PRIVILEGES, TOKEN_PRIVILEGES, TOKEN_QUERY,
    WinBuiltinAdministratorsSid, WinBuiltinUsersSid, WinLocalServiceSid, WinLocalSystemSid,
    WELL_KNOWN_SID_TYPE,
};
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteTreeW, RegDeleteValueW, RegOpenKeyExW, RegSetValueExW,
    HKEY, HKEY_LOCAL_MACHINE, KEY_ALL_ACCESS, KEY_READ, KEY_SET_VALUE, REG_DWORD, REG_MULTI_SZ,
    REG_OPTION_NON_VOLATILE, REG_SAM_FLAGS, REG_SZ,
};
use windows::Win32::System::Registry::RegQueryValueExW;
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

// ── Values taken from EqualizerAPO's source (no RegShot needed) ─────────────
// APO CLSIDs: helpers/RegistryHelper.h. FxProperties layout + install logic:
// DeviceAPOInfo.cpp. APO metadata: EqualizerAPO/EqualizerAPO.cpp.

/// EqualizerAPO's two APO class IDs (pre-mix = per-stream, post-mix = endpoint).
const APO_PRE_MIX_CLSID: &str = "{EACD2258-FCAC-4FF4-B36D-419E924A6D79}";
const APO_POST_MIX_CLSID: &str = "{EC1CC9CE-FAED-4822-828A-82A81A6F018F}";

/// AUDIO_SIGNALPROCESSINGMODE_DEFAULT — the processing mode the effect applies to.
const DEFAULT_PROCESSING_MODE: &str = "{C18E2F7E-933D-4965-B7D1-1EEF228D2AF3}";

// FxProperties value names (fmtid {d04e05a6-…} = effect CLSIDs). Modern Windows
// render devices use the SFX (stream/pre-mix) and EFX (endpoint/post-mix) slots.
const FX_LFX_VALUE: &str = "{d04e05a6-594b-4fb6-a80d-01af5eed7d1d},1";
const FX_GFX_VALUE: &str = "{d04e05a6-594b-4fb6-a80d-01af5eed7d1d},2";
const FX_SFX_VALUE: &str = "{d04e05a6-594b-4fb6-a80d-01af5eed7d1d},5";
const FX_EFX_VALUE: &str = "{d04e05a6-594b-4fb6-a80d-01af5eed7d1d},7";
// Processing-mode associations (fmtid {d3993a3f-…}), written as REG_MULTI_SZ.
const FX_SFX_MODES_VALUE: &str = "{d3993a3f-99c2-4402-b5ec-a92a0367664b},5";
const FX_EFX_MODES_VALUE: &str = "{d3993a3f-99c2-4402-b5ec-a92a0367664b},7";
// PKEY_AudioEndpoint_Disable_SysFx — if 1, all effects are off; we remove it.
const FX_DISABLE_SYSFX_VALUE: &str = "{1da5d803-d492-4edd-8c23-e0c0ffee7f0e},5";

/// Filename of the bundled APO DLL (shipped at `<exe dir>/engine/<this>`).
const ENGINE_DLL_NAME: &str = "EqualizerAPO.dll";

/// Where EqualizerAPO's APO reads its config path from.
const CONFIG_PATH_KEY: &str = "SOFTWARE\\EqualizerAPO";
const CONFIG_PATH_VALUE: &str = "ConfigPath";

// ── Stable paths ────────────────────────────────────────────────────────────

fn engine_install_dir() -> PathBuf {
    PathBuf::from(r"C:\Program Files\QuickEqualizer\engine")
}

/// User-writable config dir (the unelevated app writes config.txt here on every
/// EQ change, and the APO running in audiodg must be able to read it).
fn engine_config_dir() -> PathBuf {
    state_dir().join("config")
}

/// Path the unelevated side writes EQ filters to. Public so lib.rs can use it.
pub fn config_txt_path() -> PathBuf {
    engine_config_dir().join("config.txt")
}

/// Machine-wide state. Deliberately NOT under %APPDATA%: the elevated helper can
/// run as a *different* account (on a standard-user PC, UAC elevates using an
/// admin's credentials), so per-user paths would have the helper writing state
/// the app can never read — leaving setup looking permanently undone.
fn state_dir() -> PathBuf {
    let p = PathBuf::from(r"C:\ProgramData\QuickEqualizer");
    let _ = fs::create_dir_all(&p);
    p
}

fn marker_path() -> PathBuf {
    state_dir().join("engine_registered")
}

pub fn log_path() -> PathBuf {
    state_dir().join("engine_setup.log")
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

    // Merge into the existing DACL rather than replacing it, so we don't strip
    // SYSTEM/Administrators (and thus lock out the audio service).
    let mut old_dacl: *mut ACL = ptr::null_mut();
    let mut sd = PSECURITY_DESCRIPTOR::default();
    let got_old = GetNamedSecurityInfoW(
        PCWSTR(obj_w.as_ptr()),
        obj_type,
        DACL_SECURITY_INFORMATION,
        None,
        None,
        Some(&mut old_dacl),
        None,
        &mut sd,
    ) == ERROR_SUCCESS;

    let mut new_acl: *mut ACL = ptr::null_mut();
    let rc = SetEntriesInAclW(
        Some(&[ea]),
        if got_old && !old_dacl.is_null() {
            Some(old_dacl as *const ACL)
        } else {
            None
        },
        &mut new_acl,
    );
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
    // Grant every principal that must reach this key. We set these explicitly
    // (rather than only adding Administrators) so the key ends up in a known-good
    // state even if a previous run replaced its DACL: audiodg.exe runs as
    // LOCAL SERVICE and must be able to READ FxProperties, and SYSTEM/Users need
    // their normal access back.
    let grants: [(WELL_KNOWN_SID_TYPE, u32); 4] = [
        (WinBuiltinAdministratorsSid, KEY_ALL_ACCESS.0),
        (WinLocalSystemSid, KEY_ALL_ACCESS.0),
        (WinLocalServiceSid, KEY_READ.0),
        (WinBuiltinUsersSid, KEY_READ.0),
    ];
    let mut sid_bufs: Vec<Vec<u8>> = Vec::new();
    let mut eas: Vec<EXPLICIT_ACCESS_W> = Vec::new();
    for (kind, access) in grants {
        let mut buf = vec![0u8; SECURITY_MAX_SID_SIZE as usize];
        let sid = match well_known_sid(kind, &mut buf) {
            Ok(s) => s,
            Err(_) => continue,
        };
        eas.push(EXPLICIT_ACCESS_W {
            grfAccessPermissions: access,
            grfAccessMode: SET_ACCESS,
            grfInheritance: SUB_CONTAINERS_AND_OBJECTS_INHERIT,
            Trustee: TRUSTEE_W {
                pMultipleTrustee: ptr::null_mut(),
                MultipleTrusteeOperation: NO_MULTIPLE_TRUSTEE,
                TrusteeForm: TRUSTEE_IS_SID,
                TrusteeType: TRUSTEE_IS_GROUP,
                ptstrName: PWSTR(sid.0 as *mut u16),
            },
        });
        sid_bufs.push(buf); // keep each SID alive until SetEntriesInAcl runs
    }

    // Merge into the key's EXISTING DACL. Passing None as the old ACL would build
    // an ACL containing only our entries, stripping everything else.
    let mut old_dacl: *mut ACL = ptr::null_mut();
    let mut sd = PSECURITY_DESCRIPTOR::default();
    let got_old = GetSecurityInfo(
        HANDLE(hkey.0),
        SE_REGISTRY_KEY,
        DACL_SECURITY_INFORMATION,
        None,
        None,
        Some(&mut old_dacl),
        None,
        Some(&mut sd),
    ) == ERROR_SUCCESS;

    let mut new_acl: *mut ACL = ptr::null_mut();
    let rc2 = SetEntriesInAclW(
        Some(&eas),
        if got_old && !old_dacl.is_null() {
            Some(old_dacl as *const ACL)
        } else {
            None
        },
        &mut new_acl,
    );
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

/// Writes a REG_MULTI_SZ holding a single string (double-null terminated).
unsafe fn set_multi_sz(hkey: HKEY, name: &str, value: &str) -> Result<(), String> {
    let mut data: Vec<u16> = value.encode_utf16().collect();
    data.push(0); // terminate the string
    data.push(0); // terminate the list
    let bytes = std::slice::from_raw_parts(data.as_ptr() as *const u8, data.len() * 2);
    let rc = RegSetValueExW(hkey, PCWSTR(wide(name).as_ptr()), 0, REG_MULTI_SZ, Some(bytes));
    if rc != ERROR_SUCCESS {
        return Err(format!("set '{name}' failed (Win32 {})", rc.0));
    }
    Ok(())
}

/// Best-effort delete of a value under an HKLM subkey.
unsafe fn delete_value(subkey: &str, name: &str) {
    let mut hkey = HKEY::default();
    if RegOpenKeyExW(
        HKEY_LOCAL_MACHINE,
        PCWSTR(wide(subkey).as_ptr()),
        0,
        REG_SAM_FLAGS(KEY_SET_VALUE.0),
        &mut hkey,
    ) == ERROR_SUCCESS
    {
        let _ = RegDeleteValueW(hkey, PCWSTR(wide(name).as_ptr()));
        let _ = RegCloseKey(hkey);
    }
}

/// Best-effort recursive delete of an HKLM subkey (key + all subkeys/values).
unsafe fn delete_tree(subkey: &str) {
    let _ = RegDeleteTreeW(HKEY_LOCAL_MACHINE, PCWSTR(wide(subkey).as_ptr()));
}

/// True if an HKLM subkey exists (and we can open it for reading).
unsafe fn key_exists(subkey: &str) -> bool {
    let mut hkey = HKEY::default();
    if RegOpenKeyExW(
        HKEY_LOCAL_MACHINE,
        PCWSTR(wide(subkey).as_ptr()),
        0,
        REG_SAM_FLAGS(KEY_READ.0),
        &mut hkey,
    ) == ERROR_SUCCESS
    {
        let _ = RegCloseKey(hkey);
        true
    } else {
        false
    }
}

/// Reads a REG_SZ value, or None if the key/value is missing or unreadable.
unsafe fn read_sz(subkey: &str, name: &str) -> Option<String> {
    let mut hkey = HKEY::default();
    if RegOpenKeyExW(
        HKEY_LOCAL_MACHINE,
        PCWSTR(wide(subkey).as_ptr()),
        0,
        REG_SAM_FLAGS(KEY_READ.0),
        &mut hkey,
    ) != ERROR_SUCCESS
    {
        return None;
    }
    let name_w = wide(name);
    let mut size: u32 = 0;
    let rc = RegQueryValueExW(
        hkey,
        PCWSTR(name_w.as_ptr()),
        None,
        None,
        None,
        Some(&mut size),
    );
    if rc != ERROR_SUCCESS || size == 0 {
        let _ = RegCloseKey(hkey);
        return None;
    }
    let mut buf = vec![0u8; size as usize];
    let rc = RegQueryValueExW(
        hkey,
        PCWSTR(name_w.as_ptr()),
        None,
        None,
        Some(buf.as_mut_ptr()),
        Some(&mut size),
    );
    let _ = RegCloseKey(hkey);
    if rc != ERROR_SUCCESS {
        return None;
    }
    let units: Vec<u16> = buf
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    let end = units.iter().position(|&c| c == 0).unwrap_or(units.len());
    Some(String::from_utf16_lossy(&units[..end]))
}

// ── Individual setup steps ──────────────────────────────────────────────────

/// Copy the whole bundled engine folder (the APO DLL + its dependency DLLs) into
/// a stable install dir. We copy every file because audiodg loads the APO by full
/// path and resolves its dependencies from that same directory.
fn install_engine_files() -> Result<PathBuf, String> {
    let src_dir = bundled_engine_dir().ok_or_else(|| {
        // Name the paths actually checked — otherwise "not found" gives no clue
        // which exe was launched or where its engine folder was expected.
        let exe = std::env::current_exe()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| "<unknown>".into());
        format!("bundled engine folder not found. exe: {exe} — looked for 'engine' beside it, then '../../engine-bundle'")
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

/// Installs the engine files, then registers the APO by running the DLL's own
/// DllRegisterServer via regsvr32. regsvr32 loads the DLL with its dependencies
/// beside it (in the install dir) and calls the official RegisterAPO — so the COM
/// class and AudioProcessingObjects metadata are written by EqualizerAPO itself,
/// with nothing hand-rolled to keep in sync.
fn install_and_register_dll() -> Result<(), String> {
    let dll = install_engine_files()?;

    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let status = std::process::Command::new("regsvr32")
        .args(["/s", &dll.to_string_lossy()])
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map_err(|e| format!("spawn regsvr32: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("regsvr32 exit {}", status.code().unwrap_or(-1)))
    }
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

    // Let the unelevated app write config.txt and read the state/logs the
    // elevated helper leaves behind. Granted on the whole state dir (which the
    // config dir lives under) so it inherits down.
    const FILE_ALL_ACCESS: u32 = 0x001F_01FF;
    unsafe {
        grant_full_access(
            &state_dir().to_string_lossy(),
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
        // Verbose APO logging from inside audiodg. Invaluable while validating
        // that the effect actually loads; set to "false" before release.
        let _ = set_sz(hkey, "EnableTrace", "true");
        let _ = RegCloseKey(hkey);
        r
    }
}

/// The APO logs to GetTempPath()+EqualizerAPO.log. It runs inside audiodg.exe as
/// LOCAL SERVICE, so the file normally lands in that service profile's temp
/// directory, which a normal user process can't read.
fn apo_log_candidates() -> Vec<PathBuf> {
    let mut v = vec![PathBuf::from(
        r"C:\Windows\ServiceProfiles\LocalService\AppData\Local\Temp\EqualizerAPO.log",
    )];
    v.push(PathBuf::from(r"C:\Windows\Temp\EqualizerAPO.log"));
    if let Ok(t) = std::env::var("TEMP") {
        v.push(PathBuf::from(t).join("EqualizerAPO.log"));
    }
    v
}

/// Where we copy the APO log so the unelevated app can read it.
pub fn apo_trace_copy_path() -> PathBuf {
    state_dir().join("apo_trace.log")
}

/// Elevated: copy the APO's own log somewhere the app can read it. Returns an
/// exit code so the helper can report success.
pub fn collect_logs() -> i32 {
    for candidate in apo_log_candidates() {
        if candidate.exists() {
            if fs::copy(&candidate, apo_trace_copy_path()).is_ok() {
                return 0;
            }
        }
    }
    let _ = fs::write(
        apo_trace_copy_path(),
        "No EqualizerAPO.log found yet.\n\nThe audio engine writes this only once it is actually \
         loaded by the Windows audio service — reboot after setup, play some audio, then try \
         again. If it stays empty, the effect is not being loaded at all.\n",
    );
    1
}

/// IMMDevice::GetId() returns "{0.0.0.00000000}.{guid}", but the MMDevices
/// registry subkey is just the trailing "{guid}". Strip the container prefix.
fn registry_device_key(device_id: &str) -> &str {
    match device_id.rfind('{') {
        Some(i) => &device_id[i..],
        None => device_id,
    }
}

fn write_fx_values(device_id: &str) -> Result<(), String> {
    let device_key = registry_device_key(device_id);
    let render = format!(
        "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\MMDevices\\Audio\\Render\\{device_key}"
    );
    let fx = format!("{render}\\FxProperties");

    unsafe {
        // The whole tree is owned by TrustedInstaller. Take ownership of the
        // parent Render\{id} key (always present for a connected device) and grant
        // admins with inheritance, so we can create/modify FxProperties under it.
        take_regkey(&render).map_err(|e| format!("{e} [key: {render}]"))?;
        // FxProperties may exist with a locked ACL; non-fatal if it doesn't yet.
        let _ = take_regkey(&fx);

        // Preserve any vendor APO already on this device BEFORE we overwrite the
        // slots, so its effects can be chained to and restored on uninstall.
        backup_original_apos(device_key, &fx);

        // Remove legacy LFX/GFX entries so only the modern SFX/EFX slots apply
        // (no-op if FxProperties doesn't exist yet).
        delete_value(&fx, FX_LFX_VALUE);
        delete_value(&fx, FX_GFX_VALUE);

        // Create FxProperties if needed (inherits admin access from the parent).
        let hkey = open_or_create(&fx).map_err(|e| format!("{e} [key: {fx}]"))?;
        // SFX = pre-mix (per-stream), EFX = post-mix (endpoint).
        let res = (|| {
            set_sz(hkey, FX_SFX_VALUE, APO_PRE_MIX_CLSID).map_err(|e| format!("{e} [value: SFX]"))?;
            set_sz(hkey, FX_EFX_VALUE, APO_POST_MIX_CLSID).map_err(|e| format!("{e} [value: EFX]"))?;
            Ok::<(), String>(())
        })();
        // Processing-mode associations (REG_MULTI_SZ), best-effort.
        let _ = set_multi_sz(hkey, FX_SFX_MODES_VALUE, DEFAULT_PROCESSING_MODE);
        let _ = set_multi_sz(hkey, FX_EFX_MODES_VALUE, DEFAULT_PROCESSING_MODE);
        let _ = RegCloseKey(hkey);
        res?;

        // Force-enable enhancements (remove the "disable system effects" flag).
        delete_value(&fx, FX_DISABLE_SYSFX_VALUE);
    }
    Ok(())
}

/// Where the original (vendor) APO GUIDs are stashed so we can chain to them and
/// put them back on uninstall — the same location the APO DLL reads at runtime.
fn child_apo_key(device_key: &str) -> String {
    format!("{CONFIG_PATH_KEY}\\Child APOs\\{device_key}")
}

/// Saves whatever APO GUIDs the device already had, so the vendor's effects can
/// be chained to (rather than silently destroyed) and restored on uninstall.
/// Only writes the backup once — re-running setup must not overwrite the
/// originals with our own CLSIDs.
unsafe fn backup_original_apos(device_key: &str, fx: &str) {
    let backup = child_apo_key(device_key);
    if read_sz(&backup, "Version").is_some() {
        return; // already backed up
    }
    // Pre-mix slot was SFX (or legacy LFX); post-mix was EFX (or legacy GFX).
    let pre = read_sz(fx, FX_SFX_VALUE).or_else(|| read_sz(fx, FX_LFX_VALUE));
    let post = read_sz(fx, FX_EFX_VALUE).or_else(|| read_sz(fx, FX_GFX_VALUE));

    let is_ours =
        |g: &String| g.eq_ignore_ascii_case(APO_PRE_MIX_CLSID) || g.eq_ignore_ascii_case(APO_POST_MIX_CLSID);
    let pre = pre.filter(|g| !is_ours(g)).unwrap_or_default();
    let post = post.filter(|g| !is_ours(g)).unwrap_or_default();

    if let Ok(hk) = open_or_create(&backup) {
        let _ = set_sz(hk, "PreMixChild", &pre);
        let _ = set_sz(hk, "PostMixChild", &post);
        let _ = set_sz(hk, "AllowSilentBufferModification", "false");
        let _ = set_sz(hk, "Version", "1.4.2");
        let _ = RegCloseKey(hk);
    }
}

/// Strips our effect values from a device's FxProperties and puts back whatever
/// vendor APO was there before us, so removing the engine leaves the device as
/// we found it. Best-effort.
fn remove_fx_values(device_id: &str) {
    let key = registry_device_key(device_id);
    let render =
        format!("SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\MMDevices\\Audio\\Render\\{key}");
    let fx = format!("{render}\\FxProperties");
    unsafe {
        let backup = child_apo_key(key);
        let pre = read_sz(&backup, "PreMixChild").unwrap_or_default();
        let post = read_sz(&backup, "PostMixChild").unwrap_or_default();
        delete_tree(&backup);

        // Nothing of ours can be on a device whose FxProperties never existed —
        // don't create the key just to clean it.
        if !key_exists(&fx) {
            return;
        }
        let _ = take_regkey(&render);
        let _ = take_regkey(&fx);

        // Restore the original APO where there was one; otherwise just remove ours.
        if pre.is_empty() {
            delete_value(&fx, FX_SFX_VALUE);
            delete_value(&fx, FX_SFX_MODES_VALUE);
        } else if let Ok(hk) = open_or_create(&fx) {
            let _ = set_sz(hk, FX_SFX_VALUE, &pre);
            let _ = RegCloseKey(hk);
        }
        if post.is_empty() {
            delete_value(&fx, FX_EFX_VALUE);
            delete_value(&fx, FX_EFX_MODES_VALUE);
        } else if let Ok(hk) = open_or_create(&fx) {
            let _ = set_sz(hk, FX_EFX_VALUE, &post);
            let _ = RegCloseKey(hk);
        }
    }
}

/// The device id we last registered on, if any (stored in the marker file).
fn previously_registered_device() -> Option<String> {
    fs::read_to_string(marker_path())
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && s != "1")
}

/// True if the engine is already registered on this exact device — used to skip a
/// redundant (re)install and the "restart your PC" prompt when nothing changed.
pub fn is_active_for(device_id: &str) -> bool {
    is_registered()
        && previously_registered_device()
            .map(|reg| registry_device_key(&reg) == registry_device_key(device_id))
            .unwrap_or(false)
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

    // If we were previously registered on a different device, strip our effect
    // values from it first — otherwise the EQ stays active on the old device and
    // uninstall would never clean it up.
    if let Some(old) = previously_registered_device() {
        if registry_device_key(&old) != registry_device_key(device_id) {
            remove_fx_values(&old);
            report.push("removed effect from previously configured device".to_string());
        }
    }

    report.push(format!(
        "target: HKLM\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\MMDevices\\Audio\\Render\\{}\\FxProperties",
        registry_device_key(device_id)
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
        // Record which device we registered, so a later device change or
        // uninstall knows exactly what to clean up.
        let _ = fs::write(marker_path(), device_id);
        0
    } else {
        1
    }
}

/// Reverses `register_for_device`: removes the device effect values, the COM/APO
/// registrations, and the installed files. Best-effort — always returns 0 so the
/// app can reset its state regardless of what was still present.
pub fn unregister_for_device(device_id: Option<&str>) -> i32 {
    let mut report: Vec<String> = Vec::new();

    unsafe {
        let _ = enable_privilege(SE_TAKE_OWNERSHIP_NAME);
        let _ = enable_privilege(SE_RESTORE_NAME);
    }

    // Clean the device we were told about, and also the one we recorded at
    // registration time (they can differ if the selection changed since).
    let mut cleaned: Vec<String> = Vec::new();
    for id in device_id
        .map(str::to_string)
        .into_iter()
        .chain(previously_registered_device())
    {
        let key = registry_device_key(&id).to_string();
        if cleaned.contains(&key) {
            continue;
        }
        remove_fx_values(&id);
        cleaned.push(key);
    }
    if !cleaned.is_empty() {
        report.push(format!("removed effect from {} device(s)", cleaned.len()));
    }

    // Let the DLL undo its own registration (symmetric with regsvr32 on install),
    // then remove anything it left behind.
    let dll = engine_install_dir().join(ENGINE_DLL_NAME);
    if dll.exists() {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let _ = std::process::Command::new("regsvr32")
            .args(["/s", "/u", &dll.to_string_lossy()])
            .creation_flags(CREATE_NO_WINDOW)
            .status();
    }

    unsafe {
        delete_tree(&format!("SOFTWARE\\Classes\\CLSID\\{APO_PRE_MIX_CLSID}"));
        delete_tree(&format!("SOFTWARE\\Classes\\CLSID\\{APO_POST_MIX_CLSID}"));
        delete_tree(&format!(
            "SOFTWARE\\Classes\\AudioEngine\\AudioProcessingObjects\\{APO_PRE_MIX_CLSID}"
        ));
        delete_tree(&format!(
            "SOFTWARE\\Classes\\AudioEngine\\AudioProcessingObjects\\{APO_POST_MIX_CLSID}"
        ));
        report.push("removed COM/APO registrations".to_string());
    }

    let _ = fs::remove_dir_all(engine_install_dir());
    let _ = fs::remove_dir_all(engine_config_dir());
    let _ = fs::remove_file(marker_path());
    report.push("removed installed files".to_string());

    let _ = fs::write(
        log_path(),
        format!("UNINSTALL:\n{}\n\nRESULT: done", report.join("\n")),
    );
    0
}
