// 初始化向导入口：装载全局样式（variables 先于页面样式建立 @layer 层序）
// 与根组件
import { createApp } from "vue";
import "../styles/variables.css";
import "../styles/init.css";
import "../styles/toasts.css";
import App from "../pages/InitWizard.vue";
import { i18n } from "../i18n/index.js";

createApp(App).use(i18n).mount("#app");
