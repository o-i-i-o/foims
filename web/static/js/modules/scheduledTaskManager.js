import { apiGet, apiPost, apiPut, apiDelete } from "../utils/apiClient.js";

import {
  showToast,
  escapeHtml,
  formatDateTime,
  createSortState,
  updateSortIcons,
  initSortEvents
} from "../utils/ui.js";

import { t } from "../utils/i18n.js";
import { openModal, closeModal } from "../utils/modalLoader.js";
import { fillSelect } from "../utils/resources.js";
import { iconButton } from "../utils/icons.js";
import { showConfirm } from "../utils/confirm.js";

const taskTableState = createSortState("created_at", "desc");

export async function initScheduledTaskManager() {
  await loadScheduledTasks();
  setupEventListeners();
}

function setupEventListeners() {
  // createBtn 位于常驻 DOM：每次切换到本子标签都会执行本函数，
  // 必须用 dataset 标志做幂等守卫（与下方 taskTable 一致），否则监听器无界累积，
  // 一次点击会触发 N 次 openCreateScheduledTaskModal 并重复提交
  const createBtn = document.getElementById("create-scheduled-task-btn");
  if (createBtn && !createBtn.dataset.handlerAttached) {
    createBtn.dataset.handlerAttached = "true";
    createBtn.addEventListener("click", openCreateScheduledTaskModal);
  }

  const taskTable = document.querySelector("#scheduled-tasks-table");
  if (taskTable && !taskTable.dataset.handlerAttached) {
    taskTable.dataset.handlerAttached = "true";
    initSortEvents("scheduled-tasks-table", taskTableState, (page, sortBy, sortOrder) =>
      loadScheduledTasks(sortBy, sortOrder)
    );
    taskTable.addEventListener("click", (e) => {
      const btn = e.target.closest("button[data-action]");
      if (!btn) {
        return;
      }
      const action = btn.dataset.action;
      const taskId = btn.dataset.taskId;
      const taskName = btn.dataset.taskName;
      switch (action) {
        case "run-task":
          runScheduledTask(taskId);
          break;
        case "toggle-task":
          toggleScheduledTask(taskId, btn);
          break;
        case "edit-task":
          editScheduledTask(taskId);
          break;
        case "view-logs":
          viewTaskLogs(taskName);
          break;
        case "delete-task":
          deleteScheduledTask(taskId);
          break;
      }
    });
  }
}

/* 模态框每次打开均为全新 DOM（closeModal 后销毁），此处随开随绑，无需防重绑 */
function bindTaskModalEvents() {
  document.getElementById("scheduled-task-type")?.addEventListener("change", handleTaskTypeChange);
  document
    .getElementById("scheduled-task-form")
    ?.addEventListener("submit", handleScheduledTaskSubmit);
  // cron 示例链接：在任务模态框之上叠加打开示例表格（modalLoader 按打开顺序自动叠放层级）
  document.getElementById("cron-examples-link")?.addEventListener("click", (e) => {
    e.preventDefault();
    openModal("cron-examples-modal");
  });
  // 任一字段输入即刷新表达式预览，并清除该字段的历史标红
  CRON_FIELD_DEFS.forEach((def) => {
    document.getElementById(def.id)?.addEventListener("input", (e) => {
      e.target.classList.remove("cron-field-invalid");
      updateCronPreview();
    });
  });
}

async function handleTaskTypeChange(e) {
  const taskType = e.target.value;
  const isMacSync = taskType === "mac_sync";

  document.getElementById("mac-sync-config")?.classList.toggle("hidden", !isMacSync);
  document.getElementById("mac-sync-network-config")?.classList.toggle("hidden", !isMacSync);
  document
    .getElementById("log-cleanup-config")
    ?.classList.toggle("hidden", taskType !== "log_cleanup");
  document
    .getElementById("ip-status-sync-config")
    ?.classList.toggle("hidden", taskType !== "ip_status_sync");

  if (isMacSync) {
    // 选项就绪后再返回，编辑场景的调用方依赖此时序回显选中值
    await Promise.all([populateDeviceSelect(), populateNetworkSelect()]);
  }
}

// 下拉选项改由 resources.js 的 fillSelect 按需拉取（10s TTL 共享缓存），
// 不再于页面初始化时整表预载
function populateDeviceSelect() {
  return fillSelect("scheduled-task-device-id", "/api/resources/devices?page_size=1000", {
    placeholderKey: "scheduled_tasks.config_fields.select_device",
    itemToLabel: (dev) => dev.name || dev.hostname || dev.id,
    errorLabelKey: "common.device"
  });
}

function populateNetworkSelect() {
  // 空值选项即“全部子网”：不选具体子网时同步该设备的全部子网
  return fillSelect("scheduled-task-subnet-id", "/api/resources/networks", {
    placeholderKey: "scheduled_tasks.config_fields.all_networks",
    itemToLabel: (net) => net.name || net.id,
    errorLabelKey: "common.subnet"
  });
}

async function loadScheduledTasks(sortBy = null, sortOrder = null) {
  const tbody = document.getElementById("scheduled-tasks-tbody");
  if (!tbody) {
    return;
  }

  if (sortBy) {
    taskTableState.setSort(sortBy, sortOrder);
  }

  try {
    const response = await apiGet(
      `/api/system/scheduled-tasks?sort_by=${taskTableState.sortBy}&sort_order=${taskTableState.sortOrder}`
    );
    if (response.success) {
      renderScheduledTasks(response.data || []);
      updateSortIcons("scheduled-tasks-table", taskTableState);
    } else {
      tbody.innerHTML = `<tr><td colspan="8" class="error-message">${t("scheduled_tasks.load_failed")}</td></tr>`;
    }
  } catch (error) {
    console.error("Failed to load scheduled tasks:", error);
    tbody.innerHTML = `<tr><td colspan="8" class="error-message">${t("scheduled_tasks.load_failed")}</td></tr>`;
  }
}

function renderScheduledTasks(tasks) {
  const tbody = document.getElementById("scheduled-tasks-tbody");
  if (!tbody) {
    return;
  }

  if (tasks.length === 0) {
    tbody.innerHTML = `<tr><td colspan="8" class="no-data">${t("scheduled_tasks.no_tasks")}</td></tr>`;
    return;
  }

  tbody.innerHTML = "";
  tasks.forEach((task, index) => {
    // task_type 为动态翻译键：键缺失时 t() 原样返回键名，此时对原始枚举值转义后再输出
    const typeKey = `scheduled_tasks.task_types.${task.task_type}`;
    const typeText = t(typeKey) === typeKey ? escapeHtml(task.task_type || "") : t(typeKey);
    const row = document.createElement("tr");
    row.innerHTML = `
            <td class="index-column">${index + 1}</td>
            <td>${escapeHtml(task.name)}</td>
            <td class="col-center">${typeText}</td>
            <td><code>${escapeHtml(task.cron_expression)}</code></td>
            <td class="col-center">
                <span class="status-badge ${task.enabled ? "status-active" : "status-inactive"}">
                    ${task.enabled ? t("scheduled_tasks.enabled") : t("scheduled_tasks.disabled")}
                </span>
            </td>
            <td class="col-center">${formatDateTime(task.last_run_at)}</td>
            <td class="col-center">${escapeHtml(task.last_result || "-")}</td>
            <td class="col-center actions">
                ${iconButton({ icon: "play", label: t("scheduled_tasks.run_now"), cls: "btn-success", attrs: `data-action="run-task" data-task-id="${escapeHtml(task.id)}"` })}
                ${iconButton({ icon: "power", label: task.enabled ? t("scheduled_tasks.disable") : t("scheduled_tasks.enable"), cls: "btn-warning", attrs: `data-action="toggle-task" data-task-id="${escapeHtml(task.id)}"` })}
                ${iconButton({ icon: "edit", label: t("common.edit"), cls: "btn-secondary", attrs: `data-action="edit-task" data-task-id="${escapeHtml(task.id)}"` })}
                ${iconButton({ icon: "list", label: t("scheduled_tasks.view_logs"), cls: "btn-secondary", attrs: `data-action="view-logs" data-task-name="${escapeHtml(task.name)}"` })}
                ${iconButton({ icon: "trash", label: t("common.delete"), cls: "btn-danger", attrs: `data-action="delete-task" data-task-id="${escapeHtml(task.id)}"` })}
            </td>
        `;
    tbody.appendChild(row);
  });
}

