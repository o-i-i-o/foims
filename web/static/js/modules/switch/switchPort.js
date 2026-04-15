// Switch port management
import { apiGet, apiDelete } from "../../utils/apiClient.js";
import { showToast, appendPaginationToTable, escapeHtml } from "../../utils/ui.js";
import { SWITCH_PORT_PAGE_SIZE } from "./switchState.js";
export async function loadSwitchPortsData(switchId, page = 1) {
    try {
        const result = await apiGet(`/api/switches/${switchId}/ports?page=${page}&page_size=${SWITCH_PORT_PAGE_SIZE}`);
        const tbody = document.querySelector("#switch-ports-table tbody");
        if (!tbody)
            return;
        tbody.innerHTML = "";
        const data = result.success ? result.data : { items: [], total: 0 };
        const ports = data.items || [];
        if (ports.length === 0) {
            tbody.innerHTML = '<tr class="empty-row"><td colspan="7" class="text-center">暂无端口数据</td></tr>';
            return;
        }
        const startIndex = (page - 1) * SWITCH_PORT_PAGE_SIZE;
        ports.forEach((port, index) => {
            const row = document.createElement("tr");
            row.innerHTML = `
        <td class="index-column">${startIndex + index + 1}</td>
        <td>${escapeHtml(port.port_number)}</td>
        <td>${escapeHtml(port.name || "-")}</td>
        <td><span class="status-badge ${port.status === "up" ? "status-active" : "status-inactive"}">${port.status}</span></td>
        <td>${escapeHtml(port.speed || "-")}</td>
        <td>${escapeHtml(port.connected_device || "-")}</td>
        <td>
          <button class="btn btn-sm btn-edit" data-id="${port.id}">编辑</button>
          <button class="btn btn-sm btn-delete" data-id="${port.id}">删除</button>
        </td>
      `;
            tbody.appendChild(row);
        });
        if (data.total !== undefined) {
            appendPaginationToTable("#switch-ports-table", data, (p) => loadSwitchPortsData(switchId, p));
        }
    }
    catch (error) {
        console.error("加载端口数据失败:", error);
    }
}
export async function loadSwitchPortsBySwitchId(switchId) {
    try {
        const result = await apiGet(`/api/switches/${switchId}/ports?page_size=1000`);
        if (result.success) {
            const data = result.data;
            return data.items || [];
        }
        return [];
    }
    catch (error) {
        console.error("加载端口数据失败:", error);
        return [];
    }
}
export function manageSwitchPorts(switchId) {
    // TODO: Implement port management
    console.log("Manage ports for switch:", switchId);
}
export function openSwitchPortModal(switchId) {
    // TODO: Implement port modal
    console.log("Open port modal for switch:", switchId);
}
export async function editSwitchPort(portId) {
    // TODO: Implement port editing
    console.log("Edit port:", portId);
}
export async function deleteSwitchPort(portId) {
    if (!confirm("确定要删除此端口吗？"))
        return false;
    try {
        const result = await apiDelete(`/api/switch-ports/${portId}`);
        if (result.success) {
            showToast("端口删除成功", "success");
            return true;
        }
        else {
            showToast("删除失败: " + result.message, "error");
            return false;
        }
    }
    catch (error) {
        console.error("删除端口失败:", error);
        showToast("删除失败", "error");
        return false;
    }
}
export async function submitSwitchPortForm() {
    // TODO: Implement form submission
    showToast("端口保存功能暂未实现", "warning");
    return false;
}
export function groupPorts(ports) {
    // TODO: Implement port grouping
    return { default: ports };
}
export function showPortGroupsModal(switchId) {
    // TODO: Implement port groups modal
    console.log("Show port groups for switch:", switchId);
}
export function extractPortNumber(portName) {
    const match = portName.match(/\d+/);
    return match ? parseInt(match[0]) : 0;
}
export function extractPortLastNumber(portName) {
    const matches = portName.match(/\d+/g);
    return matches && matches.length > 0 ? parseInt(matches[matches.length - 1]) : 0;
}
//# sourceMappingURL=switchPort.js.map