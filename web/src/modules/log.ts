import {
  apiGet,
  apiPut,
} from "../utils/apiClient.js";

import {
  showToast,
  appendPaginationToTable,
  escapeHtml,
} from "../utils/ui.js";

import { t } from "../utils/i18n.js";

interface LogSearchParams {
  resource_type?: string;
  resource_id?: string;
  user_id?: string;
  action?: string;
  page?: number;
  page_size?: number;
}

let logEventsInitialized = false;

function getOperationTypeText(type: unknown): string {
  if (typeof type === "string") {
    const lowerType = type.toLowerCase();
    const translation = t(`logs.operation_types.${lowerType}`);
    if (translation !== `logs.operation_types.${lowerType}`) return translation;
    return type.split("_").map((word) => word.charAt(0).toUpperCase() + word.slice(1)).join(" ");
  }
  return String(type);
}

function getResourceTypeText(type: unknown): string {
  if (typeof type === "string") {
    const lowerType = type.toLowerCase();
    const translation = t(`logs.resource_types.${lowerType}`);
    if (translation !== `logs.resource_types.${lowerType}`) return translation;
    return type.split("_").map((word) => word.charAt(0).toUpperCase() + word.slice(1)).join(" ");
  }
  return String(type);
}

export function initLogTabs(): void {
  const logsContainer = document.getElementById("logs");
  if (!logsContainer) return;
  initLogSearch();
  const tabBtns = logsContainer.querySelectorAll(".tab-btn");
  const tabContents = logsContainer.querySelectorAll(".tab-content");
  if (!logsContainer.dataset.tabsInitialized) {
    tabBtns.forEach((btn) => {
      btn.addEventListener("click", function (this: HTMLElement) {
        const tabId = this.getAttribute("data-tab");
        if (this.classList.contains("active")) return;
        tabBtns.forEach((b) => b.classList.remove("active"));
        this.classList.add("active");
        tabContents.forEach((content) => content.classList.remove("active"));
        document.getElementById(`${tabId}-tab`)?.classList.add("active");
        if (tabId === "notifications") { loadNotificationsData(); initMacNotificationEmail(); }
        else { loadLogsData(tabId!); }
      });
    });
    logsContainer.dataset.tabsInitialized = "true";
  }
  const hash = window.location.hash;
  let targetTabId: string | null = null;
  if (hash.includes("logs")) {
    const params = new URLSearchParams(window.location.search);
    targetTabId = params.get("tab");
  }
  if (targetTabId) {
    const targetTabBtn = document.querySelector(`#logs [data-tab="${targetTabId}"]`);
    if (targetTabBtn && !targetTabBtn.classList.contains("active")) {
      tabBtns.forEach((b) => b.classList.remove("active"));
      targetTabBtn.classList.add("active");
      tabContents.forEach((content) => content.classList.remove("active"));
      document.getElementById(`${targetTabId}-tab`)?.classList.add("active");
      if (targetTabId === "notifications") { loadNotificationsData(); initMacNotificationEmail(); }
      else { loadLogsData(targetTabId); }
    }
  } else {
    let defaultTabBtn = document.querySelector("#logs .tab-btn.active") || document.querySelector("#logs .tab-btn");
    if (defaultTabBtn) {
      const defaultTabId = defaultTabBtn.getAttribute("data-tab");
      if (defaultTabId === "notifications") { loadNotificationsData(); initMacNotificationEmail(); }
      else { loadLogsData(defaultTabId!); }
    }
  }
}

