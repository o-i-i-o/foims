// 导入必要的模块
import {
  apiGet,
  apiPost,
  apiPut,
  apiDelete,
} from "./apiClient.js";

import {
  showToast,
  renderTable,
  formatDateTime,
  getElementValue,
  handleFormSubmit,
  debounce,
  handleError
} from "./ui.js";

import { openModal, closeModal } from "./modal.js";

// ====== 通用选项加载函数 ======

// 加载网络区域选项到下拉选择器
export async function loadNetworkTypeOptions() {
  try {
    const result = await apiGet("/api/resources/network-regions");
    const select = document.getElementById("network-type");

    // 保存当前选中值
    const currentValue = select.value;
    select.innerHTML = "";

    if (result.success && result.data.length > 0) {
      result.data.forEach((networkType) => {
        const option = document.createElement("option");
        option.value = networkType.id;
        option.textContent = networkType.name;
        select.appendChild(option);
      });
    } else {
      const option = document.createElement("option");
      option.value = "";
      option.textContent = "请先添加网络区域";
      option.disabled = true;
      select.appendChild(option);
    }

    // 恢复当前选中值
    if (currentValue) {
      select.value = currentValue;
    }
  } catch (error) {
    console.error("加载网络区域选项失败:", error);
  }
}

// 加载工位选项到下拉选择器
export async function loadWorkstationsForSelect() {
  try {
    const result = await apiGet("/api/resources/workstations");
    const select = document.getElementById("ip-workstation");

    if (result.success && select) {
      // 清空现有选项
      select.innerHTML = "";

      // 添加默认占位符
      const placeholder = document.createElement("option");
      placeholder.value = "";
      placeholder.textContent = "选择工位";
      select.appendChild(placeholder);

      // 添加新选项
      result.data.forEach((workstation) => {
        const option = document.createElement("option");
        option.value = workstation.id;
        // 显示 房间名-工位名 格式
        option.textContent = workstation.room_name
          ? `${workstation.room_name}-${workstation.name}`
          : workstation.name;
        select.appendChild(option);
      });

      return result.data;
    }
    return [];
  } catch (error) {
    console.error("加载工位选项失败:", error);
    return [];
  }
}

// 加载机柜机位选项到下拉选择器
export async function loadCabinetPositionsForSelect() {
  try {
    const result = await apiGet("/api/resources/positions");
    const select = document.getElementById("ip-cabinet-position");

    if (result.success && select) {
      // 清空现有选项
      select.innerHTML = "";

      // 添加默认占位符
      const placeholder = document.createElement("option");
      placeholder.value = "";
      placeholder.textContent = "选择机位";
      select.appendChild(placeholder);

      // 添加新选项
      result.data.forEach((position) => {
        const option = document.createElement("option");
        option.value = position.id;
        // 显示 机柜名-机位名 格式
        option.textContent = position.cabinet_name
          ? `${position.cabinet_name}-${position.name}`
          : position.name;
        select.appendChild(option);
      });

      return result.data;
    }
    return [];
  } catch (error) {
    console.error("加载机位选项失败:", error);
    return [];
  }
}

