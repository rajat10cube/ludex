import path from "node:path";

import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

// Tauri serves this build; dev server runs on a fixed port it can point at.
export default defineConfig({
  plugins: [react()],
  resolve: { alias: { "@": path.resolve(__dirname, "./src") } },
  clearScreen: false,
  server: { port: 5174, strictPort: true },
  build: { outDir: "dist", emptyOutDir: true, target: "chrome110" },
});