function initLogSearch(): void {
  const searchInput = document.getElementById("logs-search") as HTMLInputElement | null;
  const refreshBtn = document.getElementById("refresh-logs-btn");
  if (searchInput) {
    let timeout: ReturnType<typeof setTimeout>;
    searchInput.addEventListener("input", (e) => {
      clearTimeout(timeout);
      timeout = setTimeout(() => {
        const activeTab = document.querySelector("#logs .tab-btn.active");
        if (activeTab) {
          const tabId = activeTab.getAttribute("data-tab");
          if (tabId === "operation") loadLogsData("operation", { action: (e.target as HTMLInputElement).value, page: 1 });
          else if (tabId === "login") loadLogsData("login");
        }
      }, 500);
    });
  }
  if (refreshBtn) {
    refreshBtn.addEventListener("click", () => {
      const activeTab = document.querySelector("#logs .tab-btn.active");
      if (activeTab) {
        const tabId = activeTab.getAttribute("data-tab");
        if (tabId === "notifications") loadNotificationsData();
        else { const sv = searchInput ? searchInput.value : ""; loadLogsData(tabId!, { action: sv, page: 1 }); }
      }
    });
  }
}

export async function loadLogsData(logType = "operation", searchParams: LogSearchParams = {}): Promise<void> {
  try {
    const { resource_type = '', resource_id = '', user_id = '', action = '', page = 1, page_size = 50 } = searchParams;
    let apiUrl: string;
    if (logType === "operation") {
      const params = new URLSearchParams();
      if (resource_type) params.append('resource_type', resource_type);
      if (resource_id) params.append('resource_id', resource_id);
      if (user_id) params.append('user_id', user_id);
      if (action) params.append('action', action);
      params.append('page', String(page)); params.append('page_size', String(page_size));
      apiUrl = `/api/logs/operation?${params.toString()}`;
    } else {
      const params = new URLSearchParams();
      params.append('page', String(page)); params.append('page_size', String(page_size));
      apiUrl = `/api/logs/login?${params.toString()}`;
    }
    const data = await apiGet(apiUrl);
    const tableId = `${logType}-logs-table`;
    const tbody = document.querySelector(`#${tableId} tbody`);
    if (!tbody) return;
    if (data.success) {
      let logs: Record<string, unknown>[] = [];
      let pagination: { total: number; page: number; total_pages: number; page_size: number } | null = null;
      if (Array.isArray(data.data)) { logs = data.data as Record<string, unknown>[]; }
      else if (data.data) {
        const d = data.data as Record<string, unknown>;
        if (Array.isArray(d.items)) { logs = d.items as Record<string, unknown>[]; pagination = { total: d.total as number, page: d.page as number, total_pages: d.total_pages as number, page_size: d.page_size as number }; }
        else if (Array.isArray(d.data)) { logs = d.data as Record<string, unknown>[]; pagination = { total: d.total as number, page: d.page as number, total_pages: d.total_pages as number, page_size: d.page_size as number }; }
      }
      if (logs.length > 0) {
        tbody.innerHTML = "";
        const startIndex = (page - 1) * page_size;
        logs.forEach((log, index) => {
          const row = document.createElement("tr");
          let rowHtml = `<td class="index-column">${startIndex + index + 1}</td>`;
          if (logType === "operation") {
            const opText = getOperationTypeText(log.operation_type);
            const resText = getResourceTypeText(log.resource_type);
            const resultText = log.result ? t('common.success') : t('common.failed');
            const logData = encodeURIComponent(JSON.stringify(log));
            rowHtml += `<td>${new Date(log.created_at as string).toLocaleString()}</td><td>${escapeHtml(log.username as string) || "-"}</td><td>${escapeHtml(opText)}</td><td>${escapeHtml(resText) || "-"}</td><td>${resultText}</td><td>${escapeHtml(log.ip_address as string) || "-"}</td><td><button class="btn btn-sm btn-info view-log-details" data-log="${logData}">${t('logs.details')}</button></td>`;
          } else if (logType === "login") {
            const loginResult = log.success ? t('common.success') : t('common.failed');
            const logData = encodeURIComponent(JSON.stringify(log));
            rowHtml += `<td>${new Date(log.created_at as string).toLocaleString()}</td><td>${escapeHtml(log.username as string)}</td><td>${escapeHtml(log.ip_address as string)}</td><td>${escapeHtml(log.user_agent as string) || "-"}</td><td>${loginResult}</td><td>${escapeHtml(log.error_message as string) || "-"}</td>`;
          }
          row.innerHTML = rowHtml;
          tbody.appendChild(row);
        });
        if (pagination && pagination.total_pages > 1) {
          appendPaginationToTable(`#${logType}-logs-table`, { total: pagination.total, page: pagination.page, page_size: pagination.page_size }, (p: number) => {
            const si = document.getElementById("logs-search") as HTMLInputElement | null;
            loadLogsData(logType, { action: si ? si.value : "", page: p });
          });
        }
      } else {
        const cs = logType === "operation" ? 8 : 7;
        tbody.innerHTML = `<tr class="empty-row"><td colspan="${cs}" class="text-center">${t('common.no_data')}</td></tr>`;
      }
    } else {
      const cs = logType === "operation" ? 8 : 7;
      tbody.innerHTML = `<tr class="empty-row"><td colspan="${cs}" class="text-center">${data.message || t('common.load_failed')}</td></tr>`;
    }
  } catch (error) {
    console.error("加载日志数据失败:", error);
    const tbody = document.querySelector(`#${logType}-logs-table tbody`);
    if (tbody) { const cs = logType === "operation" ? 8 : 7; tbody.innerHTML = `<tr class="empty-row"><td colspan="${cs}" class="text-center">${t('common.load_failed_retry')}</td></tr>`; }
  }
}

