import { formatDateTime } from "../utils/formatter.js";
// 日志和通知管理模块

// ES模块导入
import { apiGet, apiPut } from "../utils/apiClient.js";

import {
  showToast,
  retreatToLastPage,
  renderTable,
  appendPaginationToTable,
  escapeHtml,
  createSortState,
  updateSortIcons,
  initSortEvents
} from "../utils/ui.js";

import { t } from "../utils/i18n.js";
import { iconButton } from "../utils/icons.js";
import { showConfirm } from "../utils/confirm.js";
import { createSeqGuard, getActiveSubtab, setActiveSubtab } from "../utils/helpers.js";
import { openModal } from "../utils/modalLoader.js";

const logSortStates = {
  operation: createSortState("created_at", "desc"),
  login: createSortState("created_at", "desc")
};
const notificationTableState = createSortState("created_at", "desc");

// 登录日志 error_message 与通知 title：后端新数据存 i18n key（server.* 前缀），
// 历史数据为中文/英文原文——key 形式翻译展示，原文原样展示。
function translateServerKey(value) {
  if (typeof value === "string" && value.startsWith("server.")) {
    return t(value);
  }
  return value;
}

// 通知 content：新格式为 JSON 字符串 {"key":"...","params":{...}}，
// 历史数据为纯文本——解析成功且含 key 字段则按参数翻译，否则原样展示。
function translateNotificationContent(content) {
  if (typeof content !== "string") {
    return content;
  }
  try {
    const parsed = JSON.parse(content);
    if (parsed && typeof parsed === "object" && typeof parsed.key === "string") {
      return t(parsed.key, parsed.params || {});
    }
  } catch {
    // 历史纯文本内容，解析失败属预期，原样展示
  }
  return content;
}

// 获取操作类型文本（支持多语言）
function getOperationTypeText(type) {
  if (typeof type === "string") {
    const lowerType = type.toLowerCase();
    const translation = t(`logs.operation_types.${lowerType}`);
    if (translation !== `logs.operation_types.${lowerType}`) {
      return translation;
    }
    return type
      .split("_")
      .map((word) => word.charAt(0).toUpperCase() + word.slice(1))
      .join(" ");
  }
  return type;
}

// 获取资源类型文本（支持多语言）
function getResourceTypeText(type) {
  if (typeof type === "string") {
    const lowerType = type.toLowerCase();
    const translation = t(`logs.resource_types.${lowerType}`);
    if (translation !== `logs.resource_types.${lowerType}`) {
      return translation;
    }
    return type
      .split("_")
      .map((word) => word.charAt(0).toUpperCase() + word.slice(1))
      .join(" ");
  }
  return type;
}

