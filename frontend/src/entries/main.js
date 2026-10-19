// 主应用入口（Vue 重构占位：主界面壳随迁移计划后续搭建，
// 过渡期主应用继续使用旧版 web/static/main.html）
import { createApp } from "vue";
import App from "../pages/MainPlaceholder.vue";
import { i18n } from "../i18n/index.js";

createApp(App).use(i18n).mount("#app");
