import { useEffect } from "react";
import { useQuery } from "@tanstack/react-query";
import {
  Download,
  FileText,
  Loader2,
  Play,
  ShieldAlert,
  Star,
  Trash2,
  X,
} from "lucide-react";

import { coverUrl, downloadUrl, getGame } from "@/api";
import { formatBytes, formatPlaytime, ludexUri, relativeTime } from "@/lib/utils";

const SETUP_HELP: Record<string, string> = {
  portable: "Portable build — the agent extracts it and runs the game directly.",
  portable_hypervisor:
    "Denuvo / hypervisor release. Before playing you must run VBS.cmd as admin and reboot with Driver Signature Enforcement disabled (press F7 at boot). Follow the notes below.",
  iso: "Disc image — the agent downloads the .iso, mounts it, and runs its setup.",
  installer: "Installer — the agent downloads the setup and runs it.",
  archive: "Archive — the agent downloads and extracts it, then looks for the game.",
};

export function GameDetail({ slug, onClose }: { slug: string; onClose: () => void }) {
  const { data: game, isLoading } = useQuery({
    queryKey: ["game", slug],
    queryFn: () => getGame(slug),
  });

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  return (
    <div
      className="fixed inset-0 z-40 flex items-start justify-center overflow-y-auto bg-ink-950/80 p-4 backdrop-blur-sm sm:p-8"
      onClick={onClose}
    >
      <div
        className="card relative w-full max-w-3xl overflow-hidden shadow-2xl"
        onClick={(e) => e.stopPropagation()}
      >
        <button
          onClick={onClose}
          className="absolute right-3 top-3 z-10 rounded-lg bg-ink-950/60 p-1.5 text-slate-300 hover:bg-ink-800 hover:text-white"
        >
          <X className="h-5 w-5" />
        </button>

        {isLoading || !game ? (
          <div className="flex h-64 items-center justify-center text-slate-400">
            <Loader2 className="h-6 w-6 animate-spin" />
          </div>
        ) : (
          <div>
            <div className="flex flex-col gap-5 p-5 sm:flex-row">
              <div className="h-64 w-44 shrink-0 overflow-hidden rounded-lg border border-ink-700 bg-ink-800">
                {game.hasCover ? (
                  <img src={coverUrl(game.slug)} alt="" className="h-full w-full object-cover" />
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
                  {game.releaseGroup && <span>· {game.releaseGroup}</span>}
                  <span>· {formatBytes(game.sizeBytes)}</span>
                  <span>· {formatPlaytime(game.playtimeSeconds)}</span>
                  {game.lastPlayed && <span>· last played {relativeTime(game.lastPlayed)}</span>}
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
                      This game needs Virtualization-Based Security and Driver Signature
                      Enforcement turned off. That weakens Windows security while active — revert
                      it when you're done. Ludex won't change these settings for you.
                    </span>
                  </div>
                )}

                <div className="mt-4 flex flex-wrap gap-2">
                  {game.installed ? (
                    <a className="btn-play" href={ludexUri("play", game.slug)}>
                      <Play className="h-4 w-4" /> Play
                    </a>
                  ) : (
                    <a className="btn-primary" href={ludexUri("install", game.slug)}>
                      <Download className="h-4 w-4" /> Install
                    </a>
                  )}
                  <a className="btn-ghost" href={downloadUrl(game.slug)}>
                    <Download className="h-4 w-4" /> Download{" "}
                    {game.downloadKind === "tar" ? "(.tar)" : ""}
                  </a>
                  {game.installed && (
                    <a className="btn-danger" href={ludexUri("uninstall", game.slug)}>
                      <Trash2 className="h-4 w-4" /> Uninstall
                    </a>
                  )}
                </div>

                {game.exeHint && (
                  <p className="mt-3 truncate text-xs text-slate-500">
                    Runs: <span className="text-slate-400">{game.exeHint}</span>
                  </p>
                )}
              </div>
            </div>

            {game.instructions && (
              <div className="border-t border-ink-700/70 bg-ink-900/50 p-5">
                <p className="mb-2 flex items-center gap-1.5 text-xs font-semibold uppercase tracking-wide text-slate-400">
                  <FileText className="h-3.5 w-3.5" /> Release notes
                </p>
                <pre className="max-h-56 overflow-auto whitespace-pre-wrap rounded-lg bg-ink-950/60 p-3 text-xs leading-relaxed text-slate-300">
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
