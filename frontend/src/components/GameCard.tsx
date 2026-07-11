import { useState } from "react";
import { CheckCircle2, Disc, Package, ShieldAlert } from "lucide-react";

import { coverUrl, type GameCard as Game } from "@/api";
import { cn, formatBytes } from "@/lib/utils";

const SETUP_LABEL: Record<string, string> = {
  portable: "Portable",
  portable_hypervisor: "Hypervisor",
  iso: "Disc image",
  installer: "Installer",
  archive: "Archive",
};

export function GameCard({ game, onOpen }: { game: Game; onOpen: () => void }) {
  const [imgOk, setImgOk] = useState(game.hasCover);

  return (
    <button
      onClick={onOpen}
      className="group flex flex-col text-left focus:outline-none"
      title={game.title}
    >
      <div
        className={cn(
          "relative aspect-[3/4] w-full overflow-hidden rounded-xl border border-ink-700/70 bg-ink-800",
          "ring-accent/0 transition group-hover:-translate-y-0.5 group-hover:ring-2 group-hover:ring-accent/60",
        )}
      >
        {imgOk ? (
          <img
            src={coverUrl(game.slug)}
            alt=""
            loading="lazy"
            className="h-full w-full object-cover"
            onError={() => setImgOk(false)}
          />
        ) : (
          <FallbackArt title={game.title} />
        )}

        <div className="absolute left-2 top-2 flex flex-col gap-1">
          {game.installed && (
            <span className="chip border-play/40 bg-play/15 text-play">
              <CheckCircle2 className="h-3 w-3" /> Installed
            </span>
          )}
          {game.requiresHypervisor && (
            <span className="chip border-amber-400/40 bg-amber-400/15 text-amber-300">
              <ShieldAlert className="h-3 w-3" /> DRM
            </span>
          )}
        </div>

        {game.missing && (
          <div className="absolute inset-0 flex items-center justify-center bg-ink-950/70 text-xs text-red-300">
            Files offline
          </div>
        )}

        <div className="absolute inset-x-0 bottom-0 flex items-center justify-between gap-2 bg-gradient-to-t from-ink-950/90 to-transparent px-2.5 pb-2 pt-8 text-[11px] text-slate-300">
          <span className="inline-flex items-center gap-1">
            {game.setupType === "iso" ? (
              <Disc className="h-3 w-3" />
            ) : (
              <Package className="h-3 w-3" />
            )}
            {SETUP_LABEL[game.setupType] ?? game.setupType}
          </span>
          <span>{formatBytes(game.sizeBytes)}</span>
        </div>
      </div>

      <div className="mt-2 px-0.5">
        <p className="truncate text-sm font-medium text-slate-100 group-hover:text-white">
          {game.title}
        </p>
        <p className="truncate text-xs text-slate-500">
          {game.version ? `v${game.version}` : game.releaseGroup ?? " "}
        </p>
      </div>
    </button>
  );
}

function FallbackArt({ title }: { title: string }) {
  return (
    <div className="flex h-full w-full items-center justify-center bg-gradient-to-br from-ink-700 to-ink-900 p-3">
      <span className="line-clamp-4 text-center text-sm font-semibold text-slate-300">
        {title}
      </span>
    </div>
  );
}
