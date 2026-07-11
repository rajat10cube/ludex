// Typed API client for the Ludex backend.

const BASE = "/api";

export type SetupType =
  | "portable"
  | "portable_hypervisor"
  | "iso"
  | "installer"
  | "archive";

export interface GameCard {
  slug: string;
  title: string;
  version: string | null;
  kind: "folder" | "installer" | "archive";
  setupType: SetupType;
  requiresHypervisor: boolean;
  releaseGroup: string | null;
  sizeBytes: number;
  fileCount: number;
  hasCover: boolean;
  missing: boolean;
  libraryId: number | null;
  downloadKind: "tar" | "file";
  downloadName: string;
  installed: boolean;
  playtimeSeconds: number;
  lastPlayed: string | null;
}

export interface GameDetail extends GameCard {
  description: string | null;
  instructions: string | null;
  exeHint: string | null;
  payloadPath: string | null;
  installedVersions: string[];
}

export interface Me {
  username: string;
  isAdmin: boolean;
  authDisabled: boolean;
}

export interface AuthStatus {
  authDisabled: boolean;
  needsSetup: boolean;
  user: { username: string; isAdmin: boolean } | null;
}

export interface LibraryItem {
  id: number;
  path: string;
  name: string | null;
  gameCount: number;
  accessible: boolean;
}

export interface BrowseResult {
  path: string;
  parent: string | null;
  dirs: { name: string; path: string }[];
}

export interface ScanStatus {
  state: "idle" | "scanning";
  started_at: string | null;
  finished_at: string | null;
  games: number;
  errors: string[];
}

let onUnauthorized: (() => void) | null = null;
export function setUnauthorizedHandler(fn: (() => void) | null): void {
  onUnauthorized = fn;
}

async function req<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(`${BASE}${path}`, { credentials: "include", ...init });
  if (res.status === 401) {
    onUnauthorized?.();
    throw new Error("unauthorized");
  }
  if (!res.ok) {
    const d = await res.json().catch(() => ({}));
    throw new Error(d?.detail || `${path} -> ${res.status}`);
  }
  if (res.status === 204) return undefined as T;
  return res.json() as Promise<T>;
}

function post<T>(path: string, body?: unknown): Promise<T> {
  return req<T>(path, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
}

// --- auth ---
export const getStatus = () => req<AuthStatus>("/auth/status");
export const getMe = async (): Promise<Me | null> => {
  const res = await fetch(`${BASE}/auth/me`, { credentials: "include" });
  if (res.status === 401) return null;
  if (!res.ok) throw new Error(`me -> ${res.status}`);
  return res.json();
};
export const setupAdmin = (username: string, password: string) =>
  post<Me>("/auth/setup", { username, password });
export const login = (username: string, password: string) =>
  post<Me>("/auth/login", { username, password });
export const logout = () => post<{ ok: boolean }>("/auth/logout");
export const changeMyPassword = (current_password: string, new_password: string) =>
  post<{ ok: boolean }>("/auth/password", { current_password, new_password });

// --- games ---
export interface GameQuery {
  search?: string;
  library?: number | null;
  sort?: "title" | "size" | "recent";
}
export function getGames(q: GameQuery = {}): Promise<{ games: GameCard[] }> {
  const params = new URLSearchParams();
  if (q.search) params.set("search", q.search);
  if (q.library != null) params.set("library", String(q.library));
  if (q.sort) params.set("sort", q.sort);
  const qs = params.toString();
  return req<{ games: GameCard[] }>(`/games${qs ? `?${qs}` : ""}`);
}
export const getGame = (slug: string) => req<GameDetail>(`/games/${encodeURIComponent(slug)}`);
export const coverUrl = (slug: string) => `${BASE}/games/${encodeURIComponent(slug)}/cover`;
export const downloadUrl = (slug: string) => `${BASE}/download/${encodeURIComponent(slug)}`;

// --- libraries (admin) ---
export const getLibraries = () => req<LibraryItem[]>("/libraries");
export const browse = (path: string) =>
  req<BrowseResult>(`/libraries/browse?path=${encodeURIComponent(path)}`);
export const addLibrary = (path: string, name?: string) =>
  post<LibraryItem>("/libraries", { path, name });
export const deleteLibrary = (id: number) =>
  req<void>(`/libraries/${id}`, { method: "DELETE" });
export const triggerScan = () => post<{ status: string }>("/libraries/scan");
export const getScanStatus = () => req<ScanStatus>("/libraries/scan/status");
