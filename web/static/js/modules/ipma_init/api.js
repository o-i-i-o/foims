/**
 * IPMA 系统初始化 - 接口 / 业务流程层
 *
 * 封装初始化过程中的全部网络请求与流程编排：
 *   - PostgreSQL 检查
 *   - 数据库状态检查
 *   - 数据库创建 / 导入
 *   - 管理员账户创建
 *   - 控制台验证码获取
 */
import { showToast, escapeHtml } from "../../utils/ui.js";
import { t } from "../../utils/i18n.js";
import { state } from "./state.js";
import { goToStep, showError, showLoading, hideLoading } from "./ui.js";

// 安全解析 JSON 响应：后端返回空 body / 非 JSON / 网络中断时给出可读错误，
// 而不是抛 "Unexpected end of JSON input" 这类让用户困惑的消息。
const parseJsonResponse = async (response) => {
  const text = await response.text();
  if (!text || !text.trim()) {
    const msg = response.ok
      ? t("init.empty_response")
      : `${t("init.parse_empty", { status: response.status })}`;
    return { success: false, message: msg, data: null };
  }
  try {
    return JSON.parse(text);
  } catch (e) {
    return {
      success: false,
      message: `${t("init.parse_invalid_json", { status: response.status })}`,
      data: null,
      _raw: text.slice(0, 200)
    };
  }
};

/** 检查 PostgreSQL 是否已安装并运行，据此渲染第一步状态并控制「下一步」按钮。 */
export const checkPostgreSQL = async () => {
  showLoading();

  try {
    const response = await fetch("/api/init/check-pgsql", {
      method: "GET",
      headers: { "Content-Type": "application/json" }
    });

    const result = await parseJsonResponse(response);
    hideLoading();

    const pgStatusElement = document.getElementById("pg-status");
    const nextButton = document.querySelector('.step-panel[data-step="1"] .btn-success');

    if (result.installed && result.running) {
      pgStatusElement.innerHTML = `
                <div class="status-success">✓</div>
                <h3>${t("init.pg_running")}</h3>
                <p>${escapeHtml(result.message || t("init.pg_connected"))}</p>
            `;
      pgStatusElement.classList.add("status-success");
      nextButton.disabled = false;
    } else {
      pgStatusElement.innerHTML = `
                <div class="status-error">✗</div>
                <h3>${t("init.pg_check_failed")}</h3>
                <p>${escapeHtml(result.error || t("init.unknown_error"))}</p>
                <div class="error-guide">
                    <h4>${t("init.pg_suggestion")}</h4>
                    <ul>
                        ${!result.installed ? `<li>${t("init.pg_install_hint")}</li>` : ""}
                        ${result.installed && !result.running ? "<li>请启动 PostgreSQL 服务</li>" : ""}
                        <li>确保数据库配置正确</li>
                        <li>检查网络连接和防火墙设置</li>
                    </ul>
                </div>
            `;
      pgStatusElement.classList.add("status-error");
      nextButton.disabled = true;
    }
  } catch (error) {
    hideLoading();
    showError(`${t("init.network_error")}: ${error.message}`);
  }
};

/** 检查数据库连接与表结构状态，渲染第二步状态并控制「下一步」按钮。 */
export const checkDatabaseStatus = async () => {
  showLoading();

  try {
    const response = await fetch("/api/init/db-status", {
      method: "GET",
      headers: { "Content-Type": "application/json" }
    });

    const result = await parseJsonResponse(response);
    hideLoading();

    if (result.success) {
      const dbStatus = result.data;
      state.dbStatus = dbStatus;
      const dbStatusElement = document.getElementById("db-status");
      const nextButton = document.querySelector('.step-panel[data-step="2"] .btn-success');

      dbStatusElement.className = "status-container";

      if (dbStatus.connected) {
        if (dbStatus.has_data) {
          dbStatusElement.innerHTML = `
                            <div class="status-warning">⚠</div>
                            <h3>${t("init.db_status_title")}</h3>
                            <p>${t("init.db_schema_ok")}</p>
                            <p class="warning-text">${t("init.db_will_reset")}</p>
                        `;
          dbStatusElement.classList.add("status-warning");
        } else if (!dbStatus.required_tables_exist) {
          dbStatusElement.innerHTML = `
                            <div class="status-warning">⚠</div>
                            <h3>${t("init.db_status_title")}</h3>
                            <p>${t("init.db_schema_incomplete")}</p>
                            <p>${t("init.select_init_method_full")}</p>
                        `;
          dbStatusElement.classList.add("status-warning");
        } else {
          dbStatusElement.innerHTML = `
                            <div class="status-success">✓</div>
                            <h3>${t("init.db_status_title")}</h3>
                            <p>${t("init.db_schema_ok")}</p>
                            <p>${t("init.select_init_method")}</p>
                        `;
          dbStatusElement.classList.add("status-success");
        }
        nextButton.disabled = false;
      } else {
        dbStatusElement.innerHTML = `
                    <div class="status-error">✗</div>
                    <h3>${t("init.db_connect_failed")}</h3>
                    <p>${t("init.db_connect_error")}: ${escapeHtml(dbStatus.error || t("init.unknown_error"))}</p>
                    <p>${t("init.will_auto_create_db")}</p>
                `;
        dbStatusElement.classList.add("status-error");
        nextButton.disabled = false;
      }
    } else {
      showError(t("init.check_db_status_failed"));
    }
  } catch (error) {
    hideLoading();
    showError(`${t("init.network_error")}: ${error.message}`);
  }
};

