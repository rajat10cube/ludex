// Frontend talks to the Rust core via Tauri commands; the Rust core is what
// actually reaches the Ludex server (no CORS, credentials stay native).

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export const DEFAULT_SERVER = "http://192.168.0.188:8000";

export type SetupType =
  | "portable"
  | "portable_hypervisor"
  | "iso"
  | "installer"
  | "archive"
  | "rar";

export interface Game {
  slug: string;
  title: string;
  version: string | null;
  kind: string;
  setupType: SetupType;
  requiresHypervisor: boolean;
  releaseGroup: string | null;
  sizeBytes: number;
  fileCount: number;
  hasCover: boolean;
  genres: string | null;
  releaseYear: number | null;
  rating: number | null;
  missing: boolean;
  downloadKind: "tar" | "file";
  downloadName: string;
  installed: boolean;
  playtimeSeconds: number;
  lastPlayed: string | null;
}

export interface GameDetail extends Game {
  description: string | null;
  instructions: string | null;
  exeHint: string | null;
  payloadPath: string | null;
  installedVersions: string[];
  // A local post-install instruction (e.g. a staged crack to copy), set by the
  // desktop core after installing — absent for games installed elsewhere.
  installNote?: string | null;
  // The launch target Play uses; null for setup-installed games until the user
  // points Ludex at the real .exe.
  exePath?: string | null;
}

export interface Session {
  server: string;
  username: string;
  installDir: string;
  connected: boolean;
  isAdmin: boolean;
}

export interface LibraryFolder {
  id: number;
  path: string;
  name: string;
  gameCount: number;
  accessible: boolean;
}

export interface ScanStatus {
  state: "idle" | "scanning";
  games: number;
  artwork?: number;
  errors: string[];
  started_at: string | null;
  finished_at: string | null;
}

export interface BrowseResult {
  path: string;
  parent: string | null;
  dirs: { name: string; path: string }[];
}

export interface KeyState {
  set: boolean;
  source: "saved" | "env" | null;
  hint: string | null;
}

export interface ArtworkSettings {
  steamgriddb_key: KeyState;
  igdb_client_id: KeyState;
  igdb_client_secret: KeyState;
  steamgriddb: boolean;
  igdb: boolean;
  enabled: boolean;
}

export interface InstallStatus {
  slug: string;
  status: "installed" | "paused" | "cancelled";
  installPath: string | null;
  exe: string | null;
  note?: string | null;
}

export interface PausedDownload {
  slug: string;
  title: string;
  bytes: number;
  total: number;
}

export interface InstallProgress {
  slug: string;
  phase: "download" | "extract" | "install" | "done" | "error" | "paused" | "cancelled";
  received: number;
  total: number;
  message: string | null;
}

export interface PlayResult {
  slug: string;
  seconds: number;
  launched: boolean;
}

// --- session / connection ---
export const connect = (server: string, username: string, password: string) =>
  invoke<Session>("connect", { server, username, password });
export const getSession = () => invoke<Session | null>("get_session");
export const disconnect = () => invoke<void>("disconnect");
export const setInstallDir = (path: string) => invoke<Session>("set_install_dir", { path });
export const pickInstallDir = () => invoke<string | null>("pick_install_dir");
export const pickGameExe = () => invoke<string | null>("pick_game_exe");
export const setGameExe = (slug: string, path: string) =>
  invoke<void>("set_game_exe", { slug, path });

// --- library ---
export const listGames = () => invoke<{ games: Game[] }>("list_games");
export const gameDetail = (slug: string) => invoke<GameDetail>("game_detail", { slug });
export const coverDataUrl = (slug: string) => invoke<string | null>("cover_data_url", { slug });

// --- server library management (admin) ---
export const listLibraries = () => invoke<LibraryFolder[]>("list_libraries");
export const addLibrary = (path: string, name?: string) =>
  invoke<LibraryFolder>("add_library", { path, name: name || null });
export const removeLibrary = (id: number) => invoke<void>("remove_library", { id });
export const scanLibraries = () => invoke<ScanStatus>("scan_libraries");
export const scanStatus = () => invoke<ScanStatus>("scan_status");
export const browseServer = (path: string) => invoke<BrowseResult>("browse_server", { path });
export const artworkSettings = () => invoke<ArtworkSettings>("artwork_settings");
export const saveArtworkSettings = (body: {
  steamgriddb_key?: string;
  igdb_client_id?: string;
  igdb_client_secret?: string;
  clear?: string[];
}) => invoke<ArtworkSettings>("save_artwork_settings", { body });
export const refreshArtwork = () => invoke<unknown>("refresh_artwork");

// --- actions ---
export const install = (slug: string) => invoke<InstallStatus>("install", { slug });
export const resumeInstall = (slug: string) => invoke<InstallStatus>("resume_install", { slug });
export const pauseInstall = (slug: string) => invoke<void>("pause_install", { slug });
export const cancelInstall = (slug: string) => invoke<void>("cancel_install", { slug });
export const pausedDownloads = () => invoke<PausedDownload[]>("paused_downloads");
export const play = (slug: string) => invoke<PlayResult>("play", { slug });
export const uninstall = (slug: string) => invoke<void>("uninstall", { slug });
export const openInstallDir = (slug: string) => invoke<void>("open_install_dir", { slug });
export const openCrackDir = (slug: string) => invoke<void>("open_crack_dir", { slug });

export function onInstallProgress(cb: (p: InstallProgress) => void): Promise<UnlistenFn> {
  return listen<InstallProgress>("install-progress", (e) => cb(e.payload));
}
