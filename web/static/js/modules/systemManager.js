import { apiRequest, apiGet, apiPut, apiPost, } from "../utils/apiClient.js";
function isBlobResponse(result) {
    return result.success === true && 'isBlob' in result && result.isBlob === true;
}
import { showToast, } from "../utils/ui.js";
import { openModal, closeModal } from "../utils/modal.js";
import { loadModal } from "../utils/modalLoader.js";
import { t } from "../utils/i18n.js";
import { loadUsersData } from "./userManager.js";
import { elementCache } from "../utils/helpers.js";
let currentServerConfig = null;
export function initSystemTabs() {
    const systemContainer = elementCache.get("system");
    if (!systemContainer)
        return;
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
                }
                else if (tabId === "system-info") {
                    loadSystemInfo();
                    loadSystemConfig();
                }
                else if (tabId === "system-smtp") {
                    loadSmtpConfig();
                    loadNotificationSettings();
                }
                else if (tabId === "import-export") {
                    loadLogsStats();
                }
                else if (tabId === "scheduled-tasks") {
                    initScheduledTasksTab();
                }
            });
        });
        systemContainer.dataset.tabsInitialized = "true";
    }
    if (systemContainer.dataset.eventsInitialized === "true")
        return;
    const systemConfigForm = elementCache.get("system-config-form");
    if (systemConfigForm) {
        systemConfigForm.addEventListener("submit", async (e) => {
            e.preventDefault();
            try {
                await saveSystemConfig();
            }
            catch (err) {
                console.error("saveSystemConfig error:", err);
            }
        });
    }
    const autoHttpsToggle = elementCache.get("auto-https");
    if (autoHttpsToggle) {
        autoHttpsToggle.addEventListener("change", handleAutoHttpsChange);
    }
    const httpEnabledToggle = elementCache.get("http-enabled");
    if (httpEnabledToggle) {
        httpEnabledToggle.addEventListener("change", handlePortDisable);
    }
    const httpsEnabledToggle = elementCache.get("https-enabled");
    if (httpsEnabledToggle) {
        httpsEnabledToggle.addEventListener("change", handlePortDisable);
    }
    const certTypeSelect = elementCache.get("cert-type");
    if (certTypeSelect) {
        certTypeSelect.addEventListener("change", handleCertTypeChange);
    }
    const updateCertBtn = elementCache.get("update-cert-btn");
    if (updateCertBtn) {
        updateCertBtn.addEventListener("click", showCertGenerateModal);
    }
    const importCertBtn = elementCache.get("import-cert-btn");
    if (importCertBtn) {
        importCertBtn.addEventListener("click", showCertImportModal);
    }
    const downloadCaBtn = elementCache.get("download-ca-btn");
    if (downloadCaBtn) {
        downloadCaBtn.addEventListener("click", downloadCertificate);
    }
    const certGenerateForm = elementCache.get("cert-generate-form");
    if (certGenerateForm) {
        certGenerateForm.addEventListener("submit", async (e) => {
            e.preventDefault();
            await generateSelfSignedCert();
        });
    }
    const certImportForm = elementCache.get("cert-import-form");
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
function formatUptime(seconds) {
    const days = Math.floor(seconds / (24 * 60 * 60));
    seconds %= 24 * 60 * 60;
    const hours = Math.floor(seconds / (60 * 60));
    seconds %= 60 * 60;
    const minutes = Math.floor(seconds / 60);
    seconds %= 60;
    let result = "";
    if (days > 0)
        result += `${days}天`;
    if (hours > 0)
        result += `${hours}小时`;
    if (minutes > 0)
        result += `${minutes}分钟`;
    result += `${seconds}秒`;
    return result;
}
async function saveSystemConfig() {
    try {
        const httpEnabled = elementCache.getChecked("http-enabled");
        const httpsEnabled = elementCache.getChecked("https-enabled");
        if (!httpEnabled && !httpsEnabled) {
            showToast("至少需要开启一个端口（HTTP或HTTPS）", "warning");
            return;
        }
        const serverConfig = {
            ...currentServerConfig,
            host: elementCache.getValue("server-host"),
            host_ipv6: elementCache.getValue("server-host-ipv6") || null,
            http_enabled: httpEnabled,
            http_port: parseInt(elementCache.getValue("http-port")) || 80,
            https_enabled: httpsEnabled,
            https_port: parseInt(elementCache.getValue("https-port")) || 443,
            auto_https: elementCache.getChecked("auto-https"),
            http_version: elementCache.getValue("http-version") || "http1.1",
            cert_type: elementCache.getValue("cert-type") || "self_signed",
            page_timeout: parseInt(elementCache.getValue("page-timeout")) || 30,
            public_url: elementCache.getValue("public-url") || "",
        };
        const rateLimitConfig = {
            enabled: elementCache.getChecked("rate-limit-enabled"),
            ip_limit: parseInt(elementCache.getValue("rate-limit-ip")) || 100,
            user_limit: parseInt(elementCache.getValue("rate-limit-user")) || 200,
            login_limit: parseInt(elementCache.getValue("rate-limit-login")) || 5,
            window_secs: parseInt(elementCache.getValue("rate-limit-window")) || 60,
        };
        const config = {
            server: serverConfig,
            rate_limit: rateLimitConfig,
        };
        const result = await apiPut("/api/system/config", config);
        if (result.success) {
            await loadSystemConfig();
            showToast("系统配置保存成功", "success");
            showToast("配置已更新，需要重启应用系统以使配置生效", "warning");
            sessionStorage.setItem("configUpdated", "true");
        }
        else {
            showToast("系统配置保存失败: " + result.message, "error");
        }
    }
    catch (error) {
        console.error("保存系统配置失败:", error);
        showToast("系统配置保存失败: " + error.message, "error");
    }
}
function checkConfigUpdateRestartPrompt() {
    if (sessionStorage.getItem("configUpdated") === "true") {
        showToast("配置已更新，需要重启应用系统以使配置生效", "warning");
    }
}
function clearConfigUpdateFlag() {
    sessionStorage.removeItem("configUpdated");
}
export async function loadSystemConfig() {
    try {
        const result = await apiGet("/api/system/info");
        if (result.success) {
            const systemInfo = result.data;
            const config = systemInfo.config;
            currentServerConfig = config.server;
            elementCache.setValue("server-host", config.server.host);
            elementCache.setValue("server-host-ipv6", config.server.host_ipv6 || "");
            elementCache.get("http-enabled").checked = config.server.http_enabled || false;
            elementCache.setValue("http-port", String(config.server.http_port || 80));
            elementCache.get("https-enabled").checked = config.server.https_enabled || false;
            elementCache.setValue("https-port", String(config.server.https_port || 443));
            elementCache.get("auto-https").checked = config.server.auto_https || false;
            elementCache.setValue("http-version", config.server.http_version || "http1.1");
            const certType = config.server.cert_type || "self_signed";
            elementCache.setValue("cert-type", certType);
            elementCache.setValue("page-timeout", String(config.server.page_timeout || 30));
            elementCache.setValue("public-url", config.server.public_url || "");
            if (config.rate_limit) {
                elementCache.get("rate-limit-enabled").checked = config.rate_limit.enabled !== false;
                elementCache.setValue("rate-limit-ip", String(config.rate_limit.ip_limit || 100));
                elementCache.setValue("rate-limit-user", String(config.rate_limit.user_limit || 200));
                elementCache.setValue("rate-limit-login", String(config.rate_limit.login_limit || 5));
                elementCache.setValue("rate-limit-window", String(config.rate_limit.window_secs || 60));
            }
            handleCertTypeChange();
            checkImportedCertificate();
            updateCertificateSectionVisibility();
            checkConfigUpdateRestartPrompt();
            checkServiceStatus();
        }
    }
    catch (error) {
        console.error("加载系统配置失败:", error);
    }
}
async function checkImportedCertificate() {
    try {
        const result = await apiGet("/api/system/certificate/status");
        if (result.success) {
            const hasImportedCert = result.data.has_imported_cert;
            const importBtn = elementCache.get("import-cert-btn");
            if (importBtn) {
                importBtn.textContent = hasImportedCert ? "更新" : "导入";
            }
        }
    }
    catch (error) {
        console.error("检查证书状态失败:", error);
    }
}
function updateCertificateSectionVisibility() {
    const httpsEnabled = elementCache.get("https-enabled").checked;
    const certificateSection = elementCache.get("certificate-section");
    if (certificateSection) {
        certificateSection.style.display = httpsEnabled ? "block" : "none";
    }
}
function handleAutoHttpsChange() {
    const autoHttps = elementCache.getChecked("auto-https");
    if (autoHttps) {
        elementCache.setChecked("http-enabled", true);
        elementCache.setChecked("https-enabled", true);
        updateCertificateSectionVisibility();
    }
}
function handlePortDisable(e) {
    const httpEnabled = elementCache.getChecked("http-enabled");
    const httpsEnabled = elementCache.getChecked("https-enabled");
    if (!httpEnabled && !httpsEnabled) {
        if (e && e.target) {
            e.target.checked = true;
            showToast("至少需要开启一个端口（HTTP或HTTPS）", "warning");
        }
        return;
    }
    if (!httpEnabled || !httpsEnabled) {
        elementCache.setChecked("auto-https", false);
    }
    updateCertificateSectionVisibility();
}
function handleCertTypeChange() {
    const certType = elementCache.getValue("cert-type");
    const updateCertBtn = elementCache.get("update-cert-btn");
    const importCertBtn = elementCache.get("import-cert-btn");
    const downloadCaBtn = elementCache.get("download-ca-btn");
    if (certType === "self_signed") {
        if (updateCertBtn)
            updateCertBtn.style.display = "inline-block";
        if (downloadCaBtn)
            downloadCaBtn.style.display = "inline-block";
        if (importCertBtn)
            importCertBtn.style.display = "none";
    }
    else if (certType === "imported") {
        if (updateCertBtn)
            updateCertBtn.style.display = "none";
        if (downloadCaBtn)
            downloadCaBtn.style.display = "none";
        if (importCertBtn)
            importCertBtn.style.display = "inline-block";
        checkImportedCertificate();
    }
}
function showCertGenerateModal() {
    loadModal("cert-generate-modal");
    openModal("cert-generate-modal");
}
function showCertImportModal() {
    loadModal("cert-import-modal");
    openModal("cert-import-modal");
}
async function downloadCertificate() {
    try {
        const result = await apiRequest("/api/system/certificate/download");
        if (!result.success) {
            showToast("下载证书失败: " + result.message, "error");
            return;
        }
        if (isBlobResponse(result)) {
            const blobResult = result;
            const url = window.URL.createObjectURL(blobResult.data);
            const a = document.createElement("a");
            a.href = url;
            a.download = blobResult.filename || "ca.pem";
            document.body.appendChild(a);
            a.click();
            window.URL.revokeObjectURL(url);
            document.body.removeChild(a);
        }
    }
    catch (error) {
        console.error("下载证书失败:", error);
        showToast("下载证书失败: " + error.message, "error");
    }
}
async function generateSelfSignedCert() {
    try {
        const form = elementCache.get("cert-generate-form");
        const formData = new FormData(form);
        const certData = {
            common_name: formData.get("common_name"),
            organization: formData.get("organization"),
            organizational_unit: formData.get("organizational_unit"),
            country: formData.get("country"),
            state: formData.get("state"),
            locality: formData.get("locality"),
            validity: parseInt(formData.get("validity")) || 365,
        };
        const result = await apiPost("/api/system/certificate/generate", certData);
        if (result.success) {
            showToast("自签名证书生成成功", "success");
            closeModal("cert-generate-modal");
            form.reset();
        }
        else {
            showToast("证书生成失败: " + result.message, "error");
        }
    }
    catch (error) {
        console.error("生成证书失败:", error);
        showToast("证书生成失败: " + error.message, "error");
    }
}
async function importCertificate() {
    try {
        const form = elementCache.get("cert-import-form");
        const formData = new FormData(form);
        const result = await apiRequest("/api/system/certificate/import", {
            method: "POST",
            body: formData,
        });
        if (result.success) {
            showToast("证书导入成功", "success");
            closeModal("cert-import-modal");
            form.reset();
            const importCertBtn = elementCache.get("import-cert-btn");
            if (importCertBtn)
                importCertBtn.textContent = "更新";
        }
        else {
            showToast("证书导入失败: " + result.message, "error");
        }
    }
    catch (error) {
        console.error("导入证书失败:", error);
        showToast("证书导入失败: " + error.message, "error");
    }
}
async function checkServiceStatus() {
    try {
        const result = await apiGet("/api/system/service-status");
        if (result.success) {
            const data = result.data;
            const registerBtn = elementCache.get("register-service-btn");
            const restartBtn = elementCache.get("restart-app-btn");
            const serviceStatusEl = elementCache.get("service-status-info");
            if (registerBtn) {
                if (data.registered) {
                    registerBtn.setAttribute("disabled", "true");
                    registerBtn.textContent = "已注册为服务";
                    registerBtn.classList.add("btn-success");
                    registerBtn.classList.remove("btn-secondary");
                }
                else {
                    registerBtn.removeAttribute("disabled");
                    registerBtn.textContent = "注册为服务";
                    registerBtn.classList.add("btn-secondary");
                    registerBtn.classList.remove("btn-success");
                }
            }
            if (serviceStatusEl) {
                let statusHtml = '<div class="service-status-grid">';
                statusHtml += `
          <div class="status-item">
            <span class="status-label">运行模式:</span>
            <span class="status-value">${data.running_as_service ? "系统服务" : "独立进程"}</span>
          </div>
        `;
                if (data.service_file_exists) {
                    statusHtml += `
            <div class="status-item">
              <span class="status-label">服务状态:</span>
              <span class="status-value ${data.active ? "status-active" : "status-inactive"}">
                ${data.status || (data.active ? "运行中" : "已停止")}
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
              <span class="status-value ${data.enabled ? "status-enabled" : "status-disabled"}">
                ${data.enabled ? "已启用" : "已禁用"}
              </span>
            </div>
          `;
                }
                statusHtml += "</div>";
                serviceStatusEl.innerHTML = statusHtml;
            }
            if (restartBtn) {
                restartBtn.textContent = data.running_as_service ? "重启服务" : "重启程序";
            }
        }
    }
    catch (error) {
        console.error("检查服务状态失败:", error);
    }
}
export async function registerService() {
    if (confirm("确定要将系统注册为服务吗？此操作将在系统启动时自动运行IPMA服务。")) {
        try {
            const result = await apiPost("/api/system/register-service", {});
            if (result.success) {
                showToast("注册为服务成功", "success");
                checkServiceStatus();
            }
            else {
                showToast("注册为服务失败: " + result.message, "error");
            }
        }
        catch (error) {
            console.error("注册为服务失败:", error);
            showToast("注册为服务失败: " + error.message, "error");
        }
    }
}
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
            }
            else {
                showToast("重启失败: " + result.message, "error");
            }
        }
        catch (error) {
            console.error("重启失败:", error);
            showToast("重启失败: " + error.message, "error");
        }
    }
}
async function checkIfRunningAsService() {
    try {
        const result = await apiGet("/api/system/service-status");
        return result.success && result.data.running_as_service;
    }
    catch {
        return false;
    }
}
export async function restartOs() {
    if (confirm("确定要重启操作系统吗？重启过程中所有服务将暂时不可用。")) {
        clearConfigUpdateFlag();
        try {
            const result = await apiPost("/api/system/restart-os", {});
            if (result.success) {
                showToast("操作系统重启命令已发送", "success");
                showToast("操作系统正在重启，请稍候...", "info");
                setTimeout(() => {
                    location.reload();
                }, 5000);
            }
            else {
                showToast("重启操作系统失败: " + result.message, "error");
            }
        }
        catch (error) {
            console.error("重启操作系统失败:", error);
            showToast("重启操作系统失败: " + error.message, "error");
        }
    }
}
async function loadSmtpConfig() {
    try {
        const result = await apiGet("/api/system/smtp/config");
        if (result.success) {
            const smtpConfig = result.data;
            if (smtpConfig) {
                elementCache.setValue("smtp-host", smtpConfig.host || "");
                elementCache.setValue("smtp-port", String(smtpConfig.port || "587"));
                elementCache.setValue("smtp-username", smtpConfig.username || "");
                elementCache.setValue("smtp-password", smtpConfig.password || "");
                elementCache.setValue("smtp-from", smtpConfig.from || "");
                const secureTypeElement = elementCache.get("smtp-secure-type");
                if (secureTypeElement) {
                    secureTypeElement.value = smtpConfig.secure ? "ssl" : "none";
                }
            }
            else {
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
    }
    catch (error) {
        console.error("加载SMTP配置失败:", error);
    }
}
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
        }
        else {
            showToast("SMTP配置保存失败: " + result.message, "error");
        }
    }
    catch (error) {
        console.error("保存SMTP配置失败:", error);
        showToast("SMTP配置保存失败: " + error.message, "error");
    }
}
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
        }
        else {
            showToast("SMTP连接测试失败: " + result.message, "error");
        }
    }
    catch (error) {
        console.error("测试SMTP连接失败:", error);
        showToast("测试SMTP连接失败: " + error.message, "error");
    }
}
export function initSmtpFunctions() {
    // test-smtp-btn is bound in eventManager.js
}
export async function loadNotificationSettings() {
    try {
        const usersResult = await apiGet("/api/users");
        const settingsResult = await apiGet("/api/system/notification/settings");
        const usersList = elementCache.get("notification-users-list");
        if (!usersList)
            return;
        usersList.innerHTML = "";
        if (!usersResult.success || !usersResult.data)
            return;
        let users = [];
        if (Array.isArray(usersResult.data)) {
            users = usersResult.data;
        }
        else if (usersResult.data.items && Array.isArray(usersResult.data.items)) {
            users = usersResult.data.items;
        }
        const selectedRecipients = settingsResult.success && settingsResult.data
            ? (settingsResult.data.email_recipients || [])
            : [];
        users.forEach((user) => {
            const item = document.createElement("div");
            item.className = "user-checkbox-item";
            const checkbox = document.createElement("input");
            checkbox.type = "checkbox";
            checkbox.id = `notify-user-${user.id}`;
            checkbox.value = String(user.id);
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
    }
    catch (error) {
        console.error("加载通知设置失败:", error);
    }
}
export async function saveNotificationSettings() {
    try {
        const checkboxes = document.querySelectorAll("#notification-users-list input[type='checkbox']:checked");
        const userIds = Array.from(checkboxes).map((cb) => cb.value);
        const result = await apiPut("/api/system/notification/settings", {
            email_recipients: userIds,
        });
        if (result.success) {
            showToast("通知设置保存成功", "success");
        }
        else {
            showToast("通知设置保存失败: " + result.message, "error");
        }
    }
    catch (error) {
        console.error("保存通知设置失败:", error);
        showToast("保存通知设置失败: " + error.message, "error");
    }
}
export async function loadSystemInfo() {
    try {
        const result = await apiGet("/api/system/info");
        if (result.success) {
            const systemInfo = result.data;
            elementCache.get("system-name").textContent = systemInfo.name;
            elementCache.get("system-version").textContent = systemInfo.version;
            elementCache.get("database-status").textContent = systemInfo.database_status;
            elementCache.get("system-time").textContent = new Date(systemInfo.timestamp).toLocaleString();
            elementCache.get("system-uptime").textContent = formatUptime(systemInfo.uptime);
        }
    }
    catch (error) {
        console.error("加载系统信息失败:", error);
    }
}
export async function downloadTemplate() {
    try {
        const result = await apiRequest("/api/system/import-export/template?type=all");
        if (!result.success) {
            showToast("下载模板失败: " + result.message, "error");
            return;
        }
        if (isBlobResponse(result)) {
            const blobResult = result;
            const url = window.URL.createObjectURL(blobResult.data);
            const a = document.createElement("a");
            a.href = url;
            a.download = blobResult.filename || "template.zip";
            document.body.appendChild(a);
            a.click();
            window.URL.revokeObjectURL(url);
            document.body.removeChild(a);
        }
    }
    catch (error) {
        console.error("下载模板失败:", error);
        showToast("下载模板失败: " + error.message, "error");
    }
}
export async function importCsvData() {
    const modeSelect = elementCache.get("csv-import-mode");
    const mode = modeSelect ? modeSelect.value : "skip";
    const fileInput = document.createElement("input");
    fileInput.type = "file";
    fileInput.accept = ".zip,.csv";
    fileInput.click();
    fileInput.addEventListener("change", async (e) => {
        const file = e.target.files?.[0];
        if (!file)
            return;
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
                    loadModal("import-result-modal");
                    const contentDiv = elementCache.get("import-result-content");
                    if (contentDiv) {
                        contentDiv.innerHTML = results.map((r) => `<div class="import-result-item">${r}</div>`).join("");
                        openModal("import-result-modal");
                    }
                }
                showToast("数据导入完成", "success");
            }
            else {
                showToast("数据导入失败: " + result.message, "error");
            }
        }
        catch (error) {
            console.error("导入CSV数据失败:", error);
            showToast("数据导入失败: " + error.message, "error");
        }
    });
}
export async function exportCsvData() {
    try {
        const exportType = elementCache.getValue("csv-export-type");
        const result = await apiRequest(`/api/system/import-export/export/csv?type=${exportType}`);
        if (!result.success) {
            showToast("导出CSV数据失败: " + result.message, "error");
            return;
        }
        if (isBlobResponse(result)) {
            const blobResult = result;
            const url = window.URL.createObjectURL(blobResult.data);
            const a = document.createElement("a");
            a.href = url;
            a.download = blobResult.filename && blobResult.filename !== "download"
                ? blobResult.filename
                : `ipma-export-${exportType}-${new Date().toISOString().slice(0, 10)}.zip`;
            document.body.appendChild(a);
            a.click();
            window.URL.revokeObjectURL(url);
            document.body.removeChild(a);
        }
    }
    catch (error) {
        console.error("导出CSV数据失败:", error);
        showToast("导出CSV数据失败: " + error.message, "error");
    }
}
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
            }
            catch (_e) { /* ignore */ }
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
            if (matches != null && matches[1]) {
                filename = matches[1].replace(/['"]/g, "");
            }
        }
        a.download = filename;
        document.body.appendChild(a);
        a.click();
        window.URL.revokeObjectURL(url);
        document.body.removeChild(a);
        showToast("数据库导出成功", "success");
    }
    catch (error) {
        console.error("导出数据库失败:", error);
        showToast("导出数据库失败: " + error.message, "error");
    }
}
export async function backupConfig() {
    try {
        const result = await apiRequest("/api/system/config/backup");
        if (!result.success) {
            showToast("备份配置失败: " + result.message, "error");
            return;
        }
        if (isBlobResponse(result)) {
            const blobResult = result;
            const url = window.URL.createObjectURL(blobResult.data);
            const a = document.createElement("a");
            a.href = url;
            a.download = blobResult.filename && blobResult.filename !== "download"
                ? blobResult.filename
                : `ipma-config-${new Date().toISOString().slice(0, 10)}.toml`;
            document.body.appendChild(a);
            a.click();
            window.URL.revokeObjectURL(url);
            document.body.removeChild(a);
        }
    }
    catch (error) {
        console.error("备份配置失败:", error);
        showToast("备份配置失败: " + error.message, "error");
    }
}
export function restoreConfig() {
    const fileInput = document.createElement("input");
    fileInput.type = "file";
    fileInput.accept = ".toml";
    fileInput.click();
    fileInput.addEventListener("change", async (e) => {
        const file = e.target.files?.[0];
        if (!file)
            return;
        const formData = new FormData();
        formData.append("file", file);
        try {
            const result = await apiRequest("/api/system/config/restore", {
                method: "POST",
                body: formData,
            });
            if (result.success) {
                showToast("配置恢复成功", "success");
                showToast("配置已更新，需要重启应用系统以使配置生效", "warning");
                sessionStorage.setItem("configUpdated", "true");
            }
            else {
                showToast("配置恢复失败: " + result.message, "error");
            }
        }
        catch (error) {
            console.error("恢复配置失败:", error);
            showToast("恢复配置失败: " + error.message, "error");
        }
    });
}
export async function loadLogsStats() {
    try {
        const result = await apiGet("/api/system/logs/stats");
        if (result.success && result.data) {
            const stats = result.data;
            elementCache.get("operation-logs-count").textContent = String(stats.operation_logs?.count || 0);
            elementCache.get("login-logs-count").textContent = String(stats.login_logs?.count || 0);
            elementCache.get("notifications-count").textContent = String(stats.notifications?.count || 0);
            if (stats.operation_logs?.oldest) {
                elementCache.get("operation-logs-oldest").textContent = `最早: ${new Date(stats.operation_logs.oldest).toLocaleDateString()}`;
            }
            if (stats.login_logs?.oldest) {
                elementCache.get("login-logs-oldest").textContent = `最早: ${new Date(stats.login_logs.oldest).toLocaleDateString()}`;
            }
        }
    }
    catch (error) {
        console.error("加载日志统计失败:", error);
    }
}
export async function clearLogs() {
    const logType = elementCache.getValue("clear-log-type");
    const daysInput = elementCache.getValue("clear-log-days");
    const days = daysInput ? parseInt(daysInput) : 0;
    let confirmMsg;
    if (days === 0) {
        confirmMsg = t("logs.clear_all_confirm") || "确定要删除全部日志吗？此操作不可撤销。";
    }
    else {
        confirmMsg = t("logs.clear_days_confirm", { days: String(days) }) || `确定要清理 ${days} 天前的日志吗？此操作不可撤销。`;
    }
    if (!confirm(confirmMsg)) {
        return;
    }
    try {
        const result = await apiPost("/api/system/logs/clear", {
            log_type: logType,
            days: days,
        });
        if (result.success) {
            showToast(result.message, "success");
            loadLogsStats();
        }
        else {
            showToast((t("logs.clear_failed") || "清理日志失败") + ": " + result.message, "error");
        }
    }
    catch (error) {
        console.error("清理日志失败:", error);
        showToast((t("logs.clear_failed") || "清理日志失败") + ": " + error.message, "error");
    }
}
export function initLogsCleanup() {
    const clearLogsBtn = elementCache.get("clear-logs-btn");
    if (clearLogsBtn) {
        clearLogsBtn.addEventListener("click", clearLogs);
    }
    loadLogsStats();
}
async function initScheduledTasksTab() {
    try {
        const scheduledTaskModule = await import("./scheduledTaskManager.js");
        await scheduledTaskModule.initScheduledTaskManager();
    }
    catch (error) {
        console.error("初始化定时任务模块失败:", error);
    }
}
//# sourceMappingURL=systemManager.js.map