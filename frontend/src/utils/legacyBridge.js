/**
 * 旧版命令式工具桥接层：visualization 画布生态（模块逐字迁移）所需的
 * i18n / 通知 / 确认框 / DOM 工具统一在此适配到 Vue 端设施。
 *
 * 函数体从旧版 web/static/js/utils/{i18n.js,ui.js,confirm.js,helpers.js}
 * 逐字复制，仅替换依赖来源：
 * - t → vue-i18n 实例（frontend/src/i18n/index.js，占位符 {{x}} 已归一化为 {x}）；
 * - showToast → Vue 全局通知（composables/useToast.js）；
 * - openModal/closeModal → legacyModalLoader.js（confirm-modal 的 HTML 由其提供）。
 */
import { i18n } from "../i18n/index.js";
import { showToast as vueShowToast } from "../composables/useToast.js";
import { openModal, closeModal } from "./legacyModalLoader.js";

// ==========================================
// i18n（旧版 i18n.js → vue-i18n）
// ==========================================

/**
 * 旧版 t(key, options) 的对应物：第二参为对象插值（占位符替换），
 * vue-i18n 的 t(key, named) 兼容该形态；未传参时显式给 undefined。
 */
export const t = (key, params) => i18n.global.t(key, params ?? undefined);

/** 可传入 root 元素限定扫描范围（如新打开的模态框），省去全文档遍历 */
export function updatePageTranslations(root = document) {
  const apply = (attribute, property) => {
    root.querySelectorAll(`[${attribute}]`).forEach((el) => {
      const translation = t(el.getAttribute(attribute));
      if (translation !== el.getAttribute(attribute)) {
        if (property === "dataset") {
          el.dataset.tooltip = translation;
        } else if (property === "ariaLabel") {
          el.setAttribute("aria-label", translation);
        } else {
          el[property] = translation;
        }
      }
    });
  };

  apply("data-i18n", "textContent");
  apply("data-i18n-placeholder", "placeholder");
  apply("data-i18n-title", "title");
  apply("data-i18n-tooltip", "dataset");
  apply("data-i18n-aria-label", "ariaLabel");
  apply("data-i18n-value", "value");
}

// ==========================================
// 全局通知（旧版 toast.js → Vue useToast）
// ==========================================

// 签名一致（message, type = "info", duration = 3000），直接复用 Vue 端实现
export const showToast = vueShowToast;

// ==========================================
// 错误处理（旧版 ui.js）
// ==========================================

export function handleError(error, defaultMessage = t("common.operation_failed"), fallback = null) {
  console.error("Error:", error);

  if (error.message) {
    showToast(error.message, "error");
  } else if (typeof error === "string") {
    showToast(error, "error");
  } else {
    showToast(defaultMessage, "error");
  }

  // 列表加载失败时重渲染为可重试的空表提示（部分调用方传入）
  if (typeof fallback === "function") {
    fallback();
  }
}

// ==========================================
// 确认框（旧版 confirm.js 逐字迁移）
// ==========================================

let confirmResolve = null;

// 模态框 HTML 位于 legacyModalLoader 的内联模板，首次调用时懒加载。
// closeModal 会销毁 DOM，下次打开重新注入 —— 此处随注入随绑定，
// 并用 dataset 标志防止"模态框保持打开时再次 showConfirm"造成的重复绑定。
function bindConfirmListeners(modal) {
  if (modal.dataset.confirmBound === "true") {
    return;
  }
  modal.dataset.confirmBound = "true";

  modal.querySelector("#confirm-cancel")?.addEventListener("click", () => {
    hideConfirm(false);
  });

  modal.querySelector("#confirm-ok")?.addEventListener("click", () => {
    hideConfirm(true);
  });

  // 关闭按钮不走全局 data-modal-id 逻辑，必须经由 hideConfirm 结束 Promise
  modal.querySelector("#confirm-close")?.addEventListener("click", () => {
    hideConfirm(false);
  });

  modal.addEventListener("click", (e) => {
    if (e.target === modal) {
      hideConfirm(false);
    }
  });
}

function hideConfirm(result) {
  // 统一走 closeModal（移除 active 并销毁 DOM），与全站模态框生命周期一致；
  // 背景/关闭按钮的监听在元素级触发，先于 document 委托执行，Promise 不会悬挂
  closeModal("confirm-modal");

  if (confirmResolve) {
    confirmResolve(result);
    confirmResolve = null;
  }
}

// Esc 视为取消（仅确认框激活时生效；全站其余模态暂无 Esc 约定）
document.addEventListener("keydown", (e) => {
  if (e.key !== "Escape" || !confirmResolve) {
    return;
  }
  const modal = document.getElementById("confirm-modal");
  if (modal?.classList.contains("active")) {
    hideConfirm(false);
  }
});

export function showConfirm(message, options = {}) {
  return new Promise((resolve) => {
    // 并发调用时先结束上一个（视为取消），避免其调用方永久悬挂
    if (confirmResolve) {
      confirmResolve(false);
    }
    confirmResolve = resolve;

    openModal("confirm-modal").then((modal) => {
      // openModal 竞态守卫：openModal 完成时上一个确认框可能已被
      // 新调用替换，晚到的旧回调不得覆写新确认框的文案
      if (confirmResolve !== resolve) {
        return;
      }
      if (!modal) {
        // 模板加载失败时直接返回取消，避免调用方永久挂起
        confirmResolve = null;
        resolve(false);
        return;
      }

      bindConfirmListeners(modal);

      const messageEl = modal.querySelector("#confirm-message");
      if (messageEl) {
        messageEl.textContent = message;
      }

      const titleEl = modal.querySelector(".modal-title");
      if (titleEl && options.title) {
        titleEl.textContent = options.title;
      }

      const okBtn = modal.querySelector("#confirm-ok");
      if (okBtn) {
        okBtn.textContent = options.confirmText || t("common.confirm");
        okBtn.className = `btn ${options.danger ? "btn-danger" : "btn-primary"}`;
      }

      const cancelBtn = modal.querySelector("#confirm-cancel");
      if (cancelBtn) {
        cancelBtn.textContent = options.cancelText || t("common.cancel");
      }
    });
  });
}

/** 与旧版 confirm.js 一致的删除确认封装（默认危险按钮文案） */
export async function confirmDelete(entityName, options = {}) {
  const message = options.message || t("common.confirm_delete", { name: entityName });
  return showConfirm(message, {
    title: t("common.delete"),
    confirmText: t("common.delete"),
    danger: true,
    ...options
  });
}

// ==========================================
// DOM / 调度工具（旧版 helpers.js 逐字迁移）
// ==========================================

export function nextFrame(callback) {
  return requestAnimationFrame(() => {
    requestAnimationFrame(callback);
  });
}

class ElementCache {
  constructor() {
    this.cache = new Map();
  }

  get(id) {
    const cached = this.cache.get(id);
    if (cached && document.contains(cached)) {
      return cached;
    }
    this.cache.delete(id);
    const el = document.getElementById(id);
    if (el) {
      this.cache.set(id, el);
    }
    return el;
  }

  getValue(id) {
    const el = this.get(id);
    return el ? el.value : "";
  }

  setValue(id, value) {
    const el = this.get(id);
    if (el) {
      el.value = value;
    }
  }

  getChecked(id) {
    const el = this.get(id);
    return el ? el.checked : false;
  }

  setChecked(id, checked) {
    const el = this.get(id);
    if (el) {
      el.checked = checked;
    }
  }

  clear(id) {
    if (id) {
      this.cache.delete(id);
    } else {
      this.cache.clear();
    }
  }
}

// 纯字符串转义：不创建临时 DOM 元素，且较 textContent 方案多覆盖引号（属性场景同样安全）
const HTML_ESCAPE_MAP = {
  "&": "&amp;",
  "<": "&lt;",
  ">": "&gt;",
  '"': "&quot;",
  "'": "&#39;"
};

export function escapeHtml(text) {
  if (!text) {
    return "";
  }
  return String(text).replace(/[&<>"']/g, (ch) => HTML_ESCAPE_MAP[ch]);
}

export const elementCache = new ElementCache();

// ==========================================
// 子标签页持久化（刷新后停留在当前子标签）
// ==========================================

const SUBTAB_STORAGE_PREFIX = "foims_subtab_";

/**
 * 记住某个页面当前激活的子标签
 * @param {string} pageId - 顶层页面 ID（如 "resources"、"logs"）
 * @param {string} tabId - 子标签 ID（如 "devices"）
 */
export function setActiveSubtab(pageId, tabId) {
  if (!pageId || !tabId) {
    return;
  }
  try {
    localStorage.setItem(SUBTAB_STORAGE_PREFIX + pageId, tabId);
  } catch (e) {
    // localStorage 不可用时记录并跳过，不影响页面功能
    console.error(`保存子标签偏好失败（${pageId}）:`, e);
  }
}

/**
 * 读取某个页面上次记住的子标签
 * @param {string} pageId - 顶层页面 ID
 * @returns {string|null} 子标签 ID，未记录或不可用时返回 null
 */
export function getActiveSubtab(pageId) {
  if (!pageId) {
    return null;
  }
  try {
    return localStorage.getItem(SUBTAB_STORAGE_PREFIX + pageId);
  } catch (e) {
    console.error(`读取子标签偏好失败（${pageId}）:`, e);
    return null;
  }
}

/**
 * 请求竞态守卫：列表请求前取号，响应返回后校验是否仍是最新请求，
 * 过期响应直接放弃渲染（翻页/排序/筛选并发后防止表格与状态错乱）。
 * 替代各列表模块各自维护的 `let xRequestSeq = 0` 三行样板。
 */
export function createSeqGuard() {
  let current = 0;
  return {
    /** 开始一次新请求，返回本次序号 */
    next() {
      current += 1;
      return current;
    },
    /** 序号是否仍是最新（过期响应返回 false） */
    isCurrent(seq) {
      return seq === current;
    }
  };
}

// ==========================================
// 防抖（旧版 ui.js）
// ==========================================

export function debounce(func, wait) {
  let timeout;
  return function executedFunction(...args) {
    const later = () => {
      clearTimeout(timeout);
      func(...args);
    };
    clearTimeout(timeout);
    timeout = setTimeout(later, wait);
  };
}

// ==========================================
// Vue 弹窗注入点
// ==========================================

// 旧版 visualizationManager 从 workstation.js/cabinet.js/position.js 导入的
// 编辑弹窗入口。ES module 导出绑定只读、无法直接覆写，因此以可变 holder
// 承载实现，VisualizationPage.vue 在挂载时对 holder 赋值注入 Vue 弹窗；
// 未注入前为空实现，画布编辑回调调用时不产生副作用。
export const vueModals = {
  editWorkstation: () => {},
  editCabinet: () => {},
  editCabinetPosition: () => {}
};

// 委托包装：调用时才解析 holder 内的当前实现（visualizationManager
// 按旧版形态导入同名具名导出，迁移代码无需改动）
export const editWorkstation = (...args) => vueModals.editWorkstation(...args);
export const editCabinet = (...args) => vueModals.editCabinet(...args);
export const editCabinetPosition = (...args) => vueModals.editCabinetPosition(...args);
