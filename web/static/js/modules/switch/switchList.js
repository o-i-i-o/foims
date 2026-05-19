import {
  apiGet,
  apiPost,
  apiPut,
  apiDelete,
} from "../../utils/apiClient.js";

import {
  showToast,
  renderTable,
  handleDelete,
  handleError,
  appendPaginationToTable,
  escapeHtml,
  debounce,
} from "../../utils/ui.js";

import { elementCache } from "../../utils/helpers.js";
import {
  listState,
  positionData,
  SWITCH_PAGE_SIZE
} from "./switchState.js";

let currentFilters = {
  name: '',
  ip: '',
  model: ''
};

async function loadSwitchesData(filters = currentFilters) {
  currentFilters = filters;
  listState.currentPage = listState.currentPage || 1;
  try {
    const params = new URLSearchParams({
      page: listState.currentPage.toString(),
      page_size: SWITCH_PAGE_SIZE.toString()
    });
    
    if (filters.name) params.append('name', filters.name);
    if (filters.ip) params.append('ip_address', filters.ip);
    if (filters.model) params.append('model', filters.model);
    
    const url = `/api/switches?${params.toString()}`;
    const result = await apiGet(url);
    const data = result.success ? result.data : { items: [], total: 0 };
    const switches = data.items || data;
    const startIndex = (listState.currentPage - 1) * SWITCH_PAGE_SIZE;

    renderTable("#switches-table", {
      data: switches,
      columns: [
        { field: 'id', render: (v, row, index) => startIndex + index + 1, className: 'index-column' },
        { field: 'name', render: (v, row) => v },
        { field: 'device_type', render: (v) => v || '-' },
        { field: 'ip_address', render: (v) => v },
        { field: 'mac_address', render: (v) => v || '-' },
        { field: 'model', render: (v) => v || '-' },
        { field: 'vendor', render: (v) => v || '-' },
        { field: 'location', render: (v) => v || '-' },
        { field: 'snmp_version', render: (v, row) => `<span class="status-badge ${row.snmp_community || row.snmp_username ? 'status-active' : 'status-inactive'}">${v || '-'}</span>` },
        { field: 'id', render: (v, row) => `
          <button class="btn btn-sm btn-edit" data-id="${v}">编辑</button>
          <button class="btn btn-sm btn-secondary btn-switch-ports" data-switch-id="${v}" data-switch-name="${escapeHtml(row.name)}">端口</button>
          <button class="btn btn-sm btn-secondary btn-switch-arp" data-switch-id="${v}">MAC表</button>
          <button class="btn btn-sm btn-secondary btn-switch-lldp" data-switch-id="${v}">LLDP</button>
          <button class="btn btn-sm btn-delete" data-id="${v}">删除</button>
        ` }
      ],
      emptyMessage: '暂无交换机数据'
    });

    if (data.total !== undefined) {
      appendPaginationToTable("#switches-table", data, (p) => {
        listState.currentPage = p;
        loadSwitchesData(filters);
      });
    }

    bindSwitchButtonsEvents();
  } catch (error) {
    handleError(error, "加载交换机数据失败", () => {
      renderTable("#switches-table", { data: [], columns: [], emptyMessage: "加载失败" });
    });
  }
}

let switchTableClickHandler = null;

function bindSwitchButtonsEvents() {
  const table = elementCache.get("switches-table");
  if (!table) return;

  if (switchTableClickHandler) {
    table.removeEventListener("click", switchTableClickHandler);
  }

  switchTableClickHandler = async (e) => {
    const target = e.target;
    const id = target.dataset.id || target.dataset.switchId;

    if (target.classList.contains("btn-delete")) {
      deleteSwitch(id);
    } else if (target.classList.contains("btn-switch-ports")) {
      const switchName = target.dataset.switchName;
      import('./switchPort.js').then(module => {
        module.manageSwitchPorts(id, switchName);
      });
    } else if (target.classList.contains("btn-switch-arp")) {
      import('./switchMacLldp.js').then(module => {
        module.viewArpTable(id);
      });
    } else if (target.classList.contains("btn-switch-lldp")) {
      import('./switchMacLldp.js').then(module => {
        module.viewLldpNeighbors(id);
      });
    }
  };

  table.addEventListener("click", switchTableClickHandler);
}

async function fetchSwitchById(id) {
  try {
    const result = await apiGet(`/api/switches/${id}`);
    if (result.success) {
      return result.data;
    } else {
      showToast("获取交换机信息失败", "error");
      return null;
    }
  } catch (error) {
    handleError(error, "获取交换机信息失败");
    return null;
  }
}

async function deleteSwitch(id) {
  await handleDelete(id, "/api/switches", "交换机删除成功", () => loadSwitchesData());
}

async function submitSwitchForm(formData) {
  const id = formData.id;

  if (!formData.name) {
    showToast("交换机名称不能为空", "warning");
    return false;
  }

  if (!formData.ips || formData.ips.length === 0) {
    showToast("请至少配置一个IP地址", "warning");
    return false;
  }

  try {
    if (positionData.cabinetId) {
      const positionPayload = {
        name: formData.name,
        cabinet_id: positionData.cabinetId,
        start_u: positionData.startU || 1,
        end_u: positionData.endU || 1,
        description: formData.description || null,
      };

      if (positionData.positionId) {
        const posResult = await apiPut(`/api/resources/positions/${positionData.positionId}`, positionPayload);
        if (!posResult.success) {
          showToast("更新机位失败：" + posResult.message, "error");
          return false;
        }
        formData.position_id = positionData.positionId;
      } else {
        const posResult = await apiPost("/api/resources/positions", positionPayload);
        if (!posResult.success) {
          showToast("创建机位失败：" + posResult.message, "error");
          return false;
        }
        formData.position_id = posResult.data?.id || null;
      }
    }

    let result;
    if (id) {
      result = await apiPut(`/api/switches/${id}`, formData);
    } else {
      result = await apiPost("/api/switches", formData);
    }

    if (result.success) {
      showToast(id ? "交换机更新成功" : "交换机添加成功", "success");
      return true;
    } else {
      showToast((id ? "更新" : "添加") + "交换机失败：" + result.message, "error");
      return false;
    }
  } catch (error) {
    handleError(error, "保存交换机失败");
    return false;
  }
}

function initSwitchFilters() {
  const filterIds = [
    'switch-name-filter',
    'switch-ip-filter',
    'switch-model-filter'
  ];
  
  const debouncedFilter = debounce(applySwitchFilters, 300);
  
  filterIds.forEach(filterId => {
    const filterElement = document.getElementById(filterId);
    if (filterElement) {
      filterElement.addEventListener('input', debouncedFilter);
    }
  });
}

function applySwitchFilters() {
  const filters = {
    name: document.getElementById('switch-name-filter')?.value || '',
    ip: document.getElementById('switch-ip-filter')?.value || '',
    model: document.getElementById('switch-model-filter')?.value || ''
  };
  
  listState.currentPage = 1;
  loadSwitchesData(filters);
}

export {
  loadSwitchesData,
  fetchSwitchById,
  deleteSwitch,
  submitSwitchForm,
  initSwitchFilters
};
