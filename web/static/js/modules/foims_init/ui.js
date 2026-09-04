/**
 * FOIMS 系统初始化 - DOM 交互 / 视图层
 *
 * 只负责页面元素的显示、隐藏、切换与状态渲染，不直接发起网络请求。
 * 依赖共享状态 state（来自 ./state.js）。
 */
import { state } from "./state.js";

/**
 * 切换到指定步骤：显示对应面板，并更新顶部步骤指示器的 active / completed 标记。
 */
export const goToStep = (stepNum) => {
  document.querySelectorAll(".step-panel").forEach((panel) => {
    panel.classList.remove("active");
  });
  document.querySelector(`.step-panel[data-step="${stepNum}"]`)?.classList.add("active");
  document.querySelectorAll(".step").forEach((step) => {
    const stepData = parseInt(step.dataset.step);
    if (stepData < stepNum) {
      step.classList.add("completed");
      step.classList.remove("active");
    } else if (stepData === stepNum) {
      step.classList.add("active");
      step.classList.remove("completed");
    } else {
      step.classList.remove("active", "completed");
    }
  });
  state.currentStep = stepNum;
};

/**
 * 切换到手动重启指引视图：后端检测到程序未注册为 systemd 服务时不执行重启，
 * 初始化实际已完成，完整展示手动重启指引并隐藏自动跳转与登录入口
 * （重启前主应用路由尚未挂载，登录必然不可用）。
 */
export const showManualRestartGuide = () => {
  const restarting = document.getElementById("init-restarting");
  const manual = document.getElementById("init-manual-restart");
  const loginActions = document.getElementById("init-login-actions");
  if (restarting) {
    restarting.hidden = true;
  }
  if (manual) {
    manual.hidden = false;
  }
  if (loginActions) {
    loginActions.hidden = true;
  }
};

/**
 * showError 共用的自动隐藏定时器：新调用会取消上一次的隐藏计时，
 * 避免第一条的定时器提前隐藏紧接着显示的第二条提示
 */
let errorHideTimer = null;

/**
 * 在右上角短暂显示错误提示，5 秒后自动隐藏。
 */
export const showError = (message) => {
  const errorElement = document.getElementById("error-message");
  if (!errorElement) {
    return;
  }
  errorElement.textContent = message;
  errorElement.hidden = false;
  if (errorHideTimer) {
    clearTimeout(errorHideTimer);
  }
  errorHideTimer = setTimeout(() => {
    errorElement.hidden = true;
    errorHideTimer = null;
  }, 5000);
};

export const showLoading = () => {
  document.getElementById("loading")?.removeAttribute("hidden");
};

export const hideLoading = () => {
  document.getElementById("loading")?.setAttribute("hidden", "");
};

/**
 * 切换数据库初始化方式（新建 / 导入）。
 * 由 init_index.html 的 inline onclick 调用，因此会被 index.js 挂到 window。
 */
export const setInitMode = (mode) => {
  state.initMode = mode;
  document.querySelectorAll(".init-tab").forEach((tab) => {
    tab.classList.remove("active");
  });
  document.querySelector(`.init-tab[data-mode="${mode}"]`)?.classList.add("active");

  const createPanel = document.getElementById("create-panel");
  const importPanel = document.getElementById("import-panel");

  if (mode === "create") {
    if (createPanel) createPanel.hidden = false;
    if (importPanel) importPanel.hidden = true;
  } else {
    if (createPanel) createPanel.hidden = true;
    if (importPanel) importPanel.hidden = false;
  }
};
