// 导入必要的模块
import {
  apiGet,
  apiPost,
  apiPut,
} from "../utils/apiClient.js";

import {
  showToast,
  handleDelete,
  handleError,
  appendPaginationToTable,
  escapeHtml,
  DEFAULT_PAGE_SIZE,
  createSortState,
  updateSortIcons,
  initSortEvents,
} from "../utils/ui.js";

import { openModal, closeModal } from "../utils/modal.js";
import { t } from "../utils/i18n.js";
import { elementCache } from "../utils/helpers.js";
import { getManager } from "../utils/ipconfig.js";

const tableState = createSortState('name', 'asc');
let isLoading = false;
let currentPage = 1;

// 加载机位数据
export async function loadCabinetPositionsData(page = 1, sortBy = null, sortOrder = null) {
  currentPage = page;
  if (sortBy) tableState.setSort(sortBy, sortOrder);
  
  // 防止重复加载
  if (isLoading) {
    return;
  }
  try {
    isLoading = true;
    const result = await apiGet(`/api/resources/positions?page=${page}&page_size=${DEFAULT_PAGE_SIZE}&sort_by=${tableState.sortBy}&sort_order=${tableState.sortOrder}`);
    const tbody = document.querySelector("#cabinet-positions-table tbody");

    // 确保tbody元素存在
    if (!tbody) {
      console.error("未找到机位表格 tbody 元素");
      return;
    }

    // 先清空表格内容
    tbody.innerHTML = "";

    const data = result.success ? result.data : { items: [], total: 0 };
    const positions = data.items || data;

    if (positions.length > 0) {
      const startIndex = (page - 1) * DEFAULT_PAGE_SIZE;
      
      const ipPromises = positions.map(position => 
        apiGet(`/api/resources/ip/cabinet-position/${position.id}`)
          .then(ipsData => ({ position, ipsData }))
          .catch(ipsError => {
            console.error(`获取机位 ${position.id} 的IP地址失败:`, ipsError);
            return { position, ipsData: { success: false, data: [] } };
          })
      );
      
      const results = await Promise.all(ipPromises);
      let rowIndex = 0;
      
      for (const { position, ipsData } of results) {
        const cabinetName = escapeHtml(position.cabinet_name) || "-";
        const positionName = escapeHtml(position.name);
        
        let ipsHtml = "-";
        let portsHtml = "-";
        let deviceTypeHtml = "-";
        if (ipsData.success && ipsData.data.length > 0) {
          ipsHtml = ipsData.data.map(ip => escapeHtml(ip.ip_address)).join("<br>");
          
          const portInfos = ipsData.data
            .filter(ip => ip.port_device_name && ip.port_device_number)
            .map(ip => `${escapeHtml(ip.port_device_name)}: ${escapeHtml(ip.port_device_number)}`);
          portsHtml = portInfos.length > 0 ? portInfos.join("<br>") : "-";
          
          // 显示设备类型
          const deviceTypes = [...new Set(ipsData.data.map(ip => ip.device_type).filter(Boolean))];
          if (deviceTypes.length > 0) {
            deviceTypeHtml = deviceTypes.map(dt => t(`ip.device_types.${dt}`, dt)).join(", ");
          }
        }

        const row = document.createElement("tr");
        const roomName = escapeHtml(position.room_name) || "-";
        const isSwitchPosition = position.device_type === 'switch';
        const deleteBtnHtml = isSwitchPosition 
          ? `<button class="btn btn-sm btn-delete disabled" data-id="${position.id}" disabled title="该机位由交换机创建，请通过交换机管理删除">删除</button>`
          : `<button class="btn btn-sm btn-delete" data-id="${position.id}">删除</button>`;
        
        row.innerHTML = `
                    <td class="index-column">${startIndex + rowIndex + 1}</td>
                    <td>${roomName}</td>
                    <td>${cabinetName}</td>
                    <td>${positionName}</td>
                    <td>${deviceTypeHtml}</td>
                    <td>${ipsHtml}</td>
                    <td>${position.start_u} - ${position.end_u} U</td>
                    <td>${portsHtml}</td>
                    <td>${escapeHtml(position.description) || "-"}</td>
                    <td>${new Date(position.created_at).toLocaleString()}</td>
                    <td>
                        <button class="btn btn-sm btn-edit" data-id="${position.id}">编辑</button>
                        ${deleteBtnHtml}
                    </td>
                `;
        tbody.appendChild(row);
        rowIndex++;
      }

      if (data.total !== undefined) {
        appendPaginationToTable("#cabinet-positions-table", data, loadCabinetPositionsData);
      }
    } else {
      tbody.innerHTML =
        '<tr class="empty-row"><td colspan="9" class="text-center">暂无机位数据</td></tr>';
    }
    updateSortIcons("cabinet-positions-table", tableState);
  } catch (error) {
    console.error("加载机位数据失败:", error);
    const tbody = document.querySelector("#cabinet-positions-table tbody");
    if (tbody) {
      tbody.innerHTML =
        '<tr class="empty-row"><td colspan="9" class="text-center">加载失败，请刷新页面重试</td></tr>';
    }
  } finally {
    isLoading = false;
  }
}

export function initPositionSortEvents() {
  initSortEvents("cabinet-positions-table", tableState, loadCabinetPositionsData);
}

// 编辑机位
export async function editCabinetPosition(id) {
  try {
    const result = await apiGet(`/api/resources/positions/${id}`);
    if (result.success) {
      openCabinetPositionModal(result.data);
    } else {
      showToast(`获取机位数据失败: ${result.message}`, "error");
    }
  } catch (error) {
    handleError(error, "获取机位数据失败");
  }
}

// 删除机位
export async function deleteCabinetPosition(id) {
  await handleDelete(id, "/api/resources/positions", "机位删除成功", loadCabinetPositionsData);
}

// 提交机柜机位表单
export async function submitCabinetPositionForm() {
  const id = elementCache.getValue("cabinet-position-id");
  const parsedId = id && id !== "" ? id : null;
  const name = elementCache.getValue("cabinet-position-name");
  const cabinetId = elementCache.getValue("cabinet-position-cabinet");
  const startU = parseInt(elementCache.getValue("cabinet-position-start-u"));
  const endU = parseInt(elementCache.getValue("cabinet-position-end-u"));
  const description = elementCache.getValue("cabinet-position-description");

  // 验证必填字段
  if (!name.trim()) {
    showToast("名称不能为空", "warning");
    return;
  }

  if (!cabinetId) {
    showToast("请选择机柜", "warning");
    return;
  }

  // 验证U位是否为有效数字
  if (isNaN(startU) || startU <= 0) {
    showToast("起始U位必须是有效的正数", "warning");
    return;
  }

  if (isNaN(endU) || endU <= 0) {
    showToast("结束U位必须是有效的正数", "warning");
    return;
  }

  if (startU > endU) {
    showToast("起始U位不能大于结束U位", "warning");
    return;
  }

  const ipManager = getManager('cabinet-position');
  const validation = ipManager.validateIps();

  if (validation.errors && validation.errors.length > 0) {
    showToast(validation.errors[0], "warning");
    return;
  }

  if (validation.ips.length === 0) {
    showToast("请至少添加一个IP地址", "warning");
    return;
  }

  const positionData = {
    name: name.trim(),
    cabinet_id: cabinetId,
    start_u: startU,
    end_u: endU,
    ips: validation.ips,
    description: description.trim() || null,
  };

  try {
    let result;
    if (parsedId) {
      result = await apiPut(`/api/resources/positions/${parsedId}`, positionData);
    } else {
      result = await apiPost("/api/resources/positions", positionData);
    }
    
    if (result.success) {
      closeModal("cabinet-position-modal");
      loadCabinetPositionsData();
      showToast("机位保存成功", "success");
    } else {
      const errorMsg = result.message || "操作失败，请检查输入信息";
      showToast(`操作失败: ${errorMsg}`, "error");
      console.error("服务器返回错误:", result);
    }
  } catch (error) {
    console.error("提交机位表单失败:", error);
    showToast("操作失败，请重试", "error");
  }
}

// ====== 机位管理模态框 ======
export async function openCabinetPositionModal(position = null) {
  openModal("cabinet-position-modal");
  
  const modal = elementCache.get('cabinet-position-modal');
  const title = elementCache.get('cabinet-position-modal-title');
  const form = elementCache.get('cabinet-position-form');

  const roomSelect = elementCache.get('cabinet-position-room');
  const cabinetSelect = elementCache.get('cabinet-position-cabinet');

  // 使用单例manager
  const ipManager = getManager('cabinet-position');
  ipManager.clear();
  
  const { handleCabinetPositionCabinetChange } = await import("../utils/ipconfig.js");
  
  // 从 cabinets 数据源只读加载房间选项（去重）
  const loadRoomsFromCabinets = async () => {
    roomSelect.innerHTML = `<option value="">${t('cabinet_position.select_room', '选择房间')}</option>`;
    
    try {
      const result = await apiGet('/api/resources/cabinets');
      if (result.success && result.data) {
        const cabinets = result.data.items || result.data;
        const roomMap = new Map();
        
        cabinets.forEach(cabinet => {
          if (cabinet.room_id && cabinet.room_name && !roomMap.has(cabinet.room_id)) {
            roomMap.set(cabinet.room_id, cabinet.room_name);
          }
        });
        
        roomMap.forEach((roomName, roomId) => {
          const option = document.createElement('option');
          option.value = roomId;
          option.textContent = roomName;
          roomSelect.appendChild(option);
        });
      }
    } catch (error) {
      console.error('从机柜数据加载房间失败:', error);
    }
  };

  // 房间选择变化时加载机柜（只读）
  const handleRoomChange = async () => {
    const roomId = roomSelect.value;
    cabinetSelect.innerHTML = `<option value="">${t('cabinet_position.select_cabinet', '选择机柜')}</option>`;
    
    if (roomId) {
      try {
        const result = await apiGet(`/api/resources/cabinets?room_id=${roomId}`);
        if (result.success && result.data) {
          const cabinets = result.data.items || result.data;
          cabinets.forEach(cabinet => {
            const option = document.createElement('option');
            option.value = cabinet.id;
            option.textContent = cabinet.name;
            cabinetSelect.appendChild(option);
          });
        }
      } catch (error) {
        console.error('加载机柜失败:', error);
      }
    }
  };

  // 通过 cabinet_id 从 cabinets 数据源只读获取房间信息
  const loadRoomByCabinetId = async (cabinetId) => {
    try {
      const result = await apiGet(`/api/resources/cabinets/${cabinetId}`);
      if (result.success && result.data) {
        return {
          roomId: result.data.room_id,
          roomName: result.data.room_name
        };
      }
    } catch (error) {
      console.error('从机柜数据获取房间信息失败:', error);
    }
    return null;
  };

  // 加载房间选项（从 cabinets 数据源只读）
  await loadRoomsFromCabinets();

  if (roomSelect) {
    roomSelect.removeEventListener('change', handleRoomChange);
    roomSelect.addEventListener('change', handleRoomChange);
  }

  if (cabinetSelect) {
    cabinetSelect.removeEventListener("change", handleCabinetPositionCabinetChange);
    cabinetSelect.addEventListener("change", handleCabinetPositionCabinetChange);
  }

  if (position) {
    // 编辑模式
    title.textContent = "编辑机位";
    elementCache.setValue('cabinet-position-id', position.id);
    elementCache.setValue('cabinet-position-name', position.name);
    elementCache.setValue('cabinet-position-start-u', position.start_u || 1);
    elementCache.setValue('cabinet-position-end-u', position.end_u || 1);
    elementCache.setValue('cabinet-position-description', position.description || "");

    // 通过 cabinet_id 从 cabinets 数据源只读获取房间信息
    if (position.cabinet_id) {
      const roomInfo = await loadRoomByCabinetId(position.cabinet_id);
      if (roomInfo && roomInfo.roomId) {
        elementCache.setValue('cabinet-position-room', roomInfo.roomId);
        await handleRoomChange();
        elementCache.setValue('cabinet-position-cabinet', position.cabinet_id);
      }
    }

    // 编辑模式下加载IP配置
    if (position.ips && position.ips.length > 0) {
      // 如果有IP信息，加载已配置的IP
      await ipManager.loadIps(position.ips);
    } else if (position.cabinet_id) {
      // 如果没有IP信息但有机柜，创建空IP容器
      await ipManager.addIpRow();
    }
  } else {
    // 添加模式
    title.textContent = "添加机位";
    form.reset();
    elementCache.setValue('cabinet-position-id', '');
    cabinetSelect.innerHTML = `<option value="">${t('cabinet_position.select_room_first', '请先选择房间')}</option>`;
  }
}
