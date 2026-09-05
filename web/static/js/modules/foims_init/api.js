/**
 * FOIMS 系统初始化 - 接口 / 业务流程层
 *
 * 封装初始化过程中的全部网络请求与流程编排：
 *   - PostgreSQL 检查
 *   - 数据库配置（连接测试充当下一步 / 创建数据库）
 *   - 数据库状态检查
 *   - 数据库创建 / 导入
 *   - 管理员账户创建
 *   - 控制台验证码获取
 */
import { showToast, escapeHtml } from "../../utils/ui.js";
import { t } from "../../utils/i18n.js";
import { translateServerMessage } from "../../utils/apiClient.js";
import { state } from "./state.js";
import { goToStep, showError, showLoading, hideLoading, showManualRestartGuide } from "./ui.js";

// 各检查步骤共用的兜底错误文案键
const T_KEY_UNKNOWN_ERROR = "init.unknown_error";
const T_KEY_NETWORK_ERROR = "init.network_error";

// 全部 fetch 共用的 JSON 请求头（本文件 8 处请求，抽常量避免字面量重复）
const JSON_HEADERS = { "Content-Type": "application/json" };

/**
 * 翻译服务端返回的 error 字段（后端约定为 i18n key）。
 * 键在字典中缺失时 t() 原样返回 key，此时回退到通用未知错误文案，
 * 避免向用户展示裸键名。
 * @param {string} errorKey 服务端 error 字段（i18n key）
 * @returns {string} 翻译后的错误文案
 */
const translateErrorField = (errorKey) => {
  if (!errorKey) {
    return t(T_KEY_UNKNOWN_ERROR);
  }
  const translated = t(errorKey);
  return translated === errorKey ? t(T_KEY_UNKNOWN_ERROR) : translated;
};

// 安全解析 JSON 响应：后端返回空 body / 非 JSON / 网络中断时给出可读错误，
// 而不是抛 "Unexpected end of JSON input" 这类让用户困惑的消息。
// 解析成功后统一翻译 message（后端返回 i18n key + message_params），
// 保证 ui.js / index.js 展示前消息已是译文。
const parseJsonResponse = async (response) => {
  const text = await response.text();
  if (!text || !text.trim()) {
    const msg = response.ok
      ? t("init.empty_response")
      : `${t("init.parse_empty", { status: response.status })}`;
    return { success: false, message: msg, data: null };
  }
  try {
    return translateServerMessage(JSON.parse(text));
  } catch (e) {
    console.error("初始化接口响应非 JSON:", e, text.slice(0, 200));
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
      headers: JSON_HEADERS
    });

    const result = await parseJsonResponse(response);
    hideLoading();

    const pgStatusElement = document.getElementById("pg-status");
    const nextButton = document.querySelector('.step-panel[data-step="1"] .btn-success');
    if (!pgStatusElement || !nextButton) {
      return;
    }

    if (result.installed && result.running) {
      pgStatusElement.innerHTML = `
                <div class="status-success">✓</div>
                <h3>${t("init.pg_running")}</h3>
                <p>${escapeHtml(result.message || t("init.pg_connected"))}</p>
            `;
      // 状态类互斥：设置前先清掉旧状态类，避免反复检测后多类叠加
      pgStatusElement.classList.remove("status-success", "status-error", "status-loading");
      pgStatusElement.classList.add("status-success");
      nextButton.disabled = false;
    } else {
      pgStatusElement.innerHTML = `
                <div class="status-error">✗</div>
                <h3>${t("init.pg_check_failed")}</h3>
                <p>${escapeHtml(translateErrorField(result.error))}</p>
                <div class="error-guide">
                    <h4>${t("init.pg_suggestion")}</h4>
                    <ul>
                        ${!result.installed ? `<li>${t("init.pg_install_hint")}</li>` : ""}
                        ${result.installed && !result.running ? `<li>${t("init.pg_start_hint")}</li>` : ""}
                        <li>${t("init.pg_config_hint")}</li>
                        <li>${t("init.pg_network_hint")}</li>
                    </ul>
                </div>
            `;
      // 状态类互斥：设置前先清掉旧状态类，避免反复检测后多类叠加
      pgStatusElement.classList.remove("status-success", "status-error", "status-loading");
      pgStatusElement.classList.add("status-error");
      nextButton.disabled = true;
    }
  } catch (error) {
    hideLoading();
    showError(`${t(T_KEY_NETWORK_ERROR)}: ${error.message}`);
  }
};

