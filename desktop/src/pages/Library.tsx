import { useMemo, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { FolderCog, Gamepad2, Loader2, LogOut, RefreshCw, Search } from "lucide-react";

import {
  disconnect,
  listGames,
  pickInstallDir,
  setInstallDir,
  type Game,
  type Session,
} from "@/api";
import { GameCard } from "@/components/GameCard";
import { GameDetail } from "@/components/GameDetail";
import { cn } from "@/lib/utils";

type Filter = "all" | "installed" | "drm";

export function Library({ session, onDisconnect }: { session: Session; onDisconnect: () => void }) {
  const qc = useQueryClient();
  const [search, setSearch] = useState("");
  const [filter, setFilter] = useState<Filter>("all");
  const [openSlug, setOpenSlug] = useState<string | null>(null);
  const [menu, setMenu] = useState(false);

  const { data, isLoading, isFetching, error, refetch } = useQuery({
    queryKey: ["games"],
    queryFn: listGames,
  });

  const games = useMemo(() => {
    let list = data?.games ?? [];
    if (search.trim()) {
      const q = search.toLowerCase();
      list = list.filter((g) => g.title.toLowerCase().includes(q));
    }
    if (filter === "installed") list = list.filter((g) => g.installed);
    if (filter === "drm") list = list.filter((g) => g.requiresHypervisor);
    return list;
  }, [data, search, filter]);

  const changeDir = async () => {
    const picked = await pickInstallDir();
    if (picked) {
      await setInstallDir(picked);
      qc.invalidateQueries({ queryKey: ["session"] });
    }
    setMenu(false);
  };

  return (
    <div className="flex h-full flex-col">
      <header className="flex h-14 shrink-0 items-center gap-3 border-b border-ink-700/70 bg-ink-950/85 px-4 backdrop-blur">
        <span className="flex items-center gap-2 font-semibold text-white">
          <span className="flex h-8 w-8 items-center justify-center rounded-lg bg-accent-soft text-accent">
            <Gamepad2 className="h-5 w-5" />
          </span>
          Ludex
        </span>

        <div className="relative ml-2 max-w-xs flex-1">
          <Search className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-slate-500" />
          <input
            className="input pl-9"
            placeholder="Search…"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
        </div>

        <SegBtns
          value={filter}
          onChange={(v) => setFilter(v as Filter)}
          options={[
            ["all", "All"],
            ["installed", "Installed"],
            ["drm", "DRM"],
          ]}
        />

        <div className="ml-auto flex items-center gap-2">
          <button
            className="btn-ghost h-9 px-2.5"
            onClick={() => refetch()}
            title="Refresh library"
          >
            <RefreshCw className={cn("h-4 w-4", isFetching && "animate-spin")} />
          </button>
          <div className="relative">
            <button className="btn-ghost h-9" onClick={() => setMenu((m) => !m)}>
              {session.username}
            </button>
            {menu && (
              <div
                className="absolute right-0 top-11 z-30 w-64 card p-1 text-sm shadow-xl"
                onMouseLeave={() => setMenu(false)}
              >
                <div className="px-3 py-2 text-xs text-slate-500">
                  <div className="truncate">Server: {session.server}</div>
                  <div className="mt-0.5 truncate">Games: {session.installDir}</div>
                </div>
                <button
                  className="flex w-full items-center gap-2 rounded-md px-3 py-2 text-left text-slate-200 hover:bg-ink-800"
                  onClick={changeDir}
                >
                  <FolderCog className="h-4 w-4" /> Change install folder
                </button>
                <button
                  className="flex w-full items-center gap-2 rounded-md px-3 py-2 text-left text-red-300 hover:bg-ink-800"
                  onClick={async () => {
                    await disconnect();
                    onDisconnect();
                  }}
                >
                  <LogOut className="h-4 w-4" /> Sign out
                </button>
              </div>
            )}
          </div>
        </div>
      </header>

      <main className="flex-1 overflow-y-auto px-4 py-6 sm:px-6">
        {isLoading ? (
          <Centered>
            <Loader2 className="h-6 w-6 animate-spin" />
          </Centered>
        ) : error ? (
          <Centered>
            <div className="text-center">
              <p>Couldn't reach {session.server}.</p>
              <button className="btn-ghost mt-3" onClick={() => refetch()}>
                Try again
              </button>
            </div>
          </Centered>
        ) : games.length === 0 ? (
          <Centered>{(data?.games?.length ?? 0) > 0 ? "No games match." : "Library is empty."}</Centered>
        ) : (
          <div className="mx-auto grid max-w-[1500px] grid-cols-2 gap-4 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6">
            {games.map((g: Game) => (
              <GameCard key={g.slug} game={g} onOpen={() => setOpenSlug(g.slug)} />
            ))}
          </div>
        )}
      </main>

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
  return <div className="flex h-full items-center justify-center text-slate-400">{children}</div>;
}
