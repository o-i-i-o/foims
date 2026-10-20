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
// modals 层：模态框动态列表（房间网段/工位/机柜/信息点、机柜机位/配线架）
import "../styles/components/dynamic-lists.css";
// modals 层：设备模块（网卡编辑器、统一端口管理、设备模板/ARP/LLDP 弹窗）与线缆标签打印
import "../styles/components/network-card.css";
import "../styles/components/device-ports.css";
import "../styles/components/device-modals.css";
import "../styles/components/cable-label-print.css";
import "../styles/toasts.css";
import "../styles/layouts/sidebar.css";
import "../styles/layouts/main-content.css";
// pages 层：可视化页（画布/拓扑/机位弹窗等由原生 JS 动态创建的 DOM 拿不到
// scoped 属性，页面级样式必须走全局层）
import "../styles/pages/visualization.css";
// pages 层：主机监控页与详情/阈值弹窗
import "../styles/pages/agents.css";
// pages 层：系统管理页（配置卡片/证书/服务/导入导出/通知矩阵/日志统计）
import "../styles/pages/system.css";

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
