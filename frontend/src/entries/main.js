// 主应用入口：装载全局样式（variables 先行建立 @layer 层序：variables <
// reset < components < layouts < pages < utilities < responsive）并接线
// Pinia（auth store）、Vue Router（hash 模式 + RBAC 守卫）与
// TanStack Query（分区页数据获取统一走 useQuery）。
import { createApp } from "vue";
import { VueQueryPlugin } from "@tanstack/vue-query";
import { createPinia } from "pinia";

import "../styles/variables.css";
import "../styles/reset.css";
// components 层：与旧版 web/static/css/components 同名文件逐字一致
import "../styles/components/tooltip.css";
import "../styles/components/buttons.css";
import "../styles/components/forms.css";
import "../styles/components/tables.css";
import "../styles/components/badges.css";
import "../styles/components/cards.css";
import "../styles/toasts.css";
import "../styles/layouts/sidebar.css";
import "../styles/layouts/main-content.css";

import App from "../pages/MainShell.vue";
import { i18n } from "../i18n/index.js";
import router from "../router/index.js";

const app = createApp(App);

app.use(createPinia());
app.use(router);
app.use(i18n);
app.use(VueQueryPlugin, {
  queryClientConfig: {
    defaultOptions: {
      queries: {
        // 数据新鲜度与重试策略由各分区页按业务声明，这里只关闭
        // 全局重试（API 客户端已内置 401 刷新与 429 退避，业务失败
        // 统一以 { success: false } 归一，不抛异常）
        retry: false
      }
    }
  }
});

app.mount("#app");
