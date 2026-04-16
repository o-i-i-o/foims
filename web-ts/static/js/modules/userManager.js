import { apiPost, } from "../utils/apiClient.js";
import { showToast, formatDateTime, appendPaginationToTable, escapeHtml, } from "../utils/ui.js";
import { openModal, closeModal } from "../utils/modal.js";
import { getUser } from "../utils/sessionManager.js";
import { t } from "../utils/i18n.js";
import { elementCache } from "../utils/helpers.js";
import { userManager } from "../utils/managers.js";
const USER_PAGE_SIZE = 20;
export async function loadUsersData(page = 1) {
    try {
        const result = await userManager.list({
            page,
            pageSize: USER_PAGE_SIZE,
        });
        if (!result)
            return;
        const data = result.data;
        const users = data.items || [];
        const pagination = data.total !== undefined ? data : null;
        const tableBody = document.querySelector("#users-table tbody");
        if (!tableBody)
            return;
        if (users.length === 0) {
            tableBody.innerHTML = `<tr class="empty-row"><td colspan="7" class="text-center">${t("common.no_data")}</td></tr>`;
            return;
        }
        tableBody.innerHTML = users.map((user) => {
            const u = user;
            return `<tr data-user-id="${u.id}">
        <td>${escapeHtml(u.username)}</td>
        <td>${escapeHtml(u.email)}</td>
        <td>${u.role === "admin" ? t("user.role_admin") : t("user.role_user")}</td>
        <td><span class="status-badge ${u.status ? "status-active" : "status-inactive"}">${u.status ? t("user.status_enabled") : t("user.status_disabled")}</span></td>
        <td><span class="two-factor-badge ${u.two_factor_enabled ? "two-factor-enabled" : "two-factor-disabled"}">${u.two_factor_enabled ? t("user.two_factor_enabled") : t("user.two_factor_disabled")}</span></td>
        <td>${formatDateTime(u.created_at)}</td>
        <td>
          <button class="btn btn-secondary btn-sm btn-edit" data-id="${u.id}">${t("common.edit")}</button>
          <button class="btn btn-secondary btn-sm user-2fa" data-id="${u.id}" data-username="${escapeHtml(u.username)}" data-enabled="${u.two_factor_enabled}">${u.two_factor_enabled ? t("user.manage_2fa") : t("user.enable_2fa")}</button>
          <button class="btn btn-danger btn-sm btn-delete" data-id="${u.id}">${t("common.delete")}</button>
        </td>
      </tr>`;
        }).join("");
        if (pagination) {
            appendPaginationToTable("#users-table", pagination, loadUsersData);
        }
    }
    catch (error) {
        console.error("加载用户数据失败:", error);
        showToast(t("common.load_failed_retry"), "error");
    }
}
export function openUserModal(userId) {
    openModal("user-modal");
    const title = elementCache.get("user-modal-title");
    const userIdInput = elementCache.get("user-id");
    const usernameInput = elementCache.get("user-username");
    const emailInput = elementCache.get("user-email");
    const roleInput = elementCache.get("user-role");
    const statusInput = elementCache.get("user-status");
    const passwordInput = elementCache.get("user-password");
    const passwordConfirmInput = elementCache.get("user-password-confirm");
    if (!title || !userIdInput || !usernameInput || !emailInput || !roleInput || !statusInput || !passwordInput || !passwordConfirmInput) {
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
    }
    else {
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
window.openUserModal = openUserModal;
async function loadUserData(userId) {
    try {
        const user = await userManager.get(userId);
        if (user) {
            elementCache.setValue("user-username", user.username);
            elementCache.setValue("user-email", user.email);
            elementCache.setValue("user-role", user.role);
            elementCache.setValue("user-status", String(user.status));
        }
    }
    catch (error) {
        console.error("加载用户数据失败:", error);
        showToast(t("common.load_failed"), "error");
    }
}
export async function deleteUser(userId) {
    const result = await userManager.delete(userId, { confirmMessage: t("user.delete_confirm") });
    if (result.success) {
        await loadUsersData();
    }
}
function initUserEvents() {
    document.addEventListener("click", (e) => {
        const target = e.target;
        if (target.classList.contains("user-2fa")) {
            const id = target.getAttribute("data-id");
            const username = target.getAttribute("data-username");
            const enabled = target.getAttribute("data-enabled") === "true";
            if (id && username)
                openTwoFactorModal(id, username, enabled);
        }
        if (target.id === "two-factor-enable-btn")
            handleTwoFactorEnable();
        if (target.id === "two-factor-disable-btn")
            handleTwoFactorDisable();
    });
}
export { initUserEvents };
async function openTwoFactorModal(userId, _username, isEnabled) {
    const modal = openModal("two-factor-modal");
    if (!modal) {
        showToast("无法打开2FA模态框", "error");
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
        showToast("2FA模态框加载失败", "error");
        return;
    }
    if (errorDiv) {
        errorDiv.textContent = "";
        errorDiv.classList.remove("show");
    }
    if (successDiv) {
        successDiv.textContent = "";
        successDiv.classList.remove("show");
    }
    if (verifyCodeInput)
        verifyCodeInput.value = "";
    if (disableCodeInput)
        disableCodeInput.value = "";
    let userIdInput = modal.querySelector("#two-factor-user-id");
    if (!userIdInput) {
        userIdInput = document.createElement("input");
        userIdInput.type = "hidden";
        userIdInput.id = "two-factor-user-id";
        modal.appendChild(userIdInput);
    }
    userIdInput.value = userId;
    if (isEnabled) {
        setupStep.classList.add("hidden");
        manageStep.classList.remove("hidden");
        enableBtn.classList.add("hidden");
        disableBtn.classList.remove("hidden");
    }
    else {
        setupStep.classList.remove("hidden");
        manageStep.classList.add("hidden");
        enableBtn.classList.remove("hidden");
        disableBtn.classList.add("hidden");
        await initTwoFactorConfig(userId);
    }
}
window.openTwoFactorModal = openTwoFactorModal;
async function initTwoFactorConfig(userId) {
    try {
        const currentUser = getUser();
        if (currentUser && String(currentUser.id) !== userId && currentUser.role !== "admin") {
            showToast("权限不足，只有管理员可以为其他用户操作2FA", "error");
            return;
        }
        const response = await apiPost("/api/two-factor/init", { user_id: userId });
        if (response.success) {
            const data = response.data;
            const qrCodeImg = elementCache.get("two-factor-qr-code");
            if (qrCodeImg && data.qr_code_base64)
                qrCodeImg.src = "data:image/png;base64," + data.qr_code_base64;
            const secretInput = elementCache.get("two-factor-secret");
            if (secretInput)
                secretInput.value = data.secret;
            const uriInput = elementCache.get("two-factor-uri");
            if (uriInput)
                uriInput.value = data.otpauth_url;
        }
        else {
            showToast("获取2FA配置失败：" + response.message, "error");
        }
    }
    catch (error) {
        console.error("获取2FA配置失败:", error);
        showToast("获取2FA配置失败，请检查网络连接", "error");
    }
}
async function handleTwoFactorEnable() {
    const codeInput = elementCache.get("two-factor-verify-code");
    const errorDiv = elementCache.get("two-factor-error");
    const successDiv = elementCache.get("two-factor-success");
    const userIdInput = elementCache.get("two-factor-user-id");
    if (!codeInput || !errorDiv || !successDiv || !userIdInput)
        return;
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
            setTimeout(() => { closeModal("two-factor-modal"); successDiv.classList.remove("show"); loadUsersData(); }, 2000);
        }
        else {
            errorDiv.textContent = "启用失败：" + response.message;
            errorDiv.classList.add("show");
        }
    }
    catch (error) {
        console.error("启用2FA失败:", error);
        errorDiv.textContent = "启用失败，请检查网络连接";
        errorDiv.classList.add("show");
    }
}
async function handleTwoFactorDisable() {
    const codeInput = elementCache.get("two-factor-disable-code");
    const errorDiv = elementCache.get("two-factor-disable-error");
    const userIdInput = elementCache.get("two-factor-user-id");
    if (!codeInput || !errorDiv || !userIdInput)
        return;
    const code = codeInput.value;
    const userId = userIdInput.value;
    if (!code || code.length !== 6) {
        errorDiv.textContent = "请输入6位验证码";
        errorDiv.classList.add("show");
        return;
    }
    if (!confirm("确定要禁用双因素认证吗？这将降低账户安全性。"))
        return;
    try {
        const response = await apiPost("/api/two-factor/disable", { code, user_id: userId });
        if (response.success) {
            showToast("双因素认证已成功禁用", "success");
            closeModal("two-factor-modal");
            loadUsersData();
        }
        else {
            errorDiv.textContent = "禁用失败：" + response.message;
            errorDiv.classList.add("show");
        }
    }
    catch (error) {
        console.error("禁用2FA失败:", error);
        errorDiv.textContent = "禁用失败，请检查网络连接";
        errorDiv.classList.add("show");
    }
}
export async function submitUserForm() {
    const form = document.getElementById("user-form");
    if (!form)
        return;
    const formData = new FormData(form);
    const userId = formData.get("user-id");
    const username = formData.get("username");
    const email = formData.get("email");
    const role = formData.get("role");
    const status = formData.get("status") === "true";
    const password = formData.get("password");
    const passwordConfirm = formData.get("password_confirm");
    if (password && password !== passwordConfirm) {
        showToast("两次输入的密码不一致", "error");
        return;
    }
    const userData = {
        username: username.trim(),
        email: email.trim(),
        role,
        status,
    };
    if (password) {
        userData.password = password;
    }
    try {
        let result;
        if (userId) {
            result = await userManager.update(userId, userData);
        }
        else {
            result = await userManager.create(userData);
        }
        if (result.success) {
            closeModal("user-modal");
            form.reset();
            await loadUsersData();
        }
    }
    catch (error) {
        console.error("保存用户失败:", error);
        showToast("保存用户失败，请检查网络连接", "error");
    }
}
const userForm = elementCache.get("user-form");
if (userForm) {
    userForm.addEventListener("submit", async (e) => { e.preventDefault(); await submitUserForm(); });
}
//# sourceMappingURL=userManager.js.map