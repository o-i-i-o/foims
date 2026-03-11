import {
  apiRequest,
  apiGet,
  apiPost,
  apiPut,
  apiDelete,
} from "../utils/apiClient.js";
import { showToast, setLoading } from "../utils/ui.js";
import { t } from "../utils/i18n.js";

export async function loadScheduledTasks() {
  const tbody = document.getElementById("scheduled-tasks-tbody");
  if (!tbody) return;

  tbody.innerHTML = '<tr><td colspan="7" class="loading-cell">Loading...</td></tr>';

  try {
    const result = await apiGet("/api/system/scheduled-tasks");
    if (result.success) {
      renderScheduledTasksTable(result.data);
    } else {
      tbody.innerHTML = `<tr><td colspan="7" class="error-cell">${result.message || t("scheduled_tasks.load_failed")}</td></tr>`;
    }
  } catch (err) {
    console.error("loadScheduledTasks error:", err);
    tbody.innerHTML = `<tr><td colspan="7" class="error-cell">${t("scheduled_tasks.load_failed")}</td></tr>`;
  }
}

function renderScheduledTasksTable(tasks) {
  const tbody = document.getElementById("scheduled-tasks-tbody");
  if (!tbody) return;

  if (!tasks || tasks.length === 0) {
    tbody.innerHTML = `<tr><td colspan="7" class="empty-cell">${t("scheduled_tasks.no_tasks")}</td></tr>`;
    return;
  }

  tbody.innerHTML = tasks.map(task => {
    const statusBadge = task.enabled 
      ? `<span class="status-badge status-active">${t("scheduled_tasks.enabled")}</span>`
      : `<span class="status-badge status-inactive">${t("scheduled_tasks.disabled")}</span>`;
    
    const lastRun = task.last_run_at 
      ? formatDateTime(task.last_run_at)
      : "-";
    
    const lastResult = task.last_result
      ? `<span class="result-success">${task.last_result}</span>`
      : "-";

    return `
      <tr data-task-id="${task.id}">
        <td>${escapeHtml(task.name)}</td>
        <td>${escapeHtml(task.task_type)}</td>
        <td><code>${escapeHtml(task.cron_expression)}</code></td>
        <td>${statusBadge}</td>
        <td>${lastRun}</td>
        <td>${lastResult}</td>
        <td class="actions-cell">
          <button class="btn btn-sm btn-primary" onclick="runScheduledTask('${task.id}')" title="${t("scheduled_tasks.run_now")}">
            ▶️
          </button>
          <button class="btn btn-sm btn-secondary" onclick="toggleScheduledTask('${task.id}')" title="${task.enabled ? t("scheduled_tasks.disable") : t("scheduled_tasks.enable")}">
            ${task.enabled ? "⏸" : "▶️"}
          </button>
          <button class="btn btn-sm btn-warning" onclick="editScheduledTask('${task.id}')" title="${t("common.edit")}">
            ✏️
          </button>
          <button class="btn btn-sm btn-danger" onclick="deleteScheduledTask('${task.id}')" title="${t("common.delete")}">
            🗑️
          </button>
        </td>
      </tr>
    `;
  }).join("");
}

function escapeHtml(text) {
  const div = document.createElement("div");
  div.textContent = text;
  return div.innerHTML;
}

export async function showCreateScheduledTaskModal() {
  const modal = document.getElementById("scheduled-task-modal");
  if (!modal) return;

  document.getElementById("scheduled-task-id").value = "";
  document.getElementById("scheduled-task-name").value = "";
  document.getElementById("scheduled-task-type").value = "mac_sync";
  document.getElementById("scheduled-task-cron").value = "0 */6 * *";
  document.getElementById("scheduled-task-enabled").checked = true;
  document.getElementById("scheduled-task-config").value = "{}";

  modal.classList.add("active");
}

export async function showEditScheduledTaskModal(taskId) {
  try {
    const result = await apiGet(`/api/system/scheduled-tasks/${taskId}`);
    if (result.success && result.data) {
      const task = result.data;
      const modal = document.getElementById("scheduled-task-modal");
      
      document.getElementById("scheduled-task-id").value = task.id;
      document.getElementById("scheduled-task-name").value = task.name;
      document.getElementById("scheduled-task-type").value = task.task_type;
      document.getElementById("scheduled-task-cron").value = task.cron_expression;
      document.getElementById("scheduled-task-enabled").checked = task.enabled;
      document.getElementById("scheduled-task-config").value = JSON.stringify(task.config || {}, null, 2);

      modal.classList.add("active");
    } else {
      showToast(result.message || t("scheduled_tasks.load_failed"), "error");
    }
  } catch (err) {
    console.error("showEditScheduledTaskModal error:", err);
    showToast(t("scheduled_tasks.load_failed"), "error");
  }
}

