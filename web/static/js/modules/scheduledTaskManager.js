import { api } from './api.js';
import { i18n } from './i18n.js';

let switches = [];
let networks = [];

export async function initScheduledTaskManager() {
    await loadSwitches();
    await loadNetworks();
    await loadScheduledTasks();
    setupEventListeners();
}

async function loadSwitches() {
    try {
        const response = await api.get('/api/switches');
        if (response.success) {
            switches = response.data || [];
        }
    } catch (error) {
        console.error('Failed to load switches:', error);
    }
}

async function loadNetworks() {
    try {
        const response = await api.get('/api/networks');
        if (response.success) {
            networks = response.data || [];
        }
    } catch (error) {
        console.error('Failed to load networks:', error);
    }
}

function setupEventListeners() {
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
            populateSwitchSelect();
            populateNetworkSelect();
        }
    } else if (taskType === 'log_cleanup') {
        if (logCleanupConfig) {
            logCleanupConfig.style.display = 'block';
        }
    }
}

function populateSwitchSelect() {
    const select = document.getElementById('scheduled-task-switch-id');
    if (!select) return;

    const currentLang = localStorage.getItem('language') || 'zh';
    const selectText = currentLang === 'zh' ? '选择交换机' : 'Select Switch';

    select.innerHTML = `<option value="">${selectText}</option>`;
    switches.forEach(sw => {
        const option = document.createElement('option');
        option.value = sw.id;
        option.textContent = sw.name || sw.hostname || sw.id;
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
    const container = document.getElementById('scheduled-tasks-container');
    if (!container) return;

    try {
        const response = await api.get('/api/scheduled-tasks');
        if (response.success) {
            renderScheduledTasks(response.data || []);
        } else {
            container.innerHTML = `<p class="error-message">${i18n.t('scheduled_tasks.load_failed')}</p>`;
        }
    } catch (error) {
        console.error('Failed to load scheduled tasks:', error);
        container.innerHTML = `<p class="error-message">${i18n.t('scheduled_tasks.load_failed')}</p>`;
    }
}

function renderScheduledTasks(tasks) {
    const container = document.getElementById('scheduled-tasks-container');
    if (!container) return;

    if (tasks.length === 0) {
        container.innerHTML = `<p class="no-data">${i18n.t('scheduled_tasks.no_tasks')}</p>`;
        return;
    }

    const table = document.createElement('table');
    table.className = 'data-table';
    table.innerHTML = `
        <thead>
            <tr>
                <th>${i18n.t('scheduled_tasks.name')}</th>
                <th>${i18n.t('scheduled_tasks.type')}</th>
                <th>${i18n.t('scheduled_tasks.cron')}</th>
                <th>${i18n.t('scheduled_tasks.status')}</th>
                <th>${i18n.t('scheduled_tasks.last_run')}</th>
                <th>${i18n.t('scheduled_tasks.next_run')}</th>
                <th>${i18n.t('scheduled_tasks.last_result')}</th>
                <th>${i18n.t('common.actions')}</th>
            </tr>
        </thead>
        <tbody></tbody>
    `;

    const tbody = table.querySelector('tbody');
    tasks.forEach(task => {
        const row = document.createElement('tr');
        row.innerHTML = `
            <td>${escapeHtml(task.name)}</td>
            <td>${i18n.t('scheduled_tasks.task_types.' + task.task_type) || task.task_type}</td>
            <td><code>${escapeHtml(task.cron_expression)}</code></td>
            <td>
                <span class="status-badge ${task.enabled ? 'status-active' : 'status-inactive'}">
                    ${task.enabled ? i18n.t('scheduled_tasks.enabled') : i18n.t('scheduled_tasks.disabled')}
                </span>
            </td>
            <td>${formatDateTime(task.last_run_at)}</td>
            <td>${formatDateTime(task.next_run_at)}</td>
            <td>${escapeHtml(task.last_result || '-')}</td>
            <td class="actions">
                <button class="btn btn-sm btn-secondary" onclick="runScheduledTask('${task.id}')" title="${i18n.t('scheduled_tasks.run_now')}">
                    <i class="icon-play"></i>
                </button>
                <button class="btn btn-sm btn-secondary" onclick="toggleScheduledTask('${task.id}')" title="${i18n.t('common.toggle')}">
                    <i class="icon-toggle-${task.enabled ? 'on' : 'off'}"></i>
                </button>
                <button class="btn btn-sm btn-secondary" onclick="editScheduledTask('${task.id}')" title="${i18n.t('common.edit')}">
                    <i class="icon-edit"></i>
                </button>
                <button class="btn btn-sm btn-secondary" onclick="viewTaskLogs('${task.name}')" title="${i18n.t('scheduled_tasks.view_logs')}">
                    <i class="icon-log"></i>
                </button>
                <button class="btn btn-sm btn-danger" onclick="deleteScheduledTask('${task.id}')" title="${i18n.t('common.delete')}">
                    <i class="icon-delete"></i>
                </button>
            </td>
        `;
        tbody.appendChild(row);
    });

    container.innerHTML = '';
    container.appendChild(table);
}

window.openCreateScheduledTaskModal = function() {
    const template = document.getElementById('scheduled-task-modal-template');
    if (!template) return;

    const modalContainer = document.getElementById('modal-container');
    if (modalContainer) {
        modalContainer.innerHTML = template.innerHTML;
        setupEventListeners();
        document.getElementById('scheduled-task-modal-title').textContent = i18n.t('scheduled_tasks.create_task');
        document.getElementById('scheduled-task-id').value = '';
        document.getElementById('scheduled-task-form').reset();
        document.getElementById('scheduled-task-enabled').checked = true;
        handleTaskTypeChange({ target: { value: 'mac_sync' } });
        modalContainer.style.display = 'flex';
    }
};

window.editScheduledTask = async function(id) {
    try {
        const response = await api.get(`/api/scheduled-tasks/${id}`);
        if (response.success) {
            const task = response.data;
            const template = document.getElementById('scheduled-task-modal-template');
            if (!template) return;

            const modalContainer = document.getElementById('modal-container');
            if (modalContainer) {
                modalContainer.innerHTML = template.innerHTML;
                setupEventListeners();
                document.getElementById('scheduled-task-modal-title').textContent = i18n.t('scheduled_tasks.edit_task');
                document.getElementById('scheduled-task-id').value = task.id;
                document.getElementById('scheduled-task-name').value = task.name;
                document.getElementById('scheduled-task-type').value = task.task_type;
                document.getElementById('scheduled-task-cron').value = task.cron_expression;
                document.getElementById('scheduled-task-enabled').checked = task.enabled;

                handleTaskTypeChange({ target: { value: task.task_type } });

                if (task.task_type === 'mac_sync' && task.config) {
                    setTimeout(() => {
                        document.getElementById('scheduled-task-switch-id').value = task.config.switch_id || '';
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
        alert(i18n.t('scheduled_tasks.required_fields'));
        return;
    }

    let config = {};

    if (taskType === 'mac_sync') {
        const switchId = document.getElementById('scheduled-task-switch-id').value;
        const networkId = document.getElementById('scheduled-task-network-id').value;
        if (!switchId || !networkId) {
            alert(i18n.t('scheduled_tasks.required_fields'));
            return;
        }
        config = { switch_id: switchId, network_id: networkId };
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
            response = await api.put(`/api/scheduled-tasks/${id}`, data);
        } else {
            response = await api.post('/api/scheduled-tasks', data);
        }

        if (response.success) {
            alert(id ? i18n.t('scheduled_tasks.update_success') : i18n.t('scheduled_tasks.create_success'));
            closeScheduledTaskModal();
            await loadScheduledTasks();
        } else {
            alert(i18n.t('scheduled_tasks.save_failed') + ': ' + (response.message || ''));
        }
    } catch (error) {
        console.error('Failed to save task:', error);
        alert(i18n.t('scheduled_tasks.save_failed'));
    }
}

window.toggleScheduledTask = async function(id) {
    try {
        const response = await api.post(`/api/scheduled-tasks/${id}/toggle`);
        if (response.success) {
            alert(i18n.t('scheduled_tasks.toggle_success'));
            await loadScheduledTasks();
        } else {
            alert(i18n.t('scheduled_tasks.toggle_failed'));
        }
    } catch (error) {
        console.error('Failed to toggle task:', error);
        alert(i18n.t('scheduled_tasks.toggle_failed'));
    }
};

window.runScheduledTask = async function(id) {
    if (!confirm(i18n.t('scheduled_tasks.confirm_run'))) return;

    try {
        const response = await api.post(`/api/scheduled-tasks/${id}/run`);
        if (response.success) {
            const result = response.data?.result;
            if (result && result.ok) {
                alert(i18n.t('scheduled_tasks.run_success') + ': ' + result.ok);
            } else if (result && result.err) {
                alert(i18n.t('scheduled_tasks.run_failed') + ': ' + result.err);
            } else {
                alert(i18n.t('scheduled_tasks.run_success'));
            }
            await loadScheduledTasks();
        } else {
            alert(i18n.t('scheduled_tasks.run_failed') + ': ' + (response.message || ''));
        }
    } catch (error) {
        console.error('Failed to run task:', error);
        alert(i18n.t('scheduled_tasks.run_failed'));
    }
};

window.deleteScheduledTask = async function(id) {
    if (!confirm(i18n.t('scheduled_tasks.confirm_delete'))) return;

    try {
        const response = await api.delete(`/api/scheduled-tasks/${id}`);
        if (response.success) {
            alert(i18n.t('scheduled_tasks.delete_success'));
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
            const response = await api.get(`/api/scheduled-tasks/logs?task_name=${encodeURIComponent(taskName)}&limit=50`);
            const tbody = document.getElementById('task-logs-tbody');
            if (tbody && response.success) {
                const logs = response.data || [];
                if (logs.length === 0) {
                    tbody.innerHTML = `<tr><td colspan="5" class="no-data">${i18n.t('common.no_data')}</td></tr>`;
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

function escapeHtml(text) {
    if (!text) return '';
    const div = document.createElement('div');
    div.textContent = text;
    return div.innerHTML;
}

function formatDateTime(dateStr) {
    if (!dateStr) return '-';
    try {
        const date = new Date(dateStr);
        return date.toLocaleString();
    } catch {
        return dateStr;
    }
}