// 加载房间选项到下拉选择器
export async function loadRoomsForSelect(onlyOffice = false) {
  // 根据rememberMe状态获取access_token
  const localRememberMe = localStorage.getItem("rememberMe");
  const sessionRememberMe = sessionStorage.getItem("rememberMe");
  
  let token;
  if (localRememberMe === "true") {
    token = localStorage.getItem("access_token");
  } else {
    token = sessionStorage.getItem("access_token");
  }
  
  if (!token) return;

  try {
    const response = await fetch("/api/resources/rooms", {
      headers: {
        Authorization: `Bearer ${token}`,
      },
    });

    const result = await response.json();
    const select = document.getElementById("workstation-room");
    const visualizationSelect = document.getElementById("room-select");

    if (result.success) {
      // 清空现有选项
      if (select) {
        select.innerHTML = "";

        // 添加默认占位符
        const placeholder = document.createElement("option");
        placeholder.value = "";
        placeholder.textContent = onlyOffice ? "选择办公室" : "选择房间";
        select.appendChild(placeholder);

        // 添加新选项
        result.data.forEach((room) => {
          // 如果只需要办公室，则过滤掉非办公室类型的房间
          if (onlyOffice) {
            let roomTypeLower = room.room_type.toLowerCase();
            if (roomTypeLower === "office") {
              const option = document.createElement("option");
              option.value = room.id;
              option.textContent = room.name;
              select.appendChild(option);
            }
          } else {
            const option = document.createElement("option");
            option.value = room.id;
            option.textContent = room.name;
            select.appendChild(option);
          }
        });

        // 如果没有选项，添加提示选项
        if (select.children.length === 1) {
          const noDataOption = document.createElement("option");
          noDataOption.value = "";
          noDataOption.textContent = onlyOffice ? "暂无办公室数据" : "暂无房间数据";
          noDataOption.disabled = true;
          select.appendChild(noDataOption);
        }
      }

      // 更新可视化管理中的房间选择（只显示办公室类型，不显示机房类型）
      if (visualizationSelect) {
        visualizationSelect.innerHTML = "";

        // 添加默认占位符
        const placeholder = document.createElement("option");
        placeholder.value = "";
        placeholder.textContent = "选择房间";
        visualizationSelect.appendChild(placeholder);

        // 添加新选项（过滤掉机房类型）
        result.data.forEach((room) => {
          const roomTypeLower = room.room_type ? room.room_type.toLowerCase() : '';
          if (roomTypeLower !== 'data_center') {
            const option = document.createElement("option");
            option.value = room.id;
            option.textContent = room.name;
            visualizationSelect.appendChild(option);
          }
        });

        // 自动选择第一个房间但不自动绘制
        if (result.data.length > 0) {
          // 只选择房间，不自动绘制工位图
        }
      }
    }
  } catch (error) {
    console.error("加载房间选项失败:", error);
  }
}

// 加载网络区域选项到选择框
export async function loadNetworkRegionsForSelect(selectElement, autoSelectFirst = false) {
  try {
    const result = await apiGet("/api/resources/network-regions");

    if (result.success && selectElement) {
      // 清空现有选项
      selectElement.innerHTML = "";

      // 添加新选项
      result.data.forEach((region) => {
        const option = document.createElement("option");
        option.value = region.id;
        option.textContent = region.name;
        selectElement.appendChild(option);
      });

      // 如果需要自动选择第一个网络区域
      if (autoSelectFirst && result.data.length > 0) {
        selectElement.value = result.data[0].id;
      }

      return result.data;
    }
    return [];
  } catch (error) {
    console.error("加载网络区域选项失败:", error);
    return [];
  }
}

// 加载机房选项到下拉选择器
export async function loadDataCenterRoomsForSelect() {
  // 根据rememberMe状态获取access_token
  const localRememberMe = localStorage.getItem("rememberMe");
  const sessionRememberMe = sessionStorage.getItem("rememberMe");
  
  let token;
  if (localRememberMe === "true") {
    token = localStorage.getItem("access_token");
  } else {
    token = sessionStorage.getItem("access_token");
  }
  
  if (!token) return;

  try {
    const response = await fetch("/api/resources/rooms", {
      headers: {
        Authorization: `Bearer ${token}`,
      },
    });

    const result = await response.json();
    const select = document.getElementById("cabinet-room");

    if (result.success && select) {
      // 清空现有选项
      select.innerHTML = "";

      // 添加默认占位符
      const placeholder = document.createElement("option");
      placeholder.value = "";
      placeholder.textContent = "选择机房";
      select.appendChild(placeholder);

      // 添加新选项，只加载类型为机房的房间
      result.data.forEach((room) => {
        // 转换房间类型为小写进行比较
        let roomTypeLower = room.room_type.toLowerCase();
        if (roomTypeLower === "data_center") {
          const option = document.createElement("option");
          option.value = room.id;
          option.textContent = room.name;
          select.appendChild(option);
        }
      });

      // 如果没有机房选项，添加提示选项
      if (select.children.length === 1) {
        const noDataOption = document.createElement("option");
        noDataOption.value = "";
        noDataOption.textContent = "暂无机房数据";
        noDataOption.disabled = true;
        select.appendChild(noDataOption);
      }
    }
  } catch (error) {
    console.error("加载机房选项失败:", error);
  }
}

