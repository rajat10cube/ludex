import { useState } from "react";
import { Gamepad2, Loader2 } from "lucide-react";

import { connect, DEFAULT_SERVER } from "@/api";

export function Connect({ onConnected }: { onConnected: () => void }) {
  const [server, setServer] = useState(DEFAULT_SERVER);
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);
    setBusy(true);
    try {
      await connect(server.trim().replace(/\/+$/, ""), username.trim(), password);
      onConnected();
    } catch (err) {
      setError(typeof err === "string" ? err : "Could not connect");
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="flex h-full items-center justify-center px-4">
      <div className="w-full max-w-sm">
        <div className="mb-6 flex flex-col items-center gap-2 text-center">
          <div className="flex h-12 w-12 items-center justify-center rounded-xl bg-accent-soft text-accent">
            <Gamepad2 className="h-7 w-7" />
          </div>
          <h1 className="text-2xl font-semibold text-white">Ludex</h1>
          <p className="text-sm text-slate-400">Connect to your game server.</p>
        </div>
        <form onSubmit={submit} className="card space-y-3 p-5">
          <Field label="Server address">
            <input
              className="input"
              value={server}
              placeholder="http://192.168.0.188:8000"
              onChange={(e) => setServer(e.target.value)}
            />
          </Field>
          <Field label="Username">
            <input
              className="input"
              value={username}
              autoFocus
              onChange={(e) => setUsername(e.target.value)}
            />
          </Field>
          <Field label="Password">
            <input
              className="input"
              type="password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
            />
          </Field>
          {error && <p className="text-sm text-red-400">{error}</p>}
          <button
            className="btn-primary w-full"
            disabled={busy || !server || !username || !password}
          >
            {busy && <Loader2 className="h-4 w-4 animate-spin" />}
            Connect
          </button>
        </form>
        <p className="mt-3 text-center text-xs text-slate-600">
          Your login is stored securely in Windows Credential Manager.
        </p>
      </div>
    </div>
  );
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div>
      <label className="mb-1 block text-xs font-medium text-slate-400">{label}</label>
      {children}
    </div>
  );
}
