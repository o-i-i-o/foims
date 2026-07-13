import {
  apiGet,
  apiPost,
  apiPut,
  apiDelete,
} from "../utils/apiClient.js";

import {
  showToast,
  renderTable,
  formatDateTime,
  appendPaginationToTable,
  escapeHtml,
} from "../utils/ui.js";

import { openModal, closeModal } from "../utils/modal.js";
import { getUser } from "../utils/sessionManager.js";
import { t } from "../utils/i18n.js";
import { elementCache } from "../utils/helpers.js";
import { showConfirm } from "../utils/confirm.js";

let currentUserPage = 1;
const USER_PAGE_SIZE = 20;

// 加载用户数据
export async function loadUsersData(page = 1) {
  currentUserPage = page;
  try {
    const response = await apiGet(`/api/users?page=${page}&page_size=${USER_PAGE_SIZE}`);
    if (response.success) {
      const data = response.data;
      const users = data.items || data;
      const pagination = data.total !== undefined ? data : null;
      const tableBody = document.querySelector("#users-table tbody");
      
      if (users.length === 0) {
        tableBody.innerHTML = `
          <tr class="empty-row">
            <td colspan="7" class="text-center">${t('common.no_data')}</td>
          </tr>
        `;
        return;
      }

      tableBody.innerHTML = users.map(user => `
        <tr data-user-id="${user.id}">
          <td>${escapeHtml(user.username)}</td>
          <td>${escapeHtml(user.email)}</td>
          <td>${user.role === 'admin' ? t('user.role_admin') : t('user.role_user')}</td>
          <td>
            <span class="status-badge ${user.status ? 'status-active' : 'status-inactive'}">
              ${user.status ? t('user.status_enabled') : t('user.status_disabled')}
            </span>
          </td>
          <td>
            <span class="two-factor-badge ${user.two_factor_enabled ? 'two-factor-enabled' : 'two-factor-disabled'}">
              ${user.two_factor_enabled ? t('user.two_factor_enabled') : t('user.two_factor_disabled')}
            </span>
          </td>
          <td>${formatDateTime(user.created_at)}</td>
          <td>
            <button class="btn btn-secondary btn-sm btn-edit" data-id="${user.id}">${t('common.edit')}</button>
            <button class="btn btn-secondary btn-sm user-2fa" data-id="${user.id}" data-username="${escapeHtml(user.username)}" data-enabled="${user.two_factor_enabled}">
              ${user.two_factor_enabled ? t('user.manage_2fa') : t('user.enable_2fa')}
            </button>
            <button class="btn btn-danger btn-sm btn-delete" data-id="${user.id}">${t('common.delete')}</button>
          </td>
        </tr>
      `).join("");

      if (pagination) {
        appendPaginationToTable("#users-table", pagination, loadUsersData);
      }
    } else {
      showToast(`${t('common.load_failed')}：${response.message}`, "error");
    }
  } catch (error) {
    console.error("加载用户数据失败:", error);
    showToast(t('common.load_failed_retry'), "error");
  }
}

// 打开用户模态框
export async function openUserModal(userId) {
  await openModal("user-modal");
  
  const modal = elementCache.get("user-modal");
  const title = elementCache.get("user-modal-title");
  const userIdInput = elementCache.get("user-id");
  const usernameInput = elementCache.get("user-username");
  const emailInput = elementCache.get("user-email");
  const roleInput = elementCache.get("user-role");
  const statusInput = elementCache.get("user-status");
  const passwordInput = elementCache.get("user-password");
  const passwordConfirmInput = elementCache.get("user-password-confirm");

  if (!modal || !title || !userIdInput || !usernameInput || !emailInput || 
      !roleInput || !statusInput || !passwordInput || !passwordConfirmInput) {
    console.error("用户模态框相关DOM元素未找到");
    return;
  }

  if (userId) {
    title.textContent = t('user.edit_user');
    userIdInput.value = userId;
    passwordInput.placeholder = t('user.password_leave_blank');
    passwordInput.required = false;
    passwordConfirmInput.placeholder = t('user.password_leave_blank');
    passwordConfirmInput.required = false;
    
    loadUserData(userId);
  } else {
    title.textContent = t('user.add_user');
    userIdInput.value = "";
    usernameInput.value = "";
    emailInput.value = "";
    roleInput.value = "user";
    statusInput.value = "true";
    passwordInput.value = "";
    passwordInput.placeholder = t('user.password_placeholder');
    passwordInput.required = true;
    passwordConfirmInput.value = "";
    passwordConfirmInput.placeholder = t('user.password_confirm_placeholder');
    passwordConfirmInput.required = true;
  }
}

window.openUserModal = openUserModal;

// 加载用户数据
async function loadUserData(userId) {
  try {
    const response = await apiGet(`/api/users/${userId}`);
    if (response.success) {
      const user = response.data;
      elementCache.setValue("user-username", user.username);
      elementCache.setValue("user-email", user.email);
      elementCache.setValue("user-role", user.role);
      elementCache.setValue("user-status", user.status.toString());
    }
  } catch (error) {
    console.error("加载用户数据失败:", error);
    showToast(t('common.load_failed'), "error");
  }
}

// 删除用户
export async function deleteUser(userId) {
  const confirmed = await showConfirm(t('user.delete_confirm'));
  if (!confirmed) {
    return;
  }

  try {
    const response = await apiDelete(`/api/users/${userId}`);
    if (response.success) {
      showToast(t('user.delete_user') + t('common.success'), "success");
      loadUsersData();
    } else {
      showToast(t('user.delete_user') + t('common.failed') + "：" + response.message, "error");
    }
  } catch (error) {
    console.error("删除用户失败:", error);
    showToast(t('user.delete_user') + t('common.failed'), "error");
  }
}


