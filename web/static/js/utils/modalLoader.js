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

// 模态框打开时的触发元素：关闭时据此归还焦点（键值随模态元素移除而释放）
const modalTriggers = new WeakMap();

// 可聚焦元素选择器：焦点陷阱与首焦点定位共用
const FOCUSABLE_SELECTOR = [
  "a[href]",
  "button:not([disabled])",
  'input:not([disabled]):not([type="hidden"])',
  "select:not([disabled])",
  "textarea:not([disabled])",
  '[tabindex]:not([tabindex="-1"])'
].join(", ");

/**
 * 收集模态框内当前可见的可聚焦元素（display:none 的隐藏步骤/面板会被 offsetParent 过滤）
 * @param {HTMLElement} modal 模态框根元素
 * @returns {HTMLElement[]}
 */
function getModalFocusable(modal) {
  return Array.from(modal.querySelectorAll(FOCUSABLE_SELECTOR)).filter(
    (el) => el.offsetParent !== null
  );
}

/**
 * Tab 焦点陷阱：键盘焦点限制在当前模态框内首尾循环；
 * 焦点落在模态框外（如点击遮罩后）时 Tab 拉回框内
 * @param {KeyboardEvent} e
 */
function trapModalFocus(e) {
  if (e.key !== "Tab") {
    return;
  }
  const modal = e.currentTarget;
  const focusable = getModalFocusable(modal);
  if (focusable.length === 0) {
    e.preventDefault();
    return;
  }
  const first = focusable[0];
  const last = focusable[focusable.length - 1];
  const active = document.activeElement;
  if (!modal.contains(active)) {
    e.preventDefault();
    (e.shiftKey ? last : first).focus();
  } else if (e.shiftKey && active === first) {
    e.preventDefault();
    last.focus();
  } else if (!e.shiftKey && active === last) {
    e.preventDefault();
    first.focus();
  }
}

