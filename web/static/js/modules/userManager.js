import { apiGet, apiPost, apiPut, apiDelete } from "../utils/apiClient.js";

import {
  showToast,
  retreatToLastPage,
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
import { createSeqGuard, elementCache } from "../utils/helpers.js";
import { showConfirm } from "../utils/confirm.js";

// 角色显示（三权分立 + 超管：admin/sysadmin/secadmin/auditor/user）
function roleLabel(role) {
  if (role === "admin") {
    return t("user.role_admin");
  }
  if (role === "sysadmin") {
    return t("user.role_sysadmin");
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
const usersSeq = createSeqGuard();

// 加载用户数据
export async function loadUsersData(page = currentUserPage, sortBy = null, sortOrder = null) {
  const requestSeq = usersSeq.next();
  currentUserPage = page;
  if (sortBy) {
    userTableState.setSort(sortBy, sortOrder);
  }
  try {
    const response = await apiGet(
      `/api/users?page=${page}&page_size=${USER_PAGE_SIZE}&sort_by=${userTableState.sortBy}&sort_order=${userTableState.sortOrder}`
    );
    if (!usersSeq.isCurrent(requestSeq)) {
      return; // 已有更新的请求,丢弃过期响应
    }
    if (response.success) {
      const data = response.data;
      const users = data.items || data;
      const pagination = data.total !== undefined ? data : null;
      const tableBody = document.querySelector("#users-table tbody");

      // 空列表且当前页大于 1：删除后页码越界，按 total_pages 一步回退（共享判定）
      const totalPages = retreatToLastPage(users, page, data, USER_PAGE_SIZE);
      if (totalPages) {
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

// 第二步（2FA）的目标用户 id：进入第二页时由编辑记录/表单隐藏域写入
let twoFactorTargetUserId = "";

// 编辑用户的 2FA 启用状态：loadUserData 时记录，供"下一步"切页时渲染视图
let twoFactorTargetEnabled = false;

// 打开用户模态框（第一步：账户信息）
export async function openUserModal(userId) {
  await openModal("user-modal");

  // 新弹窗打开即失效任何在途的用户资料请求
  userLoadToken++;
  twoFactorTargetEnabled = false;

  const modal = elementCache.get("user-modal");
  const title = elementCache.get("user-modal-title");
  const userIdInput = elementCache.get("user-id");
  const usernameInput = elementCache.get("user-username");
  const emailInput = elementCache.get("user-email");
  const roleInput = elementCache.get("user-role");
  const statusInput = elementCache.get("user-status");
  const passwordExpiryInput = elementCache.get("user-password-expiry");
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
    !passwordExpiryInput ||
    !passwordInput ||
    !passwordConfirmInput
  ) {
    console.error("用户模态框相关DOM元素未找到");
    return;
  }

  // 每次打开都回到第一步（closeModal 会移除模态框，此处的复位是并发首开兜底）
  resetUserModalSteps(modal);

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
    passwordExpiryInput.value = "0";
    passwordInput.value = "";
    passwordInput.placeholder = t("user.password_placeholder");
    passwordInput.required = true;
    passwordConfirmInput.value = "";
    passwordConfirmInput.placeholder = t("user.password_confirm_placeholder");
    passwordConfirmInput.required = true;
  }
}

// 模态框复位到第一步：仅显示账户信息页与“下一步”按钮
function resetUserModalSteps(modal) {
  const stepAccount = modal.querySelector("#user-modal-step-account");
  const step2fa = modal.querySelector("#user-modal-step-2fa");
  const nextBtn = elementCache.get("user-next-btn");
  const backBtn = elementCache.get("user-2fa-back-btn");
  const enableBtn = elementCache.get("user-2fa-enable-btn");
  const disableBtn = elementCache.get("user-2fa-disable-btn");
  const finishBtn = elementCache.get("user-2fa-finish-btn");
  const unsavedStep = modal.querySelector("#user-2fa-unsaved-step");

  stepAccount?.classList.remove("hidden");
  step2fa?.classList.add("hidden");
  nextBtn?.classList.remove("hidden");
  backBtn?.classList.add("hidden");
  enableBtn?.classList.add("hidden");
  disableBtn?.classList.add("hidden");
  finishBtn?.classList.add("hidden");
  unsavedStep?.classList.add("hidden");
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
      twoFactorTargetEnabled = Boolean(user.two_factor_enabled);
      elementCache.setValue("user-username", user.username);
      elementCache.setValue("user-email", user.email);
      elementCache.setValue("user-role", user.role);
      elementCache.setValue("user-status", user.status.toString());
      elementCache.setValue("user-password-expiry", String(user.password_expiry_days ?? 0));
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
// 此处 initUserEvents 函数保留用于处理第二页 2FA 按钮及表头排序
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
    if (e.target.id === "user-2fa-enable-btn") {
      handleUser2faEnable();
    }

    if (e.target.id === "user-2fa-disable-btn") {
      handleUser2faDisable();
    }

    if (e.target.id === "user-2fa-back-btn") {
      handleUser2faBack();
    }

    if (e.target.id === "user-2fa-finish-btn") {
      // 第二步"保存"：提交整个表单（账户+2FA 均为用户编辑的一部分），
      // 成功后关闭模态框——与原保存按钮行为一致
      finishUserForm();
    }
  });
}

// 导出初始化函数
export { initUserEvents };

// 进入第二步：双因素认证（必经此页，可不启用直接点"保存"结束）
// isEnabled 取编辑资料中的 two_factor_enabled：已启用展示管理页（可禁用），
// 未启用展示扫码设置页；新增未落库用户展示待保存提示
async function showUser2faStep(userId, isEnabled) {
  const modal = elementCache.get("user-modal");
  if (!modal) {
    return;
  }
  twoFactorTargetUserId = userId;

  const stepAccount = modal.querySelector("#user-modal-step-account");
  const step2fa = modal.querySelector("#user-modal-step-2fa");
  const nextBtn = elementCache.get("user-next-btn");
  const backBtn = elementCache.get("user-2fa-back-btn");
  const finishBtn = elementCache.get("user-2fa-finish-btn");
  const title = elementCache.get("user-modal-title");

  if (!stepAccount || !step2fa || !nextBtn || !backBtn || !finishBtn) {
    console.error("用户模态框第二页元素未找到");
    return;
  }

  // 切页与标题
  stepAccount.classList.add("hidden");
  step2fa.classList.remove("hidden");
  nextBtn.classList.add("hidden");
  backBtn.classList.remove("hidden");
  finishBtn.classList.remove("hidden");
  if (title) {
    title.textContent = t("two_factor.title");
  }

  await renderUser2faView(userId, isEnabled);
}

// 渲染第二页视图（启用↔禁用动态切换共用）：按当前状态展示对应视图与按钮
async function renderUser2faView(userId, isEnabled) {
  const modal = elementCache.get("user-modal");
  if (!modal) {
    return;
  }
  twoFactorTargetUserId = userId;

  const setupStep = modal.querySelector("#user-2fa-setup-step");
  const manageStep = modal.querySelector("#user-2fa-manage-step");
  const enableBtn = elementCache.get("user-2fa-enable-btn");
  const disableBtn = elementCache.get("user-2fa-disable-btn");

  if (!setupStep || !manageStep || !enableBtn || !disableBtn) {
    console.error("用户模态框 2FA 视图元素未找到");
    return;
  }

  // 清空上一次的输入与提示（视图切换后旧提示不应残留）
  elementCache.setValue("user-2fa-verify-code", "");
  elementCache.setValue("user-2fa-disable-code", "");
  for (const tipId of ["user-2fa-error", "user-2fa-disable-error"]) {
    const tip = elementCache.get(tipId);
    if (tip) {
      tip.textContent = "";
      tip.classList.remove("show");
    }
  }

  const unsavedStep = modal.querySelector("#user-2fa-unsaved-step");

  if (!userId) {
    // 未保存的新用户：2FA 需针对已存在用户，展示提示，
    // 保存成功后可在编辑用户中配置
    setupStep.classList.add("hidden");
    manageStep.classList.add("hidden");
    enableBtn.classList.add("hidden");
    disableBtn.classList.add("hidden");
    unsavedStep?.classList.remove("hidden");
    return;
  }
  unsavedStep?.classList.add("hidden");

  if (isEnabled) {
    // 已启用 2FA：管理页（可禁用），主按钮为“禁用2FA”
    setupStep.classList.add("hidden");
    manageStep.classList.remove("hidden");
    enableBtn.classList.add("hidden");
    disableBtn.classList.remove("hidden");
  } else {
    // 未启用 2FA：扫码设置页，主按钮为“启用2FA”
    setupStep.classList.remove("hidden");
    manageStep.classList.add("hidden");
    enableBtn.classList.remove("hidden");
    disableBtn.classList.add("hidden");

    // 初始化 2FA 配置（二维码 / 密钥）
    await initTwoFactorConfig(userId);
  }
}

// 第二步返回第一步：回到账户信息页继续编辑
function handleUser2faBack() {
  const modal = elementCache.get("user-modal");
  if (!modal) {
    return;
  }

  const stepAccount = modal.querySelector("#user-modal-step-account");
  const step2fa = modal.querySelector("#user-modal-step-2fa");
  const nextBtn = elementCache.get("user-next-btn");
  const backBtn = elementCache.get("user-2fa-back-btn");
  const enableBtn = elementCache.get("user-2fa-enable-btn");
  const disableBtn = elementCache.get("user-2fa-disable-btn");
  const finishBtn = elementCache.get("user-2fa-finish-btn");

  stepAccount?.classList.remove("hidden");
  step2fa?.classList.add("hidden");
  nextBtn?.classList.remove("hidden");
  backBtn?.classList.add("hidden");
  enableBtn?.classList.add("hidden");
  disableBtn?.classList.add("hidden");
  finishBtn?.classList.add("hidden");

  // 恢复标题：第二页覆盖成了 2FA 标题；按 user-id 有无还原"编辑/添加用户"
  const title = elementCache.get("user-modal-title");
  if (title) {
    title.textContent = elementCache.getValue("user-id")
      ? t("user.edit_user")
      : t("user.add_user");
  }
}

// 初始化2FA配置
async function initTwoFactorConfig(userId) {
  const errorDiv = elementCache.get("user-2fa-error");
  try {
    // 权限由后端强制校验：无权调用时后端返回 403，错误信息经下方 else 分支在页内透出。
    // 前端不再代为拦截（仅前端判断可被绕过，也不应伪造拦截成功的路径）
    const response = await apiPost("/api/two-factor/init", { user_id: userId });
    if (response.success) {
      const { secret, qr_code_base64 } = response.data;

      // 显示QR码和密钥
      const qrCodeImg = elementCache.get("user-2fa-qr-code");
      if (qrCodeImg && qr_code_base64) {
        qrCodeImg.src = `data:image/png;base64,${qr_code_base64}`;
      }

      const secretInput = elementCache.get("user-2fa-secret");
      if (secretInput) {
        secretInput.value = secret;
      }

      // otpauth URI 不落地展示：验证只提交动态码与用户 ID，URI 不参与请求
    } else {
      // 后端拒绝（含 403）时在第二页内展示后端返回的真实错误信息
      if (errorDiv) {
        errorDiv.textContent = `${t("two_factor.fetch_config_failed")}：${response.message}`;
        errorDiv.classList.add("show");
      }
    }
  } catch (error) {
    console.error("获取2FA配置失败:", error);
    // 网络异常也尽量透出具体信息，避免只显示笼统文案
    if (errorDiv) {
      errorDiv.textContent = `${t("two_factor.fetch_config_network")}：${error?.message ?? error}`;
      errorDiv.classList.add("show");
    }
  }
}

// 启用2FA（第二页）
async function handleUser2faEnable() {
  const codeInput = elementCache.get("user-2fa-verify-code");
  const errorDiv = elementCache.get("user-2fa-error");
  const enableBtn = elementCache.get("user-2fa-enable-btn");

  if (!codeInput || !errorDiv || !enableBtn || !twoFactorTargetUserId) {
    console.error("2FA启用相关DOM元素未找到");
    return;
  }

  const code = codeInput.value;

  if (!code || code.length !== 6) {
    errorDiv.textContent = t("two_factor.verify_code_placeholder");
    errorDiv.classList.add("show");
    return;
  }

  // 防重复提交：请求期间禁用按钮
  enableBtn.disabled = true;

  try {
    const response = await apiPost("/api/two-factor/enable", { code, user_id: twoFactorTargetUserId });
    if (response.success) {
      // 启用成功不关框：动态切到管理视图（主按钮变为“禁用2FA”），可继续操作
      showToast(t("two_factor.enable_success"), "success");
      await renderUser2faView(twoFactorTargetUserId, true);
      loadUsersData();
    } else {
      errorDiv.textContent = `${t("two_factor.enable_failed")}：${response.message}`;
      errorDiv.classList.add("show");
    }
  } catch (error) {
    console.error("启用2FA失败:", error);
    errorDiv.textContent = t("two_factor.enable_failed_network");
    errorDiv.classList.add("show");
  } finally {
    enableBtn.disabled = false;
  }
}

// 禁用2FA（第二页）
async function handleUser2faDisable() {
  const codeInput = elementCache.get("user-2fa-disable-code");
  const errorDiv = elementCache.get("user-2fa-disable-error");
  const disableBtn = elementCache.get("user-2fa-disable-btn");

  if (!codeInput || !errorDiv || !disableBtn || !twoFactorTargetUserId) {
    console.error("2FA禁用相关DOM元素未找到");
    return;
  }

  const code = codeInput.value;

  if (!code || code.length !== 6) {
    errorDiv.textContent = t("two_factor.verify_code_placeholder");
    errorDiv.classList.add("show");
    return;
  }

  const confirmed = await showConfirm(t("two_factor.disable_confirm"));
  if (!confirmed) {
    return;
  }

  disableBtn.disabled = true;

  try {
    const response = await apiPost("/api/two-factor/disable", { code, user_id: twoFactorTargetUserId });
    if (response.success) {
      // 禁用成功不关框：动态切回扫码设置页（重新 init 生成新密钥/二维码），
      // 主按钮变回“启用2FA”，可再次启用或直接“完成”
      showToast(t("two_factor.disable_success"), "success");
      await renderUser2faView(twoFactorTargetUserId, false);
      loadUsersData();
    } else {
      errorDiv.textContent = `${t("two_factor.disable_failed")}：${response.message}`;
      errorDiv.classList.add("show");
    }
  } catch (error) {
    console.error("禁用2FA失败:", error);
    errorDiv.textContent = t("two_factor.disable_failed_network");
    errorDiv.classList.add("show");
  } finally {
    disableBtn.disabled = false;
  }
}

// 第一步"下一步"：仅切换到第二页（不触发保存），保存统一由第二页"保存"提交。
// 事件委托对 submit 按钮的 click 会 preventDefault 直达回调，绕过浏览器
// 原生校验，因此必须显式 checkValidity（无效时标红并停留在第一页）
export async function goToUser2faStep() {
  const form = elementCache.get("user-form");
  if (form && !form.checkValidity()) {
    if (typeof form.reportValidity === "function") {
      form.reportValidity();
    }
    return;
  }
  const userId = elementCache.getValue("user-id");
  // 编辑用户：状态取 loadUserData 记录值；新增用户尚未落库，第二页展示待保存提示
  await showUser2faStep(userId, twoFactorTargetEnabled);
}

// 第二步"保存"：提交表单并在成功后关闭模态框；
// 表单校验失败（如上一步回改时清空了必填项）则切回第一页交由浏览器标红
async function finishUserForm() {
  const form = elementCache.get("user-form");
  if (form && !form.checkValidity()) {
    handleUser2faBack();
    if (typeof form.reportValidity === "function") {
      form.reportValidity();
    }
    return;
  }
  await submitUserForm();
}

// 用户表单提交（第二页"保存"）：保存账户信息，成功后关闭模态框；
// 失败时切回第一页便于修正（如密码复杂度、用户名重复等服务端校验）
async function submitUserForm() {
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
    status: formData.get("status") === "true",
    password_expiry_days: parseInt(formData.get("password_expiry_days"), 10) || 0
  };

  const password = formData.get("password");
  const passwordConfirm = formData.get("password_confirm");

  if (password) {
    if (password !== passwordConfirm) {
      showToast(t("user.password_mismatch"), "error");
      handleUser2faBack();
      return;
    }
    userData.password = password;
  }

  // 防重复提交：请求期间禁用“保存”按钮，结束后恢复
  const finishBtn = elementCache.get("user-2fa-finish-btn");
  const originalText = finishBtn?.textContent;
  if (finishBtn) {
    finishBtn.disabled = true;
    finishBtn.textContent = t("common.saving");
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
      // 背景列表同步刷新以反映刚才的保存
      loadUsersData();
      closeModal("user-modal");
    } else {
      showToast(
        `${userId ? t("user.update_failed") : t("user.add_failed")}：${response.message}`,
        "error"
      );
      handleUser2faBack();
    }
  } catch (error) {
    console.error("保存用户失败:", error);
    showToast(t("user.save_failed_network"), "error");
    handleUser2faBack();
  } finally {
    if (finishBtn) {
      finishBtn.disabled = false;
      finishBtn.textContent = originalText;
    }
  }
}
