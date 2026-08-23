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
import { iconButton } from "../utils/icons.js";
import { elementCache } from "../utils/helpers.js";
import { showConfirm } from "../utils/confirm.js";

let devices = [];
let networks = [];

const taskTableState = createSortState("created_at", "desc");

export async function initScheduledTaskManager() {
  await loadDevices();
  await loadNetworks();
  await loadScheduledTasks();
  setupEventListeners();
}

async function loadDevices() {
  try {
    const response = await apiGet("/api/resources/devices?page_size=1000");
    if (response.success) {
      devices = response.data?.items || response.data || [];
    }
  } catch (error) {
    console.error("Failed to load devices:", error);
  }
}

async function loadNetworks() {
  try {
    const response = await apiGet("/api/resources/networks");
    if (response.success) {
      networks = response.data || [];
    }
  } catch (error) {
    console.error("Failed to load networks:", error);
  }
}

function setupEventListeners() {
  const createBtn = document.getElementById("create-scheduled-task-btn");
  if (createBtn) {
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
      if (!btn) return;
      const action = btn.dataset.action;
      const taskId = btn.dataset.taskId;
      const taskName = btn.dataset.taskName;
      switch (action) {
        case "run-task":
          window.runScheduledTask(taskId);
          break;
        case "toggle-task":
          window.toggleScheduledTask(taskId);
          break;
        case "edit-task":
          window.editScheduledTask(taskId);
          break;
        case "view-logs":
          window.viewTaskLogs(taskName);
          break;
        case "delete-task":
          window.deleteScheduledTask(taskId);
          break;
      }
    });
  }
}

/* 模态框每次打开均为全新 DOM（closeModal 后销毁），此处随开随绑，无需防重绑 */
function bindTaskModalEvents() {
  document.getElementById("scheduled-task-type")?.addEventListener("change", handleTaskTypeChange);
  document.getElementById("scheduled-task-form")?.addEventListener("submit", handleScheduledTaskSubmit);
}

function handleTaskTypeChange(e) {
  const taskType = e.target.value;
  const isMacSync = taskType === "mac_sync";

  document.getElementById("mac-sync-config")?.classList.toggle("hidden", !isMacSync);
  document
    .getElementById("mac-sync-network-config")
    ?.classList.toggle("hidden", !isMacSync);
  document
    .getElementById("log-cleanup-config")
    ?.classList.toggle("hidden", taskType !== "log_cleanup");

  if (isMacSync) {
    populateDeviceSelect();
    populateNetworkSelect();
  }
}

function populateDeviceSelect() {
  const select = document.getElementById("scheduled-task-device-id");
  if (!select) return;

  select.innerHTML = `<option value="">${t("scheduled_tasks.config_fields.select_device")}</option>`;
  devices.forEach((dev) => {
    const option = document.createElement("option");
    option.value = dev.id;
    option.textContent = dev.name || dev.hostname || dev.id;
    select.appendChild(option);
  });
}

function populateNetworkSelect() {
  const select = document.getElementById("scheduled-task-network-id");
  if (!select) return;

  select.innerHTML = `<option value="">${t("scheduled_tasks.config_fields.select_network")}</option>`;
  networks.forEach((net) => {
    const option = document.createElement("option");
    option.value = net.id;
    option.textContent = net.name || net.id;
    select.appendChild(option);
  });
}

