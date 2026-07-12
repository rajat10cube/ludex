import { useEffect, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  Download,
  FolderOpen,
  Loader2,
  Play,
  ShieldAlert,
  Star,
  Trash2,
  X,
} from "lucide-react";

import {
  gameDetail,
  install,
  onInstallProgress,
  openInstallDir,
  play,
  uninstall,
  type InstallProgress,
} from "@/api";
import { useCover } from "@/components/GameCard";
import { cn, formatBytes, formatPlaytime } from "@/lib/utils";

const SETUP_HELP: Record<string, string> = {
  portable: "Portable build — extracted and run directly.",
  portable_hypervisor:
    "Denuvo / hypervisor release. Before playing you must run VBS.cmd as admin and reboot with Driver Signature Enforcement disabled (press F7 at boot). Ludex won't change these settings for you.",
  iso: "Disc image — downloaded, mounted, and its setup is run.",
  installer: "Installer — downloaded and run.",
  archive: "Archive — downloaded and extracted.",
};

export function GameDetail({ slug, onClose }: { slug: string; onClose: () => void }) {
  const qc = useQueryClient();
  const { data: game, isLoading } = useQuery({ queryKey: ["game", slug], queryFn: () => gameDetail(slug) });
  const { data: cover } = useCover(slug, game?.hasCover ?? false);

  const [progress, setProgress] = useState<InstallProgress | null>(null);
  const [busy, setBusy] = useState<null | "install" | "play" | "uninstall">(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const un = onInstallProgress((p) => {
      if (p.slug === slug) setProgress(p);
    });
    return () => {
      un.then((f) => f());
    };
  }, [slug]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && !busy && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose, busy]);

  const refresh = () => {
    qc.invalidateQueries({ queryKey: ["game", slug] });
    qc.invalidateQueries({ queryKey: ["games"] });
  };

  const doInstall = async () => {
    setError(null);
    setBusy("install");
    setProgress({ slug, phase: "download", received: 0, total: 0, message: "Starting…" });
    try {
      await install(slug);
      refresh();
    } catch (e) {
      setError(typeof e === "string" ? e : "Install failed");
    } finally {
      setBusy(null);
      setProgress(null);
    }
  };

  const doPlay = async () => {
    setError(null);
    setBusy("play");
    try {
      await play(slug);
      refresh();
    } catch (e) {
      setError(typeof e === "string" ? e : "Launch failed");
    } finally {
      setBusy(null);
    }
  };

  const doUninstall = async () => {
    setError(null);
    setBusy("uninstall");
    try {
      await uninstall(slug);
      refresh();
    } catch (e) {
      setError(typeof e === "string" ? e : "Uninstall failed");
    } finally {
      setBusy(null);
    }
  };

  return (
    <div
      className="fixed inset-0 z-40 flex items-start justify-center overflow-y-auto bg-ink-950/80 p-4 backdrop-blur-sm sm:p-8"
      onClick={() => !busy && onClose()}
    >
      <div className="card relative w-full max-w-3xl overflow-hidden shadow-2xl" onClick={(e) => e.stopPropagation()}>
        {!busy && (
          <button
            onClick={onClose}
            className="absolute right-3 top-3 z-10 rounded-lg bg-ink-950/60 p-1.5 text-slate-300 hover:bg-ink-800 hover:text-white"
          >
            <X className="h-5 w-5" />
          </button>
        )}

        {isLoading || !game ? (
          <div className="flex h-64 items-center justify-center text-slate-400">
            <Loader2 className="h-6 w-6 animate-spin" />
          </div>
        ) : (
          <div>
            <div className="flex flex-col gap-5 p-5 sm:flex-row">
              <div className="h-64 w-44 shrink-0 overflow-hidden rounded-lg border border-ink-700 bg-ink-800">
                {cover ? (
                  <img src={cover} alt="" className="h-full w-full object-cover" />
                ) : (
                  <div className="flex h-full items-center justify-center p-3 text-center text-sm font-semibold text-slate-400">
                    {game.title}
                  </div>
                )}
              </div>

              <div className="min-w-0 flex-1">
                <h2 className="text-xl font-semibold text-white">{game.title}</h2>
                <div className="mt-1 flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-slate-400">
                  {game.releaseYear && <span>{game.releaseYear}</span>}
                  {game.rating != null && (
                    <span className="inline-flex items-center gap-1 text-amber-300">
                      <Star className="h-3 w-3 fill-current" /> {game.rating}
                    </span>
                  )}
                  {game.version && <span>· v{game.version}</span>}
                  <span>· {formatBytes(game.sizeBytes)}</span>
                  <span>· {formatPlaytime(game.playtimeSeconds)}</span>
                </div>

                {game.genres && (
                  <div className="mt-2 flex flex-wrap gap-1.5">
                    {game.genres.split(",").map((g) => (
                      <span key={g} className="chip">
                        {g.trim()}
                      </span>
                    ))}
                  </div>
                )}

                {game.description && (
                  <p className="mt-3 text-sm leading-relaxed text-slate-300">{game.description}</p>
                )}
                <p className="mt-3 text-sm text-slate-400">{SETUP_HELP[game.setupType]}</p>

                {game.requiresHypervisor && (
                  <div className="mt-3 flex gap-2 rounded-lg border border-amber-400/30 bg-amber-400/10 p-3 text-xs text-amber-200">
                    <ShieldAlert className="mt-0.5 h-4 w-4 shrink-0" />
                    <span>
                      Needs Virtualization-Based Security + Driver Signature Enforcement disabled
                      (a reboot). Ludex launches the game as admin but won't change those settings.
                    </span>
                  </div>
                )}

                {busy === "install" && progress ? (
                  <InstallProgressBar p={progress} />
                ) : (
                  <div className="mt-4 flex flex-wrap gap-2">
                    {game.installed ? (
                      <>
                        <button className="btn-play" onClick={doPlay} disabled={!!busy}>
                          {busy === "play" ? <Loader2 className="h-4 w-4 animate-spin" /> : <Play className="h-4 w-4" />}
                          {busy === "play" ? "Running…" : "Play"}
                        </button>
                        <button className="btn-ghost" onClick={() => openInstallDir(slug)} disabled={!!busy}>
                          <FolderOpen className="h-4 w-4" /> Files
                        </button>
                        <button className="btn-danger" onClick={doUninstall} disabled={!!busy}>
                          {busy === "uninstall" ? <Loader2 className="h-4 w-4 animate-spin" /> : <Trash2 className="h-4 w-4" />}
                          Uninstall
                        </button>
                      </>
                    ) : (
                      <button className="btn-primary" onClick={doInstall} disabled={!!busy}>
                        <Download className="h-4 w-4" /> Install
                      </button>
                    )}
                  </div>
                )}

                {error && <p className="mt-3 text-sm text-red-400">{error}</p>}
              </div>
            </div>

            {game.instructions && (
              <div className="border-t border-ink-700/70 bg-ink-900/50 p-5">
                <p className="mb-2 text-xs font-semibold uppercase tracking-wide text-slate-400">
                  Release notes
                </p>
                <pre className="max-h-48 overflow-auto whitespace-pre-wrap rounded-lg bg-ink-950/60 p-3 text-xs leading-relaxed text-slate-300">
                  {game.instructions}
                </pre>
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
}

function InstallProgressBar({ p }: { p: InstallProgress }) {
  const pct = p.total > 0 ? Math.min(100, (p.received / p.total) * 100) : null;
  const label =
    p.phase === "download"
      ? pct != null
        ? `Downloading ${formatBytes(p.received)} / ${formatBytes(p.total)}`
        : `Downloading ${formatBytes(p.received)}`
      : p.phase === "extract"
        ? "Extracting…"
        : p.phase === "install"
          ? p.message ?? "Installing…"
          : "Working…";
  return (
    <div className="mt-4">
      <div className="mb-1 flex items-center justify-between text-xs text-slate-400">
        <span className="inline-flex items-center gap-2">
          <Loader2 className="h-3.5 w-3.5 animate-spin" /> {label}
        </span>
        {pct != null && <span>{pct.toFixed(0)}%</span>}
      </div>
      <div className="h-2 overflow-hidden rounded-full bg-ink-700">
        <div
          className={cn("h-full rounded-full bg-accent transition-all", pct == null && "animate-pulse w-1/3")}
          style={pct != null ? { width: `${pct}%` } : undefined}
        />
      </div>
    </div>
  );
}
