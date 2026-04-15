// Switch list management
import { showToast, appendPaginationToTable, escapeHtml } from "../../utils/ui.js";
import { switchManager } from "../../utils/managers.js";
import { SWITCH_PAGE_SIZE } from "./switchState.js";

interface Switch {
  id: string;
  name: string;
  device_type?: string;
  ip_address: string;
  mac_address?: string;
  vendor?: string;
  model?: string;
  location?: string;
  snmp_enabled?: boolean;
  snmp_version?: string;
  parent_switch_id?: string;
  parent_switch_name?: string;
  description?: string;
  status?: string;
  ports_count?: number;
  active_ports_count?: number;
  created_at?: string;
}

export async function loadSwitchesData(page = 1, searchTerm = ""): Promise<void> {
  try {
    const result = await switchManager.list({
      page,
      pageSize: SWITCH_PAGE_SIZE,
      search: searchTerm,
    });

    if (!result) return;

    const data = result.data as { items?: Switch[]; total?: number };
    const switches = data.items || [];
    const tableBody = document.querySelector("#switches-table tbody") as HTMLTableSectionElement | null;

    if (!tableBody) return;

    if (switches.length === 0) {
      tableBody.innerHTML = '<tr class="empty-row"><td colspan="11" class="text-center">暂无交换机数据</td></tr>';
      return;
    }

    const startIndex = (page - 1) * SWITCH_PAGE_SIZE;

    tableBody.innerHTML = switches.map((sw, index) => `
      <tr>
        <td class="index-column">${startIndex + index + 1}</td>
        <td>${escapeHtml(sw.name)}</td>
        <td>${escapeHtml(sw.device_type || "-")}</td>
        <td>${escapeHtml(sw.ip_address)}</td>
        <td>${escapeHtml(sw.mac_address || "-")}</td>
        <td>${escapeHtml(sw.model || "-")}</td>
        <td>${escapeHtml(sw.vendor || "-")}</td>
        <td>${escapeHtml(sw.location || "-")}</td>
        <td>${sw.snmp_enabled ? `<span class="status-badge status-active">${sw.snmp_version || "v2c"}</span>` : '<span class="status-badge status-inactive">禁用</span>'}</td>
        <td>${sw.parent_switch_name ? escapeHtml(sw.parent_switch_name) : "-"}</td>
        <td>
          <button class="btn btn-sm btn-edit" data-id="${sw.id}">编辑</button>
          <button class="btn btn-sm btn-delete" data-id="${sw.id}">删除</button>
        </td>
      </tr>
    `).join("");

    if (data.total !== undefined) {
      appendPaginationToTable("#switches-table", data, (p: number) => loadSwitchesData(p, searchTerm));
    }
  } catch (error) {
    console.error("获取交换机数据失败:", error);
    showToast("加载交换机数据失败", "error");
  }
}

export async function fetchSwitchById(id: string): Promise<Switch | null> {
  try {
    const sw = await switchManager.get(id);
    return sw as unknown as Switch | null;
  } catch (error) {
    console.error("获取交换机数据失败:", error);
    return null;
  }
}

export async function deleteSwitch(id: string | number): Promise<boolean> {
  const result = await switchManager.delete(id, { confirmMessage: "确定要删除此交换机吗？" });
  if (result.success) {
    await loadSwitchesData();
    return true;
  }
  return false;
}

export async function submitSwitchForm(): Promise<boolean> {
  try {
    showToast("交换机保存功能暂未实现", "warning");
    return false;
  } catch (error) {
    console.error("保存交换机失败:", error);
    showToast("保存失败", "error");
    return false;
  }
}