/** 检查数据库连接与表结构状态，渲染第三步状态并控制「下一步」按钮。 */
export const checkDatabaseStatus = async () => {
  showLoading();

  try {
    const response = await fetch("/api/init/db-status", {
      method: "GET",
      headers: JSON_HEADERS
    });

    const result = await parseJsonResponse(response);
    hideLoading();

    // success 但 data 为 null 视同失败：进入 else 分支按失败提示，不读取空对象
    if (result.success && result.data) {
      const dbStatus = result.data;
      const dbStatusElement = document.getElementById("db-status");
      const nextButton = document.querySelector('.step-panel[data-step="3"] .btn-success');
      if (!dbStatusElement || !nextButton) {
        return;
      }

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
                    <p>${t("init.db_connect_error")}: ${escapeHtml(translateErrorField(dbStatus.error))}</p>
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
    showError(`${t(T_KEY_NETWORK_ERROR)}: ${error.message}`);
  }
};

/**
 * 渲染数据库配置页（第 2 步）的状态反馈块。
 * @param {"success"|"error"} kind 状态类别
 * @param {string} titleKey 标题文案键
 * @param {string} detail 详述文案（已翻译）
 */
const renderDbConfigStatus = (kind, titleKey, detail) => {
  const statusElement = document.getElementById("db-config-status");
  if (!statusElement) {
    return;
  }
  const icon = kind === "success" ? "✓" : "✗";
  // 失败时附加通用操作指引：分类文案已含指引的（如库不存在）重复展示无害，
  // 未归类的失败（Other）也能得到下一步提示
  const hint = kind === "error" ? `<p>${t("init.db_test_fail_hint")}</p>` : "";
  statusElement.innerHTML = `
        <div class="status-${kind}">${icon}</div>
        <h3>${t(titleKey)}</h3>
        <p>${escapeHtml(detail)}</p>
        ${hint}
    `;
  statusElement.className = "status-container";
  statusElement.classList.add(`status-${kind}`);
  statusElement.hidden = false;
};

/** 从数据库配置表单收集请求体；字段缺失时返回 null 并提示。 */
const collectDbConfigPayload = () => {
  const form = document.getElementById("db-config-form");
  if (!form) {
    return null;
  }
  const formData = new FormData(form);
  const payload = {
    type: "pgsql",
    host: (formData.get("host") || "").toString().trim(),
    port: (formData.get("port") || "").toString().trim(),
    database: (formData.get("database") || "").toString().trim(),
    username: (formData.get("username") || "").toString().trim(),
    password: (formData.get("password") || "").toString()
  };
  if (
    !payload.host ||
    !payload.port ||
    !payload.database ||
    !payload.username ||
    !payload.password
  ) {
    showError(t("init.db_fill_required"));
    return null;
  }
  return payload;
};

/** 数据库配置页在途标志：连接测试 / 建库请求期间重复提交直接忽略 */
let dbConfigSubmitting = false;

/**
 * 第 2 步「连接测试」：充当下一步按钮。
 * 成功 -> 后端已写入配置并切换连接，进入第 3 步（表创建流程）；
 * 失败 -> 按错误分类提示「检查输入或点击创建数据库」。
 */
export const handleDbConfigSubmit = async (e) => {
  e.preventDefault();
  if (dbConfigSubmitting) {
    return;
  }

  const payload = collectDbConfigPayload();
  if (!payload) {
    return;
  }

  dbConfigSubmitting = true;
  showLoading();

  try {
    const response = await fetch("/api/init/db/test-connection", {
      method: "POST",
      headers: JSON_HEADERS,
      body: JSON.stringify(payload)
    });

    const result = await parseJsonResponse(response);
    hideLoading();

    if (result.success) {
      showToast(t("init.db_test_pass"));
      goToStep(3);
      checkDatabaseStatus();
    } else {
      renderDbConfigStatus(
        "error",
        "init.db_test_fail_title",
        result.message || t(T_KEY_UNKNOWN_ERROR)
      );
    }
  } catch (error) {
    hideLoading();
    showError(`${t(T_KEY_NETWORK_ERROR)}: ${error.message}`);
  } finally {
    dbConfigSubmitting = false;
  }
};

/**
 * 第 2 步「创建数据库」：按页面输入以幂等方式创建目标数据库。
 * 成功（含已存在）-> 提示点击「连接测试」进入下一流程；
 * 失败 -> 按错误分类反馈（不可达 / 认证失败 / 无权限等）。
 */
export const handleDbCreate = async () => {
  if (dbConfigSubmitting) {
    return;
  }

  const payload = collectDbConfigPayload();
  if (!payload) {
    return;
  }

  dbConfigSubmitting = true;
  showLoading();

  try {
    const response = await fetch("/api/init/db/provision", {
      method: "POST",
      headers: JSON_HEADERS,
      body: JSON.stringify(payload)
    });

    const result = await parseJsonResponse(response);
    hideLoading();

    if (result.success) {
      const created = result.data ? result.data.created !== false : true;
      renderDbConfigStatus(
        "success",
        created ? "init.db_create_success" : "init.db_create_existed",
        t("init.db_create_next_hint")
      );
    } else {
      renderDbConfigStatus(
        "error",
        "init.db_create_fail_title",
        result.message || t(T_KEY_UNKNOWN_ERROR)
      );
    }
  } catch (error) {
    hideLoading();
    showError(`${t(T_KEY_NETWORK_ERROR)}: ${error.message}`);
  } finally {
    dbConfigSubmitting = false;
  }
};

/** 第三步表单提交：根据初始化方式新建或导入数据库。 */
export const handleInitModeSubmit = async (e) => {
  e.preventDefault();

  const nextButton = document.querySelector('.step-panel[data-step="3"] .btn-success');
  const verificationInput = document.getElementById("verification-step2");
  if (!nextButton || !verificationInput) {
    return;
  }
  nextButton.disabled = true;

  const verificationCode = verificationInput.value;
  if (!verificationCode) {
    showError(t("init.enter_captcha"));
    nextButton.disabled = false;
    return;
  }

  showLoading();

  try {
    let apiEndpoint = "";
    const requestBody = { verification: verificationCode };

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
            goToStep(4);
          }, 1000);
        } else {
          showError(`${t("init.operation_failed")}: ${result.message || t(T_KEY_UNKNOWN_ERROR)}`);
          nextButton.disabled = false;
        }
        return;
      }
      apiEndpoint = "/api/init/db/import";
    }

    const response = await fetch(apiEndpoint, {
      method: "POST",
      headers: JSON_HEADERS,
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
        goToStep(4);
      }, 1000);
    } else {
      showError(`${t("init.operation_failed")}: ${result.message || t(T_KEY_UNKNOWN_ERROR)}`);
      nextButton.disabled = false;
    }
  } catch (error) {
    hideLoading();
    showError(`${t(T_KEY_NETWORK_ERROR)}: ${error.message}`);
    nextButton.disabled = false;
  }
};

