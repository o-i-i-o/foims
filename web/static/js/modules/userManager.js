import { apiGet, apiPost, apiPut, apiDelete } from "../utils/apiClient.js";

import {
  showToast,
  formatDateTime,
  appendPaginationToTable,
  escapeHtml,
  createSortState,
  updateSortIcons,
  initSortEvents
} from "../utils/ui.js";

import { openModal, closeModal } from "../utils/modalLoader.js";
import { t } from "../utils/i18n.js";
import { iconButton } from "../utils/icons.js";
import { elementCache } from "../utils/helpers.js";
import { showConfirm } from "../utils/confirm.js";

// 角色显示（等保三权分立：admin/secadmin/auditor/user）
function roleLabel(role) {
  if (role === "admin") {
    return t("user.role_admin");
  }
  if (role === "secadmin") {
    return t("user.role_secadmin");
  }
  if (role === "auditor") {
    return t("user.role_auditor");
  }
  return t("user.role_user");
}

let currentUserPage = 1;
const USER_PAGE_SIZE = 20;
const userTableState = createSortState("created_at", "desc");

// 列表请求序号:旧响应晚到时放弃渲染,防止翻页/排序并发后表格与状态错乱
let usersRequestSeq = 0;

// 加载用户数据
export async function loadUsersData(page = currentUserPage, sortBy = null, sortOrder = null) {
  const requestSeq = ++usersRequestSeq;
  currentUserPage = page;
  if (sortBy) {
    userTableState.setSort(sortBy, sortOrder);
  }
  try {
    const response = await apiGet(
      `/api/users?page=${page}&page_size=${USER_PAGE_SIZE}&sort_by=${userTableState.sortBy}&sort_order=${userTableState.sortOrder}`
    );
    if (requestSeq !== usersRequestSeq) {
      return; // 已有更新的请求,丢弃过期响应
    }
    if (response.success) {
      const data = response.data;
      const users = data.items || data;
      const pagination = data.total !== undefined ? data : null;
      const tableBody = document.querySelector("#users-table tbody");

      // 空列表且当前页大于 1：删除后页码越界，按 total_pages 一步回退
      //（与 room/networks 等列表模块一致，避免极端情况连环请求）
      const totalPages = data.total_pages || Math.ceil((data.total || 0) / USER_PAGE_SIZE);
      if (users.length === 0 && page > 1 && totalPages > 0 && page > totalPages) {
        loadUsersData(totalPages);
        return;
      }

      if (users.length === 0) {
        tableBody.innerHTML = `
          <tr class="empty-row">
            <td colspan="8" class="text-center">${t("common.no_data")}</td>
          </tr>
        `;
        return;
      }

      const startIndex = (page - 1) * USER_PAGE_SIZE;
      tableBody.innerHTML = users
        .map(
          (user, index) => `
        <tr data-user-id="${user.id}">
          <td class="index-column">${startIndex + index + 1}</td>
          <td>${escapeHtml(user.username)}</td>
          <td>${escapeHtml(user.email)}</td>
          <td class="col-center">${roleLabel(user.role)}</td>
          <td class="col-center">
            <span class="status-badge ${user.status ? "status-active" : "status-inactive"}">
              ${user.status ? t("user.status_enabled") : t("user.status_disabled")}
            </span>
          </td>
          <td class="col-center">
            <span class="two-factor-badge ${user.two_factor_enabled ? "two-factor-enabled" : "two-factor-disabled"}">
              ${user.two_factor_enabled ? t("user.two_factor_enabled") : t("user.two_factor_disabled")}
            </span>
          </td>
          <td class="col-center">${formatDateTime(user.created_at)}</td>
          <td class="col-center">
            ${iconButton({ icon: "edit", label: t("common.edit"), cls: "btn-edit", attrs: `data-id="${user.id}"` })}
            ${iconButton({ icon: "lock", label: user.two_factor_enabled ? t("user.manage_2fa") : t("user.enable_2fa"), cls: "btn-primary user-2fa", attrs: `data-id="${user.id}" data-username="${escapeHtml(user.username)}" data-enabled="${user.two_factor_enabled}"` })}
            ${iconButton({ icon: "trash", label: t("common.delete"), cls: "btn-danger btn-delete", attrs: `data-id="${user.id}"` })}
          </td>
        </tr>
      `
        )
        .join("");

      if (pagination) {
        appendPaginationToTable("#users-table", pagination, loadUsersData);
      }
      updateSortIcons("users-table", userTableState);
    } else {
      showToast(`${t("common.load_failed")}：${response.message}`, "error");
    }
  } catch (error) {
    console.error("加载用户数据失败:", error);
    showToast(t("common.load_failed_retry"), "error");
  }
}

