import {
  apiRequest,
  apiGet,
  apiPut,
  apiPost,
} from "../utils/apiClient.js";

import {
  showToast,
  escapeHtml,
} from "../utils/ui.js";

import { loadModal } from "../utils/modalLoader.js";
import { t } from "../utils/i18n.js";
import { loadUsersData } from "./userManager.js";
import { initSecurityTab } from "./fail2banManager.js";
import { elementCache } from "../utils/helpers.js";
import { showConfirm } from "../utils/confirm.js";

// 初始化系统管理标签页
export function initSystemTabs() {
  const systemContainer = elementCache.get("system");
  if (!systemContainer) return;
  
  const tabBtns = systemContainer.querySelectorAll(".tab-btn[data-tab]");
  const tabContents = systemContainer.querySelectorAll(".tab-content");

  if (!systemContainer.dataset.tabsInitialized) {
    tabBtns.forEach((btn) => {
      btn.addEventListener("click", function () {
        const tabId = this.getAttribute("data-tab");

        tabBtns.forEach((b) => b.classList.remove("active"));
        this.classList.add("active");

        tabContents.forEach((content) => content.classList.remove("active"));
        const tabContentElement = elementCache.get(`${tabId}-tab`);
        if (tabContentElement) {
          tabContentElement.classList.add("active");
        }

        if (tabId === "users") {
          loadUsersData();
        } else if (tabId === "system-info") {
          loadSystemInfo();
          loadSystemConfig();
        } else if (tabId === "system-smtp") {
          loadSmtpConfig();
          loadNotificationSettings();
        } else if (tabId === "import-export") {
          loadLogsStats();
        } else if (tabId === "scheduled-tasks") {
          initScheduledTasksTab();
        } else if (tabId === "security") {
          initSecurityTab();
        }
      });
    });
    systemContainer.dataset.tabsInitialized = "true";
  }

  if (systemContainer.dataset.eventsInitialized === "true") return;

  const systemConfigForm = elementCache.get("system-config-form");
  if (systemConfigForm) {
    systemConfigForm.addEventListener("submit", async (e) => {
      e.preventDefault();
      try {
        await saveSystemConfig();
      } catch (err) {
        console.error("saveSystemConfig error:", err);
      }
    });
  }

  const systemConfigTab = document.querySelector('[data-tab="system-config"]');
  if (systemConfigTab) {
    systemConfigTab.addEventListener("click", async () => {
      setTimeout(async () => {
        await loadSystemConfig();
      }, 100);
    });
  }

  const smtpConfigForm = elementCache.get("smtp-config-form");
  if (smtpConfigForm) {
    smtpConfigForm.addEventListener("submit", async (e) => {
      e.preventDefault();
      await saveSmtpConfig();
    });
  }

  const saveNotificationBtn = elementCache.get("save-notification-settings-btn");
  if (saveNotificationBtn) {
    saveNotificationBtn.addEventListener("click", saveNotificationSettings);
  }

  const clearLogsBtn = elementCache.get("clear-logs-btn");
  if (clearLogsBtn) {
    clearLogsBtn.addEventListener("click", clearLogs);
  }

  systemContainer.dataset.eventsInitialized = "true";
}

// 格式化系统运行时间（秒 -> X天X小时X分钟X秒）
function formatUptime(seconds) {
  const days = Math.floor(seconds / (24 * 60 * 60));
  seconds %= 24 * 60 * 60;
  const hours = Math.floor(seconds / (60 * 60));
  seconds %= 60 * 60;
  const minutes = Math.floor(seconds / 60);
  seconds %= 60;

  let result = "";
  if (days > 0) result += `${days}天`;
  if (hours > 0) result += `${hours}小时`;
  if (minutes > 0) result += `${minutes}分钟`;
  result += `${seconds}秒`;

  return result;
}



// 保存系统配置
let currentServerConfig = null;