// 初始化日志管理标签页
export function initLogTabs() {
  const logsContainer = document.getElementById("logs");
  if (!logsContainer) {
    return;
  }

  // 初始化搜索和刷新功能
  initLogSearch();

  const tabBtns = logsContainer.querySelectorAll(".tab-btn");
  const tabContents = logsContainer.querySelectorAll(".tab-content");

  // 检查是否已经绑定过事件
  if (!logsContainer.dataset.tabsInitialized) {
    initLogSortEvents();
    tabBtns.forEach((btn) => {
      btn.addEventListener("click", function () {
        const tabId = this.getAttribute("data-tab");
        setActiveSubtab("logs", tabId);

        // 检查是否已经是激活状态
        if (this.classList.contains("active")) {
          return; // 已经是激活状态，不需要重复处理
        }

        // 更新按钮状态
        tabBtns.forEach((b) => b.classList.remove("active"));
        this.classList.add("active");

        // 更新内容显示
        tabContents.forEach((content) => content.classList.remove("active"));
        document.getElementById(`${tabId}-tab`).classList.add("active");

        // 加载对应类型的数据
        if (tabId === "notifications") {
          loadNotificationsData();
        } else {
          loadLogsData(tabId);
        }
      });
    });
    logsContainer.dataset.tabsInitialized = "true";
  }

  // 本函数只负责纠正激活的子标签，不负责加载数据：
  // 数据加载由 navigation.js 的 whenVisible 单一入口触发（读取最终激活的标签），
  // 避免此处与 whenVisible 对同一标签双重加载
  // 检查URL中是否包含日志管理子标签信息
  const hash = window.location.hash;
  let targetTabId = null;

  if (hash.includes("logs")) {
    // 解析URL参数，查找tab参数
    const params = new URLSearchParams(window.location.search);
    targetTabId = params.get("tab");
  }

  // 如果URL中指定了子标签，仅激活该标签（数据由 whenVisible 按激活标签加载）
  if (targetTabId) {
    const targetTabBtn = document.querySelector(`#logs [data-tab="${CSS.escape(targetTabId)}"]`);
    if (targetTabBtn && !targetTabBtn.classList.contains("active")) {
      // 更新按钮状态
      tabBtns.forEach((b) => b.classList.remove("active"));
      targetTabBtn.classList.add("active");

      // 更新内容显示（getElementById 按字面匹配、不解析选择器，无需转义）
      tabContents.forEach((content) => content.classList.remove("active"));
      document.getElementById(`${targetTabId}-tab`)?.classList.add("active");
    }
    return;
  }

  // 优先恢复上次记住的子标签（刷新后停留在原标签）：
  // 仅切换激活态，不触发数据加载（由 whenVisible 统一加载）
  const savedTabId = getActiveSubtab("logs");
  const savedTabBtn = savedTabId
    ? document.querySelector(`#logs [data-tab="${CSS.escape(savedTabId)}"]`)
    : null;
  if (savedTabBtn && !savedTabBtn.classList.contains("active")) {
    tabBtns.forEach((b) => b.classList.remove("active"));
    savedTabBtn.classList.add("active");

    tabContents.forEach((content) => content.classList.remove("active"));
    document.getElementById(`${savedTabId}-tab`)?.classList.add("active");
  }
}

// 初始化日志搜索功能（搜索框位于操作日志表头最后一列）
function initLogSearch() {
  const searchInput = document.getElementById("logs-search");

  // 搜索框位于常驻 DOM：每次导航到日志页都会执行本函数，
  // 必须用 dataset 守卫防止 input 监听器无界累积（每敲一个字符触发 N 次请求）
  if (searchInput && !searchInput.dataset.searchBound) {
    searchInput.dataset.searchBound = "true";
    // 防抖搜索
    let timeout;
    searchInput.addEventListener("input", (e) => {
      clearTimeout(timeout);
      timeout = setTimeout(() => {
        const activeTab = document.querySelector("#logs .tab-btn.active");
        if (activeTab) {
          const tabId = activeTab.getAttribute("data-tab");
          if (tabId === "operation") {
            loadLogsData("operation", { action: e.target.value, page: 1 });
          }
        }
      }, 500);
    });
  }
}

// 列表请求序号:旧响应晚到时放弃渲染,防止快速切换日志类型后表格与状态错乱
const logsSeq = createSeqGuard();

