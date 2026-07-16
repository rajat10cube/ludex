//! Native process launching (no console windows) + playtime timing.

use std::fs;
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::Command;
use std::time::Instant;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Run a hidden PowerShell command, returning trimmed stdout on success.
fn powershell(script: &str) -> Result<String, String> {
    let out = Command::new("powershell")
        .args([
            "-NoProfile",
            "-WindowStyle",
            "Hidden",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(err.trim().to_string());
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Launch an executable and block until it exits; returns seconds elapsed.
/// `elevated` requests a UAC prompt (needed by disc setups and hypervisor/Denuvo
/// releases) via the real `ShellExecuteEx("runas")` API — NOT a hidden PowerShell,
/// which Windows refuses to elevate from a windowless background context ("Not
/// enough memory resources are available to process this command", error 1450).
pub fn run_and_wait(exe: &Path, workdir: &Path, elevated: bool) -> Result<u64, String> {
    let start = Instant::now();
    if elevated {
        run_elevated_wait(exe, workdir)?;
    } else {
        let mut child = Command::new(exe)
            .current_dir(workdir)
            .spawn()
            .map_err(|e| format!("Could not launch the game: {e}"))?;
        child.wait().map_err(|e| e.to_string())?;
    }
    Ok(start.elapsed().as_secs())
}

/// Launch `exe` elevated (UAC) via ShellExecuteEx and wait for it to exit.
/// Runs from Ludex's own interactive process so the consent UI shows normally.
fn run_elevated_wait(exe: &Path, workdir: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{WaitForSingleObject, INFINITE};
    use windows_sys::Win32::UI::Shell::{
        ShellExecuteExW, SEE_MASK_FLAG_NO_UI, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    fn wide(p: &Path) -> Vec<u16> {
        p.as_os_str().encode_wide().chain(std::iter::once(0)).collect()
    }
    let verb: Vec<u16> = "runas\0".encode_utf16().collect();
    let file = wide(exe);
    let dir = wide(workdir);

    // SEE_MASK_FLAG_NO_UI suppresses Windows' own error dialog so we report
    // failures ourselves; NOCLOSEPROCESS hands us the process handle to wait on.
    let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
    info.fMask = SEE_MASK_NOCLOSEPROCESS | SEE_MASK_FLAG_NO_UI;
    info.lpVerb = verb.as_ptr();
    info.lpFile = file.as_ptr();
    info.lpDirectory = dir.as_ptr();
    info.nShow = SW_SHOWNORMAL as i32;

    let ok = unsafe { ShellExecuteExW(&mut info) };
    if ok == 0 {
        let e = std::io::Error::last_os_error();
        // ERROR_CANCELLED (1223): the user dismissed the UAC prompt.
        if e.raw_os_error() == Some(1223) {
            return Err("The elevation (UAC) prompt was cancelled.".into());
        }
        return Err(format!("Windows wouldn't launch the installer elevated: {e}"));
    }
    if !info.hProcess.is_null() {
        unsafe {
            WaitForSingleObject(info.hProcess, INFINITE);
            CloseHandle(info.hProcess);
        }
    }
    Ok(())
}

/// Mount an .iso and return its drive root (e.g. ``"E:\\"``).
pub fn mount_iso(iso: &Path) -> Result<String, String> {
    let iso_s = iso.to_string_lossy().replace('\'', "''");
    let script = format!(
        "$null = Mount-DiskImage -ImagePath '{iso_s}'; \
         Start-Sleep -Milliseconds 700; \
         $l = (Get-DiskImage -ImagePath '{iso_s}' | Get-Volume).DriveLetter; \
         if (-not $l) {{ Start-Sleep -Seconds 1; \
             $l = (Get-DiskImage -ImagePath '{iso_s}' | Get-Volume).DriveLetter }}; \
         Write-Output $l"
    );
    let letter = powershell(&script).map_err(|e| {
        if e.is_empty() { "Could not mount the disc image".into() } else { e }
    })?;
    let letter = letter.trim();
    if letter.is_empty() {
        return Err("Mounted the disc but couldn't find its drive letter".into());
    }
    Ok(format!("{letter}:\\"))
}

/// Dismount a previously-mounted .iso (best effort).
pub fn dismount_iso(iso: &Path) {
    let iso_s = iso.to_string_lossy().replace('\'', "''");
    let _ = powershell(&format!("Dismount-DiskImage -ImagePath '{iso_s}' | Out-Null"));
}

/// Find and run the disc's setup (setup/install/autorun .exe) at the drive root,
/// elevated, blocking until it exits. Scene setups usually require admin.
/// Returns `Ok(true)` if a setup ran, `Ok(false)` if none was found (not an
/// error — some discs are just data), `Err` if a setup ran but failed.
///
/// `workdir` must be a **writable** directory — never the read-only disc root,
/// or InstallShield-style setups fail spuriously ("Not enough memory resources
/// are available to process this command").
pub fn run_setup_on_drive(drive: &str, workdir: &Path) -> Result<bool, String> {
    let root = Path::new(drive);
    let setup = fs::read_dir(root)
        .map_err(|e| format!("Couldn't read the disc: {e}"))?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension().map(|x| x.eq_ignore_ascii_case("exe")).unwrap_or(false)
        })
        .find(|p| {
            let n = p.file_name().unwrap_or_default().to_string_lossy().to_lowercase();
            n.contains("setup") || n.contains("install") || n.contains("autorun")
        });
    match setup {
        Some(exe) => run_and_wait(&exe, workdir, true).map(|_| true),
        None => Ok(false),
    }
}

/// Open a folder in Explorer.
pub fn open_folder(path: &Path) {
    let _ = Command::new("explorer")
        .arg(path)
        .creation_flags(CREATE_NO_WINDOW)
        .spawn();
}
