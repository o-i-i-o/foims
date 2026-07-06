import {
  apiGet,
  apiPost,
  apiPut,
  apiDelete,
} from "../utils/apiClient.js";

import {
  showToast,
  escapeHtml,
  formatDateTime,
} from "../utils/ui.js";

import { t } from "../utils/i18n.js";
import { openModal, closeModal } from "../utils/modal.js";
import { elementCache } from "../utils/helpers.js";
import { showConfirm } from "../utils/confirm.js";

let devices = [];
let networks = [];

export async function initScheduledTaskManager() {
    await loadDevices();
    await loadNetworks();
    await loadScheduledTasks();
    setupEventListeners();
}

async function loadDevices() {
    try {
        const response = await apiGet('/api/resources/devices?page_size=1000');
        if (response.success) {
            devices = response.data?.items || response.data || [];
        }
    } catch (error) {
        console.error('Failed to load devices:', error);
    }
}

async function loadNetworks() {
    try {
        const response = await apiGet('/api/resources/networks');
        if (response.success) {
            networks = response.data || [];
        }
    } catch (error) {
        console.error('Failed to load networks:', error);
    }
}

function setupEventListeners() {
    const createBtn = document.getElementById('create-scheduled-task-btn');
    if (createBtn) {
        createBtn.addEventListener('click', openCreateScheduledTaskModal);
    }

    const taskTypeSelect = document.getElementById('scheduled-task-type');
    if (taskTypeSelect) {
        taskTypeSelect.addEventListener('change', handleTaskTypeChange);
    }

    const form = document.getElementById('scheduled-task-form');
    if (form) {
        form.addEventListener('submit', handleScheduledTaskSubmit);
    }

    const cancelBtn = document.getElementById('scheduled-task-cancel-btn');
    if (cancelBtn) {
        cancelBtn.addEventListener('click', closeScheduledTaskModal);
    }
}

function handleTaskTypeChange(e) {
    const taskType = e.target.value;
    const macSyncConfig = document.getElementById('mac-sync-config');
    const logCleanupConfig = document.getElementById('log-cleanup-config');

    if (macSyncConfig) macSyncConfig.style.display = 'none';
    if (logCleanupConfig) logCleanupConfig.style.display = 'none';

    if (taskType === 'mac_sync') {
        if (macSyncConfig) {
            macSyncConfig.style.display = 'block';
            populateDeviceSelect();
            populateNetworkSelect();
        }
    } else if (taskType === 'log_cleanup') {
        if (logCleanupConfig) {
            logCleanupConfig.style.display = 'block';
        }
    }
}

function populateDeviceSelect() {
    const select = document.getElementById('scheduled-task-device-id');
    if (!select) return;

    const currentLang = localStorage.getItem('language') || 'zh';
    const selectText = currentLang === 'zh' ? '选择设备' : 'Select Device';

    select.innerHTML = `<option value="">${selectText}</option>`;
    devices.forEach(dev => {
        const option = document.createElement('option');
        option.value = dev.id;
        option.textContent = dev.name || dev.hostname || dev.id;
        select.appendChild(option);
    });
}

function populateNetworkSelect() {
    const select = document.getElementById('scheduled-task-network-id');
    if (!select) return;

    const currentLang = localStorage.getItem('language') || 'zh';
    const selectText = currentLang === 'zh' ? '选择网络' : 'Select Network';

    select.innerHTML = `<option value="">${selectText}</option>`;
    networks.forEach(net => {
        const option = document.createElement('option');
        option.value = net.id;
        option.textContent = net.name || net.id;
        select.appendChild(option);
    });
}

async function loadScheduledTasks() {
    const tbody = document.getElementById('scheduled-tasks-tbody');
    if (!tbody) return;

    try {
        const response = await apiGet('/api/system/scheduled-tasks');
        if (response.success) {
            renderScheduledTasks(response.data || []);
        } else {
            tbody.innerHTML = `<tr><td colspan="7" class="error-message">${t('scheduled_tasks.load_failed')}</td></tr>`;
        }
    } catch (error) {
        console.error('Failed to load scheduled tasks:', error);
        tbody.innerHTML = `<tr><td colspan="7" class="error-message">${t('scheduled_tasks.load_failed')}</td></tr>`;
    }
}

