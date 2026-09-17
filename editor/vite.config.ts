import { defineConfig } from "vite";

// Fixed port because tauri.conf.json points the window at it, and strictPort so
// a port already in use fails loudly instead of serving the app somewhere the
// window will never look.
export default defineConfig({
  clearScreen: false,
  server: { port: 5173, strictPort: true },
  build: { target: "es2022", outDir: "dist", emptyOutDir: true },
});
