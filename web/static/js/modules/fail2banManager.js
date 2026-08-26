import { apiGet, apiPost, apiPut } from "../utils/apiClient.js";
import { showToast, escapeHtml } from "../utils/ui.js";
import { t } from "../utils/i18n.js";
import { iconButton } from "../utils/icons.js";
import { elementCache } from "../utils/helpers.js";
import { showConfirm } from "../utils/confirm.js";

let securityTabInitialized = false;

// 初始化安全标签页
export function initSecurityTab() {
  if (!securityTabInitialized) {
    bindEvents();
    securityTabInitialized = true;
  }
  loadRateLimitConfig();
  loadAppFail2banStatus();
}

// 绑定事件
function bindEvents() {
  elementCache.get("app-fail2ban-refresh-btn")?.addEventListener("click", loadAppFail2banStatus);
  elementCache
    .get("app-fail2ban-save-config-btn")
    ?.addEventListener("click", saveAppFail2banConfig);
  elementCache.get("app-fail2ban-ban-btn")?.addEventListener("click", handleBanIp);
  elementCache.get("app-fail2ban-unban-btn")?.addEventListener("click", handleUnbanIp);
  elementCache.get("rate-limit-save-config-btn")?.addEventListener("click", saveRateLimitConfig);
}

// 加载应用层 fail2ban 状态
async function loadAppFail2banStatus() {
  const statusBadge = elementCache.get("app-fail2ban-status-badge");
  const bannedTbody = elementCache.get("app-fail2ban-banned-tbody");
  const trackedTbody = elementCache.get("app-fail2ban-tracked-tbody");
  const logPathHint = elementCache.get("app-fail2ban-log-path-hint");

  if (bannedTbody) {
    bannedTbody.innerHTML = `<tr class="empty-row"><td colspan="3" class="text-center">${t("common.loading") || "Loading..."}</td></tr>`;
  }
  if (trackedTbody) {
    trackedTbody.innerHTML = `<tr class="empty-row"><td colspan="2" class="text-center">${t("common.loading") || "Loading..."}</td></tr>`;
  }

  try {
    const result = await apiGet("/api/system/fail2ban/app/status");
    if (!result.success || !result.data) {
      if (bannedTbody) {
        bannedTbody.innerHTML = `<tr class="empty-row"><td colspan="3" class="text-center">${t("common.load_failed") || "Load failed"}</td></tr>`;
      }
      return;
    }

    const status = result.data;

    // 状态徽章
    if (statusBadge) {
      if (status.enabled) {
        statusBadge.textContent = t("security.status_active") || "Active";
        statusBadge.className = "status-badge status-active";
      } else {
        statusBadge.textContent = t("security.status_inactive") || "Inactive";
        statusBadge.className = "status-badge status-inactive";
      }
    }

    // 配置回填
    elementCache.get("app-fail2ban-enabled").value = String(status.enabled);
    elementCache.get("app-fail2ban-findtime").value = status.findtime;
    elementCache.get("app-fail2ban-maxretry").value = status.max_retry;
    elementCache.get("app-fail2ban-bantime").value = status.bantime;

    // 日志路径提示
    if (logPathHint) {
      const logPath = status.log_path || "/var/log/ipma/auth.log";
      const hint = t("security.os_integration_desc") || "Auth log path for OS fail2ban to monitor:";
      logPathHint.textContent = `${hint} ${logPath}`;
    }

    // 已封禁 IP
    if (bannedTbody) {
      if (status.banned_ips.length === 0) {
        bannedTbody.innerHTML = `<tr class="empty-row"><td colspan="3" class="text-center">${t("common.no_data") || "No data"}</td></tr>`;
      } else {
        bannedTbody.innerHTML = status.banned_ips
          .map(
            (item) => `
          <tr>
            <td>${escapeHtml(item.ip)}</td>
            <td class="col-center">${item.remaining_seconds}</td>
            <td class="col-center">${iconButton({ icon: "unlock", label: t("security.unban_ip") || "Unban", cls: "btn-success app-unban-btn", attrs: `data-ip="${escapeHtml(item.ip)}"` })}</td>
          </tr>
        `
          )
          .join("");
        bannedTbody.querySelectorAll(".app-unban-btn").forEach((btn) => {
          btn.addEventListener("click", async () => {
            const ip = btn.dataset.ip;
            const confirmed = await showConfirm(
              t("security.unban_confirm") || "Unban this IP?",
              ip
            );
            if (confirmed) {
              await doUnbanIp(ip);
            }
          });
        });
      }
    }

    // 追踪中的 IP
    if (trackedTbody) {
      if (status.tracked_ips.length === 0) {
        trackedTbody.innerHTML = `<tr class="empty-row"><td colspan="2" class="text-center">${t("common.no_data") || "No data"}</td></tr>`;
      } else {
        trackedTbody.innerHTML = status.tracked_ips
          .map(
            (item) => `
          <tr>
            <td>${escapeHtml(item.ip)}</td>
            <td class="col-center">${item.failure_count}</td>
          </tr>
        `
          )
          .join("");
      }
    }
  } catch (err) {
    console.error("loadAppFail2banStatus error:", err);
    if (bannedTbody) {
      bannedTbody.innerHTML = `<tr class="empty-row"><td colspan="3" class="text-center">${t("common.load_failed") || "Load failed"}</td></tr>`;
    }
  }
}

