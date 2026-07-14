import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  ChevronRight,
  FolderCog,
  FolderPlus,
  HardDrive,
  Image as ImageIcon,
  Laptop,
  Loader2,
  LogOut,
  RefreshCw,
  Trash2,
  X,
} from "lucide-react";

import {
  addLibrary,
  artworkSettings,
  browseServer,
  disconnect,
  listLibraries,
  pickInstallDir,
  refreshArtwork,
  removeLibrary,
  saveArtworkSettings,
  scanLibraries,
  scanStatus,
  setInstallDir,
  type BrowseResult,
  type KeyState,
  type Session,
} from "@/api";
import { cn } from "@/lib/utils";

/** Tauri commands reject with a plain string, not an Error. */
function errMsg(e: unknown, fallback = "Something went wrong"): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return fallback;
}

export function Settings({
  session,
  onClose,
  onDisconnect,
}: {
  session: Session;
  onClose: () => void;
  onDisconnect: () => void;
}) {
  return (
    <div className="fixed inset-0 z-40 overflow-y-auto bg-ink-950/90 p-4 backdrop-blur-sm sm:p-8">
      <div className="mx-auto max-w-3xl space-y-6">
        <div className="flex items-center gap-3">
          <h1 className="text-xl font-semibold text-white">Settings</h1>
          <button className="btn-ghost ml-auto h-9 px-2.5" onClick={onClose} title="Close">
            <X className="h-5 w-5" />
          </button>
        </div>

        {session.isAdmin ? (
          <>
            <LibrariesSection />
            <ArtworkSection />
          </>
        ) : (
          <section className="card p-5 text-sm text-slate-400">
            Managing libraries and artwork needs an admin account on the server.
          </section>
        )}

        <DeviceSection session={session} onDisconnect={onDisconnect} />
      </div>
    </div>
  );
}

function Section({
  title,
  icon,
  children,
  action,
}: {
  title: string;
  icon: React.ReactNode;
  children: React.ReactNode;
  action?: React.ReactNode;
}) {
  return (
    <section className="card p-5">
      <div className="mb-4 flex items-center gap-2">
        <span className="text-accent">{icon}</span>
        <h2 className="font-semibold text-white">{title}</h2>
        <div className="ml-auto">{action}</div>
      </div>
      {children}
    </section>
  );
}