// 加载日志数据（支持搜索、排序和分页）
export async function loadLogsData(logType = "operation", searchParams = {}) {
  const requestSeq = logsSeq.next();
  try {
    const {
      resource_type = "",
      resource_id = "",
      user_id = "",
      action = "",
      page = 1,
      page_size = 50,
      sort_by,
      sort_order
    } = searchParams;
    const tableState = logSortStates[logType] || logSortStates.operation;
    if (sort_by) {
      tableState.setSort(sort_by, sort_order);
    }

    let apiUrl;
    if (logType === "operation") {
      const params = new URLSearchParams();
      if (resource_type) {
        params.append("resource_type", resource_type);
      }
      if (resource_id) {
        params.append("resource_id", resource_id);
      }
      if (user_id) {
        params.append("user_id", user_id);
      }
      if (action) {
        params.append("action", action);
      }
      params.append("page", page);
      params.append("page_size", page_size);
      params.append("sort_by", tableState.sortBy);
      params.append("sort_order", tableState.sortOrder);
      apiUrl = `/api/logs/operation?${params.toString()}`;
    } else {
      const params = new URLSearchParams();
      params.append("page", page);
      params.append("page_size", page_size);
      params.append("sort_by", tableState.sortBy);
      params.append("sort_order", tableState.sortOrder);
      apiUrl = `/api/logs/login?${params.toString()}`;
    }

    const data = await apiGet(apiUrl);
    if (!logsSeq.isCurrent(requestSeq)) {
      return; // 已有更新的请求,丢弃过期响应
    }
    const tableId = `${logType}-logs-table`;
    const tbody = document.querySelector(`#${tableId} tbody`);

    if (!tbody) {
      console.error(`找不到表格元素: #${tableId} tbody`);
      return;
    }

    if (data.success) {
      let logs = [];
      let pagination = null;

      if (Array.isArray(data.data)) {
        logs = data.data;
      } else if (data.data && Array.isArray(data.data.items)) {
        // 后端分页响应统一为 items 键
        logs = data.data.items;
        pagination = {
          total: data.data.total,
          page: data.data.page,
          total_pages: data.data.total_pages,
          page_size: data.data.page_size
        };
      }

      const colSpan = logType === "operation" ? 8 : 7;
      const startIndex = (page - 1) * page_size;

      renderTable(
        `#${tableId}`,
        logs,
        (log, index) => {
          let rowHtml = `<td class="index-column">${startIndex + index + 1}</td>`;
          if (logType === "operation") {
            const operationTypeText = getOperationTypeText(log.operation_type);
            const resourceTypeText = getResourceTypeText(log.resource_type);
            const resultText = log.result ? t("common.success") : t("common.failed");

            // 安全地处理日志详情数据：整行 JSON 编码后挂到 data-log，
            // 详情弹窗解码回对象（见 view-log-details 点击委托）
            const logData = encodeURIComponent(JSON.stringify(log));

            rowHtml += `
              <td class="col-center col-time">${formatDateTime(log.created_at)}</td>
              <td class="col-operator">${escapeHtml(log.username) || "-"}</td>
              <td class="col-center">${escapeHtml(operationTypeText)}</td>
              <td class="col-center">${escapeHtml(resourceTypeText) || "-"}</td>
              <td class="col-center col-result">${resultText}</td>
              <td>${escapeHtml(log.ip_address) || "-"}</td>
              <td class="col-center">
                ${iconButton({ icon: "eye", label: t("logs.details"), cls: "btn-info view-log-details", attrs: `data-log="${logData}"` })}
              </td>
            `;
          } else {
            const loginResultText = log.success ? t("common.success") : t("common.failed");
            rowHtml += `
              <td class="col-center">${formatDateTime(log.created_at)}</td>
              <td>${escapeHtml(log.username)}</td>
              <td>${escapeHtml(log.ip_address)}</td>
              <td>${escapeHtml(log.user_agent) || "-"}</td>
              <td class="col-center">${loginResultText}</td>
              <td>${escapeHtml(translateServerKey(log.error_message)) || "-"}</td>
            `;
          }
          return rowHtml;
        },
        t("common.no_data"),
        colSpan
      );

      // 无条件渲染分页控件：单页/无分页数据时由渲染器自行清空，
      // 与其他列表保持一致，避免筛选后残留上一次的旧分页
      appendPaginationToTable(
        `#${logType}-logs-table`,
        {
          total: pagination?.total ?? logs.length,
          page: pagination?.page ?? page,
          page_size: pagination?.page_size ?? page_size,
          total_pages: pagination?.total_pages ?? 1
        },
        (p) => {
          const searchInput = document.getElementById("logs-search");
          const searchValue = searchInput ? searchInput.value : "";
          loadLogsData(logType, { action: searchValue, page: p });
        }
      );
      updateSortIcons(`${logType}-logs-table`, tableState);
    } else {
      const colSpan = logType === "operation" ? 8 : 7;
      tbody.innerHTML = `<tr class="empty-row"><td colspan="${colSpan}" class="text-center">${escapeHtml(data.message || t("common.load_failed"))}</td></tr>`;
    }
  } catch (error) {
    console.error("加载日志数据失败:", error);
    const tableId = `${logType}-logs-table`;
    const tbody = document.querySelector(`#${tableId} tbody`);

    if (tbody) {
      const colSpan = logType === "operation" ? 8 : 7;
      tbody.innerHTML = `<tr class="empty-row"><td colspan="${colSpan}" class="text-center">${t("common.load_failed_retry")}</td></tr>`;
    }
  }
}

