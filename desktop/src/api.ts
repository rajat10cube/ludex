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
  | "archive";

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
}

export interface Session {
  server: string;
  username: string;
  installDir: string;
  connected: boolean;
}

export interface InstallStatus {
  slug: string;
  status: "installed" | "paused" | "cancelled";
  installPath: string | null;
  exe: string | null;
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

// --- library ---
export const listGames = () => invoke<{ games: Game[] }>("list_games");
export const gameDetail = (slug: string) => invoke<GameDetail>("game_detail", { slug });
export const coverDataUrl = (slug: string) => invoke<string | null>("cover_data_url", { slug });

// --- actions ---
export const install = (slug: string) => invoke<InstallStatus>("install", { slug });
export const resumeInstall = (slug: string) => invoke<InstallStatus>("resume_install", { slug });
export const pauseInstall = (slug: string) => invoke<void>("pause_install", { slug });
export const cancelInstall = (slug: string) => invoke<void>("cancel_install", { slug });
export const pausedDownloads = () => invoke<PausedDownload[]>("paused_downloads");
export const play = (slug: string) => invoke<PlayResult>("play", { slug });
export const uninstall = (slug: string) => invoke<void>("uninstall", { slug });
export const openInstallDir = (slug: string) => invoke<void>("open_install_dir", { slug });

export function onInstallProgress(cb: (p: InstallProgress) => void): Promise<UnlistenFn> {
  return listen<InstallProgress>("install-progress", (e) => cb(e.payload));
}
