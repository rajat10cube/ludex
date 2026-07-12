import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useState,
  type ReactNode,
} from "react";
import { useQueryClient } from "@tanstack/react-query";

import { install as invokeInstall, onInstallProgress, type InstallProgress } from "@/api";

export interface InstallState {
  slug: string;
  title: string;
  phase: InstallProgress["phase"];
  received: number;
  total: number;
  message: string | null;
  status: "active" | "done" | "error";
  error?: string;
}

interface InstallsCtx {
  installs: Record<string, InstallState>;
  startInstall: (slug: string, title: string) => void;
  dismiss: (slug: string) => void;
  activeCount: number;
}

const Ctx = createContext<InstallsCtx | null>(null);

export function InstallsProvider({ children }: { children: ReactNode }) {
  const qc = useQueryClient();
  const [installs, setInstalls] = useState<Record<string, InstallState>>({});

  // Single global listener so progress is tracked no matter what's mounted.
  useEffect(() => {
    const un = onInstallProgress((p) => {
      setInstalls((cur) => {
        const prev = cur[p.slug];
        if (!prev) return cur;
        return {
          ...cur,
          [p.slug]: {
            ...prev,
            phase: p.phase,
            received: p.received,
            total: p.total,
            message: p.message,
          },
        };
      });
    });
    return () => {
      un.then((f) => f());
    };
  }, []);

  const startInstall = useCallback(
    (slug: string, title: string) => {
      setInstalls((cur) => {
        if (cur[slug]?.status === "active") return cur;
        return {
          ...cur,
          [slug]: {
            slug,
            title,
            phase: "download",
            received: 0,
            total: 0,
            message: "Starting…",
            status: "active",
          },
        };
      });
      invokeInstall(slug)
        .then(() => {
          setInstalls((cur) =>
            cur[slug] ? { ...cur, [slug]: { ...cur[slug], status: "done", phase: "done" } } : cur,
          );
          qc.invalidateQueries({ queryKey: ["games"] });
          qc.invalidateQueries({ queryKey: ["game", slug] });
          // auto-dismiss a successful install from the panel after a bit
          setTimeout(() => {
            setInstalls((cur) => {
              if (cur[slug]?.status !== "done") return cur;
              const next = { ...cur };
              delete next[slug];
              return next;
            });
          }, 6000);
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

  const dismiss = useCallback((slug: string) => {
    setInstalls((cur) => {
      const next = { ...cur };
      delete next[slug];
      return next;
    });
  }, []);

  const activeCount = Object.values(installs).filter((i) => i.status === "active").length;

  return (
    <Ctx.Provider value={{ installs, startInstall, dismiss, activeCount }}>{children}</Ctx.Provider>
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