// 加载机柜数据
export async function loadCabinets() {
  try {
    const result = await apiGet("/api/resources/cabinets");
    return result.success ? result.data : [];
  } catch (error) {
    console.error("加载机柜数据失败:", error);
    return [];
  }
}

// 格式化MAC地址为****-****-****格式
export function formatMacAddress(mac) {
  if (!mac) return mac;
  const cleaned = mac.replace(/[^0-9A-Fa-f]/g, '');
  if (cleaned.length !== 12) return mac;
  return cleaned.toUpperCase().match(/.{4}/g).join('-');
}

// 加载网络区域选项到下拉选择器
export async function loadNetworkRegionsForIpSelect() {
  try {
    const result = await apiGet("/api/resources/network-regions");
    const select = document.getElementById("ip-network-region");

    if (result.success && select) {
      // 保存当前选中值
      const currentValue = select.value;
      select.innerHTML = "";

      // 添加默认选项
      const defaultOption = document.createElement("option");
      defaultOption.value = "";
      defaultOption.textContent = "选择网络区域";
      select.appendChild(defaultOption);

      if (result.data.length > 0) {
        result.data.forEach((networkRegion) => {
          const option = document.createElement("option");
          option.value = networkRegion.id;
          option.textContent = networkRegion.name;
          select.appendChild(option);
        });
      }

      // 恢复当前选中值
      if (currentValue) {
        select.value = currentValue;
      }

      // 触发网络区域选择变化事件
      select.dispatchEvent(new Event('change'));
    }
  } catch (error) {
    console.error("加载网络区域选项失败:", error);
  }
}

// 加载网络选项到下拉选择器
export async function loadNetworksForRegionSelect(regionId) {
  try {
    const result = await apiGet(`/api/resources/networks?region_id=${regionId}`);
    const select = document.getElementById("ip-network");

    if (result.success && select) {
      // 清空现有选项
      select.innerHTML = "";

      // 添加默认选项
      const defaultOption = document.createElement("option");
      defaultOption.value = "";
      defaultOption.textContent = "选择网络";
      select.appendChild(defaultOption);

      if (result.data.length > 0) {
        result.data.forEach((network) => {
          const option = document.createElement("option");
          option.value = network.id;
          option.textContent = `${network.name} (${network.ipv4_cidr || network.ipv6_cidr})`;
          select.appendChild(option);
        });
      }
    }
  } catch (error) {
    console.error("加载网络选项失败:", error);
  }
}

// 通用加载设备IP地址函数
async function loadDeviceIps(apiEndpoint, containerId) {
  try {
    const result = await apiGet(apiEndpoint);
    const container = document.getElementById(containerId);

    if (result.success && container) {
      // 清空现有内容
      container.innerHTML = "";

      if (result.data && result.data.length > 0) {
        // 创建IP地址列表
        const table = document.createElement("table");
        table.className = "table table-sm table-striped";
        table.innerHTML = `
          <thead>
            <tr>
              <th>IP地址</th>
              <th>网络</th>
              <th>MAC地址</th>
              <th>主机名</th>
              <th>状态</th>
              <th>操作</th>
            </tr>
          </thead>
          <tbody>
            ${result.data.map(ip => `
              <tr>
                <td>${ip.ip_address}</td>
                <td>${ip.network_name || "未知"}</td>
                <td>${formatMacAddress(ip.mac_address) || "-"}</td>
                <td>${ip.hostname || "-"}</td>
                <td>${ip.status || "-"}</td>
                <td>
                  <button class="btn btn-sm btn-primary edit-ip-btn" data-id="${ip.id}">编辑</button>
                  <button class="btn btn-sm btn-danger delete-ip-btn" data-id="${ip.id}">删除</button>
                </td>
              </tr>
            `).join('')}
          </tbody>
        `;
        container.appendChild(table);

        // 绑定编辑和删除按钮事件
        bindIpButtonsEvents();
      } else {
        // 显示无数据提示
        container.innerHTML = '<p class="text-center text-muted">暂无IP地址</p>';
      }
    }
  } catch (error) {
    console.error(`加载IP地址失败 (${apiEndpoint}):`, error);
    const container = document.getElementById(containerId);
    if (container) {
      container.innerHTML = '<p class="text-center text-danger">加载失败，请刷新页面重试</p>';
    }
  }
}

