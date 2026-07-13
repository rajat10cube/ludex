import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useState,
  type ReactNode,
} from "react";
import { useQueryClient } from "@tanstack/react-query";

import {
  cancelInstall,
  install as invokeInstall,
  onInstallProgress,
  pauseInstall,
  pausedDownloads,
  resumeInstall,
  type InstallProgress,
} from "@/api";

export interface InstallState {
  slug: string;
  title: string;
  phase: InstallProgress["phase"];
  received: number;
  total: number;
  message: string | null;
  status: "active" | "paused" | "done" | "error";
  error?: string;
  speed: number; // bytes/sec (download phase), smoothed
  ts: number; // last progress timestamp (perf clock)
}

interface InstallsCtx {
  installs: Record<string, InstallState>;
  startInstall: (slug: string, title: string) => void;
  resume: (slug: string, title: string) => void;
  pause: (slug: string) => void;
  cancel: (slug: string) => void;
  dismiss: (slug: string) => void;
  activeCount: number;
}

const Ctx = createContext<InstallsCtx | null>(null);

export function InstallsProvider({ children }: { children: ReactNode }) {
  const qc = useQueryClient();
  const [installs, setInstalls] = useState<Record<string, InstallState>>({});

  // progress events (throttled from the Rust core)
  useEffect(() => {
    const un = onInstallProgress((p) => {
      setInstalls((cur) => {
        const prev = cur[p.slug];
        if (!prev) return cur;
        // ignore stray progress for a paused/finished item
        if (prev.status !== "active") return cur;
        const now = performance.now();
        let speed = prev.speed;
        if (p.phase === "download") {
          const dt = (now - prev.ts) / 1000;
          const db = p.received - prev.received;
          if (dt > 0.02 && db >= 0) {
            const inst = db / dt;
            speed = prev.speed > 0 ? prev.speed * 0.6 + inst * 0.4 : inst;
          }
        } else {
          speed = 0; // not downloading anymore
        }
        return {
          ...cur,
          [p.slug]: {
            ...prev,
            phase: p.phase,
            received: p.received,
            total: p.total,
            message: p.message,
            speed,
            ts: now,
          },
        };
      });
    });
    return () => {
      un.then((f) => f());
    };
  }, []);

  // restore paused / interrupted downloads on launch
  useEffect(() => {
    pausedDownloads()
      .then((list) => {
        if (!list.length) return;
        setInstalls((cur) => {
          const next = { ...cur };
          for (const d of list) {
            if (!next[d.slug]) {
              next[d.slug] = {
                slug: d.slug,
                title: d.title,
                phase: "download",
                received: d.bytes,
                total: d.total,
                message: null,
                status: "paused",
                speed: 0,
                ts: performance.now(),
              };
            }
          }
          return next;
        });
      })
      .catch(() => {});
  }, []);

  const run = useCallback(
    (slug: string, title: string, resume: boolean) => {
      setInstalls((cur) => ({
        ...cur,
        [slug]: {
          slug,
          title,
          phase: "download",
          received: resume ? (cur[slug]?.received ?? 0) : 0,
          total: cur[slug]?.total ?? 0,
          message: "Starting…",
          status: "active",
          speed: 0,
          ts: performance.now(),
        },
      }));
      const call = resume ? resumeInstall(slug) : invokeInstall(slug);
      call
        .then((res) => {
          setInstalls((cur) => {
            if (!cur[slug]) return cur;
            if (res.status === "cancelled") {
              const next = { ...cur };
              delete next[slug];
              return next;
            }
            if (res.status === "paused") {
              return { ...cur, [slug]: { ...cur[slug], status: "paused", phase: "download" } };
            }
            return { ...cur, [slug]: { ...cur[slug], status: "done", phase: "done" } };
          });
          if (res.status === "installed") {
            qc.invalidateQueries({ queryKey: ["games"] });
            qc.invalidateQueries({ queryKey: ["game", slug] });
            setTimeout(() => {
              setInstalls((cur) => {
                if (cur[slug]?.status !== "done") return cur;
                const next = { ...cur };
                delete next[slug];
                return next;
              });
            }, 6000);
          }
        })
        .catch((e) => {
          setInstalls((cur) =>
            cur[slug]
              ? {
                  ...cur,
                  [slug]: {
                    ...cur[slug],
                    status: "error",
                    error: typeof e === "string" ? e : "Install failed",
                  },
                }
              : cur,
          );
        });
    },
    [qc],
  );

  const startInstall = useCallback((slug: string, title: string) => run(slug, title, false), [run]);
  const resume = useCallback((slug: string, title: string) => run(slug, title, true), [run]);

  const pause = useCallback((slug: string) => {
    // the in-flight install() promise resolves with status "paused"
    pauseInstall(slug).catch(() => {});
    setInstalls((cur) =>
      cur[slug]?.status === "active"
        ? { ...cur, [slug]: { ...cur[slug], message: "Pausing…" } }
        : cur,
    );
  }, []);

  const cancel = useCallback((slug: string) => {
    cancelInstall(slug).catch(() => {});
    // active downloads are removed when their promise resolves "cancelled";
    // paused/errored ones have no in-flight promise, so drop them now.
    setInstalls((cur) => {
      if (cur[slug] && cur[slug].status !== "active") {
        const next = { ...cur };
        delete next[slug];
        return next;
      }
      return cur;
    });
  }, []);

  const dismiss = useCallback((slug: string) => {
    setInstalls((cur) => {
      const next = { ...cur };
      delete next[slug];
      return next;
    });
  }, []);

  const activeCount = Object.values(installs).filter((i) => i.status === "active").length;

  return (
    <Ctx.Provider
      value={{ installs, startInstall, resume, pause, cancel, dismiss, activeCount }}
    >
      {children}
    </Ctx.Provider>
  );
}

export function useInstalls(): InstallsCtx {
  const ctx = useContext(Ctx);
  if (!ctx) throw new Error("useInstalls must be used within InstallsProvider");
  return ctx;
}

/** Percent 0-100 for a determinate phase, or null when indeterminate. */
export function installPercent(st: InstallState | undefined): number | null {
  if (!st || st.total <= 1) return null;
  return Math.min(100, (st.received / st.total) * 100);
}

export function phaseLabel(st: InstallState): string {
  if (st.status === "error") return st.error ?? "Failed";
  if (st.status === "paused") return "Paused";
  switch (st.phase) {
    case "download":
      return "Downloading";
    case "extract":
      return "Extracting";
    case "install":
      return st.message ?? "Installing";
    case "done":
      return "Installed";
    default:
      return "Working";
  }
}