// 加载通知数据
// 当前过滤态与页码记录在模块级：标记已读后按原状态重载，而不是无参重置
let notificationsFilterStatus = "all";
let notificationsCurrentPage = 1;

// 通知列表请求序号:旧响应晚到时放弃渲染,防止筛选/翻页并发后列表错乱
const notificationsSeq = createSeqGuard();

export async function loadNotificationsData(
  filterStatus = notificationsFilterStatus,
  page = notificationsCurrentPage,
  sortBy = null,
  sortOrder = null
) {
  const requestSeq = notificationsSeq.next();
  notificationsFilterStatus = filterStatus;
  notificationsCurrentPage = page;
  try {
    if (sortBy) {
      notificationTableState.setSort(sortBy, sortOrder);
    }

    const params = new URLSearchParams();
    if (filterStatus && filterStatus !== "all") {
      params.append("status", filterStatus);
    }
    params.append("page", page);
    params.append("page_size", 20);
    params.append("sort_by", notificationTableState.sortBy);
    params.append("sort_order", notificationTableState.sortOrder);

    const result = await apiGet(`/api/notifications?${params.toString()}`);
    if (!notificationsSeq.isCurrent(requestSeq)) {
      return; // 已有更新的请求,丢弃过期响应
    }
    const tbody = document.querySelector("#notifications-table tbody");

    if (!tbody) {
      console.error("找不到通知表格元素");
      return;
    }
    tbody.innerHTML = "";

    // 接口失败时提示并中止，不再静默渲染空列表
    if (!result.success) {
      showToast(`${t("common.load_failed")}: ${result.message}`, "error");
      return;
    }
    const data = result.data || { items: [], total: 0 };
    const notifications = data.items || data;

    // 空列表且当前页大于 1：删除后页码越界，按 total_pages 一步回退（共享判定）
    const totalPages = retreatToLastPage(notifications, page, data, 20);
    if (totalPages) {
      loadNotificationsData(filterStatus, totalPages, sortBy, sortOrder);
      return;
    }

    if (notifications.length > 0) {
      const startIndex = (page - 1) * 20;
      notifications.forEach((notification, index) => {
        const row = document.createElement("tr");
        row.innerHTML = `
          <td class="index-column">${startIndex + index + 1}</td>
          <td class="col-center">${formatDateTime(notification.created_at)}</td>
          <td>${escapeHtml(translateServerKey(notification.title))}</td>
          <td>${escapeHtml(translateNotificationContent(notification.content))}</td>
          <td class="col-center">
            <span class="status-badge ${notification.read ? "status-active" : "status-inactive"}">
              ${notification.read ? t("notifications.read") : t("notifications.unread")}
            </span>
          </td>
          <td class="col-center">
            ${!notification.read ? iconButton({ icon: "check", label: t("notifications.mark_read"), cls: "btn-primary mark-read", attrs: `data-id="${escapeHtml(notification.id)}"` }) : ""}
          </td>
        `;
        tbody.appendChild(row);
      });

      if (data.total !== undefined) {
        appendPaginationToTable("#notifications-table", data, (p) =>
          loadNotificationsData(filterStatus, p)
        );
      }
      updateSortIcons("notifications-table", notificationTableState);
    } else {
      tbody.innerHTML = `<tr class="empty-row"><td colspan="6" class="text-center">${t("notifications.no_data")}</td></tr>`;
    }
  } catch (error) {
    console.error("加载通知数据失败:", error);
    const tbody = document.querySelector("#notifications-table tbody");
    if (tbody) {
      tbody.innerHTML = `<tr class="empty-row"><td colspan="6" class="text-center">${t("common.load_failed_retry")}</td></tr>`;
    }
  }
}

// 标记通知为已读（刷新时维持当前列表）
async function markNotificationAsRead(notificationId) {
  try {
    const result = await apiPut(`/api/notifications/${notificationId}/read`, {});
    if (result.success) {
      loadNotificationsData();
    } else {
      showToast(`${t("common.operation_failed")}: ${result.message}`, "error");
    }
  } catch (error) {
    console.error("标记通知已读失败:", error);
    showToast(t("common.operation_failed_retry"), "error");
  }
}

