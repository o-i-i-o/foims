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
import { IpConfigManager } from "../utils/ipconfig.js";
import { loadCabinetsForModalSelect } from "./cabinet.js";

// 防止重复加载的标志
let isLoadingPositions = false;

// 加载机位数据
export async function loadCabinetPositionsData() {
  // 防止重复加载
  if (isLoadingPositions) {
    return;
  }

  const token = getAccessToken();
  if (!token) {
    return;
  }

  try {
    isLoadingPositions = true;
    const response = await fetch("/api/resources/positions", {
      headers: {
        Authorization: `Bearer ${token}`,
      },
    });

    const data = await response.json();
    const tbody = document.querySelector("#cabinet-positions-table tbody");

    // 确保tbody元素存在
    if (!tbody) {
      console.error("未找到机位表格 tbody 元素");
      return;
    }

    // 先清空表格内容
    tbody.innerHTML = "";

    if (data.success && data.data.length > 0) {
      for (const position of data.data) {
        // 显示格式：机柜名+机位名，如Cabinet01-Server01
        const displayName = `${position.cabinet_name}-${position.name}`;
        // 构建交换机端口显示内容
        const portsHtml =
          position.ports && position.ports.length > 0
            ? position.ports
                .map(
                  (port) =>
                    `${port.switch_name || "未知交换机"}: ${port.port_number}${port.port_name ? ` (${port.port_name})` : ""}`,
                )
                .join("<br>")
            : "-";
        
        // 获取机位关联的IP地址
        let ipsHtml = "-";
        try {
          const ipsResponse = await fetch(`/api/resources/ip/cabinet-position/${position.id}`, {
            headers: {
              Authorization: `Bearer ${token}`,
            },
          });
          const ipsData = await ipsResponse.json();
          if (ipsData.success && ipsData.data.length > 0) {
            ipsHtml = ipsData.data.map(ip => ip.ip_address).join("<br>");
          }
        } catch (ipsError) {
          console.error(`获取机位 ${position.id} 的IP地址失败:`, ipsError);
        }

        const row = document.createElement("tr");
        row.innerHTML = `
                    <td>${displayName}</td>
                    <td>${ipsHtml}</td>
                    <td>${position.start_u} - ${position.end_u} U</td>
                    <td>${portsHtml}</td>
                    <td>${position.description || "-"}</td>
                    <td>${new Date(position.created_at).toLocaleString()}</td>
                    <td>
                        <button class="btn btn-sm btn-edit" data-id="${position.id}">编辑</button>
                        <button class="btn btn-sm btn-delete" data-id="${position.id}">删除</button>
                    </td>
                `;
        tbody.appendChild(row);
      }
    } else {
      tbody.innerHTML =
        '<tr class="empty-row"><td colspan="8" class="text-center">暂无机位数据</td></tr>';
    }
  } catch (error) {
    console.error("加载机位数据失败:", error);
    const tbody = document.querySelector("#cabinet-positions-table tbody");
    if (tbody) {
      tbody.innerHTML =
        '<tr class="empty-row"><td colspan="8" class="text-center">加载失败，请刷新页面重试</td></tr>';
    }
  } finally {
    // 无论成功失败，都重置加载状态
    isLoadingPositions = false;
  }
}

// 编辑机位
export async function editCabinetPosition(id) {
  const token = getAccessToken();
  if (!token) return;

  try {
    const response = await fetch(`/api/resources/positions/${id}`, {
      headers: {
        Authorization: `Bearer ${token}`,
      },
    });

    const result = await response.json();
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
  const token = getAccessToken();
  if (!token) return;

  const id = document.getElementById("cabinet-position-id").value;
  const parsedId = id && id !== "" ? id : null;
  const name = document.getElementById("cabinet-position-name").value;
  const cabinetId = document.getElementById("cabinet-position-cabinet").value;
  const startU = parseInt(
    document.getElementById("cabinet-position-start-u").value,
  );
  const endU = parseInt(
    document.getElementById("cabinet-position-end-u").value,
  );
  const description = document.getElementById(
    "cabinet-position-description",
  ).value;

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

  // 验证U位范围
  if (startU > endU) {
    showToast("起始U位不能大于结束U位", "warning");
    return;
  }

  // 收集所有IP地址
  const ipManager = new IpConfigManager('cabinet-position');
  const ips = ipManager.getIps();

  // 验证至少有一个IP地址
  if (ips.length === 0) {
    showToast("请至少添加一个IP地址", "warning");
    return;
  }

  const positionData = {
    name: name.trim(),
    cabinet_id: cabinetId, // 直接使用UUID字符串，不转换为整数
    start_u: startU,
    end_u: endU,
    ips: ips.length > 0 ? ips.map(ip => ({ ...ip, device_type: "cabinet_position" })) : null,
    description: description.trim() || null,
  };

  try {
    const url = parsedId
      ? `/api/resources/positions/${parsedId}`
      : "/api/resources/positions";
    const method = parsedId ? "PUT" : "POST";

    const response = await fetch(url, {
      method,
      headers: {
        Authorization: `Bearer ${token}`,
        "Content-Type": "application/json",
      },
      body: JSON.stringify(positionData),
    });

    const result = await response.json();
    if (result.success) {
      closeModal("cabinet-position-modal");
      loadCabinetPositionsData();
      showToast("机位保存成功", "success");
    } else {
      // 处理服务器返回的错误信息
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
  const modal = document.getElementById("cabinet-position-modal");
  const title = document.getElementById("cabinet-position-modal-title");
  const form = document.getElementById("cabinet-position-form");

  // 加载机柜选项
  await loadCabinetsForModalSelect();

  // 清空现有IP地址
  const ipManager = new IpConfigManager('cabinet-position');
  ipManager.clear();

  // 获取机柜选择框
  const cabinetSelect = document.getElementById("cabinet-position-cabinet");
  
  // 导入并添加机柜选择事件监听器
  const { handleCabinetPositionCabinetChange, bindCabinetPositionAddIpButton } = await import("../utils/ipconfig.js");
  
  // 添加机柜选择事件监听器，当选择机柜时，清空现有IP行
  if (cabinetSelect) {
    // 移除之前的事件监听器，避免重复添加
    cabinetSelect.removeEventListener("change", handleCabinetPositionCabinetChange);
    
    // 添加新的事件监听器
    cabinetSelect.addEventListener("change", handleCabinetPositionCabinetChange);
  }

  if (position) {
    // 编辑模式
    title.textContent = "编辑机位";
    document.getElementById("cabinet-position-id").value = position.id;
    document.getElementById("cabinet-position-name").value = position.name;
    document.getElementById("cabinet-position-cabinet").value =
      position.cabinet_id;
    document.getElementById("cabinet-position-start-u").value =
      position.start_u || 1;
    document.getElementById("cabinet-position-end-u").value =
      position.end_u || 1;
    document.getElementById("cabinet-position-description").value =
      position.description || "";

    // 如果有IP信息，加载已配置的IP（loadIps会自动加载网段和交换机端口）
    if (position.ips && position.ips.length > 0) {
      await ipManager.loadIps(position.ips);
    }
  } else {
    // 添加模式，默认添加一个IP容器
    title.textContent = "添加机位";
    form.reset();
    document.getElementById("cabinet-position-id").value = "";
  }

  openModal("cabinet-position-modal");
  
  bindCabinetPositionAddIpButton();
}
