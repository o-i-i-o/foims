import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";

// FOIMS 前端构建配置：MPA 三入口（登录 / 初始化向导 / 主应用）
// 产物文件名与旧版 web/static 布局保持一致，便于 nginx 平滑切换
export default defineConfig({
  plugins: [vue()],

  server: {
    port: 5173,
    // /api 经本机 nginx（80 端口）转发至后端 UDS，与旧前端 dev 链路一致：
    // 浏览器 → Vite(5173) → nginx(80) → axum UDS
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
