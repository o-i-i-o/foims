import {
  apiRequest,
  apiGet,
  apiPut,
  apiPost,
  apiDelete,
  redirectToLogin,
  refreshToken,
} from "../utils/apiClient.js";

import {
  showToast,
  removeToast,
  showMessage,
  renderTable,
  formatDateTime,
  setLoading,
} from "../utils/ui.js";

import { initModals, openModal, closeModal } from "../utils/modal.js";

import { loadUsersData } from "./userManager.js";

// 初始化系统管理标签页
export function initSystemTabs() {
  const systemContainer = document.getElementById("system");
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
        const tabContentElement = document.getElementById(`${tabId}-tab`);
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
        } 
      });
    });
    systemContainer.dataset.tabsInitialized = "true";
  }

  if (systemContainer.dataset.eventsInitialized === "true") return;

  const systemConfigForm = document.getElementById("system-config-form");
  if (systemConfigForm) {
    systemConfigForm.addEventListener("submit", async (e) => {
      e.preventDefault();
      await saveSystemConfig();
    });
  }

  const autoHttpsToggle = document.getElementById("auto-https");
  if (autoHttpsToggle) {
    autoHttpsToggle.addEventListener("change", handleAutoHttpsChange);
  }

  const httpEnabledToggle = document.getElementById("http-enabled");
  if (httpEnabledToggle) {
    httpEnabledToggle.addEventListener("change", handlePortDisable);
  }

  const httpsEnabledToggle = document.getElementById("https-enabled");
  if (httpsEnabledToggle) {
    httpsEnabledToggle.addEventListener("change", handlePortDisable);
  }

  const certTypeSelect = document.getElementById("cert-type");
  if (certTypeSelect) {
    certTypeSelect.addEventListener("change", handleCertTypeChange);
  }

  const updateCertBtn = document.getElementById("update-cert-btn");
  if (updateCertBtn) {
    updateCertBtn.addEventListener("click", showCertGenerateModal);
  }

  const importCertBtn = document.getElementById("import-cert-btn");
  if (importCertBtn) {
    importCertBtn.addEventListener("click", showCertImportModal);
  }

  const downloadCaBtn = document.getElementById("download-ca-btn");
  if (downloadCaBtn) {
    downloadCaBtn.addEventListener("click", downloadCertificate);
  }

  const certGenerateForm = document.getElementById("cert-generate-form");
  if (certGenerateForm) {
    certGenerateForm.addEventListener("submit", async (e) => {
      e.preventDefault();
      await generateSelfSignedCert();
    });
  }

  const certImportForm = document.getElementById("cert-import-form");
  if (certImportForm) {
    certImportForm.addEventListener("submit", async (e) => {
      e.preventDefault();
      await importCertificate();
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

  const smtpConfigForm = document.getElementById("smtp-config-form");
  if (smtpConfigForm) {
    smtpConfigForm.addEventListener("submit", async (e) => {
      e.preventDefault();
      await saveSmtpConfig();
    });
  }

  const saveNotificationBtn = document.getElementById("save-notification-settings-btn");
  if (saveNotificationBtn) {
    saveNotificationBtn.addEventListener("click", saveNotificationSettings);
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
    const httpEnabled = document.getElementById("http-enabled").checked;
    const httpsEnabled = document.getElementById("https-enabled").checked;
    
    if (!httpEnabled && !httpsEnabled) {
      showToast("至少需要开启一个端口（HTTP或HTTPS）", "warning");
      return;
    }

    const serverConfig = {
      ...currentServerConfig,
      host: document.getElementById("server-host").value,
      host_ipv6: document.getElementById("server-host-ipv6").value || null,
      http_enabled: httpEnabled,
      http_port: parseInt(document.getElementById("http-port").value) || 80,
      https_enabled: httpsEnabled,
      https_port: parseInt(document.getElementById("https-port").value) || 443,
      auto_https: document.getElementById("auto-https").checked,
      http_version: document.getElementById("http-version").value || "http1.1",
      cert_type: document.getElementById("cert-type").value || "self_signed",
      page_timeout: parseInt(document.getElementById("page-timeout").value) || 30,
      public_url: document.getElementById("public-url").value || ""
    };

    const config = { server: serverConfig };

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
    const result = await apiGet("/api/system/info");
    if (result.success) {
      const config = result.data.config;
      
      currentServerConfig = config.server;
      
      document.getElementById("server-host").value = config.server.host;
      document.getElementById("server-host-ipv6").value = config.server.host_ipv6 || "";

      document.getElementById("http-enabled").checked = config.server.http_enabled || false;
      document.getElementById("http-port").value = config.server.http_port || 80;
      document.getElementById("https-enabled").checked = config.server.https_enabled || false;
      document.getElementById("https-port").value = config.server.https_port || 443;
      document.getElementById("auto-https").checked = config.server.auto_https || false;
      document.getElementById("http-version").value = config.server.http_version || "http1.1";
      
      const certType = config.server.cert_type || "self_signed";
      document.getElementById("cert-type").value = certType;
      
      document.getElementById("page-timeout").value = config.server.page_timeout || 30;
      document.getElementById("public-url").value = config.server.public_url || "";
      
      handleCertTypeChange();
      checkImportedCertificate();
      updateCertificateSectionVisibility();
      checkConfigUpdateRestartPrompt();
      
      checkServiceStatus();
    }
  } catch (error) {
    console.error("加载系统配置失败:", error);
  }
}

// 检查是否有导入的证书
async function checkImportedCertificate() {
  try {
    const result = await apiGet("/api/system/certificate/status");
    if (result.success) {
      const hasImportedCert = result.data.has_imported_cert;
      const importBtn = document.getElementById("import-cert-btn");
      if (importBtn) {
        importBtn.textContent = hasImportedCert ? "更新" : "导入";
      }
    }
  } catch (error) {
    console.error("检查证书状态失败:", error);
  }
}

// 更新证书管理区域的显示
function updateCertificateSectionVisibility() {
  const httpsEnabled = document.getElementById("https-enabled").checked;
  const certificateSection = document.getElementById("certificate-section");
  
  if (certificateSection) {
    certificateSection.style.display = httpsEnabled ? "block" : "none";
  }
}

// 处理自动 HTTPS 功能
function handleAutoHttpsChange() {
  const autoHttps = document.getElementById("auto-https").checked;
  const httpEnabled = document.getElementById("http-enabled");
  const httpsEnabled = document.getElementById("https-enabled");
  
  if (autoHttps) {
    // 自动启用 HTTP 和 HTTPS 端口
    httpEnabled.checked = true;
    httpsEnabled.checked = true;
    updateCertificateSectionVisibility();
  }
}

// 处理 HTTP 或 HTTPS 端口禁用
function handlePortDisable(e) {
  const httpEnabled = document.getElementById("http-enabled").checked;
  const httpsEnabled = document.getElementById("https-enabled").checked;
  const autoHttps = document.getElementById("auto-https");
  
  // 确保至少有一个端口处于开启状态
  if (!httpEnabled && !httpsEnabled) {
    // 如果用户尝试同时禁用两个端口，恢复之前的状态
    if (e && e.target) {
      // 恢复被禁用的端口
      e.target.checked = true;
      showToast("至少需要开启一个端口（HTTP或HTTPS）", "warning");
    }
    return;
  }
  
  if (!httpEnabled || !httpsEnabled) {
    // 如果任一端口被禁用，取消自动 HTTPS
    autoHttps.checked = false;
  }
  
  updateCertificateSectionVisibility();
}

// 处理证书类型切换
function handleCertTypeChange() {
  const certType = document.getElementById("cert-type").value;
  const updateCertBtn = document.getElementById("update-cert-btn");
  const importCertBtn = document.getElementById("import-cert-btn");
  const downloadCaBtn = document.getElementById("download-ca-btn");
  
  if (certType === "self_signed") {
    // 自签名证书模式
    updateCertBtn.style.display = "inline-block";
    downloadCaBtn.style.display = "inline-block"; // 显示下载CA按钮
    importCertBtn.style.display = "none";
  } else if (certType === "imported") {
    // 导入证书模式
    updateCertBtn.style.display = "none";
    downloadCaBtn.style.display = "none"; // 隐藏下载CA按钮
    importCertBtn.style.display = "inline-block";
    // 检查是否已有导入的证书，更新按钮文字
    checkImportedCertificate();
  }
}

// 显示证书生成模态框
function showCertGenerateModal() {
  openModal("cert-generate-modal");
}

// 显示证书导入模态框
function showCertImportModal() {
  openModal("cert-import-modal");
}

// 下载证书
async function downloadCertificate() {
  try {
    const result = await apiRequest("/api/system/certificate/download");

    if (!result.success) {
      showToast("下载证书失败: " + result.message, "error");
      return;
    }

    if (result.isBlob) {
      const url = window.URL.createObjectURL(result.data);
      const a = document.createElement("a");
      a.href = url;
      a.download = result.filename || "ca.pem";
      document.body.appendChild(a);
      a.click();
      window.URL.revokeObjectURL(url);
      document.body.removeChild(a);
    }
  } catch (error) {
    console.error("下载证书失败:", error);
    showToast("下载证书失败: " + error.message, "error");
  }
}

// 生成自签名证书
async function generateSelfSignedCert() {
try {
    const form = document.getElementById("cert-generate-form");
    const formData = new FormData(form);
    
    const certData = {
      common_name: formData.get("common_name"),
      organization: formData.get("organization"),
      organizational_unit: formData.get("organizational_unit"),
      country: formData.get("country"),
      state: formData.get("state"),
      locality: formData.get("locality"),
      validity: parseInt(formData.get("validity"))
    };

    const result = await apiPost("/api/system/certificate/generate", certData);
    if (result.success) {
      showToast("自签名证书生成成功", "success");
      closeModal("cert-generate-modal");
      form.reset();
    } else {
      showToast("证书生成失败: " + result.message, "error");
    }
  } catch (error) {
    console.error("生成证书失败:", error);
    showToast("证书生成失败: " + error.message, "error");
  }
}

// 导入证书
async function importCertificate() {
  try {
    const form = document.getElementById("cert-import-form");
    const formData = new FormData(form);

    const result = await apiRequest("/api/system/certificate/import", {
      method: "POST",
      body: formData,
    });
    if (result.success) {
      showToast("证书导入成功", "success");
      closeModal("cert-import-modal");
      form.reset();
      // 更新导入按钮文字
      document.getElementById("import-cert-btn").textContent = "更新";
    } else {
      showToast("证书导入失败: " + result.message, "error");
    }
  } catch (error) {
    console.error("导入证书失败:", error);
    showToast("证书导入失败: " + error.message, "error");
  }
}

// 检查服务状态
async function checkServiceStatus() {
  try {
    const result = await apiGet("/api/system/service-status");
    if (result.success) {
      const data = result.data;
      const registerBtn = document.getElementById("register-service-btn");
      const restartBtn = document.getElementById("restart-app-btn");
      const serviceStatusEl = document.getElementById("service-status-info");
      
      if (data.registered) {
        registerBtn.disabled = true;
        registerBtn.textContent = "已注册为服务";
        registerBtn.classList.add("btn-success");
        registerBtn.classList.remove("btn-secondary");
      } else {
        registerBtn.disabled = false;
        registerBtn.textContent = "注册为服务";
        registerBtn.classList.add("btn-secondary");
        registerBtn.classList.remove("btn-success");
      }
      
      if (serviceStatusEl) {
        let statusHtml = '<div class="service-status-grid">';
        
        statusHtml += `
          <div class="status-item">
            <span class="status-label">运行模式:</span>
            <span class="status-value">${data.running_as_service ? '系统服务' : '独立进程'}</span>
          </div>
        `;
        
        if (data.service_file_exists) {
          statusHtml += `
            <div class="status-item">
              <span class="status-label">服务状态:</span>
              <span class="status-value ${data.active ? 'status-active' : 'status-inactive'}">
                ${data.status || (data.active ? '运行中' : '已停止')}
              </span>
            </div>
          `;
          
          if (data.active && data.uptime_seconds) {
            const uptime = formatUptime(data.uptime_seconds);
            statusHtml += `
              <div class="status-item">
                <span class="status-label">运行时间:</span>
                <span class="status-value">${uptime}</span>
              </div>
            `;
          }
          
          statusHtml += `
            <div class="status-item">
              <span class="status-label">开机自启:</span>
              <span class="status-value ${data.enabled ? 'status-enabled' : 'status-disabled'}">
                ${data.enabled ? '已启用' : '已禁用'}
              </span>
            </div>
          `;
        }
        
        statusHtml += '</div>';
        serviceStatusEl.innerHTML = statusHtml;
      }
      
      if (restartBtn) {
        if (data.running_as_service) {
          restartBtn.textContent = "重启服务";
        } else {
          restartBtn.textContent = "重启程序";
        }
      }
    }
  } catch (error) {
    console.error("检查服务状态失败:", error);
  }
}

// 注册为服务
export async function registerService() {
  if (confirm("确定要将系统注册为服务吗？此操作将在系统启动时自动运行IPMA服务。")) {
    try {
      const result = await apiPost("/api/system/register-service", {});
      if (result.success) {
        showToast("注册为服务成功", "success");
        // 注册成功后更新服务状态
        checkServiceStatus();
      } else {
        showToast("注册为服务失败: " + result.message, "error");
      }
    } catch (error) {
      console.error("注册为服务失败:", error);
      showToast("注册为服务失败: " + error.message, "error");
    }
  }
}

// 重启应用系统
export async function restartApplication() {
  const isService = await checkIfRunningAsService();
  const confirmMsg = isService 
    ? "确定要重启IPMA服务吗？重启过程中服务将暂时不可用。" 
    : "确定要重启IPMA程序吗？重启过程中服务将暂时不可用。";

  if (confirm(confirmMsg)) {
    clearConfigUpdateFlag();
    
    try {
      const result = await apiPost("/api/system/restart-application", {});
      if (result.success) {
        const successMsg = isService ? "服务重启命令已发送" : "程序重启命令已发送";
        showToast(successMsg, "success");
        showToast("正在重启，请稍候...", "info");
        setTimeout(() => {
          location.reload();
        }, 5000);
      } else {
        showToast("重启失败: " + result.message, "error");
      }
    } catch (error) {
      console.error("重启失败:", error);
      showToast("重启失败: " + error.message, "error");
    }
  }
}

async function checkIfRunningAsService() {
  try {
    const result = await apiGet("/api/system/service-status");
    return result.success && result.data.running_as_service;
  } catch {
    return false;
  }
}

// 重启操作系统
export async function restartOs() {
  if (confirm("确定要重启操作系统吗？重启过程中所有服务将暂时不可用。")) {
    clearConfigUpdateFlag();
    
    try {
      const result = await apiPost("/api/system/restart-os", {});
      if (result.success) {
        showToast("操作系统重启命令已发送", "success");
        showToast("操作系统正在重启，请稍候...", "info");
        // 5秒后刷新页面
        setTimeout(() => {
          location.reload();
        }, 5000);
      } else {
        showToast("重启操作系统失败: " + result.message, "error");
      }
    } catch (error) {
      console.error("重启操作系统失败:", error);
      showToast("重启操作系统失败: " + error.message, "error");
    }
  }
}



// 加载SMTP配置
async function loadSmtpConfig() {
  try {
    const result = await apiGet("/api/system/smtp/config");
    if (result.success) {
      const smtpConfig = result.data;
      if (smtpConfig) {
        document.getElementById("smtp-host").value = smtpConfig.host || "";
        document.getElementById("smtp-port").value = smtpConfig.port || "587";
        document.getElementById("smtp-username").value = smtpConfig.username || "";
        document.getElementById("smtp-password").value = smtpConfig.password || "";
        document.getElementById("smtp-from").value = smtpConfig.from || "";
        const secureTypeElement = document.getElementById("smtp-secure-type");
        if (secureTypeElement) {
          if (smtpConfig.secure) {
            secureTypeElement.value = "ssl";
          } else {
            secureTypeElement.value = "none";
          }
        }
      } else {
        // 没有SMTP配置，清空表单
        document.getElementById("smtp-host").value = "";
        document.getElementById("smtp-port").value = "587";
        document.getElementById("smtp-username").value = "";
        document.getElementById("smtp-password").value = "";
        document.getElementById("smtp-from").value = "";
        const secureTypeElement = document.getElementById("smtp-secure-type");
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
    const secureType = document.getElementById("smtp-secure-type").value;
    const smtpConfig = {
      host: document.getElementById("smtp-host").value,
      port: parseInt(document.getElementById("smtp-port").value),
      username: document.getElementById("smtp-username").value,
      password: document.getElementById("smtp-password").value,
      from: document.getElementById("smtp-from").value,
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
    const secureType = document.getElementById("smtp-secure-type").value;
    const smtpConfig = {
      host: document.getElementById("smtp-host").value,
      port: parseInt(document.getElementById("smtp-port").value),
      username: document.getElementById("smtp-username").value,
      password: document.getElementById("smtp-password").value,
      from: document.getElementById("smtp-from").value,
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
    
    const usersList = document.getElementById("notification-users-list");
    if (!usersList) return;
    
    usersList.innerHTML = "";
    
    if (!usersResult.success || !usersResult.data) return;
    
    const selectedRecipients = settingsResult.success && settingsResult.data 
      ? settingsResult.data.email_recipients || [] 
      : [];
    
    usersResult.data.forEach(user => {
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

      // 更新系统信息
      document.getElementById("system-name").textContent = systemInfo.name;
      document.getElementById("system-version").textContent = systemInfo.version;
      document.getElementById("database-status").textContent = systemInfo.database_status;
      document.getElementById("system-time").textContent = new Date(systemInfo.timestamp).toLocaleString();
      document.getElementById("system-uptime").textContent = formatUptime(systemInfo.uptime);
    }
  } catch (error) {
    console.error("加载系统信息失败:", error);
  }
}

// 下载模板
export async function downloadTemplate(type = "csv") {
  try {
    const result = await apiRequest(`/api/system/import-export/template?type=${type}`);

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
      const result = await apiRequest("/api/system/import-export/import/csv", {
        method: "POST",
        body: formData,
      });

      if (result.success) {
        showToast("数据导入成功", "success");
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
    const exportType = document.getElementById("csv-export-type").value;
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

// 导出数据库
export async function exportDatabase() {
  try {
    const result = await apiRequest("/api/system/import-export/export/database");

    if (!result.success) {
      showToast("导出数据库失败: " + result.message, "error");
      return;
    }

    if (result.isBlob) {
      const url = window.URL.createObjectURL(result.data);
      const a = document.createElement("a");
      a.href = url;
      a.download = result.filename && result.filename !== "download"
        ? result.filename
        : `ipma-db-${new Date().toISOString().slice(0, 10)}.sql`;
      document.body.appendChild(a);
      a.click();
      window.URL.revokeObjectURL(url);
      document.body.removeChild(a);
    }
  } catch (error) {
    console.error("导出数据库失败:", error);
    showToast("导出数据库失败: " + error.message, "error");
  }
}

// 导入数据库
export function importDatabase() {
const fileInput = document.createElement("input");
  fileInput.type = "file";
  fileInput.accept = ".sql";
  fileInput.click();

  fileInput.addEventListener("change", async (e) => {
    const file = e.target.files[0];
    if (!file) return;

    const formData = new FormData();
    formData.append("file", file);

    try {
      const result = await apiRequest("/api/system/import-export/import/database", {
        method: "POST",
        body: formData,
      });

      if (result.success) {
        showToast("数据库导入成功", "success");
      } else {
        showToast("数据库导入失败: " + result.message, "error");
      }
    } catch (error) {
      console.error("导入数据库失败:", error);
      showToast("导入数据库失败: " + error.message, "error");
    }
  });
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