// 模态框清单：按功能模块分组存放于 modals/ 对应子目录。
// 片段分两类：
// - 标准片段（带 title/titleHtml 字段）：文件只含 .modal-body（及可选 .modal-footer），
//   外壳与 header（标题/关闭按钮）由 buildModalShell 统一生成，消除 38 份手写样板；
//   title 为 i18n key，titleHtml 为复合标题（i18n span + 动态 span），"" 表示空标题
//   （由 openModal(id, title) 填充）；
// - 自包含片段（仅 path）：自带完整外壳（confirm 自定义 content 类与 close 语义、
//   forgot-password 用登录页 .login-modal 家族、topology-detail 独立 header 结构），
//   原样注入。
const MODAL_REGISTRY = {
  // 公共
  "confirm-modal": { path: "/static/modals/common/confirm-modal.html" },
  "simple-list-modal": {
    path: "/static/modals/common/simple-list-modal.html",
    contentClass: "modal-lg",
    title: ""
  },
  "import-result-modal": {
    path: "/static/modals/common/import-result-modal.html",
    contentClass: "modal-lg",
    title: "data-management.import_result"
  },
  // 网段模块（网络区域 / 网段编辑 / 使用详情）
  "network-region-modal": {
    path: "/static/modals/network/network-region-modal.html",
    title: "network.add_region"
  },
  "network-modal": {
    path: "/static/modals/network/network-modal.html",
    title: "network.add_network"
  },
  "subnet-usage-modal": {
    path: "/static/modals/network/subnet-usage-modal.html",
    contentClass: "modal-lg",
    title: "network.usage"
  },
  // IP 详情（拉取 MAC）
  "pull-mac-modal": {
    path: "/static/modals/ip/pull-mac-modal.html",
    title: "ip.pull_mac"
  },
  // 组织模块
  "organization-modal": {
    path: "/static/modals/organization/organization-modal.html",
    title: "organization.add"
  },
  "org-template-modal": {
    path: "/static/modals/organization/org-template-modal.html",
    title: "org_template.title"
  },
  "org-template-editor-modal": {
    path: "/static/modals/organization/org-template-editor-modal.html",
    title: "org_template.editor_title"
  },
  "employee-modal": {
    path: "/static/modals/organization/employee-modal.html",
    contentClass: "modal-lg",
    titleHtml:
      '<span data-i18n="employee.manager_title"></span><span id="employee-modal-org-name" class="employee-modal-org"></span>'
  },
  "employee-edit-modal": {
    path: "/static/modals/organization/employee-edit-modal.html",
    title: "employee.add_title"
  },
  // 房间 / 工位 / 机柜
  "room-modal": {
    path: "/static/modals/room/room-modal.html",
    title: "room.add_room"
  },
  "workstation-modal": {
    path: "/static/modals/workstation/workstation-modal.html",
    title: "workstation.add_workstation"
  },
  "cabinet-modal": {
    path: "/static/modals/cabinet/cabinet-modal.html",
    title: "cabinet.add_cabinet"
  },
  "cabinet-position-modal": {
    path: "/static/modals/cabinet/cabinet-position-modal.html",
    title: "cabinet_position.add_position"
  },
  // 设备模块
  "device-modal": {
    path: "/static/modals/device/device-modal.html",
    contentClass: "modal-lg",
    title: "device.add"
  },
  "device-template-modal": {
    path: "/static/modals/device/device-template-modal.html",
    title: "device_template.title"
  },
  "device-port-detail-modal": {
    path: "/static/modals/device/device-port-detail-modal.html",
    title: "device.port_detail"
  },
  "unified-device-ports-modal": {
    path: "/static/modals/device/unified-device-ports-modal.html",
    contentClass: "modal-lg-custom",
    title: "device.unified_ports"
  },
  "port-conflict-modal": {
    path: "/static/modals/device/port-conflict-modal.html",
    title: "device.conflict_title"
  },
  "arp-modal": {
    path: "/static/modals/device/arp-modal.html",
    contentClass: "modal-xl",
    titleHtml: '<span data-i18n="device.mac_table"></span> <span id="arp-device-name"></span>'
  },
  "lldp-modal": {
    path: "/static/modals/device/lldp-modal.html",
    contentClass: "modal-xl",
    titleHtml: '<span data-i18n="device.lldp_neighbors"></span> <span id="lldp-device-name"></span>'
  },
  // 线路
  "cable-link-modal": {
    path: "/static/modals/cable/cable-link-modal.html",
    title: "cable_link.add"
  },
  "cable-label-print-modal": {
    path: "/static/modals/cable/cable-label-print-modal.html",
    title: "cable_link.print_labels"
  },
  // 可视化
  "topology-connection-modal": {
    path: "/static/modals/visualization/topology-connection-modal.html",
    title: "viz.create_connection_title"
  },
  "topology-connection-detail-modal": {
    path: "/static/modals/visualization/topology-connection-detail-modal.html",
    title: "viz.connection_detail"
  },
  "topology-container-modal": {
    path: "/static/modals/visualization/topology-container-modal.html",
    title: "viz.container_coords"
  },
  "topology-detail-modal": { path: "/static/modals/visualization/topology-detail-modal.html" },
  // 主机监控（标题栏主机名由 JS 写入 #agent-detail-title-host）
  "agent-detail-modal": {
    path: "/static/modals/agents/agent-detail-modal.html",
    contentClass: "modal-lg",
    titleHtml:
      '<span data-i18n="agents.detail_title"></span><span id="agent-detail-title-host" class="agent-title-host"></span>'
  },
  // 主机资源告警阈值配置（启用开关在 footer，保存即评估一轮）
  "agent-alert-threshold-modal": {
    path: "/static/modals/agents/agent-alert-threshold-modal.html",
    title: "agents.alert_threshold"
  },
  // 日志
  "log-details-modal": {
    path: "/static/modals/log/log-details-modal.html",
    contentClass: "modal-md",
    title: "logs.detail_title"
  },
  "task-logs-modal": {
    path: "/static/modals/log/task-logs-modal.html",
    contentClass: "modal-lg",
    title: "scheduled_tasks.logs_title"
  },
  // 系统
  "user-modal": {
    path: "/static/modals/system/user-modal.html",
    title: "user.add_user"
  },
  "scheduled-task-modal": {
    path: "/static/modals/system/scheduled-task-modal.html",
    title: "scheduled_tasks.create_task"
  },
  "cron-examples-modal": {
    path: "/static/modals/system/cron-examples-modal.html",
    title: "scheduled_tasks.cron_examples.title"
  },
  "cert-generate-modal": {
    path: "/static/modals/system/cert-generate-modal.html",
    title: "cert.generate_title"
  },
  "cert-import-modal": {
    path: "/static/modals/system/cert-import-modal.html",
    title: "cert.import_title"
  },
  "ca-generate-modal": {
    path: "/static/modals/system/ca-generate-modal.html",
    title: "cert.ca_generate_title"
  },
  "ca-import-modal": {
    path: "/static/modals/system/ca-import-modal.html",
    title: "cert.ca_import_title"
  },
  "change-password-modal": {
    path: "/static/modals/system/change-password-modal.html",
    title: "auth.change_password"
  },
  "open-source-modal": {
    path: "/static/modals/system/open-source-modal.html",
    contentClass: "modal-lg",
    title: "system.open_source_components"
  },
  // 登录页
  "forgot-password-modal": { path: "/static/modals/auth/forgot-password-modal.html" }
};

