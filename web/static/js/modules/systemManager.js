import { apiRequest, apiGet, apiPut, apiPost } from "../utils/apiClient.js";

import { showToast, escapeHtml } from "../utils/ui.js";

import { loadModal, openModal, closeModal } from "../utils/modalLoader.js";
import { t } from "../utils/i18n.js";
import { loadUsersData } from "./userManager.js";
import { initSecurityTab } from "./fail2banManager.js";
import { elementCache, setActiveSubtab, getActiveSubtab } from "../utils/helpers.js";
import { showConfirm } from "../utils/confirm.js";

// 初始化系统管理标签页
export function initSystemTabs() {
  const systemContainer = elementCache.get("system");
  if (!systemContainer) {
    return;
  }

  const tabBtns = systemContainer.querySelectorAll(".tab-btn[data-tab]");
  const tabContents = systemContainer.querySelectorAll(".tab-content");

  if (!systemContainer.dataset.tabsInitialized) {
    tabBtns.forEach((btn) => {
      btn.addEventListener("click", function () {
        const tabId = this.getAttribute("data-tab");
        setActiveSubtab("system", tabId);

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
        } else if (tabId === "system-config") {
          // 证书/LDAP/SSO 配置随子标签激活加载（含刷新后程序化恢复子标签的场景）
          loadSystemConfig();
          loadCertificateInventory();
          loadLdapConfig();
          loadSsoConfig();
        } else if (tabId === "system-notification") {
          loadSmtpConfig();
          loadNotificationSettings();
        } else if (tabId === "data-management") {
          loadLogsStats();
          loadLogForwarding();
        } else if (tabId === "scheduled-tasks") {
          initScheduledTasksTab();
        } else if (tabId === "security") {
          initSecurityTab();
          loadPasswordPolicy();
        }
      });
    });
    systemContainer.dataset.tabsInitialized = "true";
  }

  // 刷新后恢复上次记住的子标签（仅当与当前激活项不同时切换）
  const savedSystemTabId = getActiveSubtab("system");
  const savedSystemTabBtn = savedSystemTabId
    ? systemContainer.querySelector(`.tab-btn[data-tab="${CSS.escape(savedSystemTabId)}"]`)
    : null;
  if (savedSystemTabBtn && !savedSystemTabBtn.classList.contains("active")) {
    savedSystemTabBtn.click();
  }

  if (systemContainer.dataset.eventsInitialized === "true") {
    return;
  }

  const systemConfigForm = elementCache.get("system-config-form");
  if (systemConfigForm) {
    systemConfigForm.addEventListener("submit", async (e) => {
      e.preventDefault();
      try {
        await saveSystemConfig();
      } catch (err) {
        console.error("保存系统配置失败:", err);
      }
    });
  }

  // 子标签点击统一走上面的通用处理器（含证书清单加载），无需重复绑定

  const logForwardingSaveBtn = elementCache.get("log-forwarding-save-btn");
  if (logForwardingSaveBtn) {
    logForwardingSaveBtn.addEventListener("click", saveLogForwarding);
  }

  const logForwardingTestBtn = elementCache.get("log-forwarding-test-btn");
  if (logForwardingTestBtn) {
    logForwardingTestBtn.addEventListener("click", testLogForwarding);
  }

  const smtpConfigForm = elementCache.get("smtp-config-form");
  if (smtpConfigForm) {
    smtpConfigForm.addEventListener("submit", async (e) => {
      e.preventDefault();
      await saveSmtpConfig();
    });
  }

  const ldapConfigForm = elementCache.get("ldap-config-form");
  if (ldapConfigForm) {
    ldapConfigForm.addEventListener("submit", async (e) => {
      e.preventDefault();
      await saveLdapConfig();
    });
  }

  const ssoConfigForm = elementCache.get("sso-config-form");
  if (ssoConfigForm) {
    ssoConfigForm.addEventListener("submit", async (e) => {
      e.preventDefault();
      await saveSsoConfig();
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

  const openSourceLink = elementCache.get("open-source-btn");
  if (openSourceLink) {
    openSourceLink.addEventListener("click", (e) => {
      e.preventDefault();
      openOpenSourceModal();
    });
  }

  // 密码策略卡片保存按钮（等保三级，安全管理员权限，面向全体用户）
  const passwordPolicySaveBtn = elementCache.get("password-policy-save-btn");
  if (passwordPolicySaveBtn) {
    passwordPolicySaveBtn.addEventListener("click", savePasswordPolicy);
  }

  initCertificateManager();

  systemContainer.dataset.eventsInitialized = "true";
}

// ==================== 关于卡片：开源组件清单（与 NOTICE 同步维护） ====================

const OPEN_SOURCE_COMPONENTS = [
  {
    license: "MIT",
    components: [
      { name: "axum / axum-extra / tower / tower-http", url: "https://github.com/tokio-rs/axum" },
      { name: "validator", url: "https://github.com/Keats/validator" },
      { name: "bcrypt", url: "https://github.com/Keats/rust-bcrypt" },
      { name: "jsonwebtoken", url: "https://github.com/Keats/jsonwebtoken" },
      { name: "tracing / tracing-subscriber", url: "https://github.com/tokio-rs/tracing" },
      { name: "tokio", url: "https://github.com/tokio-rs/tokio" },
      { name: "dashmap", url: "https://github.com/xacrimon/dashmap" },
      { name: "zip", url: "https://github.com/zip-rs/zip2" },
      { name: "lettre", url: "https://github.com/lettre/lettre" },
      { name: "totp-rs", url: "https://github.com/constantoine/totp-rs" },
      { name: "rust-i18n", url: "https://github.com/longbridgeapp/rust-i18n" },
      { name: "pem / flate2 / bytes", url: "" }
    ]
  },
  {
    license: "Apache-2.0 OR MIT",
    components: [
      { name: "sqlx", url: "https://github.com/launchbadge/sqlx" },
      { name: "serde / serde_json", url: "https://github.com/serde-rs/serde" },
      { name: "regex", url: "https://github.com/rust-lang/regex" },
      { name: "RustCrypto: sha2 / aes-gcm / aes / hex", url: "https://github.com/RustCrypto" },
      { name: "futures-util / async-trait", url: "https://github.com/rust-lang/futures-rs" },
      { name: "tokio-cron-scheduler", url: "https://github.com/mvniekerk/tokio-cron-scheduler" },
      { name: "ipnetwork / macaddr", url: "https://github.com/achanda/ipnetwork" },
      { name: "pnet", url: "https://github.com/libpnet/libpnet" },
      { name: "async-snmp", url: "https://github.com/rdklibansky/async-snmp" },
      { name: "config / toml", url: "https://github.com/mehcode/config-rs" },
      { name: "thiserror / chrono / time / uuid", url: "" },
      { name: "base64 / rand / arc-swap", url: "" },
      { name: "rustls", url: "https://github.com/rustls/rustls" },
      { name: "http / log", url: "" }
    ]
  },
  {
    license: "Unlicense OR MIT",
    components: [{ name: "csv", url: "https://github.com/BurntSushi/rust-csv" }]
  }
];

async function openOpenSourceModal() {
  const modal = await loadModal("open-source-modal");
  if (!modal) {
    return;
  }

  const listEl = document.getElementById("open-source-list");
  if (listEl) {
    listEl.innerHTML = OPEN_SOURCE_COMPONENTS.map(
      (group) => `
      <div class="open-source-group">
        <h4 class="open-source-license">${escapeHtml(group.license)}</h4>
        <ul class="open-source-items">
          ${group.components
            .map((c) => {
              const name = escapeHtml(c.name);
              const content = c.url
                ? `<a href="${escapeHtml(c.url)}" target="_blank" rel="noopener noreferrer">${name}</a>`
                : name;
              return `<li>${content}</li>`;
            })
            .join("")}
        </ul>
      </div>`
    ).join("");
  }

  openModal("open-source-modal");
}

// 格式化系统运行时间（秒 -> X天X小时X分钟X秒，各时间单位文案走 i18n 动态键）
function formatUptime(seconds) {
  const days = Math.floor(seconds / (24 * 60 * 60));
  seconds %= 24 * 60 * 60;
  const hours = Math.floor(seconds / (60 * 60));
  seconds %= 60 * 60;
  const minutes = Math.floor(seconds / 60);
  seconds %= 60;

  let result = "";
  if (days > 0) {
    result += t("system.uptime_days", { count: days });
  }
  if (hours > 0) {
    result += t("system.uptime_hours", { count: hours });
  }
  if (minutes > 0) {
    result += t("system.uptime_minutes", { count: minutes });
  }
  result += t("system.uptime_seconds", { count: seconds });

  return result;
}

// 保存系统配置
let currentServerConfig = null;
// 在途标志：保存请求期间重复提交直接忽略（防双击重复写入）
let systemConfigSaving = false;

async function saveSystemConfig() {
  if (systemConfigSaving) {
    return;
  }
  systemConfigSaving = true;
  try {
    const serverConfig = {
      ...currentServerConfig,
      page_timeout: parseInt(elementCache.getValue("page-timeout")) || 30,
      public_url: elementCache.getValue("public-url") || ""
    };

    const config = { server: serverConfig };

    const result = await apiPut("/api/system/config", config);

    if (result.success) {
      await loadSystemConfig();
      showToast(t("system.config_save_success"), "success");
      showToast(t("system.config_saved_restart_needed"), "warning");
      sessionStorage.setItem("configUpdated", "true");
    } else {
      showToast(`${t("system.config_save_failed")}: ${result.message}`, "error");
    }
  } catch (error) {
    console.error("保存系统配置失败:", error);
    showToast(`${t("system.config_save_failed")}: ${error.message}`, "error");
  } finally {
    systemConfigSaving = false;
  }
}

// 检查并显示配置更新后的重启提示
function checkConfigUpdateRestartPrompt() {
  // 读后立即清除标记：否则标记残留会导致每次进系统页加载配置都重复弹提示
  const configUpdated = sessionStorage.getItem("configUpdated");
  sessionStorage.removeItem("configUpdated");
  if (configUpdated === "true") {
    // 显示重启提示
    showToast(t("system.config_saved_restart_needed"), "warning");
  }
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

      checkConfigUpdateRestartPrompt();
    }
  } catch (error) {
    console.error("加载系统配置失败:", error);
  }
}

// 加载SMTP配置（未配置是正常业务状态：后端返回 200 + configured=false）
async function loadSmtpConfig() {
  try {
    const result = await apiGet("/api/system/smtp/config");
    if (result.success && result.data) {
      const smtpConfig = result.data;
      if (smtpConfig.configured) {
        elementCache.setValue("smtp-host", smtpConfig.host || "");
        elementCache.setValue("smtp-port", smtpConfig.port || "587");
        elementCache.setValue("smtp-username", smtpConfig.username || "");
        const pwdEl = elementCache.get("smtp-password");
        if (pwdEl) {
          pwdEl.value = "";
          pwdEl.placeholder = smtpConfig.has_password
            ? t("smtp.password_configured")
            : t("smtp.password_input");
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
        const pwdEl = elementCache.get("smtp-password");
        if (pwdEl) {
          pwdEl.placeholder = t("smtp.password_input");
        }
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

// 保存SMTP配置（在途标志防重复提交）
let smtpConfigSaving = false;

async function saveSmtpConfig() {
  if (smtpConfigSaving) {
    return;
  }
  smtpConfigSaving = true;
  try {
    const secureType = elementCache.getValue("smtp-secure-type");
    const smtpConfig = {
      host: elementCache.getValue("smtp-host"),
      port: parseInt(elementCache.getValue("smtp-port")),
      username: elementCache.getValue("smtp-username"),
      password: elementCache.getValue("smtp-password"),
      from: elementCache.getValue("smtp-from"),
      secure: secureType !== "none"
    };

    const result = await apiPut("/api/system/smtp/config", smtpConfig);
    if (result.success) {
      showToast(t("smtp.save_success"), "success");
    } else {
      showToast(`${t("smtp.save_failed")}: ${result.message}`, "error");
    }
  } catch (error) {
    console.error("保存SMTP配置失败:", error);
    showToast(`${t("smtp.save_failed")}: ${error.message}`, "error");
  } finally {
    smtpConfigSaving = false;
  }
}

// 测试SMTP连接（后端基于已保存配置测试，无需请求体）
export async function testSmtpConnection() {
  try {
    const result = await apiPost("/api/system/smtp/test", {});
    if (result.success) {
      showToast(t("smtp.test_success"), "success");
    } else {
      showToast(`${t("smtp.test_failed")}: ${result.message}`, "error");
    }
  } catch (error) {
    console.error("测试SMTP连接失败:", error);
    showToast(`${t("smtp.test_error")}: ${error.message}`, "error");
  }
}

// ==================== LDAP / SSO 认证配置 ====================

// 加载LDAP配置（未配置时展示默认值）
export async function loadLdapConfig() {
  try {
    const result = await apiGet("/api/system/ldap/config");
    if (!result.success || !result.data) {
      return;
    }

    const config = result.data;
    const enabledEl = elementCache.get("ldap-enabled");
    if (enabledEl) {
      enabledEl.checked = Boolean(config.enabled);
    }
    elementCache.setValue("ldap-url", config.url || "");
    elementCache.setValue("ldap-bind-dn", config.bind_dn || "");
    elementCache.setValue("ldap-base-dn", config.base_dn || "");
    elementCache.setValue("ldap-user-filter", config.user_filter || "");
    elementCache.setValue("ldap-default-role", config.default_role || "user");
    const pwdEl = elementCache.get("ldap-bind-password");
    if (pwdEl) {
      pwdEl.value = "";
      pwdEl.placeholder = config.has_password
        ? t("ldap.password_configured")
        : t("ldap.bind_password_placeholder");
    }
  } catch (error) {
    console.error("加载LDAP配置失败:", error);
  }
}

// 保存LDAP配置（密码留空表示沿用已保存值；在途标志防重复提交）
let ldapConfigSaving = false;

export async function saveLdapConfig() {
  if (ldapConfigSaving) {
    return;
  }
  ldapConfigSaving = true;
  try {
    const config = {
      enabled: elementCache.get("ldap-enabled")?.checked || false,
      url: elementCache.getValue("ldap-url"),
      bind_dn: elementCache.getValue("ldap-bind-dn") || "",
      bind_password: elementCache.getValue("ldap-bind-password") || "",
      base_dn: elementCache.getValue("ldap-base-dn"),
      user_filter: elementCache.getValue("ldap-user-filter"),
      default_role: elementCache.getValue("ldap-default-role") || "user"
    };

    const result = await apiPut("/api/system/ldap/config", config);
    if (result.success) {
      showToast(t("ldap.save_success"), "success");
    } else {
      showToast(`${t("ldap.save_failed")}: ${result.message}`, "error");
    }
  } catch (error) {
    console.error("保存LDAP配置失败:", error);
    showToast(`${t("ldap.save_failed")}: ${error.message}`, "error");
  } finally {
    ldapConfigSaving = false;
  }
}

// 测试已保存的LDAP配置连通性
export async function testLdapConnection() {
  try {
    const result = await apiPost("/api/system/ldap/test", {});
    if (result.success) {
      showToast(t("ldap.test_success"), "success");
    } else {
      showToast(`${t("ldap.test_failed")}: ${result.message}`, "error");
    }
  } catch (error) {
    console.error("测试LDAP连接失败:", error);
    showToast(`${t("ldap.test_error")}: ${error.message}`, "error");
  }
}

// 加载SSO（OIDC）配置
export async function loadSsoConfig() {
  try {
    const result = await apiGet("/api/system/sso/config");
    if (!result.success || !result.data) {
      return;
    }

    const config = result.data;
    const enabledEl = elementCache.get("sso-enabled");
    if (enabledEl) {
      enabledEl.checked = Boolean(config.enabled);
    }
    elementCache.setValue("sso-issuer-url", config.issuer_url || "");
    elementCache.setValue("sso-client-id", config.client_id || "");
    elementCache.setValue("sso-redirect-uri", config.redirect_uri || "");
    elementCache.setValue("sso-default-role", config.default_role || "user");
    const secretEl = elementCache.get("sso-client-secret");
    if (secretEl) {
      secretEl.value = "";
      secretEl.placeholder = config.has_secret
        ? t("sso.secret_configured")
        : t("sso.client_secret_placeholder");
    }
  } catch (error) {
    console.error("加载SSO配置失败:", error);
  }
}

// 保存SSO（OIDC）配置（密钥留空表示沿用已保存值；在途标志防重复提交）
let ssoConfigSaving = false;

export async function saveSsoConfig() {
  if (ssoConfigSaving) {
    return;
  }
  ssoConfigSaving = true;
  try {
    const config = {
      enabled: elementCache.get("sso-enabled")?.checked || false,
      issuer_url: elementCache.getValue("sso-issuer-url"),
      client_id: elementCache.getValue("sso-client-id"),
      client_secret: elementCache.getValue("sso-client-secret") || "",
      redirect_uri: elementCache.getValue("sso-redirect-uri") || "",
      default_role: elementCache.getValue("sso-default-role") || "user"
    };

    const result = await apiPut("/api/system/sso/config", config);
    if (result.success) {
      showToast(t("sso.save_success"), "success");
    } else {
      showToast(`${t("sso.save_failed")}: ${result.message}`, "error");
    }
  } catch (error) {
    console.error("保存SSO配置失败:", error);
    showToast(`${t("sso.save_failed")}: ${error.message}`, "error");
  } finally {
    ssoConfigSaving = false;
  }
}

// 测试已保存的SSO配置（执行OIDC发现文档获取）
export async function testSsoConnection() {
  try {
    const result = await apiPost("/api/system/sso/test", {});
    if (result.success) {
      showToast(t("sso.test_success"), "success");
    } else {
      showToast(`${t("sso.test_failed")}: ${result.message}`, "error");
    }
  } catch (error) {
    console.error("测试SSO连接失败:", error);
    showToast(`${t("sso.test_error")}: ${error.message}`, "error");
  }
}

// 加载通知设置
export async function loadNotificationSettings() {
  try {
    // 用户列表与通知设置互不依赖，并行加载
    const [usersResult, settingsResult] = await Promise.all([
      apiGet("/api/users"),
      apiGet("/api/system/notification/settings")
    ]);

    const usersList = elementCache.get("notification-users-list");
    if (!usersList) {
      return;
    }

    usersList.innerHTML = "";

    if (!usersResult.success || !usersResult.data) {
      return;
    }

    let users = [];
    if (Array.isArray(usersResult.data)) {
      users = usersResult.data;
    } else if (usersResult.data.items && Array.isArray(usersResult.data.items)) {
      users = usersResult.data.items;
    }

    const selectedRecipients =
      settingsResult.success && settingsResult.data
        ? settingsResult.data.email_recipients || []
        : [];

    users.forEach((user) => {
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

// 保存通知设置（在途标志防重复提交）
let notificationSettingsSaving = false;

export async function saveNotificationSettings() {
  if (notificationSettingsSaving) {
    return;
  }
  notificationSettingsSaving = true;
  try {
    const checkboxes = document.querySelectorAll(
      "#notification-users-list input[type='checkbox']:checked"
    );
    const userIds = Array.from(checkboxes).map((cb) => cb.value);

    const result = await apiPut("/api/system/notification/settings", {
      email_recipients: userIds
    });

    if (result.success) {
      showToast(t("notification.save_success"), "success");
    } else {
      showToast(`${t("notification.save_failed")}: ${result.message}`, "error");
    }
  } catch (error) {
    console.error("保存通知设置失败:", error);
    showToast(`${t("notification.save_error")}: ${error.message}`, "error");
  } finally {
    notificationSettingsSaving = false;
  }
}

// 空值安全的文本写入（元素缺失时不抛错，静默跳过）
function setTextById(id, text) {
  const el = elementCache.get(id);
  if (el) {
    el.textContent = text;
  }
}

// 加载系统信息
export async function loadSystemInfo() {
  try {
    const result = await apiGet("/api/system/info");
    if (result.success) {
      const systemInfo = result.data;

      setTextById("system-name", systemInfo.name || "IPMA");
      setTextById("system-version", systemInfo.version || "-");
      const dbStatus = systemInfo.database_status || "-";
      let dbStatusText;
      if (dbStatus === "connected") {
        dbStatusText = t("system.connected");
      } else if (dbStatus.startsWith("disconnected")) {
        dbStatusText = t("system.disconnected");
      } else {
        dbStatusText = dbStatus;
      }
      setTextById("database-status", dbStatusText);
      setTextById(
        "system-time",
        systemInfo.timestamp
          ? new Date(systemInfo.timestamp).toLocaleString()
          : new Date().toLocaleString()
      );
      setTextById("system-uptime", formatUptime(systemInfo.uptime_seconds || 0));
    }
  } catch (error) {
    console.error("加载系统信息失败:", error);
  }
}

// 触发浏览器下载 apiClient 返回的 blob 结果
function downloadBlobResult(result, fallbackName) {
  if (!result.isBlob) {
    return;
  }
  const url = window.URL.createObjectURL(result.data);
  const a = document.createElement("a");
  a.href = url;
  a.download = result.filename && result.filename !== "download" ? result.filename : fallbackName;
  document.body.appendChild(a);
  a.click();
  window.URL.revokeObjectURL(url);
  document.body.removeChild(a);
}

// 导入文件大小上限（与后端一致）
const MAX_IMPORT_SIZE = 50 * 1024 * 1024;

// 前端校验上传文件的真实类型：扩展名 + magic bytes（zip 为 PK 头，
// csv 须为不含 NUL 的 UTF-8 文本），与后端校验形成双重防线
async function validateImportFile(file) {
  const name = file.name.toLowerCase();
  const isZipName = name.endsWith(".zip");
  const isCsvName = name.endsWith(".csv");
  if (!isZipName && !isCsvName) {
    showToast(t("import_export.invalid_file_type"), "error");
    return false;
  }
  if (file.size > MAX_IMPORT_SIZE) {
    showToast(t("import_export.file_too_large"), "error");
    return false;
  }
  const head = new Uint8Array(await file.slice(0, 4).arrayBuffer());
  const isZipContent = head[0] === 0x50 && head[1] === 0x4b;
  if (isZipContent !== isZipName) {
    showToast(t("import_export.invalid_file_type"), "error");
    return false;
  }
  if (isCsvName) {
    const text = await file.slice(0, 64 * 1024).text();
    if (text.includes("\u0000")) {
      showToast(t("import_export.invalid_file_type"), "error");
      return false;
    }
  }
  return true;
}

// 下载模板
export async function downloadTemplate() {
  try {
    const result = await apiRequest("/api/system/import-export/template?type=all");

    if (!result.success) {
      showToast(`${t("import_export.download_template_failed")}: ${result.message}`, "error");
      return;
    }

    downloadBlobResult(result, "template.zip");
  } catch (error) {
    console.error("下载模板失败:", error);
    showToast(`${t("import_export.download_template_failed")}: ${error.message}`, "error");
  }
}

// 导入 CSV 模块数据（ZIP 内多个 表名.csv，或按表头识别的单个 CSV）
export async function importCsvData() {
  const fileInput = document.createElement("input");
  fileInput.type = "file";
  fileInput.accept = ".zip,.csv";
  fileInput.click();

  fileInput.addEventListener("change", async (e) => {
    const file = e.target.files[0];
    if (!file) {
      return;
    }
    if (!(await validateImportFile(file))) {
      return;
    }

    const formData = new FormData();
    formData.append("file", file);

    try {
      showToast(t("import_export.importing"), "info");

      const result = await apiRequest("/api/system/import-export/import/csv", {
        method: "POST",
        body: formData
      });

      if (result.success) {
        const results = result.data?.results || [];
        if (results.length > 0) {
          await loadModal("import-result-modal");
          const contentDiv = elementCache.get("import-result-content");
          if (contentDiv) {
            contentDiv.innerHTML = results
              .map((r) =>
                t("import_export.result_item", {
                  table: r.table || "",
                  inserted: r.inserted ?? 0,
                  updated: r.updated ?? 0
                })
              )
              .map((text) => `<div class="import-result-item">${escapeHtml(text)}</div>`)
              .join("");
            await openModal("import-result-modal");
          }
        }
        showToast(t("import_export.import_success"), "success");
      } else {
        showToast(`${t("import_export.import_failed")}: ${result.message}`, "error");
      }
    } catch (error) {
      console.error("导入CSV数据失败:", error);
      showToast(`${t("import_export.import_failed")}: ${error.message}`, "error");
    }
  });
}

// 按模块导出 CSV 数据（ZIP 打包，每张业务表一个 表名.csv）
export async function exportCsvData() {
  try {
    const exportType = elementCache.getValue("export-type") || "all";
    const result = await apiRequest(
      `/api/system/import-export/export/csv?type=${encodeURIComponent(exportType)}`
    );

    if (!result.success) {
      showToast(`${t("import_export.export_failed")}: ${result.message}`, "error");
      return;
    }

    downloadBlobResult(
      result,
      `ipma-export-${exportType}-${new Date().toISOString().slice(0, 10)}.zip`
    );
  } catch (error) {
    console.error("导出CSV数据失败:", error);
    showToast(`${t("import_export.export_failed")}: ${error.message}`, "error");
  }
}

// 导出数据库 (SQL 格式)
export async function exportDatabase() {
  try {
    const result = await apiRequest("/api/system/import-export/export/database");

    if (!result.success) {
      showToast(`${t("import_export.export_db_failed")}: ${result.message}`, "error");
      return;
    }

    downloadBlobResult(result, `ipma_backup_${new Date().toISOString().slice(0, 10)}.sql`);
    showToast(t("import_export.export_db_success"), "success");
  } catch (error) {
    console.error("导出数据库失败:", error);
    showToast(`${t("import_export.export_db_failed")}: ${error.message}`, "error");
  }
}

// 备份配置
export async function backupConfig() {
  try {
    const result = await apiRequest("/api/system/config/backup");

    if (!result.success) {
      showToast(`${t("import_export.backup_failed")}: ${result.message}`, "error");
      return;
    }

    downloadBlobResult(result, `ipma-config-${new Date().toISOString().slice(0, 10)}.toml`);
  } catch (error) {
    console.error("备份配置失败:", error);
    showToast(`${t("import_export.backup_failed")}: ${error.message}`, "error");
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
    if (!file) {
      return;
    }

    const formData = new FormData();
    formData.append("file", file);

    try {
      const result = await apiRequest("/api/system/config/restore", {
        method: "POST",
        body: formData
      });

      if (result.success) {
        showToast(t("import_export.restore_success"), "success");
        // 显示重启提示，告知用户需要重启程序
        showToast(t("system.config_saved_restart_needed"), "warning");

        // 设置重启提示标记，用于后续的持续提示
        sessionStorage.setItem("configUpdated", "true");
      } else {
        showToast(`${t("import_export.restore_failed")}: ${result.message}`, "error");
      }
    } catch (error) {
      console.error("恢复配置失败:", error);
      showToast(`${t("import_export.restore_error")}: ${error.message}`, "error");
    }
  });
}

// 加载日志统计
export async function loadLogsStats() {
  try {
    const result = await apiGet("/api/system/logs/stats");
    if (result.success && result.data) {
      const stats = result.data;

      setTextById("operation-logs-count", stats.operation_logs?.count || 0);
      setTextById("login-logs-count", stats.login_logs?.count || 0);
      setTextById("notifications-count", stats.notifications?.count || 0);

      if (stats.operation_logs?.oldest) {
        setTextById(
          "operation-logs-oldest",
          `${t("logs.earliest_label")}: ${new Date(stats.operation_logs.oldest).toLocaleDateString()}`
        );
      }
      if (stats.login_logs?.oldest) {
        setTextById(
          "login-logs-oldest",
          `${t("logs.earliest_label")}: ${new Date(stats.login_logs.oldest).toLocaleDateString()}`
        );
      }
    }
  } catch (error) {
    console.error("加载日志统计失败:", error);
  }
}

// 清理日志
export async function clearLogs() {
  const logType = elementCache.getValue("clear-log-type");
  const daysInput = (elementCache.getValue("clear-log-days") || "").trim();
  const parsedDays = parseInt(daysInput, 10);

  // 空或非数字视为未填写：直接提示，不按 0（清空全部）处理，
  // 避免误触不可逆的全量清理，也避免确认文案与请求体出现 NaN/null
  if (daysInput === "" || isNaN(parsedDays)) {
    showToast(t("logs.days_required"), "warning");
    return;
  }

  // 数值钳制到合法范围：负数归 0（0 表示删除全部），上限 36500 天（约百年）
  const days = Math.min(Math.max(parsedDays, 0), 36500);

  let confirmMsg;
  if (days === 0) {
    confirmMsg = t("logs.clear_all_confirm");
  } else {
    confirmMsg = t("logs.clear_days_confirm", { days });
  }

  if (!confirmMsg) {
    confirmMsg = t("logs.clear_all_confirm");
  }

  const confirmed = await showConfirm(confirmMsg);
  if (!confirmed) {
    return;
  }

  try {
    const result = await apiPost("/api/system/logs/clear", {
      log_type: logType,
      days
    });

    if (result.success) {
      showToast(result.message, "success");
      loadLogsStats();
    } else {
      showToast(`${t("logs.clear_failed")}: ${result.message}`, "error");
    }
  } catch (error) {
    console.error("清理日志失败:", error);
    showToast(`${t("logs.clear_failed")}: ${error.message}`, "error");
  }
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

// ==================== 日志外发（syslog） ====================

// 加载外发配置（未配置时后端返回默认值：关闭）
async function loadLogForwarding() {
  try {
    const result = await apiGet("/api/system/logs/forwarding");
    if (!result.success || !result.data) {
      return;
    }
    const config = result.data;
    elementCache.setValue("log-forwarding-enabled", config.enabled ? "true" : "false");
    elementCache.setValue("log-forwarding-protocol", (config.protocol || "udp").toLowerCase());
    elementCache.setValue("log-forwarding-host", config.host || "");
    elementCache.setValue("log-forwarding-port", String(config.port || 514));
  } catch (error) {
    console.error("加载日志外发配置失败:", error);
  }
}

// 保存日志外发配置（在途标志防重复提交）
let logForwardingSaving = false;

async function saveLogForwarding() {
  if (logForwardingSaving) {
    return;
  }
  logForwardingSaving = true;
  const body = {
    enabled: elementCache.getValue("log-forwarding-enabled") === "true",
    protocol: elementCache.getValue("log-forwarding-protocol") || "udp",
    host: elementCache.getValue("log-forwarding-host").trim(),
    port: parseInt(elementCache.getValue("log-forwarding-port"), 10) || 514
  };
  try {
    const result = await apiPut("/api/system/logs/forwarding", body);
    if (result.success) {
      showToast(result.message, "success");
    } else {
      showToast(result.message, "error");
    }
  } catch (error) {
    console.error("保存日志外发配置失败:", error);
    showToast(t("common.operation_failed"), "error");
  } finally {
    logForwardingSaving = false;
  }
}

// 发送测试报文验证连通性
async function testLogForwarding() {
  try {
    const result = await apiPost("/api/system/logs/forwarding/test", {});
    if (result.success) {
      showToast(result.message, "success");
    } else {
      showToast(result.message, "error");
    }
  } catch (error) {
    console.error("测试日志外发失败:", error);
    showToast(t("common.operation_failed"), "error");
  }
}

// ==================== 等保密码策略（安全页卡片，面向全体用户的全局策略） ====================

// 加载密码策略（未配置时后端返回默认值）
async function loadPasswordPolicy() {
  try {
    const result = await apiGet("/api/system/password-policy");
    if (!result.success || !result.data) {
      return;
    }
    const policy = result.data;
    elementCache.setValue("password-policy-min-length", String(policy.min_length ?? 8));
    elementCache.setValue("password-policy-expiry", String(policy.expiry_days ?? 90));
    elementCache.setValue("password-policy-history", String(policy.history_count ?? 5));
    const setCheck = (id, value) => {
      const el = elementCache.get(id);
      if (el) {
        el.checked = Boolean(value);
      }
    };
    setCheck("password-policy-upper", policy.require_upper);
    setCheck("password-policy-lower", policy.require_lower);
    setCheck("password-policy-digit", policy.require_digit);
    setCheck("password-policy-special", policy.require_special);
  } catch (error) {
    console.error("加载密码策略失败:", error);
  }
}

// 保存密码策略（安全管理员权限；在途标志防重复提交）
let passwordPolicySaving = false;

async function savePasswordPolicy() {
  if (passwordPolicySaving) {
    return;
  }
  passwordPolicySaving = true;
  const body = {
    min_length: parseInt(elementCache.getValue("password-policy-min-length"), 10) || 8,
    expiry_days: parseInt(elementCache.getValue("password-policy-expiry"), 10) || 0,
    history_count: parseInt(elementCache.getValue("password-policy-history"), 10) || 0,
    require_upper: elementCache.get("password-policy-upper")?.checked ?? true,
    require_lower: elementCache.get("password-policy-lower")?.checked ?? true,
    require_digit: elementCache.get("password-policy-digit")?.checked ?? true,
    require_special: elementCache.get("password-policy-special")?.checked ?? false
  };
  try {
    const result = await apiPut("/api/system/password-policy", body);
    if (result.success) {
      showToast(result.message, "success");
    } else {
      showToast(result.message, "error");
    }
  } catch (error) {
    console.error("保存密码策略失败:", error);
    showToast(t("common.operation_failed"), "error");
  } finally {
    passwordPolicySaving = false;
  }
}

// ==================== 证书管理（生成 /etc/ssl/ipma-certs，导入 /etc/ssl/ipma-import-certs，
// 站点根 CA /etc/ssl/ipma-ca，导入 CA 池 /etc/ssl/ipma-import-cas） ====================

// 可选字段：空串转 null
function certOptionalField(value) {
  const s = (value || "").trim();
  return s ? s : null;
}

function formatCertValidity(info) {
  if (!info.not_before || !info.not_after) {
    return "-";
  }
  const fmt = (iso) => new Date(iso).toLocaleDateString();
  return `${fmt(info.not_before)} ~ ${fmt(info.not_after)}`;
}

// CA 下拉选项文案：根 CA（自生成）在前，导入 CA 标注来源；无私钥的 CA 不可签发
function caOptionLabel(ca) {
  const source = ca.source === "root" ? t("cert.ca_source_root") : t("cert.ca_source_imported");
  const name = ca.name || ca.id;
  return ca.has_key ? `${name}（${source}）` : `${name}（${source}，${t("cert.ca_no_key")}）`;
}

// 单行证书记录：补齐证书类型（自生成 = 站点 CA 签发，导入 = 外部证书）
function decorateCertItem(info, kind) {
  return { ...info, kind };
}

// 证书剩余天数的展示文案与样式类（证书表与 CA 信息两处渲染共用）
function certDaysText(days) {
  return days === null || days === undefined ? "-" : String(days);
}

function certDaysClass(days) {
  // 刻意双判 null 与 undefined：后端 days_remaining 可能缺省（undefined）或显式为 null，
  // 两种情况都表示"无剩余天数数据"，不加任何样式类
  if (days == null) {
    return "";
  }
  if (days < 0) {
    return "cert-days-expired";
  }
  return days < 30 ? "cert-days-warning" : "";
}

// 渲染合并后的证书表格：自生成在前、导入在后，以"证书类型"列区分；
// 删除操作按行内 kind 调用对应端点
function renderCertTable(items) {
  const tbody = document.querySelector("#cert-list-table tbody");
  if (!tbody) {
    return;
  }

  if (!items || items.length === 0) {
    tbody.innerHTML = `<tr class="empty-row"><td colspan="6" class="text-center">${t("common.no_data")}</td></tr>`;
    return;
  }

  tbody.innerHTML = "";
  items.forEach((info) => {
    const tr = document.createElement("tr");

    const days = info.days_remaining;
    const daysText = certDaysText(days);
    const daysClass = certDaysClass(days);
    const typeClass = info.kind === "generated" ? "cert-type-generated" : "cert-type-imported";
    const typeText = info.kind === "generated" ? t("cert.type_generated") : t("cert.type_imported");

    tr.innerHTML = `
      <td title="${escapeHtml(info.cert_filename)}">${escapeHtml(info.file_stem)}</td>
      <td><span class="cert-type-badge ${typeClass}">${escapeHtml(typeText)}</span></td>
      <td>${escapeHtml(info.subject_cn || "-")}</td>
      <td>${formatCertValidity(info)}</td>
      <td class="${daysClass}">${daysText}</td>
      <td class="cert-actions-cell"></td>`;

    const actions = tr.querySelector(".cert-actions-cell");

    // 证书仅用于程序运行（HTTPS/nginx），不提供下载；删除按钮直接操作服务器文件
    const deleteBtn = document.createElement("button");
    deleteBtn.type = "button";
    deleteBtn.className = "btn btn-danger btn-sm";
    deleteBtn.textContent = t("common.delete");
    deleteBtn.addEventListener("click", async () => {
      const confirmed = await showConfirm(t("cert.delete_confirm", { file: info.file_stem }));
      if (!confirmed) {
        return;
      }
      try {
        const result = await apiRequest(
          `/api/system/certificate/${info.kind}/${encodeURIComponent(info.file_stem)}`,
          { method: "DELETE" }
        );
        if (result.success) {
          showToast(result.message, "success");
          loadCertificateInventory();
        } else {
          showToast(result.message, "error");
        }
      } catch (error) {
        console.error("删除证书失败:", error);
        showToast(t("cert.delete_failed"), "error");
      }
    });
    actions.appendChild(deleteBtn);

    tbody.appendChild(tr);
  });
}

// 最近一次拉取的 CA 列表（证书生成弹窗下拉数据源）
let lastCaList = [];

export async function loadCertificateInventory() {
  try {
    const result = await apiGet("/api/system/certificate/list");
    if (!result.success || !result.data) {
      return;
    }
    lastCaList = Array.isArray(result.data.cas) ? result.data.cas : [];
    // 自生成与导入证书合并展示，以"证书类型"列区分
    const merged = [
      ...(result.data.generated || []).map((info) => decorateCertItem(info, "generated")),
      ...(result.data.imported || []).map((info) => decorateCertItem(info, "imported"))
    ];
    renderCertTable(merged);
    renderCaStatus(result.data.ca);
  } catch (error) {
    console.error("加载证书列表失败:", error);
  }
}

// 渲染站点根 CA 状态（无 CA 时提示未配置）
function renderCaStatus(ca) {
  const container = elementCache.get("cert-ca-status");
  if (!container) {
    return;
  }

  if (!ca || !ca.available) {
    container.innerHTML = `<span class="cert-ca-missing">${t("cert.ca_none")}</span>`;
    return;
  }

  const days = ca.days_remaining;
  const daysText = certDaysText(days);
  const daysClass = certDaysClass(days);
  const keyText = ca.has_key ? t("cert.ca_has_key") : t("cert.ca_no_key");

  container.innerHTML = `
    <span class="cert-ca-field"><strong>${escapeHtml(ca.subject_cn || "-")}</strong></span>
    <span class="cert-ca-field">${formatCertValidity(ca)}</span>
    <span class="cert-ca-field ${daysClass}">${t("cert.col_remaining")}: ${daysText}</span>
    <span class="cert-ca-field">${keyText}</span>`;
}

// 下载站点根 CA（仅 PEM 格式；CA 证书是公开数据）
function downloadCa() {
  window.open("/api/certificate/ca/download", "_blank");
}

// 证书四个表单的在途标志：请求期间重复提交直接忽略（防双击重复签发/导入）
let caGenerateSaving = false;

async function openCaGenerateModal() {
  const modal = await loadModal("ca-generate-modal");
  if (!modal) {
    return;
  }

  const confirmed = await showConfirm(t("cert.ca_generate_confirm"));
  if (!confirmed) {
    closeModal("ca-generate-modal");
    return;
  }

  const form = elementCache.get("ca-generate-form");
  form.onsubmit = async (e) => {
    e.preventDefault();
    if (caGenerateSaving) {
      return;
    }
    caGenerateSaving = true;
    try {
      const fd = new FormData(form);
      const body = {
        common_name: String(fd.get("common_name") || "").trim(),
        organization: certOptionalField(fd.get("organization")),
        organizational_unit: certOptionalField(fd.get("organizational_unit")),
        country: certOptionalField(fd.get("country")),
        state: certOptionalField(fd.get("state")),
        locality: certOptionalField(fd.get("locality")),
        validity_days: parseInt(fd.get("validity_days"), 10) || null
      };
      if (!body.common_name) {
        showToast(t("cert.common_name_required"), "warning");
        return;
      }

      const result = await apiPost("/api/system/certificate/ca/generate", body);
      if (result.success) {
        showToast(result.message, "success");
        closeModal("ca-generate-modal");
        form.reset();
        loadCertificateInventory();
      } else {
        showToast(result.message, "error");
      }
    } finally {
      caGenerateSaving = false;
    }
  };

  openModal("ca-generate-modal");
}

let caImportSaving = false;

async function openCaImportModal() {
  const modal = await loadModal("ca-import-modal");
  if (!modal) {
    return;
  }

  const form = elementCache.get("ca-import-form");
  form.onsubmit = async (e) => {
    e.preventDefault();
    if (caImportSaving) {
      return;
    }
    caImportSaving = true;
    try {
      const caFile = elementCache.get("ca-file")?.files[0];
      const caKeyFile = elementCache.get("ca-key-file")?.files[0];
      if (!caFile || !caKeyFile) {
        showToast(t("cert.file_required"), "warning");
        return;
      }

      const fd = new FormData();
      fd.append("cert", caFile);
      fd.append("key", caKeyFile);

      const result = await apiRequest("/api/system/certificate/ca/import", {
        method: "POST",
        body: fd
      });
      if (result.success) {
        showToast(result.message, "success");
        closeModal("ca-import-modal");
        form.reset();
        loadCertificateInventory();
      } else {
        showToast(result.message, "error");
      }
    } catch (error) {
      console.error("导入CA失败:", error);
      showToast(t("cert.import_failed"), "error");
    } finally {
      caImportSaving = false;
    }
  };

  openModal("ca-import-modal");
}

// 填充证书生成弹窗的 CA 下拉框：根 CA（自生成）在前，导入 CA 在后；
// 无私钥的 CA（cert-only 导入）禁用选择
function fillCertCaSelect(select) {
  const cas = lastCaList;
  if (!cas.length) {
    select.innerHTML = `<option value="">${t("cert.ca_none")}</option>`;
    return;
  }
  select.innerHTML = cas
    .map((ca) => {
      const disabled = ca.has_key ? "" : "disabled";
      const selected = ca.source === "root" && ca.has_key ? "selected" : "";
      return `<option value="${escapeHtml(ca.id)}" ${disabled} ${selected}>${escapeHtml(caOptionLabel(ca))}</option>`;
    })
    .join("");
  // 根 CA 不可用（无私钥）时默认选第一个可用 CA
  if (!select.value) {
    const firstUsable = cas.find((ca) => ca.has_key);
    if (firstUsable) {
      select.value = firstUsable.id;
    }
  }
}

let certGenerateSaving = false;

async function openCertGenerateModal() {
  const modal = await loadModal("cert-generate-modal");
  if (!modal) {
    return;
  }

  const caSelect = elementCache.get("cert-ca-select");
  if (caSelect) {
    // 拉取最新 CA 列表（生成/导入 CA 后立即反映到下拉框）
    await loadCertificateInventory();
    fillCertCaSelect(caSelect);
  }

  const form = elementCache.get("cert-generate-form");
  form.onsubmit = async (e) => {
    e.preventDefault();
    if (certGenerateSaving) {
      return;
    }
    certGenerateSaving = true;
    try {
      const fd = new FormData(form);
      const sans = String(fd.get("subject_alt_names") || "")
        .split(",")
        .map((s) => s.trim())
        .filter(Boolean);

      const caId = String(fd.get("ca_id") || "").trim();
      if (!caId) {
        showToast(t("cert.ca_required"), "warning");
        return;
      }

      const body = {
        common_name: String(fd.get("common_name") || "").trim(),
        organization: certOptionalField(fd.get("organization")),
        organizational_unit: certOptionalField(fd.get("organizational_unit")),
        country: certOptionalField(fd.get("country")),
        state: certOptionalField(fd.get("state")),
        locality: certOptionalField(fd.get("locality")),
        validity_days: parseInt(fd.get("validity_days"), 10) || null,
        subject_alt_names: sans.length > 0 ? sans : null,
        ca_id: caId
      };
      if (!body.common_name) {
        showToast(t("cert.common_name_required"), "warning");
        return;
      }

      const result = await apiPost("/api/system/certificate/generate", body);
      if (result.success) {
        showToast(result.message, "success");
        closeModal("cert-generate-modal");
        form.reset();
        loadCertificateInventory();
      } else {
        showToast(result.message, "error");
      }
    } finally {
      certGenerateSaving = false;
    }
  };

  openModal("cert-generate-modal");
}

let certImportSaving = false;

async function openCertImportModal() {
  const modal = await loadModal("cert-import-modal");
  if (!modal) {
    return;
  }

  const form = elementCache.get("cert-import-form");
  form.onsubmit = async (e) => {
    e.preventDefault();
    if (certImportSaving) {
      return;
    }
    certImportSaving = true;
    try {
      const certFile = elementCache.get("cert-file")?.files[0];
      const keyFile = elementCache.get("key-file")?.files[0];
      if (!certFile || !keyFile) {
        showToast(t("cert.file_required"), "warning");
        return;
      }

      const fd = new FormData();
      fd.append("cert", certFile);
      fd.append("key", keyFile);
      // 附带的 CA（可选）仅供终端导出信任，不参与证书/私钥校验
      const caFile = elementCache.get("cert-ca-file")?.files[0];
      if (caFile) {
        fd.append("ca", caFile);
      }

      const result = await apiRequest("/api/system/certificate/import", {
        method: "POST",
        body: fd
      });
      if (result.success) {
        showToast(result.message, "success");
        closeModal("cert-import-modal");
        form.reset();
        loadCertificateInventory();
      } else {
        showToast(result.message, "error");
      }
    } catch (error) {
      console.error("导入证书失败:", error);
      showToast(t("cert.import_failed"), "error");
    } finally {
      certImportSaving = false;
    }
  };

  openModal("cert-import-modal");
}

function initCertificateManager() {
  const generateBtn = elementCache.get("cert-generate-btn");
  if (generateBtn) {
    generateBtn.addEventListener("click", openCertGenerateModal);
  }

  const importBtn = elementCache.get("cert-import-btn");
  if (importBtn) {
    importBtn.addEventListener("click", openCertImportModal);
  }

  const refreshBtn = elementCache.get("cert-refresh-btn");
  if (refreshBtn) {
    refreshBtn.addEventListener("click", loadCertificateInventory);
  }

  const caGenerateBtn = elementCache.get("ca-generate-btn");
  if (caGenerateBtn) {
    caGenerateBtn.addEventListener("click", openCaGenerateModal);
  }

  const caImportBtn = elementCache.get("ca-import-btn");
  if (caImportBtn) {
    caImportBtn.addEventListener("click", openCaImportModal);
  }

  const caDownloadBtn = elementCache.get("ca-download-btn");
  if (caDownloadBtn) {
    caDownloadBtn.addEventListener("click", downloadCa);
  }
}
