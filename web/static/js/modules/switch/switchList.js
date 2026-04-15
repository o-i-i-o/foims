// Switch list management
import { showToast, appendPaginationToTable, escapeHtml } from "../../utils/ui.js";
import { switchManager } from "../../utils/managers.js";
import { SWITCH_PAGE_SIZE } from "./switchState.js";
export async function loadSwitchesData(page = 1, searchTerm = "") {
    try {
        const result = await switchManager.list({
            page,
            pageSize: SWITCH_PAGE_SIZE,
            search: searchTerm,
        });
        if (!result)
            return;
        const data = result.data;
        const switches = data.items || [];
        const tableBody = document.querySelector("#switches-table tbody");
        if (!tableBody)
            return;
        if (switches.length === 0) {
            tableBody.innerHTML = '<tr class="empty-row"><td colspan="7" class="text-center">暂无交换机数据</td></tr>';
            return;
        }
        tableBody.innerHTML = switches.map((sw) => `
      <tr>
        <td>${escapeHtml(sw.name)}</td>
        <td>${escapeHtml(sw.ip_address)}</td>
        <td>${escapeHtml(sw.vendor)}</td>
        <td>${escapeHtml(sw.model)}</td>
        <td><span class="status-badge ${sw.status === "active" ? "status-active" : "status-inactive"}">${sw.status || "未知"}</span></td>
        <td>${sw.active_ports_count || 0}/${sw.ports_count || 0}</td>
        <td>
          <button class="btn btn-sm btn-edit" data-id="${sw.id}">编辑</button>
          <button class="btn btn-sm btn-delete" data-id="${sw.id}">删除</button>
        </td>
      </tr>
    `).join("");
        if (data.total !== undefined) {
            appendPaginationToTable("#switches-table", data, (p) => loadSwitchesData(p, searchTerm));
        }
    }
    catch (error) {
        console.error("获取交换机数据失败:", error);
        showToast("加载交换机数据失败", "error");
    }
}
export async function fetchSwitchById(id) {
    try {
        const sw = await switchManager.get(id);
        return sw;
    }
    catch (error) {
        console.error("获取交换机数据失败:", error);
        return null;
    }
}
export async function deleteSwitch(id) {
    const result = await switchManager.delete(id, { confirmMessage: "确定要删除此交换机吗？" });
    if (result.success) {
        await loadSwitchesData();
        return true;
    }
    return false;
}
export async function submitSwitchForm() {
    try {
        showToast("交换机保存功能暂未实现", "warning");
        return false;
    }
    catch (error) {
        console.error("保存交换机失败:", error);
        showToast("保存失败", "error");
        return false;
    }
}
//# sourceMappingURL=switchList.js.map