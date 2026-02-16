// 日志和通知管理模块

// ES模块导入
import {
  apiGet,
  apiPut,
} from "../utils/apiClient.js";

import {
  showToast
} from "../utils/ui.js";

// 日志字段中文映射
const logFieldMappings = {
  // 操作类型映射
  operationTypes: {
    create_user: "创建用户",
    update_user: "更新用户",
    delete_user: "删除用户",
    create_network: "创建网络",
    update_network: "更新网络",
    delete_network: "删除网络",
    create_workstation: "添加工位",
    update_workstation: "更新工位",
    delete_workstation: "删除工位",
    create_ip: "创建IP",
    update_ip: "更新IP",
    delete_ip: "删除IP",
    pull_ip: "拉取IP",
    create_region: "创建区域",
    update_region: "更新区域",
    delete_region: "删除区域",
    create_room: "创建房间",
    update_room: "更新房间",
    delete_room: "删除房间",
    backup_config: "备份配置",
    restore_config: "恢复配置",
    // 默认映射，将下划线转换为空格并首字母大写
    default: (value) =>
      value
        .split("_")
        .map((word) => word.charAt(0).toUpperCase() + word.slice(1))
        .join(" "),
  },

  // 资源类型映射
  resourceTypes: {
    user: "用户",
    network: "网络",
    workstation: "工位",
    ip: "IP",
    region: "区域",
    room: "房间",
    config: "配置",
    // 默认映射，将下划线转换为空格并首字母大写
    default: (value) =>
      value
        .split("_")
        .map((word) => word.charAt(0).toUpperCase() + word.slice(1))
        .join(" "),
  },
};

// 获取中文操作类型
function getOperationTypeText(type) {
  if (typeof type === "string") {
    // 尝试直接匹配
    if (logFieldMappings.operationTypes[type]) {
      return logFieldMappings.operationTypes[type];
    }
    // 尝试小写匹配
    const lowerType = type.toLowerCase();
    if (logFieldMappings.operationTypes[lowerType]) {
      return logFieldMappings.operationTypes[lowerType];
    }
    // 默认处理
    return logFieldMappings.operationTypes.default(type);
  }
  return type;
}

// 获取中文资源类型
function getResourceTypeText(type) {
  if (typeof type === "string") {
    return (
      logFieldMappings.resourceTypes[type] ||
      logFieldMappings.resourceTypes.default(type)
    );
  }
  return type;
}

// 初始化日志管理标签页
export function initLogTabs() {
  const logsContainer = document.getElementById("logs");
  if (!logsContainer) return;
  
  // 初始化搜索和刷新功能
  initLogSearch();

  const tabBtns = logsContainer.querySelectorAll(".tab-btn");
  const tabContents = logsContainer.querySelectorAll(".tab-content");

  // 检查是否已经绑定过事件
  if (!logsContainer.dataset.tabsInitialized) {
    tabBtns.forEach((btn) => {
      btn.addEventListener("click", function () {
        const tabId = this.getAttribute("data-tab");

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
          initMacNotificationEmail();
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
    const targetTabBtn = document.querySelector(
      `#logs [data-tab="${targetTabId}"]`,
    );
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
          initMacNotificationEmail();
        } else {
          loadLogsData(targetTabId);
        }
      }
    }
  } else {
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
        initMacNotificationEmail();
      } else {
        loadLogsData(defaultTabId);
      }
    }
  }
}

// 初始化日志搜索功能
function initLogSearch() {
  const searchInput = document.getElementById("logs-search");
  const refreshBtn = document.getElementById("refresh-logs-btn");

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
          } else if (tabId === "login") {
            // 登录日志目前不支持搜索
            loadLogsData("login");
          }
        }
      }, 500);
    });
  }

  if (refreshBtn) {
    refreshBtn.addEventListener("click", () => {
      const activeTab = document.querySelector("#logs .tab-btn.active");
      if (activeTab) {
        const tabId = activeTab.getAttribute("data-tab");
        if (tabId === "notifications") {
          loadNotificationsData();
        } else {
          // 保留当前搜索词
          const searchValue = searchInput ? searchInput.value : "";
          loadLogsData(tabId, { action: searchValue, page: 1 });
        }
      }
    });
  }
}