async function loadScheduledTasks(sortBy = null, sortOrder = null) {
  const tbody = document.getElementById("scheduled-tasks-tbody");
  if (!tbody) return;

  if (sortBy) taskTableState.setSort(sortBy, sortOrder);

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
  if (!tbody) return;

  if (tasks.length === 0) {
    tbody.innerHTML = `<tr><td colspan="8" class="no-data">${t("scheduled_tasks.no_tasks")}</td></tr>`;
    return;
  }

  tbody.innerHTML = "";
  tasks.forEach((task, index) => {
    const row = document.createElement("tr");
    row.innerHTML = `
            <td class="index-column">${index + 1}</td>
            <td>${escapeHtml(task.name)}</td>
            <td class="col-center">${t("scheduled_tasks.task_types." + task.task_type) || task.task_type}</td>
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

window.openCreateScheduledTaskModal = async function () {
  const modal = await openModal("scheduled-task-modal");
  if (!modal) return;

  bindTaskModalEvents();
  document.getElementById("scheduled-task-modal-title").textContent = t("scheduled_tasks.create_task");
  document.getElementById("scheduled-task-form").reset();
  document.getElementById("scheduled-task-id").value = "";
  document.getElementById("scheduled-task-cron").value = "0 */6 * * *";
  document.getElementById("scheduled-task-enabled").checked = true;
  handleTaskTypeChange({ target: { value: "mac_sync" } });
};

window.editScheduledTask = async function (id) {
  try {
    const response = await apiGet(`/api/system/scheduled-tasks/${id}`);
    if (!response.success) return;
    const task = response.data;

    const modal = await openModal("scheduled-task-modal");
    if (!modal) return;

    bindTaskModalEvents();
    document.getElementById("scheduled-task-modal-title").textContent = t("scheduled_tasks.edit_task");
    document.getElementById("scheduled-task-id").value = task.id;
    document.getElementById("scheduled-task-name").value = task.name;
    document.getElementById("scheduled-task-type").value = task.task_type;
    document.getElementById("scheduled-task-cron").value = task.cron_expression;
    document.getElementById("scheduled-task-enabled").checked = task.enabled;

    handleTaskTypeChange({ target: { value: task.task_type } });

    if (task.task_type === "mac_sync" && task.config) {
      document.getElementById("scheduled-task-device-id").value = task.config.device_id || "";
      document.getElementById("scheduled-task-network-id").value = task.config.network_id || "";
    } else if (task.task_type === "log_cleanup" && task.config) {
      document.getElementById("scheduled-task-keep-days").value = task.config.days || 30;
    }
  } catch (error) {
    console.error("Failed to load task:", error);
  }
};

async function handleScheduledTaskSubmit(e) {
  e.preventDefault();

  const id = document.getElementById("scheduled-task-id").value;
  const name = document.getElementById("scheduled-task-name").value.trim();
  const taskType = document.getElementById("scheduled-task-type").value;
  const cronExpression = document.getElementById("scheduled-task-cron").value.trim();
  const enabled = document.getElementById("scheduled-task-enabled").checked;

  if (!name || !taskType || !cronExpression) {
    showToast(t("scheduled_tasks.required_fields"), "warning");
    return;
  }

  let config = {};

  if (taskType === "mac_sync") {
    const deviceId = document.getElementById("scheduled-task-device-id").value;
    const networkId = document.getElementById("scheduled-task-network-id").value;
    if (!deviceId || !networkId) {
      showToast(t("scheduled_tasks.required_fields"), "warning");
      return;
    }
    config = { device_id: deviceId, network_id: networkId };
  } else if (taskType === "log_cleanup") {
    const keepDays = parseInt(document.getElementById("scheduled-task-keep-days").value, 10) || 30;
    config = { days: keepDays };
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
      showToast(t("scheduled_tasks.save_failed") + ": " + (response.message || ""), "error");
    }
  } catch (error) {
    console.error("Failed to save task:", error);
    showToast(t("scheduled_tasks.save_failed"), "error");
  }
}

window.toggleScheduledTask = async function (id) {
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
  }
};

window.runScheduledTask = async function (id) {
  const confirmed = await showConfirm(t("scheduled_tasks.confirm_run"));
  if (!confirmed) return;

  try {
    const response = await apiPost(`/api/system/scheduled-tasks/${id}/run`);
    if (response.success) {
      const result = response.data?.result;
      if (result && result.ok) {
        showToast(t("scheduled_tasks.run_success") + ": " + result.ok, "success");
      } else if (result && result.err) {
        showToast(t("scheduled_tasks.run_failed") + ": " + result.err, "error");
      } else {
        showToast(t("scheduled_tasks.run_success"), "success");
      }
      await loadScheduledTasks();
    } else {
      showToast(t("scheduled_tasks.run_failed") + ": " + (response.message || ""), "error");
    }
  } catch (error) {
    console.error("Failed to run task:", error);
    showToast(t("scheduled_tasks.run_failed"), "error");
  }
};

window.deleteScheduledTask = async function (id) {
  const confirmed = await showConfirm(t("scheduled_tasks.confirm_delete"));
  if (!confirmed) return;

  try {
    const response = await apiDelete(`/api/system/scheduled-tasks/${id}`);
    if (response.success) {
      showToast(t("scheduled_tasks.delete_success"), "success");
      await loadScheduledTasks();
    }
  } catch (error) {
    console.error("Failed to delete task:", error);
  }
};

window.viewTaskLogs = async function (taskName) {
  const modal = await openModal("task-logs-modal");
  if (!modal) return;

  const tbody = document.getElementById("task-logs-tbody");
  if (!tbody) return;

  try {
    const response = await apiGet(
      `/api/system/scheduled-tasks/logs?task_name=${encodeURIComponent(taskName)}&limit=50`
    );
    if (!response.success) return;

    const logs = response.data || [];
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
        <td><span class="status-badge ${statusClass}">${log.status}</span></td>
        <td>${formatDateTime(log.start_time)}</td>
        <td>${log.duration || 0}</td>
        <td>${escapeHtml(log.details?.message || log.details?.error || "-")}</td>
      `;
      fragment.appendChild(row);
    }
    tbody.replaceChildren(fragment);
  } catch (error) {
    console.error("Failed to load task logs:", error);
  }
};