function LibrariesSection() {
  const qc = useQueryClient();
  const [picking, setPicking] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const { data: libs, isLoading } = useQuery({ queryKey: ["libraries"], queryFn: listLibraries });
  const scan = useQuery({
    queryKey: ["scan-status"],
    queryFn: scanStatus,
    refetchInterval: (q) => (q.state.data?.state === "scanning" ? 1500 : false),
  });

  const scanning = scan.data?.state === "scanning";

  // After a scan the library list on the server has changed — pull it fresh.
  const refreshAfterScan = () => {
    qc.invalidateQueries({ queryKey: ["scan-status"] });
    setTimeout(() => {
      qc.invalidateQueries({ queryKey: ["libraries"] });
      qc.invalidateQueries({ queryKey: ["games"] });
    }, 2500);
  };

  const rescan = useMutation({
    mutationFn: scanLibraries,
    onSuccess: refreshAfterScan,
    onError: (e) => setError(errMsg(e, "Scan failed")),
  });

  const del = useMutation({
    mutationFn: (id: number) => removeLibrary(id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["libraries"] });
      qc.invalidateQueries({ queryKey: ["games"] });
    },
    onError: (e) => setError(errMsg(e, "Couldn't remove that library")),
  });

  return (
    <Section
      title="Game libraries"
      icon={<HardDrive className="h-5 w-5" />}
      action={
        <div className="flex gap-2">
          <button className="btn-ghost h-9" onClick={() => rescan.mutate()} disabled={scanning}>
            <RefreshCw className={cn("h-4 w-4", scanning && "animate-spin")} />
            {scanning ? "Scanning…" : "Scan now"}
          </button>
          <button className="btn-primary h-9" onClick={() => setPicking(true)}>
            <FolderPlus className="h-4 w-4" /> Add
          </button>
        </div>
      }
    >
      <p className="mb-3 text-sm text-slate-400">
        Folders on the <span className="text-slate-200">server</span> that hold your games. Scanning
        re-reads them and picks up anything new.
      </p>

      {isLoading ? (
        <Loader2 className="h-5 w-5 animate-spin text-slate-500" />
      ) : libs && libs.length > 0 ? (
        <ul className="divide-y divide-ink-700/70">
          {libs.map((lib) => (
            <li key={lib.id} className="flex items-center gap-3 py-3">
              <div className="min-w-0 flex-1">
                <p className="truncate font-medium text-slate-100">{lib.name}</p>
                <p className="truncate text-xs text-slate-500">{lib.path}</p>
              </div>
              <span className="chip">{lib.gameCount} games</span>
              {!lib.accessible && (
                <span className="chip border-red-500/40 bg-red-500/10 text-red-300">offline</span>
              )}
              <button
                className="btn-danger h-8 px-2"
                onClick={() => del.mutate(lib.id)}
                disabled={del.isPending}
                title="Remove from Ludex (your files aren't touched)"
              >
                <Trash2 className="h-4 w-4" />
              </button>
            </li>
          ))}
        </ul>
      ) : (
        <p className="text-sm text-slate-500">
          No libraries yet. Add a folder on the server that contains your games.
        </p>
      )}

      {scanning && (
        <p className="mt-3 text-xs text-slate-400">
          Scanning… found {scan.data?.games ?? 0} games so far.
        </p>
      )}
      {!scanning && scan.data?.finished_at && (
        <p className="mt-3 text-xs text-slate-500">
          Last scan: {scan.data.games} games
          {scan.data.artwork ? `, ${scan.data.artwork} artwork updates` : ""}.
        </p>
      )}

      {scan.data?.errors && scan.data.errors.length > 0 && (
        <div className="mt-3 rounded-lg border border-red-500/30 bg-red-500/10 p-3 text-xs text-red-300">
          {scan.data.errors.map((e, i) => (
            <div key={i}>{e}</div>
          ))}
        </div>
      )}
      {error && <p className="mt-2 text-sm text-red-400">{error}</p>}

      {picking && (
        <ServerFolderPicker
          onClose={() => setPicking(false)}
          onPick={async (path) => {
            await addLibrary(path);
            setPicking(false);
            setError(null);
            qc.invalidateQueries({ queryKey: ["libraries"] });
            refreshAfterScan();
          }}
        />
      )}
    </Section>
  );
}

