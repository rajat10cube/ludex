import { useQuery } from "@tanstack/react-query";
import { Loader2 } from "lucide-react";

import { getSession } from "@/api";
import { Connect } from "@/pages/Connect";
import { Library } from "@/pages/Library";

export function App() {
  const { data: session, isLoading, refetch } = useQuery({
    queryKey: ["session"],
    queryFn: getSession,
  });

  if (isLoading) {
    return (
      <div className="flex h-full items-center justify-center text-slate-400">
        <Loader2 className="h-6 w-6 animate-spin" />
      </div>
    );
  }

  if (!session?.connected) {
    return <Connect onConnected={() => refetch()} />;
  }
  return <Library session={session} onDisconnect={() => refetch()} />;
}
