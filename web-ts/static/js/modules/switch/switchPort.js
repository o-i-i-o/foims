import { apiGet, apiPost, apiPut, apiDelete, } from "../../utils/apiClient.js";
import { showToast, renderTable, getElementValue, handleError, appendPaginationToTable, } from "../../utils/ui.js";
import { openModal, closeModal } from "../../utils/modal.js";
import { getCurrentSwitchId, getCurrentSwitchName, setCurrentSwitchId, setCurrentSwitchName, SWITCH_PORT_PAGE_SIZE, } from "./switchState.js";
import { syncPortsFromSnmp } from "./switchSnmp.js";
import { elementCache } from "../../utils/helpers.js";
import { t } from "../../utils/i18n.js";
const PORT_TYPE_PATTERNS = [
    { typeName: "Bridge-Aggregation", regex: /(Bridge-Aggregation)/, subGroup: false },
    { typeName: "Hundred-GigabitEthernet", regex: /(Hundred-?GigabitEthernet)(\d+)/i, subGroup: true },
    { typeName: "Forty-GigabitEthernet", regex: /(Forty-?GigabitEthernet)(\d+)/i, subGroup: true },
    { typeName: "Ten-GigabitEthernet", regex: /(Ten-GigabitEthernet)(\d+)/, subGroup: true },
    { typeName: "TenGigabitEthernet", regex: /(TenGigabitEthernet)(\d+)/, subGroup: true },
    { typeName: "XGigabitEthernet", regex: /(XGigabitEthernet)(\d+)/, subGroup: true },
    { typeName: "M-GigabitEthernet", regex: /(M-GigabitEthernet)(\d+)/, subGroup: true },
    { typeName: "GigabitEthernet", regex: /(GigabitEthernet)(\d+)/, subGroup: true },
    { typeName: "Vlan-interface", regex: /(Vlan-interface)/, subGroup: false },
    { typeName: "FastEthernet", regex: /(FastEthernet)(\d+)/, subGroup: true },
    { typeName: "Ethernet", regex: /(Ethernet)(\d+)/, subGroup: true },
];
export async function loadSwitchPortsData(page = 1, searchTerm = "") {
    const currentSwitchName = getCurrentSwitchName();
    try {
        const url = `/api/switches/ports?page=${page}&page_size=${SWITCH_PORT_PAGE_SIZE}&search=${encodeURIComponent(searchTerm)}`;
        const result = await apiGet(url);
        const data = result.success ? result.data : { items: [], total: 0 };
        const ports = data?.items || (Array.isArray(data) ? data : []);
        renderTable("#switch-ports-table", {
            data: ports,
            columns: [
                { field: "switch_name", render: (v, row) => v ? `${v} (${row.switch_ip || ""})` : currentSwitchName },
                { field: "port_number", render: (v) => String(v) },
                { field: "port_name", render: (v) => v || "-" },
                { field: "port_type", render: (v) => String(v) },
                { field: "vlan_id", render: (v) => v != null ? String(v) : "-" },
                { field: "status", render: (v) => `<span class="status-badge ${v === "up" ? "status-active" : "status-inactive"}">${v}</span>` },
                { field: "speed", render: (v) => v != null ? String(v) : "-" },
                {
                    field: "id",
                    render: (v) => `
            <button class="btn btn-sm btn-edit switch-port-edit" data-id="${v}">${t("common.edit") || "编辑"}</button>
            <button class="btn btn-sm btn-delete switch-port-delete" data-id="${v}">${t("common.delete") || "删除"}</button>
          `,
                },
            ],
            emptyMessage: t("switch.no_port_data") || "暂无端口数据",
        });
        if (data.total !== undefined) {
            appendPaginationToTable("#switch-ports-table", data, (p) => loadSwitchPortsData(p, searchTerm));
        }
    }
    catch (error) {
        handleError(error, t("switch.load_port_failed") || "加载端口数据失败");
        renderTable("#switch-ports-table", { data: [], columns: [], emptyMessage: t("common.load_failed") || "加载失败" });
    }
}
export async function loadSwitchPortsBySwitchId(switchId) {
    try {
        const result = await apiGet(`/api/switches/${switchId}/ports?page_size=1000`);
        const searchContainer = document.querySelector("#switch-ports-list-tab .search-container");
        if (!searchContainer) {
            renderTable("#switch-ports-table", {
                data: result.success ? (Array.isArray(result.data) ? result.data : result.data?.items || []) : [],
                columns: [
                    { field: "port_number", render: (v) => String(v) },
                    { field: "port_name", render: (v) => v || "-" },
                    { field: "port_type", render: (v) => String(v) },
                    { field: "vlan_id", render: (v) => v != null ? String(v) : "-" },
                    { field: "status", render: (v) => `<span class="status-badge ${v === "up" ? "status-active" : "status-inactive"}">${v}</span>` },
                    { field: "speed", render: (v) => v != null ? String(v) : "-" },
                    {
                        field: "id",
                        render: (v) => `
              <button class="btn btn-sm btn-edit switch-port-edit" data-id="${v}">${t("common.edit") || "编辑"}</button>
              <button class="btn btn-sm btn-delete switch-port-delete" data-id="${v}">${t("common.delete") || "删除"}</button>
            `,
                    },
                ],
                emptyMessage: t("switch.no_port_data") || "暂无端口数据",
            });
            return [];
        }
        const actionButtonsContainer = searchContainer.parentElement?.querySelector(".action-buttons") ??
            (() => {
                const container = document.createElement("div");
                container.className = "action-buttons";
                searchContainer.parentElement?.appendChild(container);
                return container;
            })();
        let addBtn = elementCache.get("add-switch-port-btn");
        if (!addBtn) {
            addBtn = document.createElement("button");
            addBtn.id = "add-switch-port-btn";
            addBtn.className = "btn btn-primary btn-sm";
            addBtn.textContent = t("switch.add_port") || "添加端口";
            addBtn.addEventListener("click", () => openSwitchPortModal(null, getCurrentSwitchId()));
            actionButtonsContainer.appendChild(addBtn);
        }
        let snmpPortsBtn = elementCache.get("sync-snmp-ports-btn");
        if (!snmpPortsBtn) {
            snmpPortsBtn = document.createElement("button");
            snmpPortsBtn.id = "sync-snmp-ports-btn";
            snmpPortsBtn.className = "btn btn-secondary btn-sm";
            snmpPortsBtn.textContent = t("switch.sync_snmp_ports") || "从SNMP获取端口";
            snmpPortsBtn.addEventListener("click", () => {
                const sid = getCurrentSwitchId();
                if (sid)
                    syncPortsFromSnmp(sid);
            });
            actionButtonsContainer.appendChild(snmpPortsBtn);
        }
        const ports = result.success
            ? (Array.isArray(result.data) ? result.data : result.data?.items || [])
            : [];
        renderTable("#switch-ports-table", {
            data: ports,
            columns: [
                { field: "port_number", render: (v) => String(v) },
                { field: "port_name", render: (v) => v || "-" },
                { field: "port_type", render: (v) => String(v) },
                { field: "vlan_id", render: (v) => v != null ? String(v) : "-" },
                { field: "status", render: (v) => `<span class="status-badge ${v === "up" ? "status-active" : "status-inactive"}">${v}</span>` },
                { field: "speed", render: (v) => v != null ? String(v) : "-" },
                {
                    field: "id",
                    render: (v) => `
            <button class="btn btn-sm btn-edit switch-port-edit" data-id="${v}">${t("common.edit") || "编辑"}</button>
            <button class="btn btn-sm btn-delete switch-port-delete" data-id="${v}">${t("common.delete") || "删除"}</button>
          `,
                },
            ],
            emptyMessage: t("switch.no_port_data") || "暂无端口数据",
        });
        return ports;
    }
    catch (error) {
        renderTable("#switch-ports-table", { data: [], columns: [], emptyMessage: t("common.load_failed") || "加载失败" });
        return [];
    }
}
export async function manageSwitchPorts(switchId, switchName) {
    setCurrentSwitchId(switchId);
    setCurrentSwitchName(switchName || "");
    try {
        const switchResult = await apiGet(`/api/switches/${switchId}`);
        if (!switchResult.success) {
            showToast(t("switch.fetch_info_failed") || "获取交换机信息失败，请检查网络连接", "error");
            return;
        }
        const switchData = switchResult.data;
        const hasSnmpConfig = switchData.snmp_community || switchData.snmp_username;
        if (!hasSnmpConfig) {
            showToast(t("switch.no_snmp_config") || "该交换机未配置SNMP信息，无法获取端口数据", "warning");
        }
        const portsResult = await apiGet(`/api/switches/${switchId}/ports?page_size=1000`);
        if (!portsResult.success) {
            showToast(t("switch.fetch_ports_failed") || "获取端口数据失败", "error");
            return;
        }
        let ports = [];
        if (portsResult.data) {
            if (Array.isArray(portsResult.data)) {
                ports = portsResult.data;
            }
            else if (portsResult.data.items && Array.isArray(portsResult.data.items)) {
                ports = portsResult.data.items;
            }
        }
        const portGroups = groupPorts(ports);
        showPortGroupsModal(switchName || "", portGroups, switchId);
    }
    catch (error) {
        console.error("管理交换机端口失败:", error);
        showToast(t("common.operation_failed") || "操作失败，请重试", "error");
    }
}
export function groupPorts(ports) {
    const MIN_GROUP_SIZE = 3;
    const rawGroups = {};
    ports.forEach(port => {
        const portNumber = port.port_number;
        let groupKey = t("switch.other") || "其他";
        for (const { regex, subGroup } of PORT_TYPE_PATTERNS) {
            const match = portNumber.match(regex);
            if (match) {
                if (subGroup && match[2]) {
                    groupKey = `${match[1]}${match[2]}`;
                }
                else {
                    groupKey = match[1];
                }
                break;
            }
        }
        if (!rawGroups[groupKey]) {
            rawGroups[groupKey] = [];
        }
        rawGroups[groupKey].push(port);
    });
    const finalGroups = {};
    const smallGroupPorts = [];
    Object.entries(rawGroups).forEach(([groupName, groupPortsList]) => {
        if (groupPortsList.length >= MIN_GROUP_SIZE) {
            finalGroups[groupName] = groupPortsList;
        }
        else {
            smallGroupPorts.push(...groupPortsList);
        }
    });
    if (smallGroupPorts.length > 0) {
        const otherKey = t("switch.other") || "其他";
        if (finalGroups[otherKey]) {
            finalGroups[otherKey].push(...smallGroupPorts);
        }
        else {
            finalGroups[otherKey] = smallGroupPorts;
        }
    }
    return finalGroups;
}
export function showPortGroupsModal(switchName, portGroups, switchId) {
    openModal("switch-ports-group-modal");
    const modal = elementCache.get("switch-ports-group-modal");
    const title = elementCache.get("switch-ports-group-modal-title");
    const container = document.querySelector(".port-groups-container");
    const addPortBtn = elementCache.get("add-port-btn");
    if (!modal || !title || !container || !addPortBtn) {
        console.error("端口分组模态框相关DOM元素未找到");
        return;
    }
    title.textContent = `${switchName} - ${t("switch.port_groups") || "端口分组显示"}`;
    container.innerHTML = "";
    Object.entries(portGroups).forEach(([groupName, ports]) => {
        const groupElement = document.createElement("div");
        groupElement.className = "port-group";
        const groupTitle = document.createElement("h4");
        groupTitle.textContent = `${groupName} (${ports.length})`;
        groupElement.appendChild(groupTitle);
        const portGrid = document.createElement("div");
        portGrid.className = "port-grid";
        ports.sort((a, b) => {
            const aNum = extractPortNumber(a.port_number);
            const bNum = extractPortNumber(b.port_number);
            return aNum - bNum;
        });
        ports.forEach(port => {
            const portItem = document.createElement("div");
            portItem.className = `port-item status-${port.status}`;
            portItem.dataset.portId = port.id;
            portItem.dataset.switchId = port.switch_id;
            portItem.dataset.portNumber = port.port_number;
            portItem.dataset.portName = port.port_name || "";
            portItem.dataset.portType = port.port_type || "access";
            portItem.dataset.vlanId = port.vlan_id?.toString() || "";
            portItem.dataset.status = port.status || "up";
            portItem.dataset.speed = port.speed || "";
            portItem.dataset.description = port.description || "";
            const portDisplayNum = extractPortLastNumber(port.port_number);
            portItem.textContent = portDisplayNum;
            const tooltip = document.createElement("div");
            tooltip.className = "port-tooltip";
            tooltip.textContent = port.port_number;
            portItem.appendChild(tooltip);
            portGrid.appendChild(portItem);
        });
        groupElement.appendChild(portGrid);
        container.appendChild(groupElement);
    });
    container.onclick = (e) => {
        const target = e.target;
        const portItem = target.closest(".port-item");
        if (portItem) {
            openPortDetailModal({
                portId: portItem.dataset.portId || "",
                switchId: portItem.dataset.switchId || "",
                portNumber: portItem.dataset.portNumber || "",
                portName: portItem.dataset.portName || "",
                portType: portItem.dataset.portType || "access",
                vlanId: portItem.dataset.vlanId || "",
                status: portItem.dataset.status || "up",
                speed: portItem.dataset.speed || "",
                description: portItem.dataset.description || "",
            });
        }
    };
    addPortBtn.onclick = () => {
        openPortDetailModal({
            portId: "",
            switchId: switchId,
            portNumber: "",
            portName: "",
            portType: "access",
            vlanId: "",
            status: "up",
            speed: "",
            description: "",
        });
    };
    const getSnmpPortsBtn = elementCache.get("get-snmp-ports-btn");
    if (getSnmpPortsBtn) {
        getSnmpPortsBtn.onclick = async () => {
            const success = await syncPortsFromSnmp(switchId);
            if (success) {
                const portsResult = await apiGet(`/api/switches/${switchId}/ports?page_size=1000`);
                if (portsResult.success && portsResult.data) {
                    let ports = [];
                    if (Array.isArray(portsResult.data)) {
                        ports = portsResult.data;
                    }
                    else if (portsResult.data.items) {
                        ports = portsResult.data.items;
                    }
                    const newPortGroups = groupPorts(ports);
                    showPortGroupsModal(getCurrentSwitchName() || "", newPortGroups, switchId);
                }
            }
        };
    }
}
function openPortDetailModal(portData) {
    openModal("switch-port-detail-modal");
    const modal = elementCache.get("switch-port-detail-modal");
    const title = elementCache.get("switch-port-detail-modal-title");
    const form = elementCache.get("switch-port-form-expanded");
    const saveBtn = elementCache.get("save-port-btn");
    const deleteBtn = elementCache.get("delete-port-btn");
    if (!modal || !form) {
        console.error("端口详情模态框相关DOM元素未找到");
        return;
    }
    const isNewPort = !portData.portId;
    if (title) {
        title.textContent = isNewPort
            ? t("switch.add_port") || "新增端口"
            : `${t("switch.port_detail") || "端口详情"} - ${portData.portNumber}`;
    }
    elementCache.setValue("switch-port-id-expanded", portData.portId || "");
    elementCache.setValue("switch-port-switch-id-expanded", portData.switchId || "");
    elementCache.setValue("switch-port-number-expanded", portData.portNumber || "");
    elementCache.setValue("switch-port-name-expanded", portData.portName || "");
    elementCache.setValue("switch-port-type-expanded", portData.portType || "access");
    elementCache.setValue("switch-port-vlan-expanded", portData.vlanId || "");
    elementCache.setValue("switch-port-status-expanded", portData.status || "up");
    elementCache.setValue("switch-port-speed-expanded", portData.speed || "");
    elementCache.setValue("switch-port-description-expanded", portData.description || "");
    if (deleteBtn) {
        deleteBtn.style.display = isNewPort ? "none" : "inline-block";
    }
    if (saveBtn) {
        saveBtn.onclick = async () => {
            await submitSwitchPortForm();
        };
    }
    if (deleteBtn) {
        deleteBtn.onclick = async () => {
            if (confirm(t("switch.confirm_delete_port") || "确定要删除此端口吗？")) {
                const portId = elementCache.getValue("switch-port-id-expanded");
                if (portId) {
                    const result = await apiDelete(`/api/switches/ports/${portId}`);
                    if (result.success) {
                        showToast(t("switch.port_deleted") || "端口删除成功", "success");
                        closeModal("switch-port-detail-modal");
                        const currentSwitchId = getCurrentSwitchId();
                        if (currentSwitchId) {
                            manageSwitchPorts(currentSwitchId, getCurrentSwitchName() || "");
                        }
                    }
                    else {
                        showToast(`${t("switch.delete_port_failed") || "删除端口失败"}: ${result.message}`, "error");
                    }
                }
            }
        };
    }
}
export function openSwitchPortModal(portData = null, switchId = null) {
    openModal("switch-port-modal");
    const modal = elementCache.get("switch-port-modal");
    const title = elementCache.get("switch-port-modal-title");
    const form = elementCache.get("switch-port-form");
    if (!modal)
        return;
    form?.reset();
    if (portData) {
        if (title)
            title.textContent = t("switch.edit_port") || "编辑端口";
        elementCache.setValue("switch-port-id", portData.id || "");
        elementCache.setValue("switch-port-switch-id", portData.switch_id || switchId || "");
        elementCache.setValue("switch-port-number", portData.port_number || "");
        elementCache.setValue("switch-port-name", portData.port_name || "");
        elementCache.setValue("switch-port-type", portData.port_type || "access");
        elementCache.setValue("switch-port-vlan", portData.vlan_id?.toString() || "");
        elementCache.setValue("switch-port-status", portData.status || "up");
        elementCache.setValue("switch-port-speed", portData.speed || "");
        elementCache.setValue("switch-port-description", portData.description || "");
    }
    else {
        if (title)
            title.textContent = t("switch.add_port") || "添加端口";
        elementCache.setValue("switch-port-id", "");
        elementCache.setValue("switch-port-switch-id", switchId || "");
        elementCache.setValue("switch-port-status", "up");
        elementCache.setValue("switch-port-type", "access");
    }
}
export async function submitSwitchPortForm() {
    const id = getElementValue("switch-port-id-expanded");
    const switchId = getElementValue("switch-port-switch-id-expanded");
    const portNumber = getElementValue("switch-port-number-expanded");
    const portName = getElementValue("switch-port-name-expanded");
    const portType = getElementValue("switch-port-type-expanded");
    const vlanId = getElementValue("switch-port-vlan-expanded");
    const status = getElementValue("switch-port-status-expanded");
    const speed = getElementValue("switch-port-speed-expanded");
    const description = getElementValue("switch-port-description-expanded");
    if (!portNumber) {
        showToast(t("switch.port_number_required") || "请填写端口号", "warning");
        return false;
    }
    const data = {
        port_number: portNumber,
        port_name: portName || null,
        port_type: portType || "access",
        vlan_id: vlanId ? parseInt(String(vlanId)) : null,
        status: status || "up",
        speed: speed || null,
        description: description || null,
    };
    try {
        let result;
        if (id) {
            result = await apiPut(`/api/switches/ports/${id}`, data);
        }
        else {
            result = await apiPost(`/api/switches/${switchId}/ports`, data);
        }
        if (result.success) {
            closeModal("switch-port-detail-modal");
            showToast(id ? t("switch.port_updated") || "端口更新成功" : t("switch.port_added") || "端口添加成功", "success");
            const currentSwitchId = getCurrentSwitchId();
            if (currentSwitchId) {
                try {
                    await loadSwitchPortsBySwitchId(currentSwitchId);
                    const currentSwitchName = getCurrentSwitchName();
                    if (currentSwitchName) {
                        const portsResult = await apiGet(`/api/switches/${currentSwitchId}/ports?page_size=1000`);
                        if (portsResult.success && portsResult.data) {
                            let ports = [];
                            if (Array.isArray(portsResult.data)) {
                                ports = portsResult.data;
                            }
                            else if (portsResult.data.items && Array.isArray(portsResult.data.items)) {
                                ports = portsResult.data.items;
                            }
                            const portGroups = groupPorts(ports);
                            showPortGroupsModal(currentSwitchName, portGroups, currentSwitchId);
                        }
                    }
                }
                catch (refreshError) {
                    console.error("刷新端口数据失败:", refreshError);
                    showToast(t("switch.port_saved_refresh_failed") || "端口保存成功，但刷新数据失败，请手动刷新", "warning");
                }
            }
            return true;
        }
        else {
            const errorMsg = (result.message ?? (t("common.operation_failed") || "操作失败，请检查输入信息"));
            showToast(`${t("common.operation_failed") || "操作失败"}: ${errorMsg}`, "error");
            console.error("服务器返回错误:", result);
            return false;
        }
    }
    catch (error) {
        handleError(error, t("switch.submit_port_failed") || "提交端口表单失败");
        return false;
    }
}
export async function editSwitchPort(id) {
    try {
        const result = await apiGet(`/api/switches/ports/${id}`);
        if (result.success) {
            openSwitchPortModal(result.data);
        }
        else {
            showToast(t("switch.fetch_port_failed") || "获取端口信息失败", "error");
        }
    }
    catch (error) {
        handleError(error, t("switch.fetch_port_failed") || "获取端口信息失败");
    }
}
export async function deleteSwitchPort(id) {
    const currentSwitchId = getCurrentSwitchId();
    const successCallback = async () => {
        if (currentSwitchId) {
            await loadSwitchPortsBySwitchId(currentSwitchId);
        }
        else {
            await loadSwitchPortsData(1, "");
        }
    };
    if (!confirm(t("switch.confirm_delete_port") || "确定要删除此端口吗？"))
        return false;
    try {
        const result = await apiDelete(`/api/switches/ports/${id}`);
        if (result.success) {
            showToast(t("switch.port_deleted") || "端口删除成功", "success");
            await successCallback();
            return true;
        }
        else {
            showToast(`${t("switch.delete_port_failed") || "删除端口失败"}: ${result.message}`, "error");
            return false;
        }
    }
    catch (error) {
        handleError(error, t("switch.delete_port_failed") || "删除端口失败");
        return false;
    }
}
export function extractPortNumber(portNumber) {
    if (typeof portNumber !== "string" || !portNumber)
        return 0;
    const match = portNumber.match(/\d+/g);
    if (match) {
        return parseInt(match[match.length - 1]) || 0;
    }
    return 0;
}
export function extractPortLastNumber(portNumber) {
    if (typeof portNumber !== "string" || !portNumber)
        return portNumber || "";
    const match = portNumber.match(/\d+/g);
    if (match) {
        return match[match.length - 1];
    }
    return portNumber;
}
//# sourceMappingURL=switchPort.js.map