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
/// `elevated` launches with a UAC prompt (needed by hypervisor/Denuvo releases)
/// via a hidden PowerShell `Start-Process -Verb RunAs -Wait`.
pub fn run_and_wait(exe: &Path, workdir: &Path, elevated: bool) -> Result<u64, String> {
    let start = Instant::now();
    if elevated {
        let exe_s = exe.to_string_lossy().replace('\'', "''");
        let dir_s = workdir.to_string_lossy().replace('\'', "''");
        let script = format!(
            "Start-Process -FilePath '{exe_s}' -WorkingDirectory '{dir_s}' -Verb RunAs -Wait"
        );
        let status = Command::new("powershell")
            .args([
                "-NoProfile",
                "-WindowStyle",
                "Hidden",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                &script,
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .status()
            .map_err(|e| format!("Could not launch the game: {e}"))?;
        if !status.success() {
            return Err("Launch was cancelled or the UAC prompt was denied.".into());
        }
    } else {
        let mut child = Command::new(exe)
            .current_dir(workdir)
            .spawn()
            .map_err(|e| format!("Could not launch the game: {e}"))?;
        child.wait().map_err(|e| e.to_string())?;
    }
    Ok(start.elapsed().as_secs())
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
pub fn run_setup_on_drive(drive: &str) -> Result<bool, String> {
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
        Some(exe) => run_and_wait(&exe, root, true).map(|_| true),
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
