import { apiGet } from "./apiClient.js";
function extractItems(result) {
    if (!result.success || !result.data)
        return [];
    if (Array.isArray(result.data))
        return result.data;
    if ("items" in result.data && Array.isArray(result.data.items))
        return result.data.items;
    return [];
}
export async function loadNetworkTypeOptions() {
    try {
        const result = await apiGet("/api/resources/network-regions?page_size=1000");
        const select = document.getElementById("network-type");
        if (!select)
            return;
        const currentValue = select.value;
        select.innerHTML = "";
        const items = extractItems(result);
        if (items.length > 0) {
            items.forEach((networkType) => {
                const option = document.createElement("option");
                option.value = String(networkType.id);
                option.textContent = networkType.name;
                select.appendChild(option);
            });
        }
        else {
            const option = document.createElement("option");
            option.value = "";
            option.textContent = "请先添加网络区域";
            option.disabled = true;
            select.appendChild(option);
        }
        if (currentValue) {
            select.value = currentValue;
        }
    }
    catch (error) {
        console.error("加载网络区域选项失败:", error);
        const select = document.getElementById("network-type");
        if (select) {
            select.innerHTML = "";
            const option = document.createElement("option");
            option.value = "";
            option.textContent = "加载失败";
            option.disabled = true;
            select.appendChild(option);
        }
    }
}
export async function loadRoomsForSelect(onlyOffice = false) {
    try {
        const result = await apiGet("/api/resources/rooms?page_size=1000");
        const select = document.getElementById("workstation-room");
        const visualizationSelect = document.getElementById("room-select");
        const rooms = extractItems(result);
        if (select) {
            select.innerHTML = "";
            const placeholder = document.createElement("option");
            placeholder.value = "";
            placeholder.textContent = onlyOffice ? "选择办公室" : "选择房间";
            select.appendChild(placeholder);
            rooms.forEach((room) => {
                if (onlyOffice) {
                    const roomTypeLower = room.room_type ? room.room_type.toLowerCase() : "";
                    if (roomTypeLower === "office") {
                        const option = document.createElement("option");
                        option.value = String(room.id);
                        option.textContent = room.name;
                        select.appendChild(option);
                    }
                }
                else {
                    const option = document.createElement("option");
                    option.value = String(room.id);
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
            rooms.forEach((room) => {
                const roomTypeLower = room.room_type ? room.room_type.toLowerCase() : "";
                if (roomTypeLower !== "data_center") {
                    const option = document.createElement("option");
                    option.value = String(room.id);
                    option.textContent = room.name;
                    visualizationSelect.appendChild(option);
                }
            });
        }
    }
    catch (error) {
        console.error("加载房间选项失败:", error);
        const select = document.getElementById("workstation-room");
        if (select) {
            select.innerHTML = "";
            const noDataOption = document.createElement("option");
            noDataOption.value = "";
            noDataOption.textContent = "加载失败";
            noDataOption.disabled = true;
            select.appendChild(noDataOption);
        }
    }
}
export async function loadNetworkRegionsForSelect(selectElement, autoSelectFirst = false) {
    try {
        const result = await apiGet("/api/resources/network-regions?page_size=1000");
        const items = extractItems(result);
        if (selectElement) {
            selectElement.innerHTML = "";
            items.forEach((region) => {
                const option = document.createElement("option");
                option.value = String(region.id);
                option.textContent = region.name;
                selectElement.appendChild(option);
            });
            if (autoSelectFirst && items.length > 0) {
                selectElement.value = String(items[0].id);
            }
            return items;
        }
        return [];
    }
    catch (error) {
        console.error("加载网络区域选项失败:", error);
        return [];
    }
}
export async function loadDataCenterRoomsForSelect() {
    try {
        const result = await apiGet("/api/resources/rooms?page_size=1000");
        const select = document.getElementById("cabinet-room");
        const rooms = extractItems(result);
        if (select) {
            select.innerHTML = "";
            const placeholder = document.createElement("option");
            placeholder.value = "";
            placeholder.textContent = "选择机房";
            select.appendChild(placeholder);
            rooms.forEach((room) => {
                const roomTypeLower = room.room_type ? room.room_type.toLowerCase() : "";
                if (roomTypeLower === "data_center") {
                    const option = document.createElement("option");
                    option.value = String(room.id);
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
    }
    catch (error) {
        console.error("加载机房数据失败:", error);
        const select = document.getElementById("cabinet-room");
        if (select) {
            select.innerHTML = "";
            const noDataOption = document.createElement("option");
            noDataOption.value = "";
            noDataOption.textContent = "加载失败";
            noDataOption.disabled = true;
            select.appendChild(noDataOption);
        }
    }
}
export async function loadCabinets() {
    try {
        const result = await apiGet("/api/resources/cabinets?page_size=1000");
        return extractItems(result);
    }
    catch (_error) {
        return [];
    }
}
export function formatMacAddress(mac) {
    if (!mac)
        return mac;
    const cleaned = mac.replace(/[^0-9A-Fa-f]/g, "");
    if (cleaned.length !== 12)
        return mac;
    return cleaned.toUpperCase().match(/.{4}/g).join("-");
}
export async function loadRoomNetworksForCabinet(roomId) {
    const inheritedNetworksContainer = document.getElementById("cabinet-inherited-networks");
    if (!roomId) {
        if (inheritedNetworksContainer) {
            inheritedNetworksContainer.innerHTML = '<p class="text-muted">请先选择所属房间，将自动继承房间的网段配置</p>';
        }
        return;
    }
    try {
        const roomResult = await apiGet(`/api/resources/rooms/${roomId}`);
        if (roomResult.success && roomResult.data) {
            const room = roomResult.data;
            const networks = room.networks || [];
            if (networks.length > 0 && inheritedNetworksContainer) {
                let networksHtml = '<ul class="list-group">';
                networks.forEach(network => {
                    networksHtml += `
            <li class="list-group-item">
              <span class="font-weight-bold">${network.name}</span>
              <span class="text-muted">(${network.network_region})</span>
              ${network.ipv4_cidr ? `<div class="small">IPv4: ${network.ipv4_cidr}</div>` : ""}
              ${network.ipv6_cidr ? `<div class="small">IPv6: ${network.ipv6_cidr}</div>` : ""}
            </li>
          `;
                });
                networksHtml += "</ul>";
                inheritedNetworksContainer.innerHTML = networksHtml;
            }
            else if (inheritedNetworksContainer) {
                inheritedNetworksContainer.innerHTML = '<p class="text-muted">所选房间未配置网段，请先为房间添加网段配置</p>';
            }
        }
        else if (inheritedNetworksContainer) {
            inheritedNetworksContainer.innerHTML = '<p class="text-muted">获取房间信息失败，请重试</p>';
        }
    }
    catch (error) {
        console.error("加载房间网段配置失败:", error);
        if (inheritedNetworksContainer) {
            inheritedNetworksContainer.innerHTML = '<p class="text-muted">加载房间网段配置失败，请重试</p>';
        }
    }
}
export async function loadRoomNetworksForCabinetBySelect(roomSelect) {
    const roomId = roomSelect.value;
    await loadRoomNetworksForCabinet(roomId);
}
//# sourceMappingURL=resources.js.map