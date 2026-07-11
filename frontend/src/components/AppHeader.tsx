import { Gamepad2, LogOut, Settings as SettingsIcon } from "lucide-react";
import { Link, useLocation } from "react-router-dom";

import { useAuth } from "@/auth";
import { cn } from "@/lib/utils";

export function AppHeader() {
  const { user, authDisabled, logout } = useAuth();
  const { pathname } = useLocation();
  if (!user && !authDisabled) return null;

  return (
    <header className="sticky top-0 z-20 border-b border-ink-700/70 bg-ink-950/85 backdrop-blur">
      <div className="mx-auto flex h-14 w-full max-w-[1400px] items-center gap-4 px-4 sm:px-6">
        <Link to="/" className="flex items-center gap-2 font-semibold text-white">
          <span className="flex h-8 w-8 items-center justify-center rounded-lg bg-accent-soft text-accent">
            <Gamepad2 className="h-5 w-5" />
          </span>
          Ludex
        </Link>

        <nav className="ml-2 flex items-center gap-1 text-sm">
          <NavLink to="/" active={pathname === "/"}>
            Library
          </NavLink>
          {user?.isAdmin !== false && (
            <NavLink to="/settings" active={pathname.startsWith("/settings")}>
              <SettingsIcon className="h-4 w-4" />
              Settings
            </NavLink>
          )}
        </nav>

        <div className="ml-auto flex items-center gap-3 text-sm">
          {user && <span className="hidden text-slate-400 sm:inline">{user.username}</span>}
          {!authDisabled && (
            <button
              className="btn-ghost h-8 px-2.5 py-1"
              onClick={() => logout()}
              title="Sign out"
            >
              <LogOut className="h-4 w-4" />
            </button>
          )}
        </div>
      </div>
    </header>
  );
}

function NavLink({
  to,
  active,
  children,
}: {
  to: string;
  active: boolean;
  children: React.ReactNode;
}) {
  return (
    <Link
      to={to}
      className={cn(
        "flex items-center gap-1.5 rounded-lg px-3 py-1.5 font-medium transition-colors",
        active ? "bg-ink-800 text-white" : "text-slate-400 hover:bg-ink-800/60 hover:text-slate-200",
      )}
    >
      {children}
    </Link>
  );
}