// 加载日志数据（支持搜索和分页）
export async function loadLogsData(logType = "operation", searchParams = {}) {
  try {
    const { resource_type = '', resource_id = '', user_id = '', action = '', page = 1, page_size = 50 } = searchParams;
    
    let apiUrl;
    if (logType === "operation") {
      const params = new URLSearchParams();
      if (resource_type) params.append('resource_type', resource_type);
      if (resource_id) params.append('resource_id', resource_id);
      if (user_id) params.append('user_id', user_id);
      if (action) params.append('action', action);
      params.append('page', page);
      params.append('page_size', page_size);
      apiUrl = `/api/logs/operation?${params.toString()}`;
    } else {
      apiUrl = "/api/logs/login";
    }

    const data = await apiGet(apiUrl);
    const tableId = `${logType}-logs-table`;
    const tbody = document.querySelector(`#${tableId} tbody`);

    if (!tbody) {
      console.error(`找不到表格元素: #${tableId} tbody`);
      return;
    }

    if (data.success) {
      // 兼容后端直接返回数组或带分页对象的格式
      let logs = [];
      let pagination = null;
      
      if (Array.isArray(data.data)) {
        logs = data.data;
      } else if (data.data && Array.isArray(data.data.data)) {
        logs = data.data.data;
        pagination = {
          total: data.data.total,
          page: data.data.page,
          total_pages: data.data.total_pages,
          page_size: data.data.page_size
        };
      }
      
      if (logs.length > 0) {
        tbody.innerHTML = "";

        logs.forEach((log) => {
          const row = document.createElement("tr");

          let rowHtml = "";
          if (logType === "operation") {
            const operationTypeText = getOperationTypeText(log.operation_type);
            const resourceTypeText = getResourceTypeText(log.resource_type);
            const resultText = log.result ? "成功" : "失败";
            
            // 安全地处理日志详情数据
            const logData = encodeURIComponent(JSON.stringify(log));

            rowHtml = `
              <td>${new Date(log.created_at).toLocaleString()}</td>
              <td>${log.username || "-"}</td>
              <td>${operationTypeText}</td>
              <td>${resourceTypeText || "-"}</td>
              <td>${resultText}</td>
              <td>${log.ip_address || "-"}</td>
              <td>
                <button class="btn btn-sm btn-info view-log-details" data-log="${logData}">详情</button>
              </td>
            `;
          } else if (logType === "login") {
            rowHtml = `
              <td>${new Date(log.created_at).toLocaleString()}</td>
              <td>${log.username}</td>
              <td>${log.ip_address}</td>
              <td>${log.user_agent || "-"}</td>
              <td>${log.success ? "成功" : "失败"}</td>
              <td>${log.error_message || "-"}</td>
            `;
          }

          row.innerHTML = rowHtml;
          tbody.appendChild(row);
        });
        
        // 渲染分页控件
        if (pagination && pagination.total_pages > 1) {
          const colSpan = logType === 'operation' ? 7 : 6;
          const prevDisabled = pagination.page <= 1 ? 'disabled' : '';
          const nextDisabled = pagination.page >= pagination.total_pages ? 'disabled' : '';
          
          const paginationHtml = `
            <tr class="pagination-row">
              <td colspan="${colSpan}" class="pagination-cell">
                <div class="pagination-controls">
                  <span>共 ${pagination.total} 条记录，第 ${pagination.page}/${pagination.total_pages} 页</span>
                  <div class="btn-group">
                    <button class="btn btn-sm btn-secondary prev-page" ${prevDisabled} data-page="${pagination.page - 1}" data-type="${logType}">上一页</button>
                    <button class="btn btn-sm btn-secondary next-page" ${nextDisabled} data-page="${pagination.page + 1}" data-type="${logType}">下一页</button>
                  </div>
                </div>
              </td>
            </tr>
          `;
          tbody.insertAdjacentHTML('beforeend', paginationHtml);
        }
      } else {
        const colSpan = logType === "operation" ? 7 : 6;
        tbody.innerHTML = `<tr class="empty-row"><td colspan="${colSpan}" class="text-center">暂无日志数据</td></tr>`;
      }
    } else {
      const colSpan = logType === "operation" ? 7 : 6;
      tbody.innerHTML = `<tr class="empty-row"><td colspan="${colSpan}" class="text-center">${data.message || '加载失败'}</td></tr>`;
    }
  } catch (error) {
    console.error("加载日志数据失败:", error);
    const tableId = `${logType}-logs-table`;
    const tbody = document.querySelector(`#${tableId} tbody`);
    
    if (tbody) {
      const colSpan = logType === "operation" ? 7 : 6;
      tbody.innerHTML = `<tr class="empty-row"><td colspan="${colSpan}" class="text-center">加载失败，请刷新页面重试</td></tr>`;
    }
  }
}

