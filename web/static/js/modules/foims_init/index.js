/**
 * FOIMS 系统初始化 - 入口模块
 *
 * 由 init_index.html 以 <script type="module"> 加载。
 * 负责在 DOM 就绪后绑定事件并触发首次检查。
 */
import { state } from "./state.js";
import { goToStep, setInitMode } from "./ui.js";
import {
  checkPostgreSQL,
  checkDatabaseStatus,
  handleDbConfigSubmit,
  handleDbCreate,
  handleInitModeSubmit,
  handleAdminAccountSubmit,
  getVerificationCode
} from "./api.js";
import { initI18n, changeLanguage } from "../../utils/i18n.js";

document.addEventListener("DOMContentLoaded", async () => {
  await initI18n();

  // 语言切换：项目约定先按浏览器语言应用，再由页面上的语言按钮切换。
  document.getElementById("language-selector")?.addEventListener("change", (e) => {
    changeLanguage(e.target.value);
  });
  // 静态 data-i18n 文本由 changeLanguage 自动刷新；但向导里由 api.js 动态
  // 渲染的状态块（PostgreSQL 检查 / 数据库状态）需要重新拉取才能换语言。
  window.addEventListener("languagechange", () => {
    checkPostgreSQL();
    if (state.currentStep >= 3) {
      checkDatabaseStatus();
    }
  });

  document.getElementById("pg-recheck-btn")?.addEventListener("click", () => {
    location.reload();
  });

  document.querySelectorAll(".init-tab").forEach((tab) => {
    tab.addEventListener("click", () => {
      setInitMode(tab.dataset.mode);
    });
  });

  document.querySelectorAll(".get-captcha-btn").forEach((button) => {
    button.addEventListener("click", () => {
      getVerificationCode();
    });
  });

  // 第 2 步：数据库配置页（连接测试充当下一步 + 创建数据库）
  document.getElementById("db-config-form")?.addEventListener("submit", handleDbConfigSubmit);
  document.getElementById("db-create-btn")?.addEventListener("click", handleDbCreate);

  document.getElementById("init-mode-form")?.addEventListener("submit", handleInitModeSubmit);
  document
    .getElementById("admin-account-form")
    ?.addEventListener("submit", handleAdminAccountSubmit);

  document.querySelectorAll(".prev-step").forEach((button) => {
    button.addEventListener("click", () => {
      goToStep(state.currentStep - 1);
    });
  });

  document.querySelectorAll(".step").forEach((step) => {
    step.addEventListener("click", () => {
      const stepNum = parseInt(step.dataset.step);
      if (stepNum < state.currentStep) {
        goToStep(stepNum);
      }
    });
  });

  document
    .querySelector('.step-panel[data-step="1"] .btn-success')
    ?.addEventListener("click", () => {
      // PostgreSQL 检查通过：进入数据库配置页（第 2 步），
      // 连接信息由用户在页面填写，不再直接探测默认配置
      goToStep(2);
    });

  await checkPostgreSQL();
});