async function saveSystemConfig() {
  try {
    const serverConfig = {
      ...currentServerConfig,
      page_timeout: parseInt(elementCache.getValue("page-timeout")) || 30,
      public_url: elementCache.getValue("public-url") || ""
    };

    const rateLimitConfig = {
      enabled: elementCache.getChecked("rate-limit-enabled"),
      ip_limit: parseInt(elementCache.getValue("rate-limit-ip")) || 100,
      user_limit: parseInt(elementCache.getValue("rate-limit-user")) || 200,
      login_limit: parseInt(elementCache.getValue("rate-limit-login")) || 5,
      window_secs: parseInt(elementCache.getValue("rate-limit-window")) || 60,
      email_limit: parseInt(elementCache.getValue("rate-limit-email")) || 5,
      email_window_secs: parseInt(elementCache.getValue("rate-limit-email-window")) || 3600
    };

    const config = { 
      server: serverConfig,
      rate_limit: rateLimitConfig 
    };

    const result = await apiPut("/api/system/config", config);
    
    if (result.success) {
      await loadSystemConfig();
      showToast("系统配置保存成功", "success");
      showToast("配置已更新，需要重启应用系统以使配置生效", "warning");
      sessionStorage.setItem("configUpdated", "true");
    } else {
      showToast("系统配置保存失败: " + result.message, "error");
    }
  } catch (error) {
    console.error("保存系统配置失败:", error);
    showToast("系统配置保存失败: " + error.message, "error");
  }
}

// 检查并显示配置更新后的重启提示
function checkConfigUpdateRestartPrompt() {
  // 检查是否有配置更新标记
  if (sessionStorage.getItem("configUpdated") === "true") {
    // 显示重启提示
    showToast("配置已更新，需要重启应用系统以使配置生效", "warning");
  }
}

// 清除配置更新标记（在重启后调用）
function clearConfigUpdateFlag() {
  sessionStorage.removeItem("configUpdated");
}

// 加载系统配置
export async function loadSystemConfig() {
  try {
    const result = await apiGet("/api/system/config");
    if (result.success) {
      const config = result.data;
      
      currentServerConfig = config.server;
      
      elementCache.setValue("page-timeout", config.server.page_timeout || 30);
      elementCache.setValue("public-url", config.server.public_url || "");
      
      // 加载速率限制配置
      if (config.rate_limit) {
        elementCache.get("rate-limit-enabled").checked = config.rate_limit.enabled !== false;
        elementCache.setValue("rate-limit-ip", config.rate_limit.ip_limit || 100);
        elementCache.setValue("rate-limit-user", config.rate_limit.user_limit || 200);
        elementCache.setValue("rate-limit-login", config.rate_limit.login_limit || 5);
        elementCache.setValue("rate-limit-window", config.rate_limit.window_secs || 60);
        elementCache.setValue("rate-limit-email", config.rate_limit.email_limit || 5);
        elementCache.setValue("rate-limit-email-window", config.rate_limit.email_window_secs || 3600);
      }
      
      checkConfigUpdateRestartPrompt();
    }
  } catch (error) {
    console.error("加载系统配置失败:", error);
  }
}

// 加载SMTP配置
async function loadSmtpConfig() {
  try {
    const result = await apiGet("/api/system/smtp/config");
    if (result.success) {
      const smtpConfig = result.data;
      if (smtpConfig) {
        elementCache.setValue("smtp-host", smtpConfig.host || "");
        elementCache.setValue("smtp-port", smtpConfig.port || "587");
        elementCache.setValue("smtp-username", smtpConfig.username || "");
        const pwdEl = elementCache.get("smtp-password");
        if (pwdEl) {
          pwdEl.value = "";
          pwdEl.placeholder = smtpConfig.has_password ? "已配置（留空保持不变）" : "请输入密码";
        }
        elementCache.setValue("smtp-from", smtpConfig.from || "");
        const secureTypeElement = elementCache.get("smtp-secure-type");
        if (secureTypeElement) {
          if (smtpConfig.secure) {
            secureTypeElement.value = "ssl";
          } else {
            secureTypeElement.value = "none";
          }
        }
      } else {
        // 没有SMTP配置，清空表单
        elementCache.setValue("smtp-host", "");
        elementCache.setValue("smtp-port", "587");
        elementCache.setValue("smtp-username", "");
        elementCache.setValue("smtp-password", "");
        elementCache.setValue("smtp-from", "");
        const secureTypeElement = elementCache.get("smtp-secure-type");
        if (secureTypeElement) {
          secureTypeElement.value = "none";
        }
      }
    }
  } catch (error) {
    console.error("加载SMTP配置失败:", error);
  }
}

// 保存SMTP配置
async function saveSmtpConfig() {
try {
    const secureType = elementCache.getValue("smtp-secure-type");
    const smtpConfig = {
      host: elementCache.getValue("smtp-host"),
      port: parseInt(elementCache.getValue("smtp-port")),
      username: elementCache.getValue("smtp-username"),
      password: elementCache.getValue("smtp-password"),
      from: elementCache.getValue("smtp-from"),
      secure: secureType !== "none",
    };

    const result = await apiPut("/api/system/smtp/config", smtpConfig);
    if (result.success) {
      showToast("SMTP配置保存成功", "success");
    } else {
      showToast("SMTP配置保存失败: " + result.message, "error");
    }
  } catch (error) {
    console.error("保存SMTP配置失败:", error);
    showToast("SMTP配置保存失败: " + error.message, "error");
  }
}

