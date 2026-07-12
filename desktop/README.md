# Ludex Desktop

A native Windows client for [Ludex](../README.md) — a real desktop window (no
browser, no PowerShell console) that connects straight to your Ludex server,
browses the library, and **installs / plays** games with proper progress bars.

Built with **Tauri 2** (Rust core + the same React UI as the web app). The Rust
core does all the work the old PowerShell agent did — streaming downloads,
extraction (`tar.exe`), launching games, and reporting playtime — but silently
and natively.

## What it does
- **Connect** to your server (default `http://192.168.0.188:8000`) and sign in;
  your password is stored in **Windows Credential Manager** (never on disk).
- **Library** grid with covers, search, and Installed/DRM filters.
- **Install** — streams the game from the server (folder games as a `.tar`,
  loose files direct), extracts, finds the exe, and reports it back. Downloads
  **and extraction** both show a real percentage.
- **Background & concurrent installs** — installs run in the background, so you can
  close the game panel, keep browsing, and queue several at once. A **Downloads**
  panel in the header shows every in-flight install with progress, and each game
  tile shows its own progress overlay while installing.
- **Play** — launches the game and records playtime. Hypervisor/Denuvo games are
  launched **as administrator** (still your job to run `VBS.cmd` + reboot first).
- **Uninstall**, open install folder, change install directory, sign out.

## Develop
```bash
npm install
npm run tauri dev      # hot-reload app window
```

## Build the installer
```bash
npm run tauri build    # -> src-tauri/target/release/bundle/msi/Ludex_x.y.z_x64_en-US.msi
```

### Prerequisites (Windows)
- **Rust** (MSVC toolchain): https://rustup.rs
- **Visual Studio Build Tools** with *Desktop development with C++* (MSVC + Windows SDK)
- **WebView2 runtime** (preinstalled on Windows 11)
- **Node 18+**

The produced `.msi` is unsigned, so SmartScreen shows a "More info → Run anyway"
prompt on first launch — expected for self-hosted tools. Code-sign it if you want
to avoid that.

## How it talks to the server
All server calls go through the Rust core (HTTP Basic auth), so there are no CORS
or cookie concerns — the UI never touches the network directly. Endpoints used:
`/api/auth/me`, `/api/games`, `/api/games/{slug}`, `/api/games/{slug}/cover`,
`/api/download/{slug}`, and the `/api/agent/*` reporting endpoints.
