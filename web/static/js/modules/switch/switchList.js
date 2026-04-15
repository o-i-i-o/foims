import { apiGet, apiPost, apiPut, } from "../../utils/apiClient.js";
import { showToast, renderTable, handleDelete, handleError, appendPaginationToTable, } from "../../utils/ui.js";
import { elementCache } from "../../utils/helpers.js";
import { listState, SWITCH_PAGE_SIZE } from "./switchState.js";
import { t } from "../../utils/i18n.js";
let switchTableClickHandler = null;
export async function loadSwitchesData(searchTerm = "") {
    listState.currentSwitchPage = listState.currentSwitchPage || 1;
    try {
        const url = `/api/switches?page=${listState.currentSwitchPage}&page_size=${SWITCH_PAGE_SIZE}&search=${encodeURIComponent(searchTerm)}`;
        const result = await apiGet(url);
        const data = result.success ? result.data : { items: [], total: 0 };
        const switches = data?.items || (Array.isArray(data) ? data : []);
        const startIndex = (listState.currentSwitchPage - 1) * SWITCH_PAGE_SIZE;
        renderTable("#switches-table", {
            data: switches,
            columns: [
                { field: "id", render: (_v, _row, index) => String(startIndex + index + 1), className: "index-column" },
                { field: "name", render: (v) => v },
                { field: "device_type", render: (v) => v || "-" },
                { field: "ip_address", render: (v) => v },
                { field: "mac_address", render: (v) => v || "-" },
                { field: "model", render: (v) => v || "-" },
                { field: "vendor", render: (v) => v || "-" },
                { field: "location", render: (v) => v || "-" },
                {
                    field: "snmp_version",
                    render: (v, row) => {
                        const sw = row;
                        return `<span class="status-badge ${sw.snmp_community || sw.snmp_username ? "status-active" : "status-inactive"}">${v || "-"}</span>`;
                    },
                },
                { field: "parent_switch_name", render: (v) => v || "-" },
                {
                    field: "id",
                    render: (v, row) => {
                        const sw = row;
                        return `
              <button class="btn btn-sm btn-edit" data-id="${v}">${t("common.edit") || "编辑"}</button>
              <button class="btn btn-sm btn-secondary btn-switch-ports" data-switch-id="${v}" data-switch-name="${sw.name || ""}">${t("switch.ports") || "端口"}</button>
              <button class="btn btn-sm btn-secondary btn-switch-arp" data-switch-id="${v}">${t("switch.mac_table") || "MAC表"}</button>
              <button class="btn btn-sm btn-secondary btn-switch-lldp" data-switch-id="${v}">LLDP</button>
              <button class="btn btn-sm btn-delete" data-id="${v}">${t("common.delete") || "删除"}</button>
            `;
                    },
                },
            ],
            emptyMessage: t("switch.no_switch_data") || "暂无交换机数据",
        });
        if (data.total !== undefined) {
            appendPaginationToTable("#switches-table", data, (p) => {
                listState.currentSwitchPage = p;
                loadSwitchesData(searchTerm);
            });
        }
        bindSwitchButtonsEvents();
    }
    catch (error) {
        handleError(error, t("switch.load_switch_failed") || "加载交换机数据失败");
        renderTable("#switches-table", { data: [], columns: [], emptyMessage: t("common.load_failed") || "加载失败" });
    }
}
function bindSwitchButtonsEvents() {
    const table = elementCache.get("switches-table");
    if (!table)
        return;
    if (switchTableClickHandler) {
        table.removeEventListener("click", switchTableClickHandler);
    }
    switchTableClickHandler = async (e) => {
        const target = e.target;
        const id = target.dataset.id || target.dataset.switchId;
        if (target.classList.contains("btn-delete")) {
            if (id)
                deleteSwitch(id);
        }
        else if (target.classList.contains("btn-switch-ports")) {
            const switchName = target.dataset.switchName;
            import("./switchPort.js").then(module => {
                module.manageSwitchPorts(id, switchName || undefined);
            });
        }
        else if (target.classList.contains("btn-switch-arp")) {
            import("./switchMacLldp.js").then(module => {
                module.viewArpTable(id);
            });
        }
        else if (target.classList.contains("btn-switch-lldp")) {
            import("./switchMacLldp.js").then(module => {
                module.viewLldpNeighbors(id);
            });
        }
    };
    table.addEventListener("click", switchTableClickHandler);
}
export async function fetchSwitchById(id) {
    try {
        const result = await apiGet(`/api/switches/${id}`);
        if (result.success) {
            return result.data;
        }
        else {
            showToast(t("switch.fetch_switch_failed") || "获取交换机信息失败", "error");
            return null;
        }
    }
    catch (error) {
        handleError(error, t("switch.fetch_switch_failed") || "获取交换机信息失败");
        return null;
    }
}
export async function deleteSwitch(id) {
    await handleDelete(id, "/api/switches", t("switch.switch_deleted") || "交换机删除成功", () => loadSwitchesData());
    return true;
}
export async function submitSwitchForm(formData) {
    const id = formData.id;
    if (!formData.name) {
        showToast(t("switch.name_required") || "交换机名称不能为空", "warning");
        return false;
    }
    if (!formData.ips || formData.ips.length === 0) {
        showToast(t("switch.ip_required") || "请至少配置一个IP地址", "warning");
        return false;
    }
    try {
        let result;
        if (id) {
            result = await apiPut(`/api/switches/${id}`, formData);
        }
        else {
            result = await apiPost("/api/switches", formData);
        }
        if (result.success) {
            showToast(id ? t("switch.switch_updated") || "交换机更新成功" : t("switch.switch_added") || "交换机添加成功", "success");
            return true;
        }
        else {
            showToast(`${id ? t("common.update") || "更新" : t("common.add") || "添加"}${t("switch.switch") || "交换机"}${t("common.failed") || "失败"}：${result.message}`, "error");
            return false;
        }
    }
    catch (error) {
        handleError(error, t("switch.save_switch_failed") || "保存交换机失败");
        return false;
    }
}
//# sourceMappingURL=switchList.js.map