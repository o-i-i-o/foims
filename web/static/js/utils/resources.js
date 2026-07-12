import {
  apiGet,
} from "./apiClient.js";
import { t } from "./i18n.js";
import { escapeHtml } from "./helpers.js";

function extractItems(result) {
  if (!result.success || !result.data) return [];
  if (Array.isArray(result.data)) return result.data;
  if (result.data.items && Array.isArray(result.data.items)) return result.data.items;
  return [];
}

export async function loadNetworkTypeOptions(selectId = "network-type") {
  try {
    const result = await apiGet("/api/resources/network-regions?page_size=1000");
    const select = document.getElementById(selectId);

    if (!select) return;

    const currentValue = select.value;
    select.innerHTML = "";

    const items = extractItems(result);
    if (items.length > 0) {
      items.forEach((networkType) => {
        const option = document.createElement("option");
        option.value = networkType.id;
        option.textContent = networkType.name;
        select.appendChild(option);
      });
    } else {
      const option = document.createElement("option");
      option.value = "";
      option.textContent = t('network.add_region_first');
      option.disabled = true;
      select.appendChild(option);
    }

    if (currentValue) {
      select.value = currentValue;
    }
  } catch (error) {
    console.error("加载网络区域选项失败:", error);
    const select = document.getElementById(selectId);
    if (select) {
      select.innerHTML = "";
      const option = document.createElement("option");
      option.value = "";
      option.textContent = t('common.load_failed');
      option.disabled = true;
      select.appendChild(option);
    }
  }
}