// 测试SMTP连接
export async function testSmtpConnection() {
  try {
    const secureType = elementCache.getValue("smtp-secure-type");
    const smtpConfig = {
      host: elementCache.getValue("smtp-host"),
      port: parseInt(elementCache.getValue("smtp-port")),
      username: elementCache.getValue("smtp-username"),
      password: elementCache.getValue("smtp-password"),
      from: elementCache.getValue("smtp-from"),
      secure: secureType !== "none",
    };

    const result = await apiPost("/api/system/smtp/test", smtpConfig);
    if (result.success) {
      showToast("SMTP连接测试成功", "success");
    } else {
      showToast("SMTP连接测试失败: " + result.message, "error");
    }
  } catch (error) {
    console.error("测试SMTP连接失败:", error);
    showToast("测试SMTP连接失败: " + error.message, "error");
  }
}

// 初始化SMTP相关功能
export function initSmtpFunctions() {
  // 注意：test-smtp-btn 已在 eventManager.js 中绑定，此处不再重复绑定
}

// 加载通知设置
export async function loadNotificationSettings() {
  try {
    // 加载用户列表
    const usersResult = await apiGet("/api/users");
    const settingsResult = await apiGet("/api/system/notification/settings");
    
    const usersList = elementCache.get("notification-users-list");
    if (!usersList) return;
    
    usersList.innerHTML = "";
    
    if (!usersResult.success || !usersResult.data) return;
    
    let users = [];
    if (Array.isArray(usersResult.data)) {
      users = usersResult.data;
    } else if (usersResult.data.items && Array.isArray(usersResult.data.items)) {
      users = usersResult.data.items;
    }
    
    const selectedRecipients = settingsResult.success && settingsResult.data 
      ? settingsResult.data.email_recipients || [] 
      : [];
    
    users.forEach(user => {
      const item = document.createElement("div");
      item.className = "user-checkbox-item";
      
      const checkbox = document.createElement("input");
      checkbox.type = "checkbox";
      checkbox.id = `notify-user-${user.id}`;
      checkbox.value = user.id;
      checkbox.checked = selectedRecipients.includes(user.id);
      
      const label = document.createElement("label");
      label.htmlFor = `notify-user-${user.id}`;
      label.className = "user-info";
      
      const nameSpan = document.createElement("span");
      nameSpan.className = "user-name";
      nameSpan.textContent = user.username;
      
      const emailSpan = document.createElement("span");
      emailSpan.className = "user-email";
      emailSpan.textContent = user.email;
      
      label.appendChild(nameSpan);
      label.appendChild(emailSpan);
      
      item.appendChild(checkbox);
      item.appendChild(label);
      usersList.appendChild(item);
    });
  } catch (error) {
    console.error("加载通知设置失败:", error);
  }
}

// 保存通知设置
export async function saveNotificationSettings() {
  try {
    const checkboxes = document.querySelectorAll("#notification-users-list input[type='checkbox']:checked");
    const userIds = Array.from(checkboxes).map(cb => cb.value);
    
    const result = await apiPut("/api/system/notification/settings", {
      email_recipients: userIds
    });
    
    if (result.success) {
      showToast("通知设置保存成功", "success");
    } else {
      showToast("通知设置保存失败: " + result.message, "error");
    }
  } catch (error) {
    console.error("保存通知设置失败:", error);
    showToast("保存通知设置失败: " + error.message, "error");
  }
}

// 加载系统信息
export async function loadSystemInfo() {
  try {
    const result = await apiGet("/api/system/info");
    if (result.success) {
      const systemInfo = result.data;

      elementCache.get("system-name").textContent = systemInfo.name || "IPMA";
      elementCache.get("system-version").textContent = systemInfo.version || "-";
      const dbStatus = systemInfo.database_status || "-";
      let dbStatusText;
      if (dbStatus === "connected") {
        dbStatusText = t('system.connected');
      } else if (dbStatus.startsWith("disconnected")) {
        dbStatusText = t('system.disconnected');
      } else {
        dbStatusText = dbStatus;
      }
      elementCache.get("database-status").textContent = dbStatusText;
      elementCache.get("system-time").textContent = systemInfo.timestamp 
        ? new Date(systemInfo.timestamp).toLocaleString() 
        : new Date().toLocaleString();
      elementCache.get("system-uptime").textContent = formatUptime(systemInfo.uptime_seconds || 0);
    }
  } catch (error) {
    console.error("加载系统信息失败:", error);
  }
}

