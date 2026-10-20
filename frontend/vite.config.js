import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";

// FOIMS 前端构建配置：MPA 三入口（登录 / 初始化向导 / 主应用）
// 产物入口文件名沿用旧版布局（index / init_index / main），nginx 路由零改动
export default defineConfig({
  plugins: [vue()],

  server: {
    port: 5173,
    // /api 经本机 nginx（80 端口）转发至后端 UDS，与旧前端 dev 链路一致：
    // 浏览器 → Vite(5173) → nginx(80) → axum UDS
    // 拓扑画布运行时 fetch /icons/lucide/*.svg，图标位于 public/icons，
    // dev 与 build 均由 Vite 直接提供（生产 nginx /icons/ location 同映射）
    proxy: {
      "/api": {
        target: "http://localhost",
        changeOrigin: false
      }
    }
  },

  build: {
    outDir: "dist",
    rollupOptions: {
      input: {
        login: "index.html",
        init: "init_index.html",
        main: "main.html"
      }
    }
  }
});