// 加载通知数据
export async function loadNotificationsData(filterStatus = 'all') {
  try {
    const params = new URLSearchParams();
    if (filterStatus && filterStatus !== 'all') {
      params.append('status', filterStatus);
    }
    
    const data = await apiGet(`/api/notifications?${params.toString()}`);
    const tbody = document.querySelector("#notifications-table tbody");

    if (!tbody) {
      console.error("找不到通知表格元素");
      return;
    }

    if (data.success && data.data && data.data.length > 0) {
      tbody.innerHTML = "";

      data.data.forEach((notification) => {
        const row = document.createElement("tr");
        row.innerHTML = `
          <td>${new Date(notification.created_at).toLocaleString()}</td>
          <td>${notification.title}</td>
          <td>${notification.content}</td>
          <td>
            <span class="status-badge ${notification.read ? "status-active" : "status-inactive"}">
              ${notification.read ? "已读" : "未读"}
            </span>
          </td>
          <td>
            ${!notification.read ? `<button class="btn btn-sm btn-primary mark-read" data-id="${notification.id}">标记已读</button>` : ""}
          </td>
        `;
        tbody.appendChild(row);
      });
    } else {
      tbody.innerHTML = '<tr class="empty-row"><td colspan="5" class="text-center">暂无通知数据</td></tr>';
    }
  } catch (error) {
    console.error("加载通知数据失败:", error);
    const tbody = document.querySelector("#notifications-table tbody");
    if (tbody) {
      tbody.innerHTML = '<tr class="empty-row"><td colspan="5" class="text-center">加载失败，请刷新重试</td></tr>';
    }
  }
}

// 标记通知为已读
export async function markNotificationAsRead(notificationId) {
  try {
    const result = await apiPut(`/api/notifications/${notificationId}/read`, {});
    if (result.success) {
      loadNotificationsData();
    } else {
      showToast("操作失败: " + result.message, "error");
    }
  } catch (error) {
    console.error("标记通知已读失败:", error);
    showToast("操作失败，请重试", "error");
  }
}

// 清除已读通知
export async function clearReadNotifications() {
  if (confirm("确定要清除所有已读通知吗？")) {
    try {
      const result = await apiPut("/api/notifications/mark-all-read", {});
      if (result.success) {
        loadNotificationsData();
        showToast("已读通知已清除", "success");
      } else {
        showToast("操作失败: " + result.message, "error");
      }
    } catch (error) {
      console.error("清除已读通知失败:", error);
      showToast("操作失败，请重试", "error");
    }
  }
}

// 保存MAC变动通知邮箱
export async function saveMacNotificationEmail() {
  const emailInput = document.getElementById("mac-notification-email");
  const email = emailInput.value.trim();

  if (email) {
    // 校验邮件地址格式
    const emailRegex = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;
    if (!emailRegex.test(email)) {
      showToast("请输入有效的邮箱地址", "error");
      return;
    }

    // 检查SMTP配置是否存在
    try {
      const result = await apiGet("/api/system/smtp/config");
      if (!result.success || !result.data || !result.data.host) {
        showToast("请先在系统设置中配置SMTP服务器", "error");
        return;
      }

      // SMTP配置存在，保存邮箱
      sessionStorage.setItem("macNotificationEmail", email);
      showToast("MAC变动通知邮箱已保存", "success");
    } catch (error) {
      console.error("检查SMTP配置失败:", error);
      showToast("检查SMTP配置失败，请重试", "error");
    }
  } else {
    sessionStorage.removeItem("macNotificationEmail");
    showToast("MAC变动通知邮箱已清除", "info");
  }
}