/** Browses the *server's* filesystem — inside the LXC that's /mnt/games, not D:\. */
function ServerFolderPicker({
  onClose,
  onPick,
}: {
  onClose: () => void;
  onPick: (path: string) => Promise<void>;
}) {
  const [path, setPath] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const { data, isLoading } = useQuery<BrowseResult>({
    queryKey: ["browse", path],
    queryFn: () => browseServer(path),
  });

  const confirm = async () => {
    if (!data?.path) return;
    setBusy(true);
    setError(null);
    try {
      await onPick(data.path);
    } catch (e) {
      setError(errMsg(e, "Failed to add"));
      setBusy(false);
    }
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-ink-950/80 p-4 backdrop-blur-sm"
      onClick={onClose}
    >
      <div className="card w-full max-w-lg p-4" onClick={(e) => e.stopPropagation()}>
        <p className="mb-1 text-sm font-medium text-white">Choose a folder on the server</p>
        <p className="mb-2 text-xs text-slate-500">
          These are the server's paths (e.g. /mnt/games), not your PC's.
        </p>
        <div className="mb-2 truncate rounded-lg bg-ink-900 px-3 py-2 text-xs text-slate-400">
          {data?.path || "Select a drive"}
        </div>
        <div className="max-h-72 overflow-y-auto rounded-lg border border-ink-700">
          {isLoading ? (
            <div className="flex justify-center py-6">
              <Loader2 className="h-5 w-5 animate-spin text-slate-500" />
            </div>
          ) : (
            <>
              {data?.parent != null && (
                <button
                  className="flex w-full items-center gap-2 px-3 py-2 text-left text-sm text-slate-400 hover:bg-ink-800"
                  onClick={() => setPath(data.parent!)}
                >
                  ‹ Up
                </button>
              )}
              {data?.dirs.map((d) => (
                <button
                  key={d.path}
                  className="flex w-full items-center gap-2 px-3 py-2 text-left text-sm text-slate-200 hover:bg-ink-800"
                  onClick={() => setPath(d.path)}
                >
                  <ChevronRight className="h-4 w-4 text-slate-500" />
                  <span className="truncate">{d.name}</span>
                </button>
              ))}
              {data && data.dirs.length === 0 && data.parent != null && (
                <p className="px-3 py-6 text-center text-xs text-slate-500">No subfolders here.</p>
              )}
            </>
          )}
        </div>
        {error && <p className="mt-2 text-sm text-red-400">{error}</p>}
        <div className="mt-3 flex justify-end gap-2">
          <button className="btn-ghost" onClick={onClose}>
            Cancel
          </button>
          <button className="btn-primary" onClick={confirm} disabled={busy || !data?.path}>
            {busy && <Loader2 className="h-4 w-4 animate-spin" />}
            Add this folder
          </button>
        </div>
      </div>
    </div>
  );
}

function KeyField({
  label,
  help,
  state,
  value,
  onChange,
}: {
  label: string;
  help: React.ReactNode;
  state?: KeyState;
  value: string;
  onChange: (v: string) => void;
}) {
  const placeholder = state?.set
    ? `${state.hint ?? "set"} — saved (${state.source}), leave blank to keep`
    : "not set — paste key";
  return (
    <div>
      <div className="mb-1 flex items-center gap-2">
        <label className="text-xs font-medium text-slate-300">{label}</label>
        {state?.set && (
          <span className="chip border-play/40 bg-play/15 px-2 py-0.5 text-[10px] text-play">
            {state.source === "env" ? "from env" : "saved"}
          </span>
        )}
      </div>
      <input
        className="input font-mono text-xs"
        type="password"
        autoComplete="off"
        placeholder={placeholder}
        value={value}
        onChange={(e) => onChange(e.target.value)}
      />
      <p className="mt-1 text-[11px] text-slate-500">{help}</p>
    </div>
  );
}

