import {
  apiGet,
} from "./apiClient.js";

export async function loadNetworkTypeOptions() {
  try {
    const result = await apiGet("/api/resources/network-regions");
    const select = document.getElementById("network-type");

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

    if (currentValue) {
      select.value = currentValue;
    }
  } catch (error) {
  }
}

export async function loadRoomsForSelect(onlyOffice = false) {
  try {
    const result = await apiGet("/api/resources/rooms");
    const select = document.getElementById("workstation-room");
    const visualizationSelect = document.getElementById("room-select");

    if (result.success) {
      if (select) {
        select.innerHTML = "";

        const placeholder = document.createElement("option");
        placeholder.value = "";
        placeholder.textContent = onlyOffice ? "选择办公室" : "选择房间";
        select.appendChild(placeholder);

        result.data.forEach((room) => {
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

        if (select.children.length === 1) {
          const noDataOption = document.createElement("option");
          noDataOption.value = "";
          noDataOption.textContent = onlyOffice ? "暂无办公室数据" : "暂无房间数据";
          noDataOption.disabled = true;
          select.appendChild(noDataOption);
        }
      }

      if (visualizationSelect) {
        visualizationSelect.innerHTML = "";

        result.data.forEach((room) => {
          const roomTypeLower = room.room_type ? room.room_type.toLowerCase() : '';
          if (roomTypeLower !== 'data_center') {
            const option = document.createElement("option");
            option.value = room.id;
            option.textContent = room.name;
            visualizationSelect.appendChild(option);
          }
        });
      }
    }
  } catch (error) {
  }
}

export async function loadNetworkRegionsForSelect(selectElement, autoSelectFirst = false) {
  try {
    const result = await apiGet("/api/resources/network-regions");

    if (result.success && selectElement) {
      selectElement.innerHTML = "";

      result.data.forEach((region) => {
        const option = document.createElement("option");
        option.value = region.id;
        option.textContent = region.name;
        selectElement.appendChild(option);
      });

      if (autoSelectFirst && result.data.length > 0) {
        selectElement.value = result.data[0].id;
      }

      return result.data;
    }
    return [];
  } catch (error) {
    return [];
  }
}

export async function loadDataCenterRoomsForSelect() {
  try {
    const result = await apiGet("/api/resources/rooms");
    const select = document.getElementById("cabinet-room");

    if (result.success && select) {
      select.innerHTML = "";

      const placeholder = document.createElement("option");
      placeholder.value = "";
      placeholder.textContent = "选择机房";
      select.appendChild(placeholder);

      result.data.forEach((room) => {
        let roomTypeLower = room.room_type.toLowerCase();
        if (roomTypeLower === "data_center") {
          const option = document.createElement("option");
          option.value = room.id;
          option.textContent = room.name;
          select.appendChild(option);
        }
      });

      if (select.children.length === 1) {
        const noDataOption = document.createElement("option");
        noDataOption.value = "";
        noDataOption.textContent = "暂无机房数据";
        noDataOption.disabled = true;
        select.appendChild(noDataOption);
      }
    }
  } catch (error) {
  }
}

export async function loadCabinets() {
  try {
    const result = await apiGet("/api/resources/cabinets");
    return result.success ? result.data : [];
  } catch (error) {
    return [];
  }
}

export function formatMacAddress(mac) {
  if (!mac) return mac;
  const cleaned = mac.replace(/[^0-9A-Fa-f]/g, '');
  if (cleaned.length !== 12) return mac;
  return cleaned.toUpperCase().match(/.{4}/g).join('-');
}

export async function loadRoomNetworksForCabinet(roomId) {
  const inheritedNetworksContainer = document.getElementById("cabinet-inherited-networks");
  
  if (!roomId) {
    inheritedNetworksContainer.innerHTML = '<p class="text-muted">请先选择所属房间，将自动继承房间的网段配置</p>';
    return;
  }
  
  try {
    const roomResult = await apiGet(`/api/resources/rooms/${roomId}`);
    
    if (roomResult.success && roomResult.data) {
      const room = roomResult.data;
      
      if (room.networks && room.networks.length > 0) {
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
    inheritedNetworksContainer.innerHTML = '<p class="text-muted">加载房间网段配置失败，请重试</p>';
  }
}
