import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
// @ts-ignore - node built-ins; this file is type-checked without @types/node.
import { existsSync, readFileSync } from "node:fs";
// @ts-ignore
import { homedir, platform } from "node:os";
// @ts-ignore
import { env } from "node:process";
// @ts-ignore
import { join } from "node:path";

// Dev-only: read the admin token the desktop launcher would inject via ?token=xxx
// so that `npm run dev` against the local control plane (127.0.0.1:18788) does not
// get 401 on mutating endpoints. Mirrors ui_url_with_token() in main.rs and the
// data-dir resolution in crates/veil/src/data_dir.rs.
function devAdminToken(): string | undefined {
  const override = env.VEIL_DATA_DIR || env.DGW_DATA_DIR;
  const dir = (() => {
    if (override) return override;
    if (platform() === "win32") {
      return join(env.LOCALAPPDATA || homedir(), "veil");
    }
    if (platform() === "darwin") {
      return join(homedir(), "Library", "Application Support", "veil");
    }
    return join(env.XDG_DATA_HOME || join(homedir(), ".local", "share"), "veil");
  })();
  const veil = dir;
  const legacy = join(veil, "..", "dgw");
  const resolved = existsSync(veil) || !existsSync(legacy) ? veil : legacy;
  try {
    const raw = readFileSync(join(resolved, "admin.token"), "utf8");
    const t = raw.trim();
    return t || undefined;
  } catch {
    return undefined;
  }
}

export default defineConfig({
  plugins: [react()],
  server: {
    proxy: {
      "/api": {
        target: "http://127.0.0.1:18788",
        configure: (proxy) => {
          const token = devAdminToken();
          if (!token) return;
          proxy.on("proxyReq", (req) => {
            // Only inject when the page did not already provide one (e.g. a
            // manually entered token in the sidebar), so reset/rotate flows
            // can still be exercised against the dev server.
            if (!req.getHeader("x-veil-admin-token")) {
              req.setHeader("x-veil-admin-token", token);
            }
          });
        },
      },
    },
  },
  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: "./src/setup.ts",
  },
});