function ArtworkSection() {
  const qc = useQueryClient();
  const { data: settings } = useQuery({ queryKey: ["artwork-settings"], queryFn: artworkSettings });
  const [msg, setMsg] = useState<string | null>(null);
  const [sgdb, setSgdb] = useState("");
  const [igdbId, setIgdbId] = useState("");
  const [igdbSecret, setIgdbSecret] = useState("");

  const save = useMutation({
    mutationFn: () =>
      saveArtworkSettings({
        ...(sgdb.trim() ? { steamgriddb_key: sgdb.trim() } : {}),
        ...(igdbId.trim() ? { igdb_client_id: igdbId.trim() } : {}),
        ...(igdbSecret.trim() ? { igdb_client_secret: igdbSecret.trim() } : {}),
      }),
    onSuccess: () => {
      setMsg('Keys saved. Click "Refresh artwork" to fetch covers now.');
      setSgdb("");
      setIgdbId("");
      setIgdbSecret("");
      qc.invalidateQueries({ queryKey: ["artwork-settings"] });
    },
    onError: (e) => setMsg(errMsg(e, "Failed to save keys")),
  });

  const refresh = useMutation({
    mutationFn: refreshArtwork,
    onSuccess: () => {
      setMsg("Fetching covers + metadata in the background. Give it a minute, then refresh.");
      setTimeout(() => qc.invalidateQueries({ queryKey: ["games"] }), 8000);
    },
    onError: (e) => setMsg(errMsg(e, "Failed to refresh artwork")),
  });

  const enabled = settings?.enabled ?? false;
  const nothingTyped = !sgdb.trim() && !igdbId.trim() && !igdbSecret.trim();

  return (
    <Section
      title="Artwork & metadata"
      icon={<ImageIcon className="h-5 w-5" />}
      action={
        <button
          className="btn-ghost h-9"
          onClick={() => refresh.mutate()}
          disabled={!enabled || refresh.isPending}
          title={enabled ? "Fetch covers + metadata now" : "Add a key first"}
        >
          <RefreshCw className={cn("h-4 w-4", refresh.isPending && "animate-spin")} />
          Refresh artwork
        </button>
      }
    >
      <p className="text-sm text-slate-400">
        Cover art comes from SteamGridDB and descriptions/genres from IGDB. Keys are stored on the
        server and take effect immediately.
      </p>

      <div className="mt-4 space-y-3">
        <KeyField
          label="SteamGridDB API key"
          state={settings?.steamgriddb_key}
          value={sgdb}
          onChange={setSgdb}
          help="Free from steamgriddb.com → preferences → API. Gives the portrait covers."
        />
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
          <KeyField
            label="IGDB Client ID"
            state={settings?.igdb_client_id}
            value={igdbId}
            onChange={setIgdbId}
            help="Twitch app at dev.twitch.tv/console/apps."
          />
          <KeyField
            label="IGDB Client Secret"
            state={settings?.igdb_client_secret}
            value={igdbSecret}
            onChange={setIgdbSecret}
            help="From the same Twitch app."
          />
        </div>
      </div>

      <div className="mt-4 flex flex-wrap items-center gap-2">
        <button
          className="btn-primary"
          onClick={() => save.mutate()}
          disabled={nothingTyped || save.isPending}
        >
          {save.isPending && <Loader2 className="h-4 w-4 animate-spin" />}
          Save keys
        </button>
        <span className={cn("chip", settings?.steamgriddb && "border-play/40 bg-play/15 text-play")}>
          SteamGridDB {settings?.steamgriddb ? "connected" : "off"}
        </span>
        <span className={cn("chip", settings?.igdb && "border-play/40 bg-play/15 text-play")}>
          IGDB {settings?.igdb ? "connected" : "off"}
        </span>
      </div>

      {msg && <p className="mt-2 text-xs text-slate-400">{msg}</p>}
    </Section>
  );
}

function DeviceSection({
  session,
  onDisconnect,
}: {
  session: Session;
  onDisconnect: () => void;
}) {
  const qc = useQueryClient();
  const [dir, setDir] = useState(session.installDir);

  const changeDir = async () => {
    const picked = await pickInstallDir();
    if (!picked) return;
    await setInstallDir(picked);
    setDir(picked);
    qc.invalidateQueries({ queryKey: ["session"] });
  };

  return (
    <Section title="This PC" icon={<Laptop className="h-5 w-5" />}>
      <dl className="space-y-3 text-sm">
        <div className="flex items-center gap-3">
          <div className="min-w-0 flex-1">
            <dt className="text-xs text-slate-500">Games are installed to</dt>
            <dd className="truncate text-slate-200">{dir}</dd>
          </div>
          <button className="btn-ghost h-9 shrink-0" onClick={changeDir}>
            <FolderCog className="h-4 w-4" /> Change
          </button>
        </div>
        <div className="flex items-center gap-3">
          <div className="min-w-0 flex-1">
            <dt className="text-xs text-slate-500">Signed in as</dt>
            <dd className="truncate text-slate-200">
              {session.username}
              {session.isAdmin && <span className="ml-2 chip">admin</span>}
              <span className="ml-2 text-xs text-slate-500">{session.server}</span>
            </dd>
          </div>
          <button
            className="btn-danger h-9 shrink-0"
            onClick={async () => {
              await disconnect();
              onDisconnect();
            }}
          >
            <LogOut className="h-4 w-4" /> Sign out
          </button>
        </div>
      </dl>
    </Section>
  );
}