// 注意：用户编辑/删除按钮的点击事件已在 eventManager.js 中统一处理
// 此处 initUserEvents 函数保留用于处理 2FA 相关按钮
let userEventsInitialized = false;

function initUserEvents() {
  if (userEventsInitialized) return;
  userEventsInitialized = true;
  
  document.addEventListener("click", (e) => {
    if (e.target.classList.contains("user-2fa")) {
      const id = e.target.getAttribute("data-id");
      const username = e.target.getAttribute("data-username");
      const enabled = e.target.getAttribute("data-enabled") === "true";
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
window.openTwoFactorModal = async function(userId, username, isEnabled) {
  const modal = openModal("two-factor-modal");
  
  if (!modal) {
    showToast("无法打开2FA模态框", "error");
    return;
  }
  
  const setupStep = modal.querySelector("#two-factor-setup-step");
  const manageStep = modal.querySelector("#two-factor-manage-step");
  const enableBtn = modal.querySelector("#two-factor-enable-btn");
  const disableBtn = modal.querySelector("#two-factor-disable-btn");
  const qrCodeImg = modal.querySelector("#two-factor-qr-code");
  const secretInput = modal.querySelector("#two-factor-secret");
  const verifyCodeInput = modal.querySelector("#two-factor-verify-code");
  const disableCodeInput = modal.querySelector("#two-factor-disable-code");
  const errorDiv = modal.querySelector("#two-factor-error");
  const successDiv = modal.querySelector("#two-factor-success");

  if (!setupStep || !manageStep || !enableBtn || !disableBtn) {
    console.error("2FA模态框元素未找到", { setupStep, manageStep, enableBtn, disableBtn });
    showToast("2FA模态框加载失败", "error");
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
  if (verifyCodeInput) verifyCodeInput.value = "";
  if (disableCodeInput) disableCodeInput.value = "";

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
};

// 初始化2FA配置
async function initTwoFactorConfig(userId) {
  try {
    const currentUser = getUser();
    if (currentUser && currentUser.id !== userId) {
      // 不是当前用户，需要检查是否是管理员
      if (currentUser.role !== 'admin') {
        showToast("权限不足，只有管理员可以为其他用户操作2FA", "error");
        return;
      }
    }
    
    const response = await apiPost("/api/two-factor/init", { user_id: userId });
    if (response.success) {
      const { secret, qr_code_base64, otpauth_url } = response.data;
      
      // 显示QR码和密钥
      const qrCodeImg = elementCache.get("two-factor-qr-code");
      if (qrCodeImg && qr_code_base64) {
        qrCodeImg.src = "data:image/png;base64," + qr_code_base64;
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
      showToast("获取2FA配置失败：" + response.message, "error");
    }
  } catch (error) {
    console.error("获取2FA配置失败:", error);
    showToast("获取2FA配置失败，请检查网络连接", "error");
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
    errorDiv.textContent = "请输入6位验证码";
    errorDiv.classList.add("show");
    return;
  }

  try {
    const response = await apiPost("/api/two-factor/enable", { code, user_id: userId });
    if (response.success) {
      successDiv.textContent = "双因素认证已成功启用！";
      successDiv.classList.add("show");
      
      setTimeout(() => {
        closeModal("two-factor-modal");
        successDiv.classList.remove("show");
        loadUsersData();
      }, 2000);
    } else {
      errorDiv.textContent = "启用失败：" + response.message;
      errorDiv.classList.add("show");
    }
  } catch (error) {
    console.error("启用2FA失败:", error);
    errorDiv.textContent = "启用失败，请检查网络连接";
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
    errorDiv.textContent = "请输入6位验证码";
    errorDiv.classList.add("show");
    return;
  }

  const confirmed = await showConfirm(t('two_factor.disable_confirm'));
  if (!confirmed) {
    return;
  }

  try {
    const response = await apiPost("/api/two-factor/disable", { code, user_id: userId });
    if (response.success) {
      showToast("双因素认证已成功禁用", "success");
      closeModal("two-factor-modal");
      loadUsersData();
    } else {
      errorDiv.textContent = "禁用失败：" + response.message;
      errorDiv.classList.add("show");
    }
  } catch (error) {
    console.error("禁用2FA失败:", error);
    errorDiv.textContent = "禁用失败，请检查网络连接";
    errorDiv.classList.add("show");
  }
}

// 用户表单提交
export async function submitUserForm() {
  const userId = elementCache.getValue("user-id");
  const form = elementCache.get("user-form");
  const formData = new FormData(form);
  const userData = {
    username: formData.get("username"),
    email: formData.get("email"),
    role: formData.get("role"),
    status: formData.get("status") === "true",
  };

  const password = formData.get("password");
  const passwordConfirm = formData.get("password_confirm");
  
  if (password) {
    if (password !== passwordConfirm) {
      showToast("两次输入的密码不一致", "error");
      return;
    }
    userData.password = password;
  }

  try {
    let response;
    if (userId) {
      response = await apiPut(`/api/users/${userId}`, userData);
    } else {
      response = await apiPost("/api/users", userData);
    }

    if (response.success) {
      showToast(userId ? "用户更新成功" : "用户添加成功", "success");
      closeModal("user-modal");
      form.reset();
      loadUsersData();
    } else {
      showToast((userId ? "更新" : "添加") + "用户失败：" + response.message, "error");
    }
  } catch (error) {
    console.error("保存用户失败:", error);
    showToast("保存用户失败，请检查网络连接", "error");
  }
}

const userForm = elementCache.get("user-form");
if (userForm) {
  userForm.addEventListener("submit", async (e) => {
    e.preventDefault();
    await submitUserForm();
  });
}

