import { updatePageTranslations, t } from "./i18n.js";
import { getIcon } from "./icons.js";
import { MODULE_VERSION } from "./resourceLoader.js";
import { showToast } from "./toast.js";

const loadedModals = new Set();
const loadingModals = new Map();
const htmlCache = new Map();

// 层叠模态框计数：设备模态框上再开端口详情等场景，仅当全部关闭时恢复页面滚动
let openModalCount = 0;

// 模态框层级规则（唯一层级来源，CSS 侧不再按 DOM 相邻性判定）：
// 按打开顺序显式设置顶层元素内联 z-index —— 基准 1050（与 --z-modal 一致），
// 每多打开一层 +50（第 1 层 1050、第 2 层 1100、第 3 层 1150……），
// 保证多开时后打开的模态框一定覆盖先打开的
const MODAL_Z_INDEX_BASE = 1050;
const MODAL_Z_INDEX_STEP = 50;
// 层级封顶：超过后与顶层同 z（DOM 顺序后者居上），确保任何深度模态
// 内的 data-tooltip（--z-tooltip 3000）都不被模态压住
const MODAL_Z_INDEX_MAX = 2900;

// 模态框清单：按功能模块分组存放于 modals/ 对应子目录
const MODAL_REGISTRY = {
  // 公共
  "confirm-modal": "/static/modals/common/confirm-modal.html",
  "simple-list-modal": "/static/modals/common/simple-list-modal.html",
  "import-result-modal": "/static/modals/common/import-result-modal.html",
  // 网段模块（网络区域 / 网段编辑 / 使用详情）
  "network-region-modal": "/static/modals/network/network-region-modal.html",
  "network-modal": "/static/modals/network/network-modal.html",
  "subnet-usage-modal": "/static/modals/network/subnet-usage-modal.html",
  // IP 查询（拉取 MAC）
  "pull-mac-modal": "/static/modals/ip/pull-mac-modal.html",
  // 组织模块
  "organization-modal": "/static/modals/organization/organization-modal.html",
  "org-template-modal": "/static/modals/organization/org-template-modal.html",
  "org-template-editor-modal": "/static/modals/organization/org-template-editor-modal.html",
  "employee-modal": "/static/modals/organization/employee-modal.html",
  "employee-edit-modal": "/static/modals/organization/employee-edit-modal.html",
  // 房间 / 工位 / 机柜
  "room-modal": "/static/modals/room/room-modal.html",
  "workstation-modal": "/static/modals/workstation/workstation-modal.html",
  "cabinet-modal": "/static/modals/cabinet/cabinet-modal.html",
  "cabinet-position-modal": "/static/modals/cabinet/cabinet-position-modal.html",
  // 设备模块
  "device-modal": "/static/modals/device/device-modal.html",
  "device-template-modal": "/static/modals/device/device-template-modal.html",
  "device-port-detail-modal": "/static/modals/device/device-port-detail-modal.html",
  "unified-device-ports-modal": "/static/modals/device/unified-device-ports-modal.html",
  "port-conflict-modal": "/static/modals/device/port-conflict-modal.html",
  "arp-modal": "/static/modals/device/arp-modal.html",
  "lldp-modal": "/static/modals/device/lldp-modal.html",
  // 线路
  "cable-link-modal": "/static/modals/cable/cable-link-modal.html",
  "cable-label-print-modal": "/static/modals/cable/cable-label-print-modal.html",
  // 可视化
  "topology-connection-modal": "/static/modals/visualization/topology-connection-modal.html",
  "topology-connection-detail-modal":
    "/static/modals/visualization/topology-connection-detail-modal.html",
  "topology-container-modal": "/static/modals/visualization/topology-container-modal.html",
  "topology-detail-modal": "/static/modals/visualization/topology-detail-modal.html",
  // 日志
  "log-details-modal": "/static/modals/log/log-details-modal.html",
  "task-logs-modal": "/static/modals/log/task-logs-modal.html",
  // 系统
  "user-modal": "/static/modals/system/user-modal.html",
  "two-factor-modal": "/static/modals/system/two-factor-modal.html",
  "scheduled-task-modal": "/static/modals/system/scheduled-task-modal.html",
  "cert-generate-modal": "/static/modals/system/cert-generate-modal.html",
  "cert-import-modal": "/static/modals/system/cert-import-modal.html",
  "ca-generate-modal": "/static/modals/system/ca-generate-modal.html",
  "ca-import-modal": "/static/modals/system/ca-import-modal.html",
  "change-password-modal": "/static/modals/system/change-password-modal.html",
  "open-source-modal": "/static/modals/system/open-source-modal.html",
  // 登录页
  "forgot-password-modal": "/static/modals/auth/forgot-password-modal.html"
};

export async function fetchModalHtml(modalId) {
  const cacheKey = `${modalId}_${MODULE_VERSION}`;

  if (htmlCache.has(cacheKey)) {
    return htmlCache.get(cacheKey);
  }

  if (loadingModals.has(cacheKey)) {
    return loadingModals.get(cacheKey);
  }

  const url = MODAL_REGISTRY[modalId];
  if (!url) {
    return null;
  }

  const promise = (async () => {
    try {
      const urlWithVersion = url.includes("?")
        ? `${url}&v=${MODULE_VERSION}`
        : `${url}?v=${MODULE_VERSION}`;
      const response = await fetch(urlWithVersion);
      if (!response.ok) {
        throw new Error(`Failed to load modal template: ${response.status}`);
      }

      const html = await response.text();

      const match = html.match(/<template[^>]*>([\s\S]*?)<\/template>/);
      const innerHtml = match ? match[1].trim() : html.trim();

      htmlCache.set(cacheKey, innerHtml);
      return innerHtml;
    } catch (error) {
      console.error(`加载模态框模板失败 [${modalId}]:`, error);
      return null;
    } finally {
      loadingModals.delete(cacheKey);
    }
  })();

  loadingModals.set(cacheKey, promise);
  return promise;
}

export async function loadModal(id) {
  if (loadedModals.has(id)) {
    const existing = document.getElementById(id);
    if (existing) {
      return existing;
    }
    loadedModals.delete(id);
  }
  const innerHtml = await fetchModalHtml(id);
  if (!innerHtml) {
    showToast(t("common.load_failed"), "error");
    return null;
  }

  // fetch 期间可能已被并发调用装载完成，避免重复解析追加
  const concurrent = document.getElementById(id);
  if (concurrent) {
    return concurrent;
  }

  const container = document.createElement("div");
  container.innerHTML = innerHtml;
  const modal = container.firstElementChild;

  if (!modal) {
    return null;
  }

  document.body.appendChild(modal);
  loadedModals.add(id);

  // 静态模板中的图标按钮以 data-icon 声明图标名，注入时统一填充 SVG
  modal.querySelectorAll("button[data-icon]").forEach((btn) => {
    if (!btn.querySelector("svg")) {
      btn.insertAdjacentHTML("afterbegin", getIcon(btn.dataset.icon || ""));
    }
  });

  // 仅扫描模态框子树，避免每次开框对全文档跑多轮翻译查询
  updatePageTranslations(modal);

  return modal;
}

export async function openModal(id, title = "") {
  let modal = document.getElementById(id);

  if (!modal) {
    modal = await loadModal(id);
  }

  if (!modal) {
    return;
  }

  // 激活状态判定放在（可能的）await 之后：并发首开时两个调用方共享
  // loadModal 的同一 Promise，先恢复者完成激活，后恢复者据此跳过计数，
  // 否则 openModalCount 虚高、closeModal 归不了零，body.overflow 永久锁死
  const alreadyActive = modal.classList.contains("active");

  modal.classList.add("active");

  if (!alreadyActive) {
    openModalCount++;
    // 按当前打开层数显式指定层级：后打开的一定在上
    modal.style.zIndex = String(
      Math.min(MODAL_Z_INDEX_BASE + (openModalCount - 1) * MODAL_Z_INDEX_STEP, MODAL_Z_INDEX_MAX)
    );
  }

  if (title) {
    const titleElement = modal.querySelector(".modal-title");
    if (titleElement) {
      titleElement.textContent = title;
    }
  }

  document.body.style.overflow = "hidden";

  return modal;
}

export function closeModal(id) {
  const modal = document.getElementById(id);

  if (!modal) {
    return;
  }

  const wasActive = modal.classList.contains("active");

  modal.classList.remove("active");

  // 重置全部表单（多表单模态框只重置第一个会残留旧输入，
  // 含 hidden id，再次打开可能误走更新分支）
  modal.querySelectorAll("form").forEach((form) => {
    form.reset();
    const hiddenIdField = form.querySelector('input[type="hidden"]');
    if (hiddenIdField) {
      hiddenIdField.value = "";
    }
  });

  const ipContainers = modal.querySelectorAll('[id$="-ips-container"]');
  ipContainers.forEach((container) => {
    container.innerHTML = "";
  });

  if (wasActive) {
    openModalCount = Math.max(0, openModalCount - 1);
    // 恢复（清除）内联层级：下次打开按新的打开顺序重新计算
    modal.style.zIndex = "";
  }
  if (openModalCount === 0) {
    document.body.style.overflow = "";
  }

  modal.remove();
  loadedModals.delete(id);
}

/**
 * idle 时分批预热模态框 HTML 到内存缓存（只 fetch 不注入 DOM）。
 * 单个模态框平均不足 5KB，预热后首次打开任意弹框零网络等待；
 * 每批之间让出主线程，不与首屏资源争抢带宽。
 */
export function prefetchModalsOnIdle(delay = 2500) {
  const ids = Object.keys(MODAL_REGISTRY);
  const BATCH_SIZE = 6;
  let index = 0;

  const runBatch = () => {
    ids.slice(index, index + BATCH_SIZE).forEach((id) => {
      fetchModalHtml(id);
    });
    index += BATCH_SIZE;
    if (index < ids.length) {
      scheduleNext(runBatch, 500);
    }
  };

  const scheduleNext = (task, timeout) => {
    if (typeof requestIdleCallback === "function") {
      requestIdleCallback(task, { timeout: Math.max(timeout, 5000) });
    } else {
      setTimeout(task, timeout);
    }
  };

  scheduleNext(runBatch, delay);
}
