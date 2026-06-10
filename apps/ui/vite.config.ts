/// <reference types="vitest/config" />
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

const devPort = Number(process.env.TAURI_DEV_PORT ?? 5173);

const rootPkg = JSON.parse(
  readFileSync(fileURLToPath(new URL("../../package.json", import.meta.url)), "utf-8"),
) as { version: string };

export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  resolve: {
    alias: {
      "@": fileURLToPath(new URL("./src", import.meta.url)),
    },
  },
  server: {
    port: devPort,
    strictPort: true,
    host: "127.0.0.1",
    watch: {
      ignored: ["**/crates/**", "**/target/**"],
    },
  },
  test: {
    // Default stays node — store tests are DOM-free. Files that need a DOM
    // (toast TTL timers, theme DOM application) opt in per-file with a
    // `// @vitest-environment jsdom` docblock.
    environment: "node",
    setupFiles: ["./vitest.setup.ts"],
  },
  envPrefix: ["VITE_", "TAURI_"],
  define: {
    __APP_VERSION__: JSON.stringify(rootPkg.version),
  },
  build: {
    target: "esnext",
    minify: "esbuild",
    sourcemap: false,
    // three.js core is ~900 kB raw / ~245 kB gzip — irreducible and only
    // loaded lazily via Dashboard / TrackerDetail routes. Raise the
    // warning threshold so honest big vendors don't pollute build logs.
    chunkSizeWarningLimit: 1000,
    rollupOptions: {
      output: {
        manualChunks(id) {
          if (!id.includes("node_modules")) return;
          if (id.includes("@react-three")) return "three-fiber";
          if (id.includes("/three/")) return "three";
          if (id.includes("@phosphor-icons")) return "icons";
          if (id.includes("i18next") || id.includes("react-i18next")) return "i18n";
          if (id.includes("cmdk")) return "cmdk";
        },
      },
    },
  },
});