function renderScheduledTasks(tasks) {
    const tbody = document.getElementById('scheduled-tasks-tbody');
    if (!tbody) return;

    if (tasks.length === 0) {
        tbody.innerHTML = `<tr><td colspan="7" class="no-data">${t('scheduled_tasks.no_tasks')}</td></tr>`;
        return;
    }

    tbody.innerHTML = '';
    tasks.forEach(task => {
        const row = document.createElement('tr');
        row.innerHTML = `
            <td>${escapeHtml(task.name)}</td>
            <td>${t('scheduled_tasks.task_types.' + task.task_type) || task.task_type}</td>
            <td><code>${escapeHtml(task.cron_expression)}</code></td>
            <td>
                <span class="status-badge ${task.enabled ? 'status-active' : 'status-inactive'}">
                    ${task.enabled ? t('scheduled_tasks.enabled') : t('scheduled_tasks.disabled')}
                </span>
            </td>
            <td>${formatDateTime(task.last_run_at)}</td>
            <td>${escapeHtml(task.last_result || '-')}</td>
            <td class="actions">
                <button class="btn btn-secondary btn-sm" onclick="runScheduledTask('${task.id}')">${t('scheduled_tasks.run_now')}</button>
                <button class="btn btn-secondary btn-sm" onclick="toggleScheduledTask('${task.id}')">${task.enabled ? t('scheduled_tasks.disable') : t('scheduled_tasks.enable')}</button>
                <button class="btn btn-secondary btn-sm" onclick="editScheduledTask('${task.id}')">${t('common.edit')}</button>
                <button class="btn btn-secondary btn-sm" onclick="viewTaskLogs('${task.name}')">${t('scheduled_tasks.view_logs')}</button>
                <button class="btn btn-danger btn-sm" onclick="deleteScheduledTask('${task.id}')">${t('common.delete')}</button>
            </td>
        `;
        tbody.appendChild(row);
    });
}

window.openCreateScheduledTaskModal = function() {
    const template = document.getElementById('scheduled-task-modal-template');
    if (!template) return;

    const modalContainer = document.getElementById('modal-container');
    if (modalContainer) {
        modalContainer.innerHTML = template.innerHTML;
        setupEventListeners();
        document.getElementById('scheduled-task-modal-title').textContent = t('scheduled_tasks.create_task');
        document.getElementById('scheduled-task-id').value = '';
        document.getElementById('scheduled-task-form').reset();
        document.getElementById('scheduled-task-enabled').checked = true;
        handleTaskTypeChange({ target: { value: 'mac_sync' } });
        modalContainer.style.display = 'flex';
    }
};

window.editScheduledTask = async function(id) {
    try {
        const response = await apiGet(`/api/system/scheduled-tasks/${id}`);
        if (response.success) {
            const task = response.data;
            const template = document.getElementById('scheduled-task-modal-template');
            if (!template) return;

            const modalContainer = document.getElementById('modal-container');
            if (modalContainer) {
                modalContainer.innerHTML = template.innerHTML;
                setupEventListeners();
                document.getElementById('scheduled-task-modal-title').textContent = t('scheduled_tasks.edit_task');
                document.getElementById('scheduled-task-id').value = task.id;
                document.getElementById('scheduled-task-name').value = task.name;
                document.getElementById('scheduled-task-type').value = task.task_type;
                document.getElementById('scheduled-task-cron').value = task.cron_expression;
                document.getElementById('scheduled-task-enabled').checked = task.enabled;

                handleTaskTypeChange({ target: { value: task.task_type } });

                if (task.task_type === 'mac_sync' && task.config) {
                    setTimeout(() => {
                        document.getElementById('scheduled-task-device-id').value = task.config.device_id || '';
                        document.getElementById('scheduled-task-network-id').value = task.config.network_id || '';
                    }, 100);
                } else if (task.task_type === 'log_cleanup' && task.config) {
                    document.getElementById('scheduled-task-keep-days').value = task.config.days || 30;
                }

                modalContainer.style.display = 'flex';
            }
        }
    } catch (error) {
        console.error('Failed to load task:', error);
    }
};

window.closeScheduledTaskModal = function() {
    const modalContainer = document.getElementById('modal-container');
    if (modalContainer) {
        modalContainer.style.display = 'none';
        modalContainer.innerHTML = '';
    }
};