// 下载模板
export async function downloadTemplate() {
  try {
    const result = await apiRequest("/api/system/import-export/template?type=all");

    if (!result.success) {
      showToast("下载模板失败: " + result.message, "error");
      return;
    }

    if (result.isBlob) {
      const url = window.URL.createObjectURL(result.data);
      const a = document.createElement("a");
      a.href = url;
      a.download = result.filename || "template.zip";
      document.body.appendChild(a);
      a.click();
      window.URL.revokeObjectURL(url);
      document.body.removeChild(a);
    }
  } catch (error) {
    console.error("下载模板失败:", error);
    showToast("下载模板失败: " + error.message, "error");
  }
}

// 导入CSV数据
export async function importCsvData() {
  const modeSelect = elementCache.get("csv-import-mode");
  const mode = modeSelect ? modeSelect.value : "skip";

  const fileInput = document.createElement("input");
  fileInput.type = "file";
  fileInput.accept = ".zip,.csv";
  fileInput.click();

  fileInput.addEventListener("change", async (e) => {
    const file = e.target.files[0];
    if (!file) return;

    const formData = new FormData();
    formData.append("file", file);

    try {
      showToast("正在导入数据，请稍候...", "info");
      
      const result = await apiRequest(`/api/system/import-export/import/csv?mode=${mode}`, {
        method: "POST",
        body: formData,
      });

      if (result.success) {
        const results = result.data?.results || [];
        if (results.length > 0) {
          await loadModal('import-result-modal');
          const contentDiv = elementCache.get("import-result-content");
          if (contentDiv) {
            contentDiv.innerHTML = results.map(r => `<div class="import-result-item">${escapeHtml(r)}</div>`).join("");
            await openModal("import-result-modal");
          }
        }
        showToast("数据导入完成", "success");
      } else {
        showToast("数据导入失败: " + result.message, "error");
      }
    } catch (error) {
      console.error("导入CSV数据失败:", error);
      showToast("数据导入失败: " + error.message, "error");
    }
  });
}

// 导出CSV数据
export async function exportCsvData() {
  try {
    const exportType = elementCache.getValue("csv-export-type");
    const result = await apiRequest(`/api/system/import-export/export/csv?type=${exportType}`);

    if (!result.success) {
      showToast("导出CSV数据失败: " + result.message, "error");
      return;
    }

    if (result.isBlob) {
      const url = window.URL.createObjectURL(result.data);
      const a = document.createElement("a");
      a.href = url;
      // 如果 ApiClient 没有解析出文件名，则使用默认生成的文件名
      a.download = result.filename && result.filename !== "download" 
        ? result.filename 
        : `ipma-export-${exportType}-${new Date().toISOString().slice(0, 10)}.zip`;
      document.body.appendChild(a);
      a.click();
      window.URL.revokeObjectURL(url);
      document.body.removeChild(a);
    }
  } catch (error) {
    console.error("导出CSV数据失败:", error);
    showToast("导出CSV数据失败: " + error.message, "error");
  }
}

// 导出数据库 (SQL 格式)
export async function exportDatabase() {
  try {
    const response = await fetch("/api/system/import-export/export/database", {
      method: "GET",
      credentials: "include",
    });

    if (!response.ok) {
      let errorMsg = "请求失败";
      try {
        const errorData = await response.json();
        errorMsg = errorData.message || errorMsg;
      } catch (e) {}
      showToast("导出数据库失败: " + errorMsg, "error");
      return;
    }

    const blob = await response.blob();
    const url = window.URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;

    const disposition = response.headers.get("Content-Disposition");
    let filename = `ipma_backup_${new Date().toISOString().slice(0, 10)}.sql`;
    if (disposition && disposition.indexOf("attachment") !== -1) {
      const filenameRegex = /filename[^;=\n]*=((['"]).*?\2|[^;\n]*)/;
      const matches = filenameRegex.exec(disposition);
      if (matches !== null && matches[1]) {
        filename = matches[1].replace(/['"]/g, "");
      }
    }

    a.download = filename;
    document.body.appendChild(a);
    a.click();
    window.URL.revokeObjectURL(url);
    document.body.removeChild(a);

    showToast("数据库导出成功", "success");
  } catch (error) {
    console.error("导出数据库失败:", error);
    showToast("导出数据库失败: " + error.message, "error");
  }
}

// 备份配置
export async function backupConfig() {
  try {
    const result = await apiRequest("/api/system/config/backup");

    if (!result.success) {
      showToast("备份配置失败: " + result.message, "error");
      return;
    }

    if (result.isBlob) {
      const url = window.URL.createObjectURL(result.data);
      const a = document.createElement("a");
      a.href = url;
      a.download = result.filename && result.filename !== "download"
        ? result.filename
        : `ipma-config-${new Date().toISOString().slice(0, 10)}.toml`;
      document.body.appendChild(a);
      a.click();
      window.URL.revokeObjectURL(url);
      document.body.removeChild(a);
    }
  } catch (error) {
    console.error("备份配置失败:", error);
    showToast("备份配置失败: " + error.message, "error");
  }
}

// 恢复配置
export function restoreConfig() {
const fileInput = document.createElement("input");
  fileInput.type = "file";
  fileInput.accept = ".toml";
  fileInput.click();

  fileInput.addEventListener("change", async (e) => {
    const file = e.target.files[0];
    if (!file) return;

    const formData = new FormData();
    formData.append("file", file);

    try {
      const result = await apiRequest("/api/system/config/restore", {
        method: "POST",
        body: formData,
      });

      if (result.success) {
        showToast("配置恢复成功", "success");
        // 显示重启提示，告知用户需要重启程序
        showToast("配置已更新，需要重启应用系统以使配置生效", "warning");
        
        // 设置重启提示标记，用于后续的持续提示
        sessionStorage.setItem("configUpdated", "true");
      } else {
        showToast("配置恢复失败: " + result.message, "error");
      }
    } catch (error) {
      console.error("恢复配置失败:", error);
      showToast("恢复配置失败: " + error.message, "error");
    }
  });
}

// 加载日志统计
export async function loadLogsStats() {
  try {
    const result = await apiGet("/api/system/logs/stats");
    if (result.success && result.data) {
      const stats = result.data;
      
      elementCache.get("operation-logs-count").textContent = 
        stats.operation_logs?.count || 0;
      elementCache.get("login-logs-count").textContent = 
        stats.login_logs?.count || 0;
      elementCache.get("notifications-count").textContent = 
        stats.notifications?.count || 0;
      
      if (stats.operation_logs?.oldest) {
        elementCache.get("operation-logs-oldest").textContent = 
          `最早: ${new Date(stats.operation_logs.oldest).toLocaleDateString()}`;
      }
      if (stats.login_logs?.oldest) {
        elementCache.get("login-logs-oldest").textContent = 
          `最早: ${new Date(stats.login_logs.oldest).toLocaleDateString()}`;
      }
    }
  } catch (error) {
    console.error("加载日志统计失败:", error);
  }
}

// 清理日志
export async function clearLogs() {
  const logType = elementCache.getValue("clear-log-type");
  const daysInput = elementCache.getValue("clear-log-days");
  const days = daysInput ? parseInt(daysInput) : 0;
  
  let confirmMsg;
  if (days === 0) {
    confirmMsg = t('logs.clear_all_confirm') || "确定要删除全部日志吗？此操作不可撤销。";
  } else {
    confirmMsg = t('logs.clear_days_confirm', { days }) || `确定要清理 ${days} 天前的日志吗？此操作不可撤销。`;
  }
  
  if (!confirmMsg) {
    confirmMsg = t('logs.clear_all_confirm');
  }
  
  const confirmed = await showConfirm(confirmMsg);
  if (!confirmed) {
    return;
  }
  
  try {
    const result = await apiPost("/api/system/logs/clear", {
      log_type: logType,
      days: days
    });
    
    if (result.success) {
      showToast(result.message, "success");
      loadLogsStats();
    } else {
      showToast(t('logs.clear_failed') + ": " + result.message, "error");
    }
  } catch (error) {
    console.error("清理日志失败:", error);
    showToast(t('logs.clear_failed') + ": " + error.message, "error");
  }
}

// 初始化日志清理功能
export function initLogsCleanup() {
  const clearLogsBtn = elementCache.get("clear-logs-btn");
  if (clearLogsBtn) {
    clearLogsBtn.addEventListener("click", clearLogs);
  }
  loadLogsStats();
}

// 初始化定时任务标签页
async function initScheduledTasksTab() {
  try {
    const scheduledTaskModule = await import("./scheduledTaskManager.js");
    await scheduledTaskModule.initScheduledTaskManager();
  } catch (error) {
    console.error("初始化定时任务模块失败:", error);
  }
}