async function openCreateScheduledTaskModal() {
  const modal = await openModal("scheduled-task-modal");
  if (!modal) {
    return;
  }

  bindTaskModalEvents();
  document.getElementById("scheduled-task-modal-title").textContent = t(
    "scheduled_tasks.create_task"
  );
  document.getElementById("scheduled-task-form").reset();
  document.getElementById("scheduled-task-id").value = "";
  setCronFieldValues(["0", "*/6", "*", "*", "*"]);
  document.getElementById("scheduled-task-enabled").checked = true;
  handleTaskTypeChange({ target: { value: "mac_sync" } });
}

async function editScheduledTask(id) {
  try {
    const response = await apiGet(`/api/system/scheduled-tasks/${id}`);
    if (!response.success) {
      return;
    }
    const task = response.data;

    const modal = await openModal("scheduled-task-modal");
    if (!modal) {
      return;
    }

    bindTaskModalEvents();
    document.getElementById("scheduled-task-modal-title").textContent = t(
      "scheduled_tasks.edit_task"
    );
    document.getElementById("scheduled-task-id").value = task.id;
    document.getElementById("scheduled-task-name").value = task.name;
    document.getElementById("scheduled-task-type").value = task.task_type;
    // 兼容历史 6 段（含秒）表达式：新表单不含秒位，去掉秒段后回填
    const cronFields = (task.cron_expression || "").trim().split(/\s+/);
    if (cronFields.length === 6) {
      cronFields.shift();
    }
    setCronFieldValues(cronFields);
    document.getElementById("scheduled-task-enabled").checked = task.enabled;

    // 等待选项填充完成后再回显选中值（fillSelect 为异步填充）
    await handleTaskTypeChange({ target: { value: task.task_type } });

    if (task.task_type === "mac_sync" && task.config) {
      document.getElementById("scheduled-task-device-id").value = task.config.device_id || "";
      document.getElementById("scheduled-task-subnet-id").value = task.config.subnet_id || "";
    } else if (task.task_type === "log_cleanup" && task.config) {
      document.getElementById("scheduled-task-keep-days").value = task.config.days || 30;
    } else if (task.task_type === "ip_status_sync" && task.config) {
      document.getElementById("scheduled-task-stale-days").value = task.config.days || 30;
    }
  } catch (error) {
    console.error("Failed to load task:", error);
  }
}

// cron 五字段定义：标准 Linux cron 顺序（分 时 日 月 星期），min/max 为取值边界；
// 日字段额外支持 L（当月最后一天，后端解析器与触发库 croner 均支持）
const CRON_FIELD_DEFS = [
  { id: "scheduled-task-cron-minute", min: 0, max: 59 },
  { id: "scheduled-task-cron-hour", min: 0, max: 23 },
  { id: "scheduled-task-cron-day", min: 1, max: 31, allowLastDay: true },
  { id: "scheduled-task-cron-month", min: 1, max: 12 },
  { id: "scheduled-task-cron-weekday", min: 0, max: 7 }
];

// 单段语法：*、数字、区间 a-b，均可带 /步进；逗号列表由调用方拆段逐个校验
const CRON_SEGMENT_PATTERN = /^(\*|\d+|\d+-\d+)(\/\d+)?$/;

// 单字段合法性：逐段校验语法，数值端点须落在该字段边界内，区间还须起点≤终点
function isValidCronFieldValue(value, field) {
  if (!value) {
    return false;
  }
  return value.split(",").every((part) => {
    if (field.allowLastDay && part === "L") {
      return true;
    }
    if (!CRON_SEGMENT_PATTERN.test(part)) {
      return false;
    }
    const [base, step] = part.split("/");
    // 步长必须为正整数：0 步长会被后端拒绝（除零防护）
    if (step !== undefined && Number(step) < 1) {
      return false;
    }
    if (base === "*") {
      return true;
    }
    const bounds = base.split("-").map(Number);
    if (bounds.some((n) => n < field.min || n > field.max)) {
      return false;
    }
    return bounds.length < 2 || bounds[0] <= bounds[1];
  });
}

