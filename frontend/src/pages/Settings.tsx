import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  Check,
  ChevronRight,
  Copy,
  FolderPlus,
  HardDrive,
  Image as ImageIcon,
  Loader2,
  RefreshCw,
  Terminal,
  Trash2,
} from "lucide-react";

import {
  addLibrary,
  browse,
  changeMyPassword,
  deleteLibrary,
  getArtworkSettings,
  getLibraries,
  getScanStatus,
  refreshArtwork,
  saveArtworkKeys,
  triggerScan,
  type ArtworkFieldState,
  type ArtworkKeysIn,
  type BrowseResult,
} from "@/api";
import { useAuth } from "@/auth";
import { cn } from "@/lib/utils";

export function Settings() {
  return (
    <div className="mx-auto max-w-3xl space-y-8">
      <h1 className="text-xl font-semibold text-white">Settings</h1>
      <LibrariesSection />
      <ArtworkSection />
      <AgentSection />
      <AccountSection />
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
  state?: ArtworkFieldState;
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
  const { data: settings } = useQuery({ queryKey: ["artwork-settings"], queryFn: getArtworkSettings });
  const qc = useQueryClient();
  const [msg, setMsg] = useState<string | null>(null);
  const [sgdb, setSgdb] = useState("");
  const [igdbId, setIgdbId] = useState("");
  const [igdbSecret, setIgdbSecret] = useState("");

  const save = useMutation({
    mutationFn: () => {
      const body: ArtworkKeysIn = {};
      if (sgdb.trim()) body.steamgriddb_key = sgdb.trim();
      if (igdbId.trim()) body.igdb_client_id = igdbId.trim();
      if (igdbSecret.trim()) body.igdb_client_secret = igdbSecret.trim();
      return saveArtworkKeys(body);
    },
    onSuccess: () => {
      setMsg("Keys saved. Click “Refresh artwork” to fetch covers now.");
      setSgdb("");
      setIgdbId("");
      setIgdbSecret("");
      qc.invalidateQueries({ queryKey: ["artwork-settings"] });
    },
    onError: (e) => setMsg(e instanceof Error ? e.message : "Failed"),
  });

  const refresh = useMutation({
    mutationFn: refreshArtwork,
    onSuccess: () => {
      setMsg("Fetching covers + metadata in the background. Refresh the library in a minute.");
      setTimeout(() => qc.invalidateQueries({ queryKey: ["games"] }), 8000);
    },
    onError: (e) => setMsg(e instanceof Error ? e.message : "Failed"),
  });

  const enabled = settings?.enabled ?? false;
  const nothingTyped = !sgdb.trim() && !igdbId.trim() && !igdbSecret.trim();

  return (
    <Section
      title="Game artwork & metadata"
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
        Paste your keys here to fetch cover art (SteamGridDB) and descriptions/genres (IGDB) — like
        a TVDB key in Jellyfin. Saved keys take effect immediately, no restart.
      </p>

      <div className="mt-4 space-y-3">
        <KeyField
          label="SteamGridDB API key"
          state={settings?.steamgriddb_key}
          value={sgdb}
          onChange={setSgdb}
          help={
            <>
              Free from{" "}
              <a
                className="text-accent hover:underline"
                href="https://www.steamgriddb.com/profile/preferences/api"
                target="_blank"
                rel="noreferrer"
              >
                steamgriddb.com → preferences → API
              </a>{" "}
              — gives the portrait cover art.
            </>
          }
        />
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
          <KeyField
            label="IGDB Client ID"
            state={settings?.igdb_client_id}
            value={igdbId}
            onChange={setIgdbId}
            help={
              <>
                Twitch app at{" "}
                <a
                  className="text-accent hover:underline"
                  href="https://dev.twitch.tv/console/apps"
                  target="_blank"
                  rel="noreferrer"
                >
                  dev.twitch.tv
                </a>{" "}
                — gives descriptions/genres.
              </>
            }
          />
          <KeyField
            label="IGDB Client Secret"
            state={settings?.igdb_client_secret}
            value={igdbSecret}
            onChange={setIgdbSecret}
            help="From the same Twitch app (Client Secret)."
          />
        </div>
      </div>

      <div className="mt-4 flex flex-wrap items-center gap-2">
        <button className="btn-primary" onClick={() => save.mutate()} disabled={nothingTyped || save.isPending}>
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
      <p className="mt-2 text-[11px] text-slate-500">
        Keys are stored in the server's data dir. Env vars ({" "}
        <code>LUDEX_STEAMGRIDDB_KEY</code> etc.) still work as a fallback.
      </p>
    </Section>
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
  const { data: libs, isLoading } = useQuery({ queryKey: ["libraries"], queryFn: getLibraries });
  const scan = useQuery({
    queryKey: ["scan-status"],
    queryFn: getScanStatus,
    refetchInterval: (q) => (q.state.data?.state === "scanning" ? 1500 : false),
  });

  const del = useMutation({
    mutationFn: (id: number) => deleteLibrary(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["libraries"] }),
  });
  const rescan = useMutation({
    mutationFn: triggerScan,
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["scan-status"] });
      setTimeout(() => qc.invalidateQueries({ queryKey: ["games"] }), 2500);
    },
  });

  const scanning = scan.data?.state === "scanning";

  return (
    <Section
      title="Libraries"
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
                title="Remove library"
              >
                <Trash2 className="h-4 w-4" />
              </button>
            </li>
          ))}
        </ul>
      ) : (
        <p className="text-sm text-slate-500">
          No libraries yet. Add a folder that contains your games.
        </p>
      )}

      {scan.data?.errors && scan.data.errors.length > 0 && (
        <div className="mt-3 rounded-lg border border-red-500/30 bg-red-500/10 p-3 text-xs text-red-300">
          {scan.data.errors.map((e, i) => (
            <div key={i}>{e}</div>
          ))}
        </div>
      )}

      {picking && (
        <FolderPicker
          onClose={() => setPicking(false)}
          onPick={async (path) => {
            await addLibrary(path);
            setPicking(false);
            qc.invalidateQueries({ queryKey: ["libraries"] });
            setTimeout(() => {
              qc.invalidateQueries({ queryKey: ["scan-status"] });
              qc.invalidateQueries({ queryKey: ["games"] });
            }, 2500);
          }}
        />
      )}
    </Section>
  );
}

