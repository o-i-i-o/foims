// Scheduled task manager
import { apiGet, apiPost, apiDelete } from "../utils/apiClient.js";
import { showToast, escapeHtml } from "../utils/ui.js";
export async function initScheduledTaskManager() {
    await loadScheduledTasks();
    initEventListeners();
}
async function loadScheduledTasks() {
    try {
        const result = await apiGet("/api/scheduled-tasks");
        const tbody = document.querySelector("#scheduled-tasks-table tbody");
        if (!tbody)
            return;
        tbody.innerHTML = "";
        const tasks = result.data?.items || [];
        if (tasks.length === 0) {
            tbody.innerHTML = '<tr class="empty-row"><td colspan="7" class="text-center">暂无定时任务</td></tr>';
            return;
        }
        tasks.forEach((task, index) => {
            const row = document.createElement("tr");
            row.innerHTML = `
        <td class="index-column">${index + 1}</td>
        <td>${escapeHtml(task.name)}</td>
        <td>${escapeHtml(task.description || "-")}</td>
        <td>${escapeHtml(task.task_type)}</td>
        <td>${escapeHtml(task.schedule)}</td>
        <td><span class="status-badge ${task.enabled ? "status-active" : "status-inactive"}">${task.enabled ? "启用" : "禁用"}</span></td>
        <td>
          <button class="btn btn-sm btn-secondary" data-id="${task.id}" data-action="run">运行</button>
          <button class="btn btn-sm btn-edit" data-id="${task.id}">编辑</button>
          <button class="btn btn-sm btn-delete" data-id="${task.id}">删除</button>
        </td>
      `;
            tbody.appendChild(row);
        });
    }
    catch (error) {
        console.error("加载定时任务失败:", error);
    }
}
function initEventListeners() {
    const container = document.getElementById("scheduled-tasks");
    if (!container)
        return;
    container.addEventListener("click", async (e) => {
        const target = e.target;
        const id = target.getAttribute("data-id");
        if (!id)
            return;
        if (target.classList.contains("btn-edit")) {
            await editScheduledTask(id);
        }
        else if (target.classList.contains("btn-delete")) {
            await deleteScheduledTask(id);
        }
        else if (target.getAttribute("data-action") === "run") {
            await runScheduledTask(id);
        }
    });
}
async function editScheduledTask(id) {
    // TODO: Implement edit functionality
    console.log("Edit scheduled task:", id);
}
async function deleteScheduledTask(id) {
    if (!confirm("确定要删除此定时任务吗？"))
        return;
    try {
        const result = await apiDelete(`/api/scheduled-tasks/${id}`);
        if (result.success) {
            showToast("定时任务删除成功", "success");
            await loadScheduledTasks();
        }
        else {
            showToast("删除失败: " + result.message, "error");
        }
    }
    catch (error) {
        console.error("删除定时任务失败:", error);
        showToast("删除失败", "error");
    }
}
async function runScheduledTask(id) {
    try {
        const result = await apiPost(`/api/scheduled-tasks/${id}/run`, {});
        if (result.success) {
            showToast("任务已启动", "success");
        }
        else {
            showToast("启动失败: " + result.message, "error");
        }
    }
    catch (error) {
        console.error("启动任务失败:", error);
        showToast("启动失败", "error");
    }
}
export async function createScheduledTask(taskData) {
    try {
        const result = await apiPost("/api/scheduled-tasks", taskData);
        if (result.success) {
            showToast("定时任务创建成功", "success");
            await loadScheduledTasks();
            return true;
        }
        else {
            showToast("创建失败: " + result.message, "error");
            return false;
        }
    }
    catch (error) {
        console.error("创建定时任务失败:", error);
        showToast("创建失败", "error");
        return false;
    }
}
//# sourceMappingURL=scheduledTaskManager.js.map