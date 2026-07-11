import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]): string {
  return twMerge(clsx(inputs));
}

export function formatBytes(bytes: number): string {
  if (!bytes) return "—";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let value = bytes;
  let i = 0;
  while (value >= 1024 && i < units.length - 1) {
    value /= 1024;
    i += 1;
  }
  return `${value.toFixed(value >= 100 || i === 0 ? 0 : 1)} ${units[i]}`;
}

export function formatPlaytime(seconds: number): string {
  if (!seconds) return "Never played";
  const hours = seconds / 3600;
  if (hours >= 1) return `${hours.toFixed(1)} h played`;
  const mins = Math.max(1, Math.round(seconds / 60));
  return `${mins} min played`;
}

export function relativeTime(iso: string | null): string {
  if (!iso) return "";
  const then = new Date(iso).getTime();
  const diff = Date.now() - then;
  const days = Math.floor(diff / 86_400_000);
  if (days <= 0) return "today";
  if (days === 1) return "yesterday";
  if (days < 30) return `${days} days ago`;
  const months = Math.floor(days / 30);
  if (months < 12) return `${months} mo ago`;
  return `${Math.floor(months / 12)} yr ago`;
}

/** Build a ludex:// deep link the Windows agent handles. */
export function ludexUri(action: "install" | "play" | "uninstall", slug: string): string {
  return `ludex://${action}/${encodeURIComponent(slug)}`;
}