export async function loadNotificationsData(filterStatus = 'all', page = 1): Promise<void> {
  try {
    const params = new URLSearchParams();
    if (filterStatus && filterStatus !== 'all') params.append('status', filterStatus);
    params.append('page', String(page)); params.append('page_size', '20');
    const result = await apiGet(`/api/notifications?${params.toString()}`);
    const tbody = document.querySelector("#notifications-table tbody");
    if (!tbody) return;
    tbody.innerHTML = "";
    const dataObj = result.success ? result.data : { items: [], total: 0 };
    const notifications = (Array.isArray(dataObj) ? dataObj : (dataObj as Record<string, unknown>).items || dataObj) as Record<string, unknown>[];
    if (notifications.length > 0) {
      const startIndex = (page - 1) * 20;
      notifications.forEach((n, index) => {
        const row = document.createElement("tr");
        row.innerHTML = `<td class="index-column">${startIndex + index + 1}</td><td>${new Date(n.created_at as string).toLocaleString()}</td><td>${n.title as string}</td><td>${n.content as string}</td><td><span class="status-badge ${n.read ? "status-active" : "status-inactive"}">${n.read ? "已读" : "未读"}</span></td><td>${!n.read ? `<button class="btn btn-sm btn-primary mark-read" data-id="${n.id}">标记已读</button>` : ""}</td>`;
        tbody.appendChild(row);
      });
      const dr = dataObj as Record<string, unknown>;
      if (dr.total !== undefined) appendPaginationToTable("#notifications-table", dr as { total?: number; page?: number; page_size?: number }, (p: number) => loadNotificationsData(filterStatus, p));
    } else { tbody.innerHTML = '<tr class="empty-row"><td colspan="6" class="text-center">暂无通知数据</td></tr>'; }
  } catch (error) { console.error("加载通知数据失败:", error); const tbody = document.querySelector("#notifications-table tbody"); if (tbody) tbody.innerHTML = '<tr class="empty-row"><td colspan="6" class="text-center">加载失败，请刷新重试</td></tr>'; }
}

export async function markNotificationAsRead(notificationId: string): Promise<void> {
  try { const result = await apiPut(`/api/notifications/${notificationId}/read`, {}); if (result.success) loadNotificationsData(); else showToast("操作失败: " + result.message, "error"); }
  catch (error) { console.error("标记通知已读失败:", error); showToast("操作失败，请重试", "error"); }
}

export async function clearReadNotifications(): Promise<void> {
  if (confirm("确定要清除所有已读通知吗？")) {
    try { const result = await apiPut("/api/notifications/mark-all-read", {}); if (result.success) { loadNotificationsData(); showToast("已读通知已清除", "success"); } else showToast("操作失败: " + result.message, "error"); }
    catch (error) { console.error("清除已读通知失败:", error); showToast("操作失败，请重试", "error"); }
  }
}