// 加载工位的IP地址列表
export async function loadWorkstationIps(workstationId) {
  await loadDeviceIps(`/api/resources/ip/workstation/${workstationId}`, "workstation-ips-container");
}

// 加载机柜机位IP地址
export async function loadCabinetPositionIps(cabinetPositionId) {
  await loadDeviceIps(`/api/resources/ip/cabinet-position/${cabinetPositionId}`, "cabinet-position-ips-container");
}

// 加载交换机IP地址
export async function loadSwitchIps(switchId) {
  await loadDeviceIps(`/api/resources/ip/switch/${switchId}`, "switch-ips-container");
}

// 绑定IP地址按钮事件
export function bindIpButtonsEvents() {
  // 绑定编辑按钮事件
  document.querySelectorAll('.edit-ip-btn').forEach(button => {
    button.addEventListener('click', function() {
      const ipId = this.getAttribute('data-id');
      // 这里可以实现编辑IP地址的功能
      console.log('编辑IP地址:', ipId);
    });
  });

  // 绑定删除按钮事件
  document.querySelectorAll('.delete-ip-btn').forEach(button => {
    button.addEventListener('click', function() {
      const ipId = this.getAttribute('data-id');
      // 这里可以实现删除IP地址的功能
      console.log('删除IP地址:', ipId);
    });
  });
}

// 加载所选房间的网段配置
export async function loadRoomNetworksForCabinet(roomId) {
  const inheritedNetworksContainer = document.getElementById("cabinet-inherited-networks");
  
  if (!roomId) {
    inheritedNetworksContainer.innerHTML = '<p class="text-muted">请先选择所属房间，将自动继承房间的网段配置</p>';
    return;
  }
  
  try {
    // 获取房间详情，包括其关联的网段
    const roomResult = await apiGet(`/api/resources/rooms/${roomId}`);
    
    if (roomResult.success && roomResult.data) {
      const room = roomResult.data;
      
      if (room.networks && room.networks.length > 0) {
        // 显示房间的网段配置
        let networksHtml = '<ul class="list-group">';
        room.networks.forEach(network => {
          networksHtml += `
            <li class="list-group-item">
              <span class="font-weight-bold">${network.name}</span>
              <span class="text-muted">(${network.network_region})</span>
              ${network.ipv4_cidr ? `<div class="small">IPv4: ${network.ipv4_cidr}</div>` : ''}
              ${network.ipv6_cidr ? `<div class="small">IPv6: ${network.ipv6_cidr}</div>` : ''}
            </li>
          `;
        });
        networksHtml += '</ul>';
        inheritedNetworksContainer.innerHTML = networksHtml;
      } else {
        inheritedNetworksContainer.innerHTML = '<p class="text-muted">所选房间未配置网段，请先为房间添加网段配置</p>';
      }
    } else {
      inheritedNetworksContainer.innerHTML = '<p class="text-muted">获取房间信息失败，请重试</p>';
    }
  } catch (error) {
    console.error("加载房间网段配置失败:", error);
    inheritedNetworksContainer.innerHTML = '<p class="text-muted">加载房间网段配置失败，请重试</p>';
  }
}
