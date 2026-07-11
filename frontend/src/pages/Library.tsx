import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Gamepad2, Loader2, Search } from "lucide-react";

import { getGames, type GameCard as Game } from "@/api";
import { GameCard } from "@/components/GameCard";
import { GameDetail } from "@/components/GameDetail";
import { cn } from "@/lib/utils";

type Sort = "title" | "size" | "recent";
type Filter = "all" | "installed" | "drm";

export function Library() {
  const [search, setSearch] = useState("");
  const [sort, setSort] = useState<Sort>("title");
  const [filter, setFilter] = useState<Filter>("all");
  const [openSlug, setOpenSlug] = useState<string | null>(null);

  const { data, isLoading, error } = useQuery({
    queryKey: ["games", search, sort],
    queryFn: () => getGames({ search, sort }),
  });

  const games = useMemo(() => {
    const list = data?.games ?? [];
    if (filter === "installed") return list.filter((g) => g.installed);
    if (filter === "drm") return list.filter((g) => g.requiresHypervisor);
    return list;
  }, [data, filter]);

  return (
    <div>
      <div className="mb-5 flex flex-col gap-3 sm:flex-row sm:items-center">
        <div className="relative flex-1">
          <Search className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-slate-500" />
          <input
            className="input pl-9"
            placeholder="Search your library…"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
        </div>
        <div className="flex items-center gap-2">
          <SegBtns
            value={filter}
            onChange={(v) => setFilter(v as Filter)}
            options={[
              ["all", "All"],
              ["installed", "Installed"],
              ["drm", "DRM"],
            ]}
          />
          <select
            className="input w-auto"
            value={sort}
            onChange={(e) => setSort(e.target.value as Sort)}
          >
            <option value="title">Name</option>
            <option value="size">Size</option>
            <option value="recent">Recently added</option>
          </select>
        </div>
      </div>

      {isLoading ? (
        <Centered>
          <Loader2 className="h-6 w-6 animate-spin" />
        </Centered>
      ) : error ? (
        <Centered>Couldn't load your library.</Centered>
      ) : games.length === 0 ? (
        <EmptyState hasGames={(data?.games?.length ?? 0) > 0} />
      ) : (
        <div className="grid grid-cols-2 gap-4 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6">
          {games.map((g: Game) => (
            <GameCard key={g.slug} game={g} onOpen={() => setOpenSlug(g.slug)} />
          ))}
        </div>
      )}

      {openSlug && <GameDetail slug={openSlug} onClose={() => setOpenSlug(null)} />}
    </div>
  );
}

function SegBtns({
  value,
  onChange,
  options,
}: {
  value: string;
  onChange: (v: string) => void;
  options: [string, string][];
}) {
  return (
    <div className="flex rounded-lg border border-ink-600 bg-ink-800 p-0.5 text-sm">
      {options.map(([v, label]) => (
        <button
          key={v}
          onClick={() => onChange(v)}
          className={cn(
            "rounded-md px-3 py-1.5 font-medium transition-colors",
            value === v ? "bg-accent text-ink-950" : "text-slate-400 hover:text-slate-200",
          )}
        >
          {label}
        </button>
      ))}
    </div>
  );
}

function Centered({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex h-64 items-center justify-center text-slate-400">{children}</div>
  );
}

function EmptyState({ hasGames }: { hasGames: boolean }) {
  return (
    <div className="flex flex-col items-center justify-center gap-3 py-24 text-center">
      <div className="flex h-14 w-14 items-center justify-center rounded-2xl bg-ink-800 text-slate-500">
        <Gamepad2 className="h-7 w-7" />
      </div>
      <p className="text-slate-300">
        {hasGames ? "No games match your filters." : "Your library is empty."}
      </p>
      {!hasGames && (
        <p className="max-w-sm text-sm text-slate-500">
          Add a folder of games in <span className="text-slate-300">Settings → Libraries</span>,
          then run a scan.
        </p>
      )}
    </div>
  );
}