// 用户资料加载序号：每次打开模态框自增，晚到的旧响应据此丢弃，
// 防止 A 用户的资料被写入 B 用户的编辑表单（隐藏 id 是 B，保存即串号）
let userLoadToken = 0;

// 打开用户模态框
export async function openUserModal(userId) {
  await openModal("user-modal");

  // 新弹窗打开即失效任何在途的用户资料请求
  userLoadToken++;

  const modal = elementCache.get("user-modal");
  const title = elementCache.get("user-modal-title");
  const userIdInput = elementCache.get("user-id");
  const usernameInput = elementCache.get("user-username");
  const emailInput = elementCache.get("user-email");
  const roleInput = elementCache.get("user-role");
  const statusInput = elementCache.get("user-status");
  const passwordInput = elementCache.get("user-password");
  const passwordConfirmInput = elementCache.get("user-password-confirm");

  if (
    !modal ||
    !title ||
    !userIdInput ||
    !usernameInput ||
    !emailInput ||
    !roleInput ||
    !statusInput ||
    !passwordInput ||
    !passwordConfirmInput
  ) {
    console.error("用户模态框相关DOM元素未找到");
    return;
  }

  if (userId) {
    title.textContent = t("user.edit_user");
    userIdInput.value = userId;
    passwordInput.placeholder = t("user.password_leave_blank");
    passwordInput.required = false;
    passwordConfirmInput.placeholder = t("user.password_leave_blank");
    passwordConfirmInput.required = false;

    loadUserData(userId);
  } else {
    title.textContent = t("user.add_user");
    userIdInput.value = "";
    usernameInput.value = "";
    emailInput.value = "";
    roleInput.value = "user";
    statusInput.value = "true";
    passwordInput.value = "";
    passwordInput.placeholder = t("user.password_placeholder");
    passwordInput.required = true;
    passwordConfirmInput.value = "";
    passwordConfirmInput.placeholder = t("user.password_confirm_placeholder");
    passwordConfirmInput.required = true;
  }
}

// 加载用户数据
async function loadUserData(userId) {
  const token = userLoadToken;
  try {
    const response = await apiGet(`/api/users/${userId}`);
    if (token !== userLoadToken) {
      // 响应期间已打开新的模态框：旧响应不得写入当前表单
      return;
    }
    if (response.success) {
      const user = response.data;
      elementCache.setValue("user-username", user.username);
      elementCache.setValue("user-email", user.email);
      elementCache.setValue("user-role", user.role);
      elementCache.setValue("user-status", user.status.toString());
    }
  } catch (error) {
    console.error("加载用户数据失败:", error);
    if (token === userLoadToken) {
      showToast(t("common.load_failed"), "error");
    }
  }
}

// 删除用户
export async function deleteUser(userId) {
  const confirmed = await showConfirm(t("user.delete_confirm"));
  if (!confirmed) {
    return;
  }

  try {
    const response = await apiDelete(`/api/users/${userId}`);
    if (response.success) {
      showToast(t("user.delete_user") + t("common.success"), "success");
      loadUsersData();
    } else {
      showToast(`${t("user.delete_user") + t("common.failed")}：${response.message}`, "error");
    }
  } catch (error) {
    console.error("删除用户失败:", error);
    showToast(t("user.delete_user") + t("common.failed"), "error");
  }
}

// 注意：用户编辑/删除按钮的点击事件已在 eventManager.js 中统一处理
// 此处 initUserEvents 函数保留用于处理 2FA 相关按钮及表头排序
let userEventsInitialized = false;

function initUserEvents() {
  if (userEventsInitialized) {
    return;
  }
  userEventsInitialized = true;

  initSortEvents("users-table", userTableState, loadUsersData);

  // 语言切换时 updatePageTranslations 只刷新静态 data-i18n 元素，
  // 表格行文本是渲染时固化的，需重新拉取渲染
  window.addEventListener("languagechange", () => {
    loadUsersData(currentUserPage);
  });

  document.addEventListener("click", (e) => {
    const twoFaBtn = e.target.closest(".user-2fa");
    if (twoFaBtn) {
      const id = twoFaBtn.getAttribute("data-id");
      const username = twoFaBtn.getAttribute("data-username");
      const enabled = twoFaBtn.getAttribute("data-enabled") === "true";
      if (id && username) {
        openTwoFactorModal(id, username, enabled);
      }
    }

    if (e.target.id === "two-factor-enable-btn") {
      handleTwoFactorEnable();
    }

    if (e.target.id === "two-factor-disable-btn") {
      handleTwoFactorDisable();
    }
  });
}

// 导出初始化函数
export { initUserEvents };

// 打开2FA模态框
async function openTwoFactorModal(userId, username, isEnabled) {
  // openModal 是异步的（按需拉取模板再挂载），必须 await 拿到真实 DOM，
  // 否则后续 querySelector 全部失效，二维码与密钥不会加载
  const modal = await openModal("two-factor-modal");

  if (!modal) {
    showToast(t("two_factor.open_modal_failed"), "error");
    return;
  }

  const setupStep = modal.querySelector("#two-factor-setup-step");
  const manageStep = modal.querySelector("#two-factor-manage-step");
  const enableBtn = modal.querySelector("#two-factor-enable-btn");
  const disableBtn = modal.querySelector("#two-factor-disable-btn");
  const verifyCodeInput = modal.querySelector("#two-factor-verify-code");
  const disableCodeInput = modal.querySelector("#two-factor-disable-code");
  const errorDiv = modal.querySelector("#two-factor-error");
  const successDiv = modal.querySelector("#two-factor-success");

  if (!setupStep || !manageStep || !enableBtn || !disableBtn) {
    console.error("2FA模态框元素未找到", { setupStep, manageStep, enableBtn, disableBtn });
    showToast(t("two_factor.modal_load_failed"), "error");
    return;
  }

  // 清空错误和成功消息
  if (errorDiv) {
    errorDiv.textContent = "";
    errorDiv.classList.remove("show");
  }
  if (successDiv) {
    successDiv.textContent = "";
    successDiv.classList.remove("show");
  }

  // 清空输入框
  if (verifyCodeInput) {
    verifyCodeInput.value = "";
  }
  if (disableCodeInput) {
    disableCodeInput.value = "";
  }

  // 确保有一个隐藏的输入字段来存储userId
  let userIdInput = modal.querySelector("#two-factor-user-id");
  if (!userIdInput) {
    userIdInput = document.createElement("input");
    userIdInput.type = "hidden";
    userIdInput.id = "two-factor-user-id";
    modal.appendChild(userIdInput);
  }
  userIdInput.value = userId;

  if (isEnabled) {
    // 已启用2FA，显示管理界面
    setupStep.classList.add("hidden");
    manageStep.classList.remove("hidden");
    enableBtn.classList.add("hidden");
    disableBtn.classList.remove("hidden");
  } else {
    // 未启用2FA，显示设置界面
    setupStep.classList.remove("hidden");
    manageStep.classList.add("hidden");
    enableBtn.classList.remove("hidden");
    disableBtn.classList.add("hidden");

    // 初始化2FA配置
    await initTwoFactorConfig(userId);
  }
}

// 初始化2FA配置
async function initTwoFactorConfig(userId) {
  try {
    // 权限由后端强制校验：无权调用时后端返回 403，错误信息经下方 else 分支透传展示。
    // 前端不再代为拦截（仅前端判断可被绕过，也不应伪造拦截成功的路径）
    const response = await apiPost("/api/two-factor/init", { user_id: userId });
    if (response.success) {
      const { secret, qr_code_base64, otpauth_url } = response.data;

      // 显示QR码和密钥
      const qrCodeImg = elementCache.get("two-factor-qr-code");
      if (qrCodeImg && qr_code_base64) {
        qrCodeImg.src = `data:image/png;base64,${qr_code_base64}`;
      }

      const secretInput = elementCache.get("two-factor-secret");
      if (secretInput) {
        secretInput.value = secret;
      }

      // 存储URI用于验证
      const uriInput = elementCache.get("two-factor-uri");
      if (uriInput) {
        uriInput.value = otpauth_url;
      }
    } else {
      // 后端拒绝（含 403）时展示后端返回的真实错误信息
      showToast(`${t("two_factor.fetch_config_failed")}：${response.message}`, "error");
    }
  } catch (error) {
    console.error("获取2FA配置失败:", error);
    // 网络异常也尽量透出具体信息，避免只显示笼统文案
    showToast(`${t("two_factor.fetch_config_network")}：${error?.message ?? error}`, "error");
  }
}