function getCronExpression() {
  return CRON_FIELD_DEFS.map((def) => document.getElementById(def.id)?.value.trim() ?? "").join(" ");
}

function updateCronPreview() {
  const preview = document.getElementById("scheduled-task-cron-preview");
  if (preview) {
    preview.textContent = getCronExpression();
  }
}

function setCronFieldValues(fields) {
  CRON_FIELD_DEFS.forEach((def, i) => {
    const input = document.getElementById(def.id);
    if (input) {
      // 缺失段回退为通配，避免半截表达式产生空输入
      input.value = fields[i] ?? "*";
    }
  });
  updateCronPreview();
}

// 逐字段校验并标红非法输入（cron-field-invalid）：全部合法时返回拼接后的表达式，否则返回 null
function validateCronFields() {
  let allValid = true;
  const parts = [];
  CRON_FIELD_DEFS.forEach((def) => {
    const input = document.getElementById(def.id);
    const value = input ? input.value.trim() : "";
    const valid = isValidCronFieldValue(value, def);
    input?.classList.toggle("cron-field-invalid", !valid);
    if (!valid) {
      allValid = false;
    }
    parts.push(value);
  });
  return allValid ? parts.join(" ") : null;
}

// 提交在途标志：请求未返回前拦截重复提交（双击/回车），防止重复创建同名任务
let submitInFlight = false;

async function handleScheduledTaskSubmit(e) {
  e.preventDefault();
  if (submitInFlight) {
    return;
  }
  submitInFlight = true;
  try {
    await saveScheduledTask();
  } finally {
    submitInFlight = false;
  }
}

async function saveScheduledTask() {
  const id = document.getElementById("scheduled-task-id").value;
  const name = document.getElementById("scheduled-task-name").value.trim();
  const taskType = document.getElementById("scheduled-task-type").value;
  const enabled = document.getElementById("scheduled-task-enabled").checked;

  if (!name || !taskType) {
    showToast(t("scheduled_tasks.required_fields"), "warning");
    return;
  }

  // cron 逐字段校验，非法时提前拦截：后端对创建仅告警落库，任务会静默永不执行
  const cronExpression = validateCronFields();
  if (!cronExpression) {
    showToast(t("scheduled_tasks.cron_invalid"), "warning");
    return;
  }

  let config = {};

  if (taskType === "mac_sync") {
    const deviceId = document.getElementById("scheduled-task-device-id").value;
    // 子网可缺省：空值表示全部子网，仅设备必选
    const subnetId = document.getElementById("scheduled-task-subnet-id").value;
    if (!deviceId) {
      showToast(t("scheduled_tasks.required_fields"), "warning");
      return;
    }
    config = subnetId ? { device_id: deviceId, subnet_id: subnetId } : { device_id: deviceId };
  } else if (taskType === "log_cleanup") {
    const keepDays = parseInt(document.getElementById("scheduled-task-keep-days").value, 10) || 30;
    config = { days: keepDays };
  } else if (taskType === "ip_status_sync") {
    // 判停阈值下限为 1 天：0 会在下一轮把全部地址判停
    const staleDays =
      parseInt(document.getElementById("scheduled-task-stale-days").value, 10) || 30;
    if (staleDays < 1) {
      showToast(t("scheduled_tasks.config_fields.stale_days_invalid"), "warning");
      return;
    }
    config = { days: staleDays };
  }

  const data = {
    name,
    task_type: taskType,
    cron_expression: cronExpression,
    enabled,
    config
  };

  try {
    let response;
    if (id) {
      response = await apiPut(`/api/system/scheduled-tasks/${id}`, data);
    } else {
      response = await apiPost("/api/system/scheduled-tasks", data);
    }

    if (response.success) {
      showToast(
        id ? t("scheduled_tasks.update_success") : t("scheduled_tasks.create_success"),
        "success"
      );
      closeModal("scheduled-task-modal");
      await loadScheduledTasks();
    } else {
      showToast(`${t("scheduled_tasks.save_failed")}: ${response.message || ""}`, "error");
    }
  } catch (error) {
    console.error("Failed to save task:", error);
    showToast(t("scheduled_tasks.save_failed"), "error");
  }
}

