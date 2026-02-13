
// 导入必要的模块
import {
  apiGet,
  apiPost,
  apiPut,
  apiDelete,
  getAccessToken,
} from "../utils/apiClient.js";

import {
  showToast,
  renderTable,
  formatDateTime,
  getElementValue,
  handleFormSubmit,
  handleDelete,
  debounce,
  handleError
} from "../utils/ui.js";

import { openModal, closeModal } from "../utils/modal.js";

import {
  IpConfigManager
} from "../utils/ipconfig.js";

import {
  loadRoomsForSelect
} from "../utils/resources.js";

import {
  handleWorkstationRoomChange,
  bindWorkstationAddIpButton
} from "../utils/ipconfig.js";

// 编辑工位
export async function editWorkstation(id) {
  const token = getAccessToken();
  if (!token) return;

  try {
    const response = await fetch(`/api/resources/workstations/${id}`, {
      headers: {
        Authorization: `Bearer ${token}`,
      },
    });

    const result = await response.json();
    if (result.success) {
      openWorkstationModal(result.data);
    } else {
      showToast("获取工位数据失败: " + result.message, "error");
    }
  } catch (error) {
    console.error("获取工位数据失败:", error);
    showToast("操作失败，请重试", "error");
  }
}

// 删除工位
export async function deleteWorkstation(id) {
  await handleDelete(id, "/api/resources/workstations", "工位删除成功", loadWorkstationsData);
}

// 防止重复加载的标志
let isLoadingWorkstations = false;

// 加载工位数据
export async function loadWorkstationsData() {
  // 防止重复加载
  if (isLoadingWorkstations) {
    return;
  }

  const token = getAccessToken();
  if (!token) {
    return;
  }

  try {
    isLoadingWorkstations = true;
    const response = await fetch("/api/resources/workstations", {
      headers: {
        Authorization: `Bearer ${token}`,
      },
    });

    const data = await response.json();
    const tbody = document.querySelector("#workstations-table tbody");

    // 确保tbody元素存在
    if (!tbody) {
      console.error("未找到工位表格 tbody 元素");
      return;
    }

    // 先清空表格内容
    tbody.innerHTML = "";

    if (data.success && data.data.length > 0) {
      for (const workstation of data.data) {
        // 显示格式：房间名+工位名，如112-1
        const displayName = `${workstation.room_name}-${workstation.name}`;
        // 构建交换机端口显示内容
        const portsHtml =
          workstation.ports && workstation.ports.length > 0
            ? workstation.ports
                .map(
                  (port) =>
                    `${port.switch_name || "未知交换机"}: ${port.port_number}${port.port_name ? ` (${port.port_name})` : ""}`,
                )
                .join("<br>")
            : "-";
        
        // 获取工位关联的IP地址
        let ipsHtml = "-";
        try {
          const ipsResponse = await fetch(`/api/resources/ip/workstation/${workstation.id}`, {
            headers: {
              Authorization: `Bearer ${token}`,
            },
          });
          const ipsData = await ipsResponse.json();
          if (ipsData.success && ipsData.data.length > 0) {
            ipsHtml = ipsData.data.map(ip => ip.ip_address).join("<br>");
          }
        } catch (ipsError) {
          console.error(`获取工位 ${workstation.id} 的IP地址失败:`, ipsError);
        }

        const row = document.createElement("tr");
        row.innerHTML = `
                    <td>${displayName}</td>
                    <td>${ipsHtml}</td>
                    <td>${workstation.manager || "-"}</td>
                    <td>${portsHtml}</td>
                    <td>${workstation.description || "-"}</td>
                    <td>${new Date(workstation.created_at).toLocaleString()}</td>
                    <td>
                        <button class="btn btn-sm btn-edit" data-id="${workstation.id}">编辑</button>
                        <button class="btn btn-sm btn-delete" data-id="${workstation.id}">删除</button>
                    </td>
                `;
        tbody.appendChild(row);
      }
    } else {
      tbody.innerHTML =
        '<tr class="empty-row"><td colspan="8" class="text-center">暂无工位数据</td></tr>';
    }
  } catch (error) {
    console.error("加载工位数据失败:", error);
    const tbody = document.querySelector("#workstations-table tbody");
    if (tbody) {
      tbody.innerHTML =
        '<tr class="empty-row"><td colspan="8" class="text-center">加载失败，请刷新页面重试</td></tr>';
    }
  } finally {
    // 无论成功失败，都重置加载状态
    isLoadingWorkstations = false;
  }
}

// ====== 工位管理模态框 ======
export async function openWorkstationModal(workstation = null) {
  const modal = document.getElementById("workstation-modal");
  const title = document.getElementById("workstation-modal-title");
  const form = document.getElementById("workstation-form");

  // 加载房间选项（只加载办公室）
  await loadRoomsForSelect(true);

  // 清空现有IP地址
  const ipManager = new IpConfigManager('workstation');
  ipManager.clear();

  // 获取房间选择框
  const roomSelect = document.getElementById("workstation-room");
  
  // 添加房间选择事件监听器，当选择房间时，清空现有IP行
  if (roomSelect) {
    // 移除之前的事件监听器，避免重复添加
    roomSelect.removeEventListener("change", handleWorkstationRoomChange);
    
    // 添加新的事件监听器
    roomSelect.addEventListener("change", handleWorkstationRoomChange);
  }

  if (workstation) {
    // 编辑模式
    title.textContent = "编辑工位";
    document.getElementById("workstation-id").value = workstation.id;
    document.getElementById("workstation-name").value = workstation.name;
    document.getElementById("workstation-room").value = workstation.room_id;
    document.getElementById("workstation-manager").value =
      workstation.manager || "";
    document.getElementById("workstation-description").value =
      workstation.description || "";

    // 如果有IP信息，加载已配置的IP
    if (workstation.ips && workstation.ips.length > 0) {
      await ipManager.loadIps(workstation.ips);
    }
  } else {
    // 添加模式，默认添加一个IP容器
    title.textContent = "添加工位";
    form.reset();
    document.getElementById("workstation-id").value = "";
  }

  openModal("workstation-modal");
  
  bindWorkstationAddIpButton();
}

// 提交工位表单
export async function submitWorkstationForm() {
  const token = getAccessToken();
  if (!token) return;

  const id = document.getElementById("workstation-id").value;
  const parsedId = id && id !== "" ? id : null;
  const name = document.getElementById("workstation-name").value;
  const roomId = document.getElementById("workstation-room").value;
  const manager = document.getElementById("workstation-manager").value;
  const description = document.getElementById("workstation-description").value;

  // 验证必填字段
  if (!name.trim()) {
    showToast("工位名称不能为空", "warning");
    return;
  }

  if (!roomId) {
    showToast("请选择房间", "warning");
    return;
  }

  // 收集所有IP地址
  const ipManager = new IpConfigManager('workstation');
  const ips = ipManager.getIps();
  
  // 验证至少有一个IP地址
  if (ips.length === 0) {
    showToast("请至少添加一个IP地址", "warning");
    return;
  }

  const workstationData = {
    name: name.trim(),
    room_id: roomId, // 直接使用UUID字符串，不转换为整数
    manager: manager.trim() || null,
    ports: null,
    ips: ips.length > 0 ? ips.map(ip => ({ ...ip, device_type: "workstation" })) : null,
    description: description.trim() || null,
  };

  try {
    const url = parsedId
      ? `/api/resources/workstations/${parsedId}`
      : "/api/resources/workstations";
    const method = parsedId ? "PUT" : "POST";

    const response = await fetch(url, {
      method,
      headers: {
        Authorization: `Bearer ${token}`,
        "Content-Type": "application/json",
      },
      body: JSON.stringify(workstationData),
    });

    const result = await response.json();
    if (result.success) {
      closeModal("workstation-modal");
      loadWorkstationsData();
      showToast("工位保存成功", "success");
    } else {
      // 处理服务器返回的错误信息
      const errorMsg = result.message || "操作失败，请检查输入信息";
      showToast(`操作失败: ${errorMsg}`, "error");
      console.error("服务器返回错误:", result);
    }
  } catch (error) {
    console.error("提交工位表单失败:", error);
    showToast("操作失败，请重试", "error");
  }
}
