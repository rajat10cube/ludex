<#
  Ludex Windows companion agent.

  Invoked two ways:
    * by the ludex:// protocol handler:   -Uri "ludex://install/<slug>"
    * directly, for maintenance:          -Command <hello|sync|list|install|play|uninstall> [-Slug <slug>]

  It downloads games from your Ludex server, extracts/installs them locally,
  launches them, and reports installs + playtime back to the server.
#>
[CmdletBinding()]
param(
    [string]$Uri,
    [ValidateSet("hello", "sync", "list", "install", "play", "uninstall", "")]
    [string]$Command = "",
    [string]$Slug
)

$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"   # keep multi-GB downloads fast
$Home_ = Join-Path $env:LOCALAPPDATA "Ludex"
$ConfigPath = Join-Path $Home_ "config.json"
$StatePath = Join-Path $Home_ "state.json"

$JUNK_EXE = @("unins", "setup", "install", "redist", "vcredist", "vc_redist", "dxsetup",
    "dxwebsetup", "dotnet", "oalinst", "crashhandler", "unitycrashhandler", "crashreport",
    "crashpad", "touchup", "cleanup", "activate", "register")

function Info($m) { Write-Host "[ludex] $m" -ForegroundColor Cyan }
function Ok($m)   { Write-Host "[ludex] $m" -ForegroundColor Green }
function Warn($m) { Write-Host "[ludex] $m" -ForegroundColor Yellow }
function Fail($m) { Write-Host "[ludex] $m" -ForegroundColor Red }

# --- config + auth -----------------------------------------------------------
if (-not (Test-Path $ConfigPath)) { throw "Not configured. Run install.ps1 first." }
$Config = Get-Content $ConfigPath -Raw | ConvertFrom-Json
$Server = $Config.server.TrimEnd("/")

function Get-Auth {
    $sec = ConvertTo-SecureString $Config.password
    $plain = [Runtime.InteropServices.Marshal]::PtrToStringAuto(
        [Runtime.InteropServices.Marshal]::SecureStringToBSTR($sec))
    $pair = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes("$($Config.username):$plain"))
    return @{ Authorization = "Basic $pair" }
}
$Auth = Get-Auth

function Api($path, $method = "Get", $body = $null) {
    $req = @{ Uri = "$Server/api$path"; Headers = $Auth; Method = $method; TimeoutSec = 60 }
    if ($null -ne $body) {
        $req.Body = ($body | ConvertTo-Json -Depth 6)
        $req.ContentType = "application/json"
    }
    return Invoke-RestMethod @req
}

# --- local install state -----------------------------------------------------
function Load-State {
    if (Test-Path $StatePath) {
        $obj = Get-Content $StatePath -Raw | ConvertFrom-Json
        $h = @{}
        foreach ($p in $obj.PSObject.Properties) { $h[$p.Name] = $p.Value }
        return $h
    }
    return @{}
}
function Save-State($state) {
    $state | ConvertTo-Json -Depth 6 | Set-Content -Path $StatePath -Encoding UTF8
}
function Report-Installed($state) {
    $games = @()
    foreach ($slug in $state.Keys) {
        $games += @{ slug = $slug; version = $state[$slug].version; install_path = $state[$slug].path }
    }
    try { Api "/agent/installed" "Post" @{ device = $Config.device; games = $games } | Out-Null }
    catch { Warn "Could not sync installed list: $_" }
}

