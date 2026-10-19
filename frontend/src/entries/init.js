// 初始化向导入口（Vue 重构占位：向导页面按迁移计划后续移植，
// 过渡期初始化流程继续使用旧版 web/static/init_index.html）
import { createApp } from "vue";
import App from "../pages/InitPlaceholder.vue";
import { i18n } from "../i18n/index.js";

createApp(App).use(i18n).mount("#app");