async function toggleScheduledTask(id, btn = null) {
  // 处理期间禁用触发按钮防抖：连点会在请求返回前再次切换，最终状态被切回原样
  if (btn) {
    btn.disabled = true;
  }
  try {
    const response = await apiPost(`/api/system/scheduled-tasks/${id}/toggle`);
    if (response.success) {
      showToast(t("scheduled_tasks.toggle_success"), "success");
      await loadScheduledTasks();
    } else {
      showToast(t("scheduled_tasks.toggle_failed"), "error");
    }
  } catch (error) {
    console.error("Failed to toggle task:", error);
    showToast(t("scheduled_tasks.toggle_failed"), "error");
  } finally {
    // 列表重载后按钮已是新 DOM，此处恢复仅覆盖请求失败等未重载的场景
    if (btn) {
      btn.disabled = false;
    }
  }
}

async function runScheduledTask(id) {
  const confirmed = await showConfirm(t("scheduled_tasks.confirm_run"));
  if (!confirmed) {
    return;
  }

  try {
    const response = await apiPost(`/api/system/scheduled-tasks/${id}/run`);
    if (response.success) {
      const result = response.data?.result;
      if (result && result.ok) {
        showToast(`${t("scheduled_tasks.run_success")}: ${result.ok}`, "success");
      } else if (result && result.err) {
        showToast(`${t("scheduled_tasks.run_failed")}: ${result.err}`, "error");
      } else {
        showToast(t("scheduled_tasks.run_success"), "success");
      }
      await loadScheduledTasks();
    } else {
      showToast(`${t("scheduled_tasks.run_failed")}: ${response.message || ""}`, "error");
    }
  } catch (error) {
    console.error("Failed to run task:", error);
    showToast(t("scheduled_tasks.run_failed"), "error");
  }
}

async function deleteScheduledTask(id) {
  const confirmed = await showConfirm(t("scheduled_tasks.confirm_delete"));
  if (!confirmed) {
    return;
  }

  try {
    const response = await apiDelete(`/api/system/scheduled-tasks/${id}`);
    if (response.success) {
      showToast(t("scheduled_tasks.delete_success"), "success");
      await loadScheduledTasks();
    } else {
      showToast(`${t("scheduled_tasks.delete_failed")}: ${response.message || ""}`, "error");
    }
  } catch (error) {
    console.error("Failed to delete task:", error);
    showToast(t("scheduled_tasks.delete_failed"), "error");
  }
}

async function viewTaskLogs(taskName) {
  const modal = await openModal("task-logs-modal");
  if (!modal) {
    return;
  }

  const tbody = document.getElementById("task-logs-tbody");
  if (!tbody) {
    return;
  }

  try {
    const response = await apiGet(
      `/api/system/scheduled-tasks/logs?task_name=${encodeURIComponent(taskName)}&page=1&page_size=50`
    );
    if (!response.success) {
      showToast(`${t("common.load_failed")}: ${response.message || ""}`, "error");
      return;
    }

    // 后端为固定五键分页响应（items/total/page/page_size/total_pages）
    const logs = response.data?.items || [];
    if (logs.length === 0) {
      tbody.innerHTML = `<tr><td colspan="5" class="no-data">${t("common.no_data")}</td></tr>`;
      return;
    }

    const fragment = document.createDocumentFragment();
    for (const log of logs) {
      const statusClass = log.status === "success" ? "status-active" : "status-inactive";
      const row = document.createElement("tr");
      row.innerHTML = `
        <td>${escapeHtml(log.task_name)}</td>
        <td><span class="status-badge ${statusClass}">${escapeHtml(log.status)}</span></td>
        <td>${formatDateTime(log.start_time)}</td>
        <td>${escapeHtml(String(log.duration ?? 0))}</td>
        <td>${escapeHtml(log.details?.message || log.details?.error || "-")}</td>
      `;
      fragment.appendChild(row);
    }
    tbody.replaceChildren(fragment);
  } catch (error) {
    console.error("Failed to load task logs:", error);
  }
}
