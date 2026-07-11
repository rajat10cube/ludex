import { useState, type ReactNode } from "react";
import { Gamepad2, Loader2 } from "lucide-react";

import { setupAdmin } from "@/api";
import { useAuth } from "@/auth";

export function AuthGate({ children }: { children: ReactNode }) {
  const { ready, user, needsSetup, authDisabled } = useAuth();

  if (!ready) {
    return (
      <div className="flex min-h-full items-center justify-center text-slate-400">
        <Loader2 className="h-6 w-6 animate-spin" />
      </div>
    );
  }
  if (user || authDisabled) return <>{children}</>;
  return <AuthScreen mode={needsSetup ? "setup" : "login"} />;
}

function AuthScreen({ mode }: { mode: "setup" | "login" }) {
  const { login, setUser } = useAuth();
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);
    setBusy(true);
    try {
      if (mode === "setup") {
        const me = await setupAdmin(username.trim(), password);
        setUser(me);
      } else {
        await login(username.trim(), password);
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : "Something went wrong");
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="flex min-h-full items-center justify-center px-4">
      <div className="w-full max-w-sm">
        <div className="mb-6 flex flex-col items-center gap-2 text-center">
          <div className="flex h-12 w-12 items-center justify-center rounded-xl bg-accent-soft text-accent">
            <Gamepad2 className="h-7 w-7" />
          </div>
          <h1 className="text-2xl font-semibold text-white">Ludex</h1>
          <p className="text-sm text-slate-400">
            {mode === "setup"
              ? "Create your admin account to get started."
              : "Sign in to your game library."}
          </p>
        </div>
        <form onSubmit={submit} className="card space-y-3 p-5">
          <div>
            <label className="mb-1 block text-xs font-medium text-slate-400">Username</label>
            <input
              className="input"
              value={username}
              autoFocus
              autoComplete="username"
              onChange={(e) => setUsername(e.target.value)}
            />
          </div>
          <div>
            <label className="mb-1 block text-xs font-medium text-slate-400">Password</label>
            <input
              className="input"
              type="password"
              value={password}
              autoComplete={mode === "setup" ? "new-password" : "current-password"}
              onChange={(e) => setPassword(e.target.value)}
            />
          </div>
          {error && <p className="text-sm text-red-400">{error}</p>}
          <button className="btn-primary w-full" disabled={busy || !username || !password}>
            {busy && <Loader2 className="h-4 w-4 animate-spin" />}
            {mode === "setup" ? "Create account" : "Sign in"}
          </button>
        </form>
      </div>
    </div>
  );
}