// 初始化MAC变动通知邮箱输入框
function initMacNotificationEmail() {
  const emailInput = document.getElementById("mac-notification-email");
  const savedEmail = sessionStorage.getItem("macNotificationEmail");
  if (savedEmail) {
    emailInput.value = savedEmail;
  }
}

function initLogEvents() {
  if (initLogEvents.initialized) return;
  initLogEvents.initialized = true;

  document.addEventListener("click", (e) => {
    if (e.target.classList.contains("mark-read")) {
      const id = e.target.getAttribute("data-id");
      if (id) {
        markNotificationAsRead(id);
      }
    }
    
    if (e.target.classList.contains("view-log-details")) {
      const logData = e.target.getAttribute("data-log");
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
            showToast("查看详情失败", "error");
          }
        }
      }
    }

    if (e.target.classList.contains("prev-page") || e.target.classList.contains("next-page")) {
      const page = parseInt(e.target.getAttribute("data-page"));
      const type = e.target.getAttribute("data-type");
      const searchInput = document.getElementById("logs-search");
      const searchValue = searchInput ? searchInput.value : "";
      
      if (type && page > 0) {
        if (type === "operation") {
          loadLogsData(type, { action: searchValue, page: page });
        } else {
          loadLogsData(type);
        }
      }
    }
  });
}

initLogEvents();

// 显示日志详情弹窗
function showLogDetails(log) {
  const operationTypeText = getOperationTypeText(log.operation_type);
  const resourceTypeText = getResourceTypeText(log.resource_type);
  const resultText = log.result ? "成功" : "失败";
  
  let detailsHtml = '';
  if (log.details) {
    try {
      const details = typeof log.details === 'string' ? JSON.parse(log.details) : log.details;
      detailsHtml = `<pre class="log-details-json">${JSON.stringify(details, null, 2)}</pre>`;
    } catch (e) {
      detailsHtml = `<p>${log.details}</p>`;
    }
  } else {
    detailsHtml = '<p class="text-muted">无详细信息</p>';
  }
  
  const modalHtml = `
    <div id="log-details-modal" class="modal modal-flex">
      <div class="modal-content modal-md">
        <div class="modal-header">
          <h3 class="modal-title">操作日志详情</h3>
          <span class="close" data-action="close-modal">&times;</span>
        </div>
        <div class="modal-body">
          <div class="log-detail-row">
            <label>操作时间:</label>
            <span>${new Date(log.created_at).toLocaleString()}</span>
          </div>
          <div class="log-detail-row">
            <label>操作人:</label>
            <span>${log.username || "-"}</span>
          </div>
          <div class="log-detail-row">
            <label>操作类型:</label>
            <span>${operationTypeText}</span>
          </div>
          <div class="log-detail-row">
            <label>资源类型:</label>
            <span>${resourceTypeText || "-"}</span>
          </div>
          <div class="log-detail-row">
            <label>资源ID:</label>
            <span>${log.resource_id || "-"}</span>
          </div>
          <div class="log-detail-row">
            <label>执行结果:</label>
            <span class="status-badge ${log.result ? 'status-active' : 'status-inactive'}">${resultText}</span>
          </div>
          <div class="log-detail-row">
            <label>IP地址:</label>
            <span>${log.ip_address || "-"}</span>
          </div>
          <div class="log-detail-section">
            <label>详细信息:</label>
            ${detailsHtml}
          </div>
        </div>
        <div class="modal-footer">
          <button class="btn btn-secondary" data-action="close-modal">关闭</button>
        </div>
      </div>
    </div>
  `;
  
  document.body.insertAdjacentHTML('beforeend', modalHtml);
  
  const modal = document.getElementById('log-details-modal');
  modal.addEventListener('click', (e) => {
    if (e.target === modal || e.target.dataset.action === 'close-modal') {
      modal.remove();
    }
  });
}