async function handleScheduledTaskSubmit(e) {
    e.preventDefault();

    const id = document.getElementById('scheduled-task-id').value;
    const name = document.getElementById('scheduled-task-name').value.trim();
    const taskType = document.getElementById('scheduled-task-type').value;
    const cronExpression = document.getElementById('scheduled-task-cron').value.trim();
    const enabled = document.getElementById('scheduled-task-enabled').checked;

    if (!name || !taskType || !cronExpression) {
        alert(t('scheduled_tasks.required_fields'));
        return;
    }

    let config = {};

    if (taskType === 'mac_sync') {
        const deviceId = document.getElementById('scheduled-task-device-id').value;
        const networkId = document.getElementById('scheduled-task-network-id').value;
        if (!deviceId || !networkId) {
            alert(t('scheduled_tasks.required_fields'));
            return;
        }
        config = { device_id: deviceId, network_id: networkId };
    } else if (taskType === 'log_cleanup') {
        const keepDays = parseInt(document.getElementById('scheduled-task-keep-days').value, 10) || 30;
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
            response = await apiPost('/api/system/scheduled-tasks', data);
        }

        if (response.success) {
            showToast(id ? t('scheduled_tasks.update_success') : t('scheduled_tasks.create_success'), 'success');
            closeScheduledTaskModal();
            await loadScheduledTasks();
        } else {
            showToast(t('scheduled_tasks.save_failed') + ': ' + (response.message || ''), 'error');
        }
    } catch (error) {
        console.error('Failed to save task:', error);
        showToast(t('scheduled_tasks.save_failed'), 'error');
    }
}

window.toggleScheduledTask = async function(id) {
    try {
        const response = await apiPost(`/api/system/scheduled-tasks/${id}/toggle`);
        if (response.success) {
            showToast(t('scheduled_tasks.toggle_success'), 'success');
            await loadScheduledTasks();
        } else {
            showToast(t('scheduled_tasks.toggle_failed'), 'error');
        }
    } catch (error) {
        console.error('Failed to toggle task:', error);
        showToast(t('scheduled_tasks.toggle_failed'), 'error');
    }
};

window.runScheduledTask = async function(id) {
    const confirmed = await showConfirm(t('scheduled_tasks.confirm_run'));
    if (!confirmed) return;

    try {
        const response = await apiPost(`/api/system/scheduled-tasks/${id}/run`);
        if (response.success) {
            const result = response.data?.result;
            if (result && result.ok) {
                showToast(t('scheduled_tasks.run_success') + ': ' + result.ok, 'success');
            } else if (result && result.err) {
                showToast(t('scheduled_tasks.run_failed') + ': ' + result.err, 'error');
            } else {
                showToast(t('scheduled_tasks.run_success'), 'success');
            }
            await loadScheduledTasks();
        } else {
            showToast(t('scheduled_tasks.run_failed') + ': ' + (response.message || ''), 'error');
        }
    } catch (error) {
        console.error('Failed to run task:', error);
        showToast(t('scheduled_tasks.run_failed'), 'error');
    }
};

window.deleteScheduledTask = async function(id) {
    const confirmed = await showConfirm(t('scheduled_tasks.confirm_delete'));
    if (!confirmed) return;

    try {
        const response = await apiDelete(`/api/system/scheduled-tasks/${id}`);
        if (response.success) {
            showToast(t('scheduled_tasks.delete_success'), 'success');
            await loadScheduledTasks();
        }
    } catch (error) {
        console.error('Failed to delete task:', error);
    }
};

window.viewTaskLogs = async function(taskName) {
    const template = document.getElementById('task-logs-modal-template');
    if (!template) return;

    const modalContainer = document.getElementById('modal-container');
    if (modalContainer) {
        modalContainer.innerHTML = template.innerHTML;
        modalContainer.style.display = 'flex';

        try {
            const response = await apiGet(`/api/system/scheduled-tasks/logs?task_name=${encodeURIComponent(taskName)}&limit=50`);
            const tbody = document.getElementById('task-logs-tbody');
            if (tbody && response.success) {
                const logs = response.data || [];
                if (logs.length === 0) {
                    tbody.innerHTML = `<tr><td colspan="5" class="no-data">${t('common.no_data')}</td></tr>`;
                } else {
                    logs.forEach(log => {
                        const row = document.createElement('tr');
                        const statusClass = log.status === 'success' ? 'status-active' : 'status-inactive';
                        row.innerHTML = `
                            <td>${escapeHtml(log.task_name)}</td>
                            <td><span class="status-badge ${statusClass}">${log.status}</span></td>
                            <td>${formatDateTime(log.start_time)}</td>
                            <td>${log.duration || 0}</td>
                            <td>${escapeHtml(log.details?.message || log.details?.error || '-')}</td>
                        `;
                        tbody.appendChild(row);
                    });
                }
            }
        } catch (error) {
            console.error('Failed to load task logs:', error);
        }
    }
};

window.closeTaskLogsModal = function() {
    const modalContainer = document.getElementById('modal-container');
    if (modalContainer) {
        modalContainer.style.display = 'none';
        modalContainer.innerHTML = '';
    }
};
