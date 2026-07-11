import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useState,
  type ReactNode,
} from "react";

import { getStatus, login as apiLogin, logout as apiLogout, setUnauthorizedHandler, type Me } from "@/api";

interface AuthState {
  ready: boolean;
  needsSetup: boolean;
  authDisabled: boolean;
  user: { username: string; isAdmin: boolean } | null;
  refresh: () => Promise<void>;
  login: (username: string, password: string) => Promise<void>;
  logout: () => Promise<void>;
  setUser: (u: Me) => void;
}

const Ctx = createContext<AuthState | null>(null);

export function AuthProvider({ children }: { children: ReactNode }) {
  const [ready, setReady] = useState(false);
  const [needsSetup, setNeedsSetup] = useState(false);
  const [authDisabled, setAuthDisabled] = useState(false);
  const [user, setUserState] = useState<{ username: string; isAdmin: boolean } | null>(null);

  const refresh = useCallback(async () => {
    const s = await getStatus();
    setNeedsSetup(s.needsSetup);
    setAuthDisabled(s.authDisabled);
    setUserState(s.user);
    setReady(true);
  }, []);

  useEffect(() => {
    setUnauthorizedHandler(() => setUserState(null));
    refresh().catch(() => setReady(true));
    return () => setUnauthorizedHandler(null);
  }, [refresh]);

  const login = useCallback(async (username: string, password: string) => {
    const me = await apiLogin(username, password);
    setUserState({ username: me.username, isAdmin: me.isAdmin });
  }, []);

  const logout = useCallback(async () => {
    await apiLogout();
    setUserState(null);
  }, []);

  const setUser = useCallback((u: Me) => {
    setUserState({ username: u.username, isAdmin: u.isAdmin });
    setNeedsSetup(false);
  }, []);

  return (
    <Ctx.Provider
      value={{ ready, needsSetup, authDisabled, user, refresh, login, logout, setUser }}
    >
      {children}
    </Ctx.Provider>
  );
}

export function useAuth(): AuthState {
  const ctx = useContext(Ctx);
  if (!ctx) throw new Error("useAuth must be used within AuthProvider");
  return ctx;
}