// 全部标记已读（后端语义即为 mark-all-read）
export async function markAllNotificationsRead() {
  const confirmed = await showConfirm(t("notifications.confirm_mark_all_read"));
  if (!confirmed) {
    return;
  }
  try {
    const result = await apiPut("/api/notifications/mark-all-read", {});
    if (result.success) {
      loadNotificationsData();
      showToast(t("notifications.marked_all_read"), "success");
    } else {
      showToast(`${t("common.operation_failed")}: ${result.message}`, "error");
    }
  } catch (error) {
    console.error("标记全部已读失败:", error);
    showToast(t("common.operation_failed_retry"), "error");
  }
}

function initLogEvents() {
  if (initLogEvents.initialized) {
    return;
  }
  initLogEvents.initialized = true;

  document.addEventListener("click", (e) => {
    const markReadBtn = e.target.closest(".mark-read");
    if (markReadBtn) {
      const id = markReadBtn.getAttribute("data-id");
      if (id) {
        markNotificationAsRead(id);
      }
    }

    const detailsBtn = e.target.closest(".view-log-details");
    if (detailsBtn) {
      const logData = detailsBtn.getAttribute("data-log");
      if (logData) {
        try {
          const log = JSON.parse(decodeURIComponent(logData));
          showLogDetails(log);
        } catch (err) {
          console.error("解析日志数据失败:", err);
          try {
            const log = JSON.parse(logData);
            showLogDetails(log);
          } catch (e2) {
            console.error("解析未编码日志数据失败:", e2);
            showToast(t("logs.view_detail_failed"), "error");
          }
        }
      }
    }
  });
}

initLogEvents();

// 初始化三张表（操作日志/登录日志/通知）的表头排序事件
function initLogSortEvents() {
  const getSearchValue = () => document.getElementById("logs-search")?.value || "";

  initSortEvents("operation-logs-table", logSortStates.operation, (page, sortBy, sortOrder) =>
    loadLogsData("operation", {
      action: getSearchValue(),
      page,
      sort_by: sortBy,
      sort_order: sortOrder
    })
  );
  initSortEvents("login-logs-table", logSortStates.login, (page, sortBy, sortOrder) =>
    loadLogsData("login", { page, sort_by: sortBy, sort_order: sortOrder })
  );
  initSortEvents("notifications-table", notificationTableState, (page, sortBy, sortOrder) =>
    loadNotificationsData(notificationsFilterStatus, page, sortBy, sortOrder)
  );
}

// 显示日志详情弹窗（模板位于 modals/log/log-details-modal.html）
async function showLogDetails(log) {
  const operationTypeText = getOperationTypeText(log.operation_type);
  const resourceTypeText = getResourceTypeText(log.resource_type);
  const resultText = log.result ? t("common.success") : t("common.failed");

  const modal = await openModal("log-details-modal");
  if (!modal) {
    return;
  }

  const setText = (selector, text) => {
    const el = modal.querySelector(selector);
    if (el) {
      el.textContent = text;
    }
  };

  setText("#log-detail-time", formatDateTime(log.created_at));
  setText("#log-detail-user", log.username || "-");
  setText("#log-detail-type", operationTypeText);
  setText("#log-detail-resource-type", resourceTypeText || "-");
  setText("#log-detail-resource-id", log.resource_id ? String(log.resource_id) : "-");
  setText("#log-detail-ip", log.ip_address || "-");

  const resultEl = modal.querySelector("#log-detail-result");
  if (resultEl) {
    resultEl.innerHTML = `<span class="status-badge ${log.result ? "status-active" : "status-inactive"}">${resultText}</span>`;
  }

  const detailsEl = modal.querySelector("#log-detail-details");
  if (detailsEl) {
    if (log.details) {
      try {
        const details = typeof log.details === "string" ? JSON.parse(log.details) : log.details;
        detailsEl.innerHTML = `<pre class="log-details-json">${escapeHtml(JSON.stringify(details, null, 2))}</pre>`;
      } catch (e) {
        // details 非合法 JSON 时退回纯文本展示
        console.error("日志详情解析失败:", e);
        detailsEl.innerHTML = `<p>${escapeHtml(String(log.details))}</p>`;
      }
    } else {
      detailsEl.innerHTML = `<p class="text-muted">${t("logs.no_detail")}</p>`;
    }
  }
}
