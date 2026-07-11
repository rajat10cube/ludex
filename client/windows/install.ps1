<#
  Ludex Windows companion agent - installer / updater.

  Run in an *admin-not-required* PowerShell window (per-user install):

      irm https://your-server/api/client/install.ps1 | iex

  It downloads the agent, registers the ludex:// protocol handler, stores your
  Ludex login (encrypted with Windows DPAPI for your account), and picks a folder
  where games get installed. Re-run any time to update the agent in place.
#>
[CmdletBinding()]
param(
    [string]$Server = "@@SERVER_URL@@",
    [string]$Username,
    [string]$Password,
    [string]$GamesDir
)

$ErrorActionPreference = "Stop"
$Home_ = Join-Path $env:LOCALAPPDATA "Ludex"
$AgentPath = Join-Path $Home_ "ludex-agent.ps1"
$ConfigPath = Join-Path $Home_ "config.json"

function Info($m) { Write-Host "[ludex] $m" -ForegroundColor Cyan }
function Ok($m)   { Write-Host "[ludex] $m" -ForegroundColor Green }
function Warn($m) { Write-Host "[ludex] $m" -ForegroundColor Yellow }

if ($Server -like "*@@SERVER_URL@@*" -or [string]::IsNullOrWhiteSpace($Server)) {
    $Server = Read-Host "Ludex server URL (e.g. http://192.168.1.10:8810)"
}
$Server = $Server.TrimEnd("/")

# --- collect credentials -----------------------------------------------------
if (-not $Username) { $Username = Read-Host "Ludex username" }
if (-not $Password) {
    $sec = Read-Host "Ludex password" -AsSecureString
    $Password = [Runtime.InteropServices.Marshal]::PtrToStringAuto(
        [Runtime.InteropServices.Marshal]::SecureStringToBSTR($sec))
}

# verify the login before we commit anything
$pair = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes("${Username}:${Password}"))
$auth = @{ Authorization = "Basic $pair" }
try {
    $me = Invoke-RestMethod -Uri "$Server/api/auth/me" -Headers $auth -TimeoutSec 20
    Ok "Signed in as $($me.username)."
} catch {
    throw "Could not sign in to $Server - check the URL and credentials. ($_)"
}

# --- choose a games install directory ---------------------------------------
if (-not $GamesDir) {
    $default = Join-Path $env:USERPROFILE "Games\Ludex"
    $entered = Read-Host "Install games into [$default]"
    $GamesDir = if ([string]::IsNullOrWhiteSpace($entered)) { $default } else { $entered }
}
New-Item -ItemType Directory -Force -Path $Home_, $GamesDir | Out-Null

# --- download the agent ------------------------------------------------------
Info "Downloading agent..."
Invoke-WebRequest -Uri "$Server/api/client/ludex-agent.ps1" -OutFile $AgentPath -Headers $auth

# --- store config (password protected with DPAPI, current user scope) --------
$secured = ConvertTo-SecureString $Password -AsPlainText -Force |
    ConvertFrom-SecureString
$config = [ordered]@{
    server   = $Server
    username = $Username
    password = $secured        # DPAPI blob - only decryptable by this Windows user
    gamesDir = $GamesDir
    device   = $env:COMPUTERNAME
}
$config | ConvertTo-Json | Set-Content -Path $ConfigPath -Encoding UTF8

# --- register the ludex:// protocol -----------------------------------------
Info "Registering ludex:// handler..."
$key = "HKCU:\Software\Classes\ludex"
New-Item -Path $key -Force | Out-Null
Set-ItemProperty -Path $key -Name "(default)" -Value "URL:Ludex Protocol"
Set-ItemProperty -Path $key -Name "URL Protocol" -Value ""
$cmdKey = Join-Path $key "shell\open\command"
New-Item -Path $cmdKey -Force | Out-Null
$launch = "powershell.exe -NoProfile -ExecutionPolicy Bypass -File `"$AgentPath`" -Uri `"%1`""
Set-ItemProperty -Path $cmdKey -Name "(default)" -Value $launch

# --- say hello + initial sync -----------------------------------------------
try {
    & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $AgentPath -Command hello | Out-Null
    Ok "Agent registered with the server."
} catch { Warn "Registered locally, but the initial hello failed: $_" }

Ok "Done! Games install into: $GamesDir"
Info "Head back to $Server and click Install / Play on a game."