/**
 * 标准片段外壳：header（标题 + 关闭按钮）全站唯一实现，
 * 片段只提供 .modal-body 与可选 .modal-footer。
 * 标题元素保留 {id}-title 供 aria-labelledby 与 JS 按需写入；
 * data-i18n 标题由 loadModal 注入后的 updatePageTranslations 即时填充。
 */
function buildModalShell(id, entry, innerHtml) {
  const titleI18nAttr = entry.titleHtml || !entry.title ? "" : ` data-i18n="${entry.title}"`;
  const titleContent = entry.titleHtml || "";
  const contentClass = entry.contentClass ? ` ${entry.contentClass}` : "";

  return `<div id="${id}" class="modal" role="dialog" aria-modal="true" aria-labelledby="${id}-title">
  <div class="modal-content${contentClass}">
    <header class="modal-header">
      <h3 class="modal-title" id="${id}-title"${titleI18nAttr}>${titleContent}</h3>
      <button type="button" class="close" data-modal-id="${id}" aria-label="Close">&times;</button>
    </header>
${innerHtml}
  </div>
</div>`;
}

export async function fetchModalHtml(modalId) {
  const cacheKey = `${modalId}_${MODULE_VERSION}`;

  if (htmlCache.has(cacheKey)) {
    return htmlCache.get(cacheKey);
  }

  if (loadingModals.has(cacheKey)) {
    return loadingModals.get(cacheKey);
  }

  const url = MODAL_REGISTRY[modalId]?.path;
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

      // 标准片段（注册表带 title/titleHtml）由外壳函数补全 header；
      // 自包含片段原样注入
      const entry = MODAL_REGISTRY[modalId];
      const result =
        entry && ("title" in entry || "titleHtml" in entry)
          ? buildModalShell(modalId, entry, innerHtml)
          : innerHtml;

      htmlCache.set(cacheKey, result);
      return result;
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
  // await 之前捕获触发元素：模板加载期间焦点仍在调用按钮上，关闭时据此归还
  const trigger = document.activeElement instanceof HTMLElement ? document.activeElement : null;

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

    // ARIA 语义与焦点管理：自包含片段可能缺 role，打开时统一补齐；
    // 焦点移入框内并启用 Tab 陷阱，关闭时归还触发元素（见 closeModal）
    modal.setAttribute("role", "dialog");
    modal.setAttribute("aria-modal", "true");
    modalTriggers.set(modal, trigger);
    modal.addEventListener("keydown", trapModalFocus);

    const focusable = getModalFocusable(modal);
    focusable[0]?.focus();
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

  // 焦点归还触发元素：仅当焦点当前仍在本模态框内（关闭底层模态时不抢夺上层焦点）
  if (modal.contains(document.activeElement)) {
    const trigger = modalTriggers.get(modal);
    if (trigger instanceof HTMLElement && document.contains(trigger)) {
      trigger.focus();
    }
  }
  modalTriggers.delete(modal);

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