export async function saveMacNotificationEmail(): Promise<void> {
  const emailInput = document.getElementById("mac-notification-email") as HTMLInputElement | null;
  const email = emailInput?.value.trim() ?? "";
  if (email) {
    if (!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email)) { showToast("请输入有效的邮箱地址", "error"); return; }
    try {
      const result = await apiGet("/api/system/smtp/config");
      if (!result.success || !result.data || !(result.data as Record<string, unknown>).host) { showToast("请先在系统设置中配置SMTP服务器", "error"); return; }
      sessionStorage.setItem("macNotificationEmail", email); showToast("MAC变动通知邮箱已保存", "success");
    } catch (error) { console.error("检查SMTP配置失败:", error); showToast("检查SMTP配置失败，请重试", "error"); }
  } else { sessionStorage.removeItem("macNotificationEmail"); showToast("MAC变动通知邮箱已清除", "info"); }
}

function initMacNotificationEmail(): void {
  const emailInput = document.getElementById("mac-notification-email") as HTMLInputElement | null;
  if (!emailInput) return;
  const savedEmail = sessionStorage.getItem("macNotificationEmail");
  if (savedEmail) emailInput.value = savedEmail;
}

function initLogEvents(): void {
  if (logEventsInitialized) return;
  logEventsInitialized = true;
  document.addEventListener("click", (e) => {
    const target = e.target as HTMLElement;
    if (target.classList.contains("mark-read")) { const id = target.getAttribute("data-id"); if (id) markNotificationAsRead(id); }
    if (target.classList.contains("view-log-details")) {
      const logData = target.getAttribute("data-log");
      if (logData) { try { showLogDetails(JSON.parse(decodeURIComponent(logData)) as Record<string, unknown>); } catch (_err) { try { showLogDetails(JSON.parse(logData) as Record<string, unknown>); } catch (_e2) { showToast("查看详情失败", "error"); } } }
    }
  });
}
initLogEvents();

function showLogDetails(log: Record<string, unknown>): void {
  const opText = getOperationTypeText(log.operation_type);
  const resText = getResourceTypeText(log.resource_type);
  const resultText = log.result ? "成功" : "失败";
  let detailsHtml = '';
  if (log.details) { try { const d = typeof log.details === 'string' ? JSON.parse(log.details) : log.details; detailsHtml = `<pre class="log-details-json">${JSON.stringify(d, null, 2)}</pre>`; } catch (_e) { detailsHtml = `<p>${log.details}</p>`; } }
  else { detailsHtml = '<p class="text-muted">无详细信息</p>'; }
  const modalHtml = `<div id="log-details-modal" class="modal modal-flex"><div class="modal-content modal-md"><div class="modal-header"><h3 class="modal-title">操作日志详情</h3><span class="close" data-action="close-modal">&times;</span></div><div class="modal-body"><div class="log-detail-row"><label>操作时间:</label><span>${new Date(log.created_at as string).toLocaleString()}</span></div><div class="log-detail-row"><label>操作人:</label><span>${log.username || "-"}</span></div><div class="log-detail-row"><label>操作类型:</label><span>${opText}</span></div><div class="log-detail-row"><label>资源类型:</label><span>${resText || "-"}</span></div><div class="log-detail-row"><label>资源ID:</label><span>${log.resource_id || "-"}</span></div><div class="log-detail-row"><label>执行结果:</label><span class="status-badge ${log.result ? 'status-active' : 'status-inactive'}">${resultText}</span></div><div class="log-detail-row"><label>IP地址:</label><span>${log.ip_address || "-"}</span></div><div class="log-detail-section"><label>详细信息:</label>${detailsHtml}</div></div><div class="modal-footer"><button class="btn btn-secondary" data-action="close-modal">关闭</button></div></div></div>`;
  document.body.insertAdjacentHTML('beforeend', modalHtml);
  const modal = document.getElementById('log-details-modal')!;
  modal.addEventListener('click', (e) => { if (e.target === modal || (e.target as HTMLElement).dataset.action === 'close-modal') modal.remove(); });
}
