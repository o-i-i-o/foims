// 日志和通知管理模块

// ES模块导入
import { apiGet, apiPut } from "../utils/apiClient.js";

import {
  showToast,
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
import { setActiveSubtab, getActiveSubtab } from "../utils/helpers.js";
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

  // 检查URL中是否包含日志管理子标签信息
  const hash = window.location.hash;
  let targetTabId = null;

  if (hash.includes("logs")) {
    // 解析URL参数，查找tab参数
    const params = new URLSearchParams(window.location.search);
    targetTabId = params.get("tab");
  }

  // 如果URL中指定了子标签，激活该标签
  if (targetTabId) {
    const targetTabBtn = document.querySelector(`#logs [data-tab="${targetTabId}"]`);
    if (targetTabBtn) {
      // 检查是否已经是激活状态
      if (!targetTabBtn.classList.contains("active")) {
        // 更新按钮状态
        tabBtns.forEach((b) => b.classList.remove("active"));
        targetTabBtn.classList.add("active");

        // 更新内容显示
        tabContents.forEach((content) => content.classList.remove("active"));
        document.getElementById(`${targetTabId}-tab`).classList.add("active");

        // 加载对应类型的数据
        if (targetTabId === "notifications") {
          loadNotificationsData();
        } else {
          loadLogsData(targetTabId);
        }
      }
    }
  } else {
    // 优先恢复上次记住的子标签（刷新后停留在原标签）
    const savedTabId = getActiveSubtab("logs");
    const savedTabBtn = savedTabId
      ? document.querySelector(`#logs [data-tab="${CSS.escape(savedTabId)}"]`)
      : null;
    if (savedTabBtn && !savedTabBtn.classList.contains("active")) {
      // 复用点击逻辑完成激活与数据加载
      savedTabBtn.click();
      return;
    }

    // 如果URL中没有指定子标签，为默认选中的标签加载数据
    // 找到默认选中的标签按钮（通常是第一个或带有active类的）
    let defaultTabBtn = document.querySelector("#logs .tab-btn.active");
    if (!defaultTabBtn) {
      // 如果没有默认选中的标签，选择第一个
      defaultTabBtn = document.querySelector("#logs .tab-btn");
    }

    if (defaultTabBtn) {
      const defaultTabId = defaultTabBtn.getAttribute("data-tab");
      // 根据默认标签加载对应数据
      if (defaultTabId === "notifications") {
        loadNotificationsData();
      } else {
        loadLogsData(defaultTabId);
      }
    }
  }
}

// 初始化日志搜索功能（搜索框位于操作日志表头最后一列）
function initLogSearch() {
  const searchInput = document.getElementById("logs-search");

  if (searchInput) {
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

// 加载日志数据（支持搜索、排序和分页）
export async function loadLogsData(logType = "operation", searchParams = {}) {
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
              <td class="col-center col-time">${new Date(log.created_at).toLocaleString()}</td>
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
              <td class="col-center">${new Date(log.created_at).toLocaleString()}</td>
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

      // 渲染分页控件
      if (pagination && pagination.total_pages > 1) {
        appendPaginationToTable(
          `#${logType}-logs-table`,
          {
            total: pagination.total,
            page: pagination.page,
            page_size: pagination.page_size,
            total_pages: pagination.total_pages
          },
          (p) => {
            const searchInput = document.getElementById("logs-search");
            const searchValue = searchInput ? searchInput.value : "";
            loadLogsData(logType, { action: searchValue, page: p });
          }
        );
      }
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
export async function loadNotificationsData(
  filterStatus = "all",
  page = 1,
  sortBy = null,
  sortOrder = null
) {
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
    const tbody = document.querySelector("#notifications-table tbody");

    if (!tbody) {
      console.error("找不到通知表格元素");
      return;
    }
    tbody.innerHTML = "";

    const data = result.success ? result.data : { items: [], total: 0 };
    const notifications = data.items || data;

    if (notifications.length > 0) {
      const startIndex = (page - 1) * 20;
      notifications.forEach((notification, index) => {
        const row = document.createElement("tr");
        row.innerHTML = `
          <td class="index-column">${startIndex + index + 1}</td>
          <td class="col-center">${new Date(notification.created_at).toLocaleString()}</td>
          <td>${escapeHtml(translateServerKey(notification.title))}</td>
          <td>${escapeHtml(translateNotificationContent(notification.content))}</td>
          <td class="col-center">
            <span class="status-badge ${notification.read ? "status-active" : "status-inactive"}">
              ${notification.read ? t("notifications.read") : t("notifications.unread")}
            </span>
          </td>
          <td class="col-center">
            ${!notification.read ? iconButton({ icon: "check", label: t("notifications.mark_read"), cls: "btn-primary mark-read", attrs: `data-id="${notification.id}"` }) : ""}
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

// 标记通知为已读（刷新时保留当前过滤视图）
async function markNotificationAsRead(notificationId) {
  try {
    const result = await apiPut(`/api/notifications/${notificationId}/read`, {});
    if (result.success) {
      const filter = document.getElementById("notifications-filter")?.value || "all";
      loadNotificationsData(filter);
    } else {
      showToast(`${t("common.operation_failed")}: ${result.message}`, "error");
    }
  } catch (error) {
    console.error("标记通知已读失败:", error);
    showToast(t("common.operation_failed_retry"), "error");
  }
}

// 全部标记已读（后端语义即为 mark-all-read；刷新时保留当前过滤视图）
export async function markAllNotificationsRead() {
  const confirmed = await showConfirm(t("notifications.confirm_mark_all_read"));
  if (!confirmed) {
    return;
  }
  try {
    const result = await apiPut("/api/notifications/mark-all-read", {});
    if (result.success) {
      const filter = document.getElementById("notifications-filter")?.value || "all";
      loadNotificationsData(filter);
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
  const getNotificationFilter = () =>
    document.getElementById("notifications-filter")?.value || "all";

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
    loadNotificationsData(getNotificationFilter(), page, sortBy, sortOrder)
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

  setText("#log-detail-time", new Date(log.created_at).toLocaleString());
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
