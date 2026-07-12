//! Native process launching (no console windows) + playtime timing.

use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::Command;
use std::time::Instant;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

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

/// Mount an .iso, run its setup, dismount — via hidden PowerShell (no console flash).
pub fn run_installer_from_iso(iso: &Path) -> Result<(), String> {
    let iso_str = iso.to_string_lossy().replace('\'', "''");
    let script = format!(
        "$img = Mount-DiskImage -ImagePath '{iso}' -PassThru; \
         $v = ($img | Get-Volume).DriveLetter; \
         $s = Get-ChildItem \"$($v):\\\" -Filter *.exe | \
              Where-Object {{ $_.Name -match 'setup|install|autorun' }} | Select-Object -First 1; \
         if ($s) {{ Start-Process -FilePath $s.FullName -Wait }} else {{ Start-Process \"$($v):\\\" }}; \
         Start-Sleep -Seconds 2; Dismount-DiskImage -ImagePath '{iso}' | Out-Null",
        iso = iso_str
    );
    let status = std::process::Command::new("powershell")
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
        .map_err(|e| e.to_string())?;
    if !status.success() {
        return Err("The disc image setup did not complete".into());
    }
    Ok(())
}

/// Open a folder in Explorer.
pub fn open_folder(path: &Path) {
    let _ = Command::new("explorer")
        .arg(path)
        .creation_flags(CREATE_NO_WINDOW)
        .spawn();
}