// 管理员账户提交在途标志：请求期间重复提交直接忽略（防双击重复创建）
let adminAccountSubmitting = false;

/** 第四步表单提交：创建管理员账户并触发系统重启。 */
export const handleAdminAccountSubmit = async (e) => {
  e.preventDefault();
  if (adminAccountSubmitting) {
    return;
  }
  adminAccountSubmitting = true;

  try {
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
      password,
      email: formData.get("email"),
      role: "admin",
      verification: formData.get("verification")
    };

    showLoading();

    const response = await fetch("/api/init", {
      method: "POST",
      headers: JSON_HEADERS,
      body: JSON.stringify(initConfig)
    });

    const result = await parseJsonResponse(response);
    hideLoading();

    if (result.success) {
      goToStep(5);
      setTimeout(async () => {
        // 初始化已完成：重启为纯 systemd 重启；后端检测到未注册单元时
        // 不执行重启（restart_mode = manual），前端切换为手动重启指引页
        let restartMode = "systemd";
        try {
          const response = await fetch("/api/init/restart", {
            method: "POST",
            headers: JSON_HEADERS
          });
          const restartResult = await parseJsonResponse(response);
          if (restartResult?.data?.restart_mode === "manual") {
            restartMode = "manual";
          }
        } catch (error) {
          // 重启请求失败视为重启未发生：按手动重启引导，不自动跳转
          console.warn("init restart request failed:", error);
          restartMode = "manual";
        }
        if (restartMode === "manual") {
          showManualRestartGuide();
          return;
        }
        setTimeout(() => {
          window.location.href = "/main.html";
        }, 2000);
      }, 3000);
    } else {
      showError(`${t("init.init_failed")}: ${result.message || t(T_KEY_UNKNOWN_ERROR)}`);
    }
  } catch (error) {
    hideLoading();
    showError(`${t(T_KEY_NETWORK_ERROR)}: ${error.message}`);
  } finally {
    adminAccountSubmitting = false;
  }
};

/** 获取并展示服务器控制台验证码。由 index.js 的 .get-captcha-btn 点击监听调用。 */
export const getVerificationCode = async () => {
  // 请求期间禁用全部验证码按钮，防止连点重复请求；finally 恢复
  const captchaButtons = document.querySelectorAll(".get-captcha-btn");
  captchaButtons.forEach((btn) => {
    btn.disabled = true;
  });
  showLoading();

  try {
    const response = await fetch("/api/init/verification-code", {
      method: "GET",
      headers: JSON_HEADERS
    });

    const result = await parseJsonResponse(response);
    hideLoading();

    if (result.success) {
      showToast(result.message);
    } else {
      showError(`${t("init.captcha_failed")}: ${result.message || t(T_KEY_UNKNOWN_ERROR)}`);
    }
  } catch (error) {
    hideLoading();
    showError(`${t(T_KEY_NETWORK_ERROR)}: ${error.message}`);
  } finally {
    captchaButtons.forEach((btn) => {
      btn.disabled = false;
    });
  }
};