function Find-Exe($root, $hint) {
    if ($hint) {
        $p = Join-Path $root ($hint -replace "/", "\")
        if (Test-Path $p) { return $p }
    }
    $exes = Get-ChildItem -Path $root -Recurse -Filter *.exe -ErrorAction SilentlyContinue |
        Where-Object { $n = $_.Name.ToLower(); -not ($JUNK_EXE | Where-Object { $n -like "*$_*" }) }
    if (-not $exes) { return $null }
    return ($exes | Sort-Object Length -Descending | Select-Object -First 1).FullName
}

function Download-Game($slug, $outFile) {
    Info "Downloading... (this can take a while for big games)"
    Invoke-WebRequest -Uri "$Server/api/download/$slug" -Headers $Auth -OutFile $outFile
}

# --- verbs -------------------------------------------------------------------
function Do-Hello {
    $r = Api "/agent/hello" "Post" @{ device = $Config.device; platform = "windows"; agent_version = "0.1.0" }
    Ok "Connected to Ludex $($r.serverVersion) as $($r.user)."
}

function Do-Install($slug) {
    $game = Api "/games/$slug"
    Info "Installing $($game.title)..."
    $dest = Join-Path $Config.gamesDir $slug
    New-Item -ItemType Directory -Force -Path $dest | Out-Null
    $tmp = Join-Path $env:TEMP "ludex-$slug-$([guid]::NewGuid().ToString('N').Substring(0,8))"

    if ($game.downloadKind -eq "tar") {
        $tar = "$tmp.tar"
        Download-Game $slug $tar
        if (-not (Get-Command tar.exe -ErrorAction SilentlyContinue)) {
            throw "tar.exe not found (needs Windows 10 1803+). Extract $tar manually into $dest."
        }
        Info "Extracting..."
        & tar.exe -xf $tar -C $dest
        Remove-Item $tar -Force -ErrorAction SilentlyContinue
        # A folder release whose real payload is an .iso / installer inside it.
        if ($game.payloadPath) {
            $payload = Join-Path $dest ($game.payloadPath -replace "/", "\")
            if ($game.setupType -eq "iso" -and (Test-Path $payload)) {
                Install-FromIso $payload $game
            } elseif ($game.setupType -eq "installer" -and (Test-Path $payload)) {
                Info "Running installer $($game.payloadPath) - follow its prompts."
                Start-Process -FilePath $payload -Wait
            }
        }
    }
    else {
        $file = Join-Path $dest $game.downloadName
        Download-Game $slug $file
        $ext = [IO.Path]::GetExtension($file).ToLower()
        if ($ext -eq ".zip") {
            Info "Extracting archive..."
            Expand-Archive -Path $file -DestinationPath $dest -Force
            Remove-Item $file -Force -ErrorAction SilentlyContinue
        }
        elseif ($ext -eq ".iso") {
            Install-FromIso $file $game
        }
        elseif ($ext -in ".exe", ".msi") {
            Info "Launching installer - follow its prompts."
            Start-Process -FilePath $file -Wait
        }
    }

    $exe = Find-Exe $dest $game.exeHint
    $state = Load-State
    $state[$slug] = @{ version = $game.version; path = $dest; exe = $exe;
        title = $game.title; setupType = $game.setupType }
    Save-State $state
    Report-Installed $state

    Ok "$($game.title) installed."
    if (-not $exe -and $game.setupType -in @("iso", "installer")) {
        Info "This game was installed by its own setup - launch it from your Start Menu."
    }
    if ($game.requiresHypervisor) { Show-Drm $game }
}

function Install-FromIso($iso, $game) {
    Info "Mounting disc image..."
    $mount = Mount-DiskImage -ImagePath $iso -PassThru
    try {
        $vol = ($mount | Get-Volume).DriveLetter
        $setup = Get-ChildItem "${vol}:\" -Filter *.exe -ErrorAction SilentlyContinue |
            Where-Object { $_.Name -match "setup|install|autorun" } | Select-Object -First 1
        if ($setup) {
            Info "Running $($setup.Name) from the disc - follow its prompts."
            Start-Process -FilePath $setup.FullName -Wait
        } else {
            Warn "No setup.exe found on the disc. Opening it so you can run it manually."
            Start-Process "${vol}:\"
            Read-Host "Press Enter once the game has finished installing"
        }
    } finally {
        Dismount-DiskImage -ImagePath $iso | Out-Null
    }
}

function Show-Drm($game) {
    Write-Host ""
    Warn "=============================================================="
    Warn " $($game.title) uses Denuvo / a hypervisor bypass."
    Warn " Before it will run you must disable Virtualization-Based"
    Warn " Security + Driver Signature Enforcement and reboot."
    Warn " Ludex will NOT change these security settings for you."
    Warn "=============================================================="
    if ($game.instructions) {
        Write-Host ""
        Write-Host $game.instructions -ForegroundColor Gray
    }
    Write-Host ""
}

function Do-Play($slug) {
    $state = Load-State
    if (-not $state.ContainsKey($slug)) {
        Warn "Not installed yet - installing first."
        Do-Install $slug
        $state = Load-State
    }
    $entry = $state[$slug]
    $exe = $entry.exe
    if (-not $exe -or -not (Test-Path $exe)) { $exe = Find-Exe $entry.path $null }
    if (-not $exe) {
        if ($entry.setupType -in @("iso", "installer")) {
            Warn "This game was installed by its own setup - launch it from your Start Menu."
            Warn "(Re-run Install to run its installer again.)"
            return
        }
        throw "Couldn't find an executable to launch under $($entry.path)."
    }

    # refresh DRM guidance at launch time
    try {
        $game = Api "/games/$slug"
        if ($game.requiresHypervisor) {
            Show-Drm $game
            $vbs = Get-ChildItem $entry.path -Recurse -Filter "VBS.cmd" -ErrorAction SilentlyContinue |
                Select-Object -First 1
            if ($vbs) { Warn "Tip: run '$($vbs.FullName)' as admin first (it reboots)." }
            $go = Read-Host "Launch anyway? (y/N)"
            if ($go -ne "y") { return }
        }
    } catch { }

    Info "Launching $($entry.title)..."
    $sw = [Diagnostics.Stopwatch]::StartNew()
    try {
        $proc = Start-Process -FilePath $exe -WorkingDirectory (Split-Path $exe) -PassThru
        $proc.WaitForExit()
    } catch {
        Fail "Could not launch: $_"; return
    }
    $sw.Stop()
    $secs = [int]$sw.Elapsed.TotalSeconds
    if ($secs -gt 5) {
        try { Api "/agent/session" "Post" @{ device = $Config.device; slug = $slug; seconds = $secs } | Out-Null }
        catch { }
        Ok ("Session recorded: {0:n1} min." -f ($secs / 60))
    }
}

function Do-Uninstall($slug) {
    $state = Load-State
    if ($state.ContainsKey($slug)) {
        $path = $state[$slug].path
        if ($path -and (Test-Path $path)) { Remove-Item $path -Recurse -Force -ErrorAction SilentlyContinue }
        $state.Remove($slug)
        Save-State $state
        Report-Installed $state
        Ok "Uninstalled."
    } else {
        Warn "That game isn't installed."
    }
}

function Do-List {
    $state = Load-State
    if ($state.Count -eq 0) { Info "No games installed."; return }
    foreach ($slug in $state.Keys) {
        "{0,-30} {1}" -f $state[$slug].title, $state[$slug].path | Write-Host
    }
}

function Do-Sync {
    Do-Hello
    Report-Installed (Load-State)
    Ok "Synced."
}

# --- dispatch ----------------------------------------------------------------
if ($Uri) {
    $rest = $Uri -replace "^ludex://", ""
    $rest = $rest.Trim("/")
    $parts = $rest -split "/", 2
    $Command = $parts[0]
    if ($parts.Count -gt 1) { $Slug = [uri]::UnescapeDataString($parts[1]) }
}

try {
    switch ($Command) {
        "hello"     { Do-Hello }
        "sync"      { Do-Sync }
        "list"      { Do-List }
        "install"   { Do-Install $Slug }
        "play"      { Do-Play $Slug }
        "uninstall" { Do-Uninstall $Slug }
        default     { Info "Usage: -Command <hello|sync|list|install|play|uninstall> [-Slug <slug>]" }
    }
} catch {
    Fail $_.Exception.Message
    if ($Uri) { Read-Host "Press Enter to close" }   # keep the window up when launched via protocol
    exit 1
}

# When launched from a browser click, pause briefly so the user sees the result.
if ($Uri -and $Command -in @("install", "uninstall")) { Start-Sleep -Seconds 2 }
