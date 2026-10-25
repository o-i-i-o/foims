// 临时冒烟配置（仅本地验证用，验证后删除）：本机 nginx 的 80 端口对任何请求
// 301 到 https，导致默认 vite dev 代理拿不到响应，这里直接把 /api 代理到
// https://localhost 并跳过自签名证书校验，请求仍经 vite 同源转发（CSP 不受影响）。
import { defineConfig, mergeConfig } from "vite";
import base from "./vite.config.js";

export default mergeConfig(
  base,
  defineConfig({
    server: {
      port: 5174,
      proxy: {
        "/api": {
          target: "https://localhost",
          changeOrigin: true,
          secure: false,
        },
      },
    },
  }),
);
