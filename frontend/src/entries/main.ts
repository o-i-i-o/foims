// 主应用入口：装载全局样式（variables 先行建立 @layer 层序：variables <
// reset < components < modals < layouts < pages < responsive）并接线
// Pinia（auth store）、Vue Router（hash 模式 + RBAC 守卫）与
// TanStack Query（分区页数据获取统一走 useQuery）。
import { createApp } from "vue";
import { VueQueryPlugin } from "@tanstack/vue-query";
import { createPinia } from "pinia";

import "../styles/variables.css";
import "../styles/reset.css";
// components 层：共享组件样式（tooltip/按钮/表单/表格/徽标/卡片）
import "../styles/components/tooltip.css";
import "../styles/components/buttons.css";
import "../styles/components/forms.css";
import "../styles/components/tables.css";
import "../styles/components/badges.css";
import "../styles/components/cards.css";
// modals 层：模态框外壳（遮罩/页头/页脚/关闭钮，ModalShell.vue 的样式来源）
import "../styles/components/modals.css";
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
// pages 层：可视化页（v-html 注入的图标与跨组件共享的画布/拓扑/机位弹窗
// DOM 拿不到 scoped 属性，页面级样式必须走全局层）
import "../styles/pages/visualization.css";
// pages 层：主机监控页与详情/阈值弹窗
import "../styles/pages/agents.css";
// pages 层：系统管理页（配置卡片/证书/服务/导入导出/通知矩阵/日志统计）
import "../styles/pages/system.css";
// pages 层：组织管理页（树节点/徽章配色/模板编辑器，被递归子组件与弹窗跨组件命中）
import "../styles/pages/organization.css";

import App from "../pages/MainShell.vue";
import { i18n } from "../i18n/index";
import router from "../router/index";

const app = createApp(App);

app.use(createPinia());
app.use(router);
app.use(i18n);
app.use(VueQueryPlugin, {
  queryClientConfig: {
    defaultOptions: {
      queries: {
        // 数据新鲜度策略由各分区页按业务声明；全局允许 1 次重试：
        // IDE 预览面板（iframe）挂起恢复瞬间会在途请求被浏览器中止
        // （net::ERR_ABORTED → Failed to fetch），恢复后的立即重试时
        // 渲染帧已激活可成功；真实 API 失败多试一次幂等 GET 无副作用，
        // 写操作走 mutation（默认不重试）不受影响
        retry: 1
      }
    }
  }
});

app.mount("#app");