export async function saveScheduledTask() {
  const taskId = document.getElementById("scheduled-task-id").value;
  const name = document.getElementById("scheduled-task-name").value.trim();
  const taskType = document.getElementById("scheduled-task-type").value;
  const cronExpression = document.getElementById("scheduled-task-cron").value.trim();
  const enabled = document.getElementById("scheduled-task-enabled").checked;
  const configValue = document.getElementById("scheduled-task-config").value;
  
  let config;
  try {
    config = configValue ? JSON.parse(configValue) : {};
  } catch (e) {
    showToast(t("scheduled_tasks.invalid_config"), "error");
    return;
  }

  if (!name || !taskType || !cronExpression) {
    showToast(t("scheduled_tasks.required_fields"), "error");
    return;
  }

  const data = {
    name,
    task_type: taskType,
    cron_expression: cronExpression,
    enabled,
    config,
  };

  try {
    setLoading(true);
    let result;
    if (taskId) {
      result = await apiPut(`/api/system/scheduled-tasks/${taskId}`, data);
    } else {
      result = await apiPost("/api/system/scheduled-tasks", data);
    }

    if (result.success) {
      showToast(taskId ? t("scheduled_tasks.update_success") : t("scheduled_tasks.create_success"), "success");
      closeScheduledTaskModal();
      loadScheduledTasks();
    } else {
      showToast(result.message || t("scheduled_tasks.save_failed"), "error");
    }
  } catch (err) {
    console.error("saveScheduledTask error:", err);
    showToast(t("scheduled_tasks.save_failed"), "error");
  } finally {
    setLoading(false);
  }
}

export async function runScheduledTask(taskId) {
  if (!confirm(t("scheduled_tasks.confirm_run"))) return;

  try {
    setLoading(true);
    const result = await apiPost(`/api/system/scheduled-tasks/${taskId}/run`, {});
    
    if (result.success) {
      showToast(t("scheduled_tasks.run_success"), "success");
      loadScheduledTasks();
    } else {
      showToast(result.message || t("scheduled_tasks.run_failed"), "error");
    }
  } catch (err) {
    console.error("runScheduledTask error:", err);
    showToast(t("scheduled_tasks.run_failed"), "error");
  } finally {
    setLoading(false);
  }
}

export async function toggleScheduledTask(taskId) {
  try {
    const result = await apiPost(`/api/system/scheduled-tasks/${taskId}/toggle`, {});
    
    if (result.success) {
      showToast(t("scheduled_tasks.toggle_success"), "success");
      loadScheduledTasks();
    } else {
      showToast(result.message || t("scheduled_tasks.toggle_failed"), "error");
    }
  } catch (err) {
    console.error("toggleScheduledTask error:", err);
    showToast(t("scheduled_tasks.toggle_failed"), "error");
  }
}

export async function deleteScheduledTask(taskId) {
  if (!confirm(t("scheduled_tasks.confirm_delete"))) return;

  try {
    setLoading(true);
    const result = await apiDelete(`/api/system/scheduled-tasks/${taskId}`);
    
    if (result.success) {
      showToast(t("scheduled_tasks.delete_success"), "success");
      loadScheduledTasks();
    } else {
      showToast(result.message || t("scheduled_tasks.delete_failed"), "error");
    }
  } catch (err) {
    console.error("deleteScheduledTask error:", err);
    showToast(t("scheduled_tasks.delete_failed"), "error");
  } finally {
    setLoading(false);
  }
}

export function closeScheduledTaskModal() {
  const modal = document.getElementById("scheduled-task-modal");
  if (modal) {
    modal.classList.remove("active");
  }
}

export function initScheduledTasks() {
  const createBtn = document.getElementById("create-scheduled-task-btn");
  if (createBtn) {
    createBtn.addEventListener("click", showCreateScheduledTaskModal);
  }

  const modal = document.getElementById("scheduled-task-modal");
  if (modal) {
    const form = document.getElementById("scheduled-task-form");
    const cancelBtn = document.getElementById("scheduled-task-cancel-btn");
    const configTextarea = document.getElementById("scheduled-task-config");

    form.addEventListener("submit", (e) => {
      e.preventDefault();
      saveScheduledTask();
    });

    cancelBtn.addEventListener("click", closeScheduledTaskModal);

    const typeSelect = document.getElementById("scheduled-task-type");
    if (typeSelect) {
      typeSelect.addEventListener("change", (e) => {
        const taskType = e.target.value;
        let defaultConfig = {};
        
        switch (taskType) {
          case "mac_sync":
            defaultConfig = {
              switch_id: "",
              network_id: "",
            };
            break;
          case "token_cleanup":
            defaultConfig = {};
            break;
          case "backup":
            defaultConfig = {};
            break;
        }
        
        configTextarea.value = JSON.stringify(defaultConfig, null, 2);
      });
    }
  }
}