function FolderPicker({
  onClose,
  onPick,
}: {
  onClose: () => void;
  onPick: (path: string) => Promise<void>;
}) {
  const [path, setPath] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const { data } = useQuery<BrowseResult>({
    queryKey: ["browse", path],
    queryFn: () => browse(path),
  });

  const confirm = async () => {
    if (!data?.path) return;
    setBusy(true);
    setError(null);
    try {
      await onPick(data.path);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to add");
      setBusy(false);
    }
  };

  return (
    <div
      className="fixed inset-0 z-40 flex items-center justify-center bg-ink-950/80 p-4 backdrop-blur-sm"
      onClick={onClose}
    >
      <div className="card w-full max-w-lg p-4" onClick={(e) => e.stopPropagation()}>
        <p className="mb-2 text-sm font-medium text-white">Choose a game folder</p>
        <div className="mb-2 truncate rounded-lg bg-ink-900 px-3 py-2 text-xs text-slate-400">
          {data?.path || "Select a drive"}
        </div>
        <div className="max-h-72 overflow-y-auto rounded-lg border border-ink-700">
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
        </div>
        {error && <p className="mt-2 text-sm text-red-400">{error}</p>}
        <div className="mt-3 flex justify-end gap-2">
          <button className="btn-ghost" onClick={onClose}>
            Cancel
          </button>
          <button
            className="btn-primary"
            onClick={confirm}
            disabled={busy || !data?.path}
          >
            {busy && <Loader2 className="h-4 w-4 animate-spin" />}
            Add this folder
          </button>
        </div>
      </div>
    </div>
  );
}

function AgentSection() {
  const origin = window.location.origin;
  const oneLiner = `irm ${origin}/api/client/install.ps1 | iex`;
  const [copied, setCopied] = useState(false);

  const copy = async () => {
    await navigator.clipboard.writeText(oneLiner);
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  };

  return (
    <Section title="Windows companion agent" icon={<Terminal className="h-5 w-5" />}>
      <p className="text-sm text-slate-400">
        Install the agent on your Windows laptop so the{" "}
        <span className="text-slate-200">Install</span> and{" "}
        <span className="text-slate-200">Play</span> buttons work. In an{" "}
        <span className="text-slate-200">admin PowerShell</span> window, run:
      </p>
      <div className="mt-3 flex items-stretch gap-2">
        <code className="flex-1 overflow-x-auto rounded-lg border border-ink-700 bg-ink-950 px-3 py-2.5 text-xs text-accent">
          {oneLiner}
        </code>
        <button className="btn-ghost shrink-0" onClick={copy}>
          {copied ? <Check className="h-4 w-4 text-play" /> : <Copy className="h-4 w-4" />}
        </button>
      </div>
      <p className="mt-3 text-xs text-slate-500">
        It registers the <span className="text-slate-400">ludex://</span> handler and signs in with
        your Ludex account. Games download from this server, extract locally, and report playtime
        back here. It never changes your security settings automatically — DRM games print the
        manual steps for you to run.
      </p>
    </Section>
  );
}

function AccountSection() {
  const { user, authDisabled, logout } = useAuth();
  const [current, setCurrent] = useState("");
  const [next, setNext] = useState("");
  const [msg, setMsg] = useState<string | null>(null);

  const change = useMutation({
    mutationFn: () => changeMyPassword(current, next),
    onSuccess: () => {
      setMsg("Password updated.");
      setCurrent("");
      setNext("");
    },
    onError: (e) => setMsg(e instanceof Error ? e.message : "Failed"),
  });

  return (
    <Section title="Account" icon={<HardDrive className="h-5 w-5" />}>
      <div className="flex items-center gap-3">
        <div className="flex-1">
          <p className="font-medium text-slate-100">{user?.username ?? "guest"}</p>
          <p className="text-xs text-slate-500">
            {authDisabled ? "Authentication is disabled" : user?.isAdmin ? "Administrator" : "User"}
          </p>
        </div>
        {!authDisabled && (
          <button className="btn-ghost" onClick={() => logout()}>
            Sign out
          </button>
        )}
      </div>

      {!authDisabled && (
        <div className="mt-4 grid grid-cols-1 gap-2 sm:grid-cols-3 sm:items-end">
          <div>
            <label className="mb-1 block text-xs text-slate-400">Current password</label>
            <input
              className="input"
              type="password"
              value={current}
              onChange={(e) => setCurrent(e.target.value)}
            />
          </div>
          <div>
            <label className="mb-1 block text-xs text-slate-400">New password</label>
            <input
              className="input"
              type="password"
              value={next}
              onChange={(e) => setNext(e.target.value)}
            />
          </div>
          <button
            className="btn-primary"
            onClick={() => change.mutate()}
            disabled={!current || next.length < 4 || change.isPending}
          >
            Update
          </button>
        </div>
      )}
      {msg && <p className="mt-2 text-xs text-slate-400">{msg}</p>}
    </Section>
  );
}