export async function loadRoomsForSelect(selectId = "workstation-room", options = {}) {
  const { onlyOffice = false, includeVisualization = true } = options;
  try {
    const result = await apiGet("/api/resources/rooms?page_size=1000");
    const select = document.getElementById(selectId);
    const visualizationSelect = includeVisualization ? document.getElementById("room-select") : null;

    const rooms = extractItems(result);

    if (select) {
      select.innerHTML = "";

      const placeholder = document.createElement("option");
      placeholder.value = "";
      placeholder.textContent = onlyOffice ? t('room.select_office') : t('room.select_room');
      select.appendChild(placeholder);

      rooms.forEach((room) => {
        if (onlyOffice) {
          const roomTypeLower = room.room_type ? room.room_type.toLowerCase() : '';
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
        noDataOption.textContent = onlyOffice ? t('room.no_office_data') : t('room.no_room_data');
        noDataOption.disabled = true;
        select.appendChild(noDataOption);
      }
    }

    if (visualizationSelect) {
      visualizationSelect.innerHTML = "";

      rooms.forEach((room) => {
        const roomTypeLower = room.room_type ? room.room_type.toLowerCase() : '';
        if (roomTypeLower !== 'data_center') {
          const option = document.createElement("option");
          option.value = room.id;
          option.textContent = room.name;
          visualizationSelect.appendChild(option);
        }
      });
    }
  } catch (error) {
    console.error("加载房间选项失败:", error);
    if (select) {
      select.innerHTML = "";
      const noDataOption = document.createElement("option");
      noDataOption.value = "";
      noDataOption.textContent = t('common.load_failed');
      noDataOption.disabled = true;
      select.appendChild(noDataOption);
    }
  }
}

export async function loadDataCenterRoomsForSelect(selectId = "cabinet-room") {
  try {
    const result = await apiGet("/api/resources/rooms?page_size=1000");
    const select = document.getElementById(selectId);

    const rooms = extractItems(result);
    if (select) {
      select.innerHTML = "";

      const placeholder = document.createElement("option");
      placeholder.value = "";
      placeholder.textContent = t('room.select_datacenter');
      select.appendChild(placeholder);

      rooms.forEach((room) => {
        const roomTypeLower = room.room_type ? room.room_type.toLowerCase() : '';
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
        noDataOption.textContent = t('room.no_datacenter_data');
        noDataOption.disabled = true;
        select.appendChild(noDataOption);
      }
    }
  } catch (error) {
    console.error("加载机房数据失败:", error);
    const select = document.getElementById(selectId);
    if (select) {
      select.innerHTML = "";
      const noDataOption = document.createElement("option");
      noDataOption.value = "";
      noDataOption.textContent = t('common.load_failed');
      noDataOption.disabled = true;
      select.appendChild(noDataOption);
    }
  }
}

export async function loadCabinets() {
  try {
    const result = await apiGet("/api/resources/cabinets?page_size=1000");
    return extractItems(result);
  } catch (error) {
    return [];
  }
}

export async function loadRoomNetworksForCabinet(roomId, containerId = "cabinet-inherited-networks") {
  const inheritedNetworksContainer = document.getElementById(containerId);
  
  if (!inheritedNetworksContainer) return;
  
  if (!roomId) {
    inheritedNetworksContainer.innerHTML = '<p class="text-muted">' + t('cabinet.select_room_first') + '</p>';
    return;
  }
  
  try {
    const roomResult = await apiGet(`/api/resources/rooms/${roomId}`);
    
    if (roomResult.success && roomResult.data) {
      const room = roomResult.data;
      const networks = room.networks || [];
      
      if (networks.length > 0) {
        let networksHtml = '<ul class="list-group">';
        networks.forEach(network => {
          networksHtml += `
            <li class="list-group-item">
              <span class="font-weight-bold">${escapeHtml(network.name)}</span>
              <span class="text-muted">(${escapeHtml(network.network_region)})</span>
              ${network.ipv4_cidr ? `<div class="small">IPv4: ${escapeHtml(network.ipv4_cidr)}</div>` : ''}
              ${network.ipv6_cidr ? `<div class="small">IPv6: ${escapeHtml(network.ipv6_cidr)}</div>` : ''}
            </li>
          `;
        });
        networksHtml += '</ul>';
        inheritedNetworksContainer.innerHTML = networksHtml;
      } else {
        inheritedNetworksContainer.innerHTML = '<p class="text-muted">' + t('cabinet.no_network_config') + '</p>';
      }
    } else {
      inheritedNetworksContainer.innerHTML = '<p class="text-muted">' + t('common.load_failed_retry') + '</p>';
    }
  } catch (error) {
    console.error("加载房间网段配置失败:", error);
    inheritedNetworksContainer.innerHTML = '<p class="text-muted">' + t('common.load_failed_retry') + '</p>';
  }
}

export async function loadOrgsForSelect(selectId = "room-org-id") {
  try {
    const result = await apiGet("/api/resources/organizations/tree");
    const select = document.getElementById(selectId);

    if (!select) return;

    const currentValue = select.value;
    select.innerHTML = `<option value="">${t('organization.select_org') || '选择组织节点'}</option>`;

    if (result.success && result.data) {
      const flatOrgs = flattenOrgTree(result.data);
      flatOrgs.forEach(org => {
        const option = document.createElement("option");
        option.value = org.id;
        const indent = "\u00A0\u00A0\u00A0\u00A0".repeat(org.depth);
        option.textContent = `${indent}${org.name} (${org.org_type})`;
        select.appendChild(option);
      });
    }

    if (currentValue) {
      select.value = currentValue;
    }
  } catch (error) {
    console.error("加载组织选项失败:", error);
  }
}

function flattenOrgTree(nodes, depth = 0) {
  const result = [];
  for (const node of nodes) {
    result.push({ id: node.id, name: node.name, org_type: node.org_type, depth });
    if (node.children && node.children.length > 0) {
      result.push(...flattenOrgTree(node.children, depth + 1));
    }
  }
  return result;
}

export async function loadNetOutletsForSelect(selectId, roomId = null) {
  try {
    const url = roomId
      ? `/api/resources/net-outlets?room_id=${roomId}&page_size=1000`
      : '/api/resources/net-outlets?page_size=1000';
    const result = await apiGet(url);
    const select = document.getElementById(selectId);

    if (!select) return;

    const currentValue = select.value;
    select.innerHTML = `<option value="">${t('net_outlet.select_net_outlet') || '选择网络端口'}</option>`;

    const items = extractItems(result);
    items.forEach(outlet => {
      const option = document.createElement("option");
      option.value = outlet.id;
      option.textContent = outlet.name;
      select.appendChild(option);
    });

    if (currentValue) {
      select.value = currentValue;
    }
  } catch (error) {
    console.error("加载网络端口选项失败:", error);
  }
}

export async function loadDeviceTemplatesForSelect(selectId) {
  try {
    const result = await apiGet("/api/resources/device-templates");
    const select = document.getElementById(selectId);

    if (!select) return;

    const currentValue = select.value;
    select.innerHTML = `<option value="">${t('device.select_template') || '选择模板'}</option>`;

    const items = extractItems(result);
    items.forEach(tmpl => {
      const option = document.createElement("option");
      option.value = tmpl.id;
      option.textContent = tmpl.name;
      select.appendChild(option);
    });

    if (currentValue) {
      select.value = currentValue;
    }
  } catch (error) {
    console.error("加载设备模板选项失败:", error);
  }
}

export async function loadWorkstationsForSelect(selectId, roomId = null) {
  try {
    const url = roomId
      ? `/api/resources/workstations?room_id=${roomId}&page_size=1000`
      : '/api/resources/workstations?page_size=1000';
    const result = await apiGet(url);
    const select = document.getElementById(selectId);

    if (!select) return;

    const currentValue = select.value;
    select.innerHTML = `<option value="">${t('device.select_workstation') || '选择工位'}</option>`;

    const items = extractItems(result);
    items.forEach(ws => {
      const option = document.createElement("option");
      option.value = ws.id;
      option.textContent = ws.name;
      select.appendChild(option);
    });

    if (currentValue) {
      select.value = currentValue;
    }
  } catch (error) {
    console.error("加载工位选项失败:", error);
  }
}

export async function loadPositionsForSelect(selectId, cabinetId = null) {
  try {
    const url = cabinetId
      ? `/api/resources/positions?cabinet_id=${cabinetId}&page_size=1000`
      : '/api/resources/positions?page_size=1000';
    const result = await apiGet(url);
    const select = document.getElementById(selectId);

    if (!select) return;

    const currentValue = select.value;
    select.innerHTML = `<option value="">${t('device.select_position') || '选择机位'}</option>`;

    const items = extractItems(result);
    items.forEach(pos => {
      const option = document.createElement("option");
      option.value = pos.id;
      option.textContent = pos.name;
      select.appendChild(option);
    });

    if (currentValue) {
      select.value = currentValue;
    }
  } catch (error) {
    console.error("加载机位选项失败:", error);
  }
}
