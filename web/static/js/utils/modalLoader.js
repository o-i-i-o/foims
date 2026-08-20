import { updatePageTranslations } from "./i18n.js";
import { MODULE_VERSION } from "./resourceLoader.js";

const loadedModals = new Set();
const loadingModals = new Map();
const htmlCache = new Map();

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
  // 组织模块
  "organization-modal": "/static/modals/organization/organization-modal.html",
  "org-template-modal": "/static/modals/organization/org-template-modal.html",
  "org-template-editor-modal": "/static/modals/organization/org-template-editor-modal.html",
  // 房间 / 工位 / 机柜
  "room-modal": "/static/modals/room/room-modal.html",
  "workstation-modal": "/static/modals/workstation/workstation-modal.html",
  "cabinet-modal": "/static/modals/cabinet/cabinet-modal.html",
  "cabinet-position-modal": "/static/modals/cabinet/cabinet-position-modal.html",
  // 设备模块
  "device-modal": "/static/modals/device/device-modal.html",
  "device-template-modal": "/static/modals/device/device-template-modal.html",
  "device-ports-group-modal": "/static/modals/device/device-ports-group-modal.html",
  "device-port-detail-modal": "/static/modals/device/device-port-detail-modal.html",
  "unified-device-ports-modal": "/static/modals/device/unified-device-ports-modal.html",
  "port-conflict-modal": "/static/modals/device/port-conflict-modal.html",
  "arp-modal": "/static/modals/device/arp-modal.html",
  "lldp-modal": "/static/modals/device/lldp-modal.html",
  // 线路
  "cable-link-modal": "/static/modals/cable/cable-link-modal.html",
  // 可视化
  "topology-connection-modal": "/static/modals/visualization/topology-connection-modal.html",
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
    return null;
  }

  const container = document.createElement("div");
  container.innerHTML = innerHtml;
  const modal = container.firstElementChild;

  if (!modal) {
    return null;
  }

  const existingModal = document.getElementById(id);
  if (existingModal) {
    existingModal.remove();
  }

  document.body.appendChild(modal);
  loadedModals.add(id);

  updatePageTranslations();

  return modal;
}

export async function openModal(id, title = "") {
  let modal = document.getElementById(id);

  if (!modal) {
    modal = await loadModal(id);
  }

  if (!modal) return;

  modal.classList.add("active");

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

  if (!modal) return;

  modal.classList.remove("active");

  const form = modal.querySelector("form");
  if (form) {
    form.reset();
    const hiddenIdField = form.querySelector('input[type="hidden"]');
    if (hiddenIdField) {
      hiddenIdField.value = "";
    }
  }

  const ipContainers = modal.querySelectorAll('[id$="-ips-container"]');
  ipContainers.forEach((container) => {
    container.innerHTML = "";
  });

  document.body.style.overflow = "";

  modal.remove();
  loadedModals.delete(id);
}

export function initModalTemplates() {}

export function preloadModalsOnIdle() {}