// 启用2FA
async function handleTwoFactorEnable() {
  const codeInput = elementCache.get("two-factor-verify-code");
  const errorDiv = elementCache.get("two-factor-error");
  const successDiv = elementCache.get("two-factor-success");
  const userIdInput = elementCache.get("two-factor-user-id");

  if (!codeInput || !errorDiv || !successDiv || !userIdInput) {
    console.error("2FA相关DOM元素未找到");
    return;
  }

  const code = codeInput.value;
  const userId = userIdInput.value;

  if (!code || code.length !== 6) {
    errorDiv.textContent = t("two_factor.verify_code_placeholder");
    errorDiv.classList.add("show");
    return;
  }

  try {
    const response = await apiPost("/api/two-factor/enable", { code, user_id: userId });
    if (response.success) {
      successDiv.textContent = t("two_factor.enable_success");
      successDiv.classList.add("show");

      setTimeout(() => {
        closeModal("two-factor-modal");
        successDiv.classList.remove("show");
        loadUsersData();
      }, 2000);
    } else {
      errorDiv.textContent = `${t("two_factor.enable_failed")}：${response.message}`;
      errorDiv.classList.add("show");
    }
  } catch (error) {
    console.error("启用2FA失败:", error);
    errorDiv.textContent = t("two_factor.enable_failed_network");
    errorDiv.classList.add("show");
  }
}

// 禁用2FA
async function handleTwoFactorDisable() {
  const codeInput = elementCache.get("two-factor-disable-code");
  const errorDiv = elementCache.get("two-factor-disable-error");
  const userIdInput = elementCache.get("two-factor-user-id");

  if (!codeInput || !errorDiv || !userIdInput) {
    console.error("2FA禁用相关DOM元素未找到");
    return;
  }

  const code = codeInput.value;
  const userId = userIdInput.value;

  if (!code || code.length !== 6) {
    errorDiv.textContent = t("two_factor.verify_code_placeholder");
    errorDiv.classList.add("show");
    return;
  }

  const confirmed = await showConfirm(t("two_factor.disable_confirm"));
  if (!confirmed) {
    return;
  }

  try {
    const response = await apiPost("/api/two-factor/disable", { code, user_id: userId });
    if (response.success) {
      showToast(t("two_factor.disable_success"), "success");
      closeModal("two-factor-modal");
      loadUsersData();
    } else {
      errorDiv.textContent = `${t("two_factor.disable_failed")}：${response.message}`;
      errorDiv.classList.add("show");
    }
  } catch (error) {
    console.error("禁用2FA失败:", error);
    errorDiv.textContent = t("two_factor.disable_failed_network");
    errorDiv.classList.add("show");
  }
}

// 用户表单提交
export async function submitUserForm() {
  const userId = elementCache.getValue("user-id");
  const form = elementCache.get("user-form");
  // 模态模板加载失败时表单不存在,直接中止避免空引用
  if (!form) {
    console.error("用户表单元素未找到");
    return;
  }
  const formData = new FormData(form);
  const userData = {
    username: formData.get("username"),
    email: formData.get("email"),
    role: formData.get("role"),
    status: formData.get("status") === "true"
  };

  const password = formData.get("password");
  const passwordConfirm = formData.get("password_confirm");

  if (password) {
    if (password !== passwordConfirm) {
      showToast(t("user.password_mismatch"), "error");
      return;
    }
    userData.password = password;
  }

  // 防重复提交：请求期间禁用保存按钮，结束后恢复（双击会重复提交产生重复用户）
  const saveBtn = document.querySelector("#user-form button[type='submit']");
  const originalText = saveBtn?.textContent;
  if (saveBtn) {
    saveBtn.disabled = true;
    saveBtn.textContent = t("common.saving");
  }

  try {
    let response;
    if (userId) {
      response = await apiPut(`/api/users/${userId}`, userData);
    } else {
      response = await apiPost("/api/users", userData);
    }

    if (response.success) {
      showToast(userId ? t("user.update_success") : t("user.add_success"), "success");
      closeModal("user-modal");
      form.reset();
      loadUsersData();
    } else {
      showToast(
        `${userId ? t("user.update_failed") : t("user.add_failed")}：${response.message}`,
        "error"
      );
    }
  } catch (error) {
    console.error("保存用户失败:", error);
    showToast(t("user.save_failed_network"), "error");
  } finally {
    if (saveBtn) {
      saveBtn.disabled = false;
      saveBtn.textContent = originalText;
    }
  }
}
