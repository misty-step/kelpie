import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauri dev expects the frontend on 1420 and passes TAURI_DEV_HOST when set.
const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    watch: { ignored: ["**/src-tauri/**"] },
  },
});
