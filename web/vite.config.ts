import { defineConfig } from "vite";
import { resolve } from "path";

export default defineConfig({
  root: "static",
  base: "/static/",
  resolve: {
    alias: {
      "@utils": resolve(__dirname, "src/utils"),
      "@types": resolve(__dirname, "src/types"),
    },
  },
  build: {
    outDir: resolve(__dirname, "dist"),
    emptyOutDir: true,
    copyPublicDir: true,
    rollupOptions: {
      input: {
        main: resolve(__dirname, "static/main.html"),
        index: resolve(__dirname, "static/index.html"),
      },
      output: {
        entryFileNames: "js/assets/[name]-[hash].js",
        chunkFileNames: "js/assets/[name]-[hash].js",
        assetFileNames: "js/assets/[name]-[hash][extname]",
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
