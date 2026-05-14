import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  build: {
    // Co-located with the crate that serves it; linear-clone's default
    // --web-dir points here. Keep this in sync with that default.
    outDir: "../crates/linear-clone/static",
    emptyOutDir: true,
  },
  server: {
    port: 5173,
    proxy: {
      "/graphql": "http://127.0.0.1:4000",
      "/api": "http://127.0.0.1:4000",
    },
  },
});
