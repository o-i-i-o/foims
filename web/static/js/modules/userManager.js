import {
  apiRequest,
  apiGet,
  apiPost,
  apiPut,
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

import { openModal, closeModal } from "../utils/modal.js";

// 加载用户数据
export async function loadUsersData() {
  try {
    const response = await apiGet("/api/users");
    if (response.success) {
      const users = response.data;
      const tableBody = document.querySelector("#users-table tbody");
      
      if (users.length === 0) {
        tableBody.innerHTML = `
          <tr class="empty-row">
            <td colspan="7" class="text-center">暂无用户数据</td>
          </tr>
        `;
        return;
      }

      tableBody.innerHTML = users.map(user => `
        <tr data-user-id="${user.id}">
          <td>${user.username}</td>
          <td>${user.email}</td>
          <td>${user.role === 'admin' ? '管理员' : '普通用户'}</td>
          <td>
            <span class="status-badge ${user.status ? 'status-active' : 'status-inactive'}">
              ${user.status ? '启用' : '禁用'}
            </span>
          </td>
          <td>
            <span class="two-factor-badge ${user.two_factor_enabled ? 'two-factor-enabled' : 'two-factor-disabled'}">
              ${user.two_factor_enabled ? '已启用' : '未启用'}
            </span>
          </td>
          <td>${formatDateTime(user.created_at)}</td>
          <td>
            <button class="btn btn-secondary btn-sm user-edit" data-id="${user.id}">编辑</button>
            <button class="btn btn-secondary btn-sm user-2fa" data-id="${user.id}" data-username="${user.username}" data-enabled="${user.two_factor_enabled}">
              ${user.two_factor_enabled ? '管理2FA' : '启用2FA'}
            </button>
            <button class="btn btn-danger btn-sm user-delete" data-id="${user.id}">删除</button>
          </td>
        </tr>
      `).join("");
    } else {
      showMessage("加载用户数据失败：" + response.message, "error");
    }
  } catch (error) {
    console.error("加载用户数据失败:", error);
    showMessage("加载用户数据失败，请检查网络连接", "error");
  }
}

// 打开用户模态框
export function openUserModal(userId) {
  const modal = document.getElementById("user-modal");
  const title = document.getElementById("user-modal-title");
  const userIdInput = document.getElementById("user-id");
  const usernameInput = document.getElementById("user-username");
  const emailInput = document.getElementById("user-email");
  const roleInput = document.getElementById("user-role");
  const statusInput = document.getElementById("user-status");
  const passwordInput = document.getElementById("user-password");

  if (userId) {
    title.textContent = "编辑用户";
    userIdInput.value = userId;
    passwordInput.placeholder = "留空不修改密码";
    passwordInput.required = false;
    
    loadUserData(userId);
  } else {
    title.textContent = "添加用户";
    userIdInput.value = "";
    usernameInput.value = "";
    emailInput.value = "";
    roleInput.value = "user";
    statusInput.value = "true";
    passwordInput.value = "";
    passwordInput.placeholder = "请输入密码（添加时必填）";
    passwordInput.required = true;
  }

  openModal("user-modal");
}

window.openUserModal = openUserModal;

// 加载用户数据
async function loadUserData(userId) {
  try {
    const response = await apiGet(`/api/users/${userId}`);
    if (response.success) {
      const user = response.data;
      document.getElementById("user-username").value = user.username;
      document.getElementById("user-email").value = user.email;
      document.getElementById("user-role").value = user.role;
      document.getElementById("user-status").value = user.status.toString();
    }
  } catch (error) {
    console.error("加载用户数据失败:", error);
    showMessage("加载用户数据失败", "error");
  }
}

// 删除用户
export async function deleteUser(userId) {
  if (!confirm("确定要删除此用户吗？")) {
    return;
  }

  try {
    const response = await apiDelete(`/api/users/${userId}`);
    if (response.success) {
      showMessage("用户删除成功", "success");
      loadUsersData();
    } else {
      showMessage("删除用户失败：" + response.message, "error");
    }
  } catch (error) {
    console.error("删除用户失败:", error);
    showMessage("删除用户失败，请检查网络连接", "error");
  }
}

// 为了保持与现有代码的兼容性，仍然在window对象上注册该函数
window.deleteUser = deleteUser;

// 注意：用户编辑/删除按钮的点击事件已在 eventManager.js 中统一处理
// 此处 initUserEvents 函数保留用于处理 2FA 相关按钮
function initUserEvents() {
  document.addEventListener("click", (e) => {
    if (e.target.classList.contains("user-2fa")) {
      const id = e.target.getAttribute("data-id");
      const username = e.target.getAttribute("data-username");
      const enabled = e.target.getAttribute("data-enabled") === "true";
      if (id && username) {
        openTwoFactorModal(id, username, enabled);
      }
    }
  });
}

// 导出初始化函数
export { initUserEvents };

// 打开2FA模态框
window.openTwoFactorModal = async function(userId, username, isEnabled) {
  const modal = document.getElementById("two-factor-modal");
  const setupStep = document.getElementById("two-factor-setup-step");
  const manageStep = document.getElementById("two-factor-manage-step");
  const enableBtn = document.getElementById("two-factor-enable-btn");
  const disableBtn = document.getElementById("two-factor-disable-btn");
  const qrCodeImg = document.getElementById("two-factor-qr-code");
  const secretInput = document.getElementById("two-factor-secret");
  const verifyCodeInput = document.getElementById("two-factor-verify-code");
  const disableCodeInput = document.getElementById("two-factor-disable-code");
  const errorDiv = document.getElementById("two-factor-error");
  const successDiv = document.getElementById("two-factor-success");

  // 清空错误和成功消息
  errorDiv.textContent = "";
  errorDiv.classList.remove("show");
  successDiv.textContent = "";
  successDiv.classList.remove("show");

  // 清空输入框
  verifyCodeInput.value = "";
  disableCodeInput.value = "";

  // 确保有一个隐藏的输入字段来存储userId
  let userIdInput = document.getElementById("two-factor-user-id");
  if (!userIdInput) {
    userIdInput = document.createElement("input");
    userIdInput.type = "hidden";
    userIdInput.id = "two-factor-user-id";
    modal.appendChild(userIdInput);
  }
  userIdInput.value = userId;

  if (isEnabled) {
    // 已启用2FA，显示管理界面
    setupStep.style.display = "none";
    manageStep.style.display = "block";
    enableBtn.style.display = "none";
    disableBtn.style.display = "inline-block";
  } else {
    // 未启用2FA，显示设置界面
    setupStep.style.display = "block";
    manageStep.style.display = "none";
    enableBtn.style.display = "inline-block";
    disableBtn.style.display = "none";
    
    // 初始化2FA配置
    await initTwoFactorConfig(userId);
  }

  openModal("two-factor-modal");
};

// 初始化2FA配置
async function initTwoFactorConfig(userId) {
  try {
    // 检查当前登录用户是否有权限为其他用户操作2FA
    const currentUser = JSON.parse(sessionStorage.getItem("user"));
    if (currentUser && currentUser.id !== userId) {
      // 不是当前用户，需要检查是否是管理员
      if (currentUser.role !== 'admin') {
        showMessage("权限不足，只有管理员可以为其他用户操作2FA", "error");
        return;
      }
    }
    
    const response = await apiPost("/api/two-factor/init", { user_id: userId });
    if (response.success) {
      const { secret, qr_code, uri } = response.data;
      
      // 显示QR码和密钥
      document.getElementById("two-factor-qr-code").src = qr_code;
      document.getElementById("two-factor-secret").value = secret;
      
      // 存储URI用于验证
      const uriInput = document.getElementById("two-factor-uri");
      if (uriInput) {
        uriInput.value = uri;
      }
    } else {
      showMessage("获取2FA配置失败：" + response.message, "error");
    }
  } catch (error) {
    console.error("获取2FA配置失败:", error);
    showMessage("获取2FA配置失败，请检查网络连接", "error");
  }
}

// 启用2FA
document.getElementById("two-factor-enable-btn").addEventListener("click", async () => {
  const code = document.getElementById("two-factor-verify-code").value;
  const errorDiv = document.getElementById("two-factor-error");
  const successDiv = document.getElementById("two-factor-success");
  const userId = document.getElementById("two-factor-user-id").value;

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
      
      // 2秒后关闭模态框并刷新用户列表
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
});

// 禁用2FA
document.getElementById("two-factor-disable-btn").addEventListener("click", async () => {
  const code = document.getElementById("two-factor-disable-code").value;
  const errorDiv = document.getElementById("two-factor-disable-error");
  const userId = document.getElementById("two-factor-user-id").value;

  if (!code || code.length !== 6) {
    errorDiv.textContent = "请输入6位验证码";
    errorDiv.classList.add("show");
    return;
  }

  if (!confirm("确定要禁用双因素认证吗？这将降低账户安全性。")) {
    return;
  }

  try {
    const response = await apiPost("/api/two-factor/disable", { code, user_id: userId });
    if (response.success) {
      showMessage("双因素认证已成功禁用", "success");
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
});

// 用户表单提交
export async function submitUserForm() {
  const userId = document.getElementById("user-id").value;
  const form = document.getElementById("user-form");
  const formData = new FormData(form);
  const userData = {
    username: formData.get("username"),
    email: formData.get("email"),
    role: formData.get("role"),
    status: formData.get("status") === "true",
  };

  const password = formData.get("password");
  if (password) {
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
      showMessage(userId ? "用户更新成功" : "用户添加成功", "success");
      closeModal("user-modal");
      form.reset();
      loadUsersData();
    } else {
      showMessage((userId ? "更新" : "添加") + "用户失败：" + response.message, "error");
    }
  } catch (error) {
    console.error("保存用户失败:", error);
    showMessage("保存用户失败，请检查网络连接", "error");
  }
}

document.getElementById("user-form").addEventListener("submit", async (e) => {
  e.preventDefault();
  await submitUserForm();
});

