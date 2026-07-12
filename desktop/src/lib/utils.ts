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
