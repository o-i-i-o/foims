import { defineConfig } from "vite";
import { resolve } from "path";

export default defineConfig({
  root: "static",
  resolve: {
    alias: {
      "@utils": resolve(__dirname, "src/utils"),
      "@types": resolve(__dirname, "src/types"),
    },
  },
  build: {
    outDir: resolve(__dirname, "static/js"),
    emptyOutDir: false,
    copyPublicDir: false,
    rollupOptions: {
      input: {
        app: resolve(__dirname, "static/main.html"),
        login: resolve(__dirname, "static/index.html"),
      },
    },
  },
  server: {
    port: 5173,
    proxy: {
      "/api": {
        target: "https://localhost:8443",
        changeOrigin: true,
        secure: false,
      },
      "/static/js/i18n": {
        target: "https://localhost:8443",
        changeOrigin: true,
        secure: false,
      },
    },
  },
});