/** 第二步表单提交：根据初始化方式新建或导入数据库。 */
export const handleInitModeSubmit = async (e) => {
  e.preventDefault();

  const nextButton = document.querySelector('.step-panel[data-step="2"] .btn-success');
  nextButton.disabled = true;

  const verificationCode = document.getElementById("verification-step2").value;
  if (!verificationCode) {
    showError(t("init.enter_captcha"));
    nextButton.disabled = false;
    return;
  }

  showLoading();

  try {
    let apiEndpoint = "";
    let requestBody = { verification: verificationCode };

    if (state.initMode === "create") {
      apiEndpoint = "/api/init/db/create";
    } else if (state.initMode === "import") {
      const fileInput = document.getElementById("sql-file");
      if (fileInput && fileInput.files.length > 0) {
        const formData = new FormData();
        formData.append("verification", verificationCode);
        formData.append("sql_file", fileInput.files[0]);

        const response = await fetch("/api/init/db/import-file", {
          method: "POST",
          body: formData
        });

        const result = await parseJsonResponse(response);
        hideLoading();

        if (result.success) {
          let message = t("init.db_init_success");
          if (result.data && result.data.backup_file) {
            message += `\n${t("init.backup_file")} ${result.data.backup_file}`;
          }
          showToast(message);
          setTimeout(() => {
            goToStep(3);
          }, 1000);
        } else {
          showError(`${t("init.operation_failed")}: ${result.message || t("init.unknown_error")}`);
          nextButton.disabled = false;
        }
        return;
      } else {
        apiEndpoint = "/api/init/db/import";
      }
    }

    const response = await fetch(apiEndpoint, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(requestBody)
    });

    const result = await parseJsonResponse(response);
    hideLoading();

    if (result.success) {
      let message = t("init.db_init_success");
      if (result.data && result.data.backup_file) {
        message += `\n${t("init.backup_file")} ${result.data.backup_file}`;
      }
      showToast(message);
      setTimeout(() => {
        goToStep(3);
      }, 1000);
    } else {
      showError(`${t("init.operation_failed")}: ${result.message || t("init.unknown_error")}`);
      nextButton.disabled = false;
    }
  } catch (error) {
    hideLoading();
    showError(`${t("init.network_error")}: ${error.message}`);
    nextButton.disabled = false;
  }
};

/** 第三步表单提交：创建管理员账户并触发系统重启。 */
export const handleAdminAccountSubmit = async (e) => {
  e.preventDefault();

  const form = e.target;
  const formData = new FormData(form);

  const password = formData.get("password");
  const confirmPassword = formData.get("confirm_password");

  if (password !== confirmPassword) {
    showError(t("init.password_mismatch"));
    return;
  }

  const initConfig = {
    username: formData.get("username"),
    password: password,
    email: formData.get("email"),
    role: "admin",
    verification: formData.get("verification")
  };

  showLoading();

  try {
    const response = await fetch("/api/init", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(initConfig)
    });

    const result = await parseJsonResponse(response);
    hideLoading();

    if (result.success) {
      goToStep(4);
      setTimeout(async () => {
        try {
          await fetch("/api/init/restart", {
            method: "POST",
            headers: { "Content-Type": "application/json" }
          });
        } catch (error) {}
        setTimeout(() => {
          window.location.href = "/main.html";
        }, 2000);
      }, 3000);
    } else {
      showError(`${t("init.init_failed")}: ${result.message || t("init.unknown_error")}`);
    }
  } catch (error) {
    hideLoading();
    showError(`${t("init.network_error")}: ${error.message}`);
  }
};

/** 获取并展示服务器控制台验证码。由 HTML inline onclick 调用。 */
export const getVerificationCode = async () => {
  showLoading();

  try {
    const response = await fetch("/api/init/verification-code", {
      method: "GET",
      headers: { "Content-Type": "application/json" }
    });

    const result = await parseJsonResponse(response);
    hideLoading();

    if (result.success) {
      showToast(result.message);
    } else {
      showError(`${t("init.captcha_failed")}: ${result.message || t("init.unknown_error")}`);
    }
  } catch (error) {
    hideLoading();
    showError(`${t("init.network_error")}: ${error.message}`);
  }
};