// 保存配置
async function saveAppFail2banConfig() {
  const config = {
    enabled: elementCache.get("app-fail2ban-enabled").value === "true",
    findtime: parseInt(elementCache.get("app-fail2ban-findtime").value, 10),
    max_retry: parseInt(elementCache.get("app-fail2ban-maxretry").value, 10),
    bantime: parseInt(elementCache.get("app-fail2ban-bantime").value, 10)
  };

  try {
    const result = await apiPut("/api/system/fail2ban/app/config", config);
    if (result.success) {
      showToast(t("security.config_saved") || "Config saved", "success");
      loadAppFail2banStatus();
    } else {
      showToast(result.error || t("common.save_failed") || "Save failed", "error");
    }
  } catch (err) {
    console.error("saveAppFail2banConfig error:", err);
    showToast(t("common.save_failed") || "Save failed", "error");
  }
}

// 封禁 IP
async function handleBanIp() {
  const ipInput = elementCache.get("app-fail2ban-ip-input");
  const ip = ipInput?.value?.trim();
  if (!ip) {
    showToast(t("security.input_ip") || "Please input IP", "warning");
    return;
  }

  try {
    const result = await apiPost("/api/system/fail2ban/app/ban", { ip });
    if (result.success) {
      showToast(result.data?.message || t("security.ban_success") || "Ban success", "success");
      ipInput.value = "";
      loadAppFail2banStatus();
    } else {
      showToast(result.error || t("security.ban_failed") || "Ban failed", "error");
    }
  } catch (err) {
    console.error("banIp error:", err);
    showToast(t("security.ban_failed") || "Ban failed", "error");
  }
}

// 解封 IP
async function handleUnbanIp() {
  const ipInput = elementCache.get("app-fail2ban-ip-input");
  const ip = ipInput?.value?.trim();
  if (!ip) {
    showToast(t("security.input_ip") || "Please input IP", "warning");
    return;
  }
  const confirmed = await showConfirm(t("security.unban_confirm") || "Unban this IP?", ip);
  if (!confirmed) {
    return;
  }
  await doUnbanIp(ip);
  ipInput.value = "";
}

// 执行解封
async function doUnbanIp(ip) {
  try {
    const result = await apiPost("/api/system/fail2ban/app/unban", { ip });
    if (result.success) {
      showToast(result.data?.message || t("security.unban_success") || "Unban success", "success");
      loadAppFail2banStatus();
    } else {
      showToast(result.error || t("security.unban_failed") || "Unban failed", "error");
    }
  } catch (err) {
    console.error("unbanIp error:", err);
    showToast(t("security.unban_failed") || "Unban failed", "error");
  }
}

// 加载限流配置
async function loadRateLimitConfig() {
  try {
    const result = await apiGet("/api/system/config");
    if (result.success && result.data.rate_limit) {
      const rateLimit = result.data.rate_limit;

      elementCache.setValue("rate-limit-enabled", String(rateLimit.enabled !== false));
      elementCache.setValue("rate-limit-ip", rateLimit.ip_limit || 100);
      elementCache.setValue("rate-limit-user", rateLimit.user_limit || 200);
      elementCache.setValue("rate-limit-login", rateLimit.login_limit || 5);
      elementCache.setValue("rate-limit-window", rateLimit.window_secs || 60);
      elementCache.setValue("rate-limit-email", rateLimit.email_limit || 5);
      elementCache.setValue("rate-limit-email-window", rateLimit.email_window_secs || 3600);
    }
  } catch (err) {
    console.error("loadRateLimitConfig error:", err);
  }
}

// 保存限流配置
async function saveRateLimitConfig() {
  const rateLimitConfig = {
    enabled: elementCache.getValue("rate-limit-enabled") === "true",
    ip_limit: parseInt(elementCache.getValue("rate-limit-ip")) || 100,
    user_limit: parseInt(elementCache.getValue("rate-limit-user")) || 200,
    login_limit: parseInt(elementCache.getValue("rate-limit-login")) || 5,
    window_secs: parseInt(elementCache.getValue("rate-limit-window")) || 60,
    email_limit: parseInt(elementCache.getValue("rate-limit-email")) || 5,
    email_window_secs: parseInt(elementCache.getValue("rate-limit-email-window")) || 3600
  };

  try {
    const result = await apiPut("/api/system/config", { rate_limit: rateLimitConfig });
    if (result.success) {
      showToast(t("config.rate_limit_saved") || "Rate limit config saved", "success");
    } else {
      showToast(result.error || t("common.save_failed") || "Save failed", "error");
    }
  } catch (err) {
    console.error("saveRateLimitConfig error:", err);
    showToast(t("common.save_failed") || "Save failed", "error");
  }
}
