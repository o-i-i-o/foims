// Main switch device management module
import { openModal } from "../../utils/modal.js";
import { showToast, debounce } from "../../utils/ui.js";
import { elementCache } from "../../utils/helpers.js";
import { setSwitchFormValues, } from "./switchForm.js";
import { toggleSnmpConfig, testSnmpConnection, syncPortsFromSnmp, } from "./switchSnmp.js";
import { PositionSelector } from "./switchPosition.js";
import { updateNetworkRegion, SWITCH_PAGE_SIZE, SWITCH_PORT_PAGE_SIZE, } from "./switchState.js";
import { loadSwitchesData, fetchSwitchById, deleteSwitch, submitSwitchForm, } from "./switchList.js";
import { loadSwitchPortsData, deleteSwitchPort, } from "./switchPort.js";
import { viewArpTable, viewLldpNeighbors, } from "./switchMacLldp.js";
let positionSelector = null;
async function filterSwitchesData(searchTerm) {
    await loadSwitchesData(1, searchTerm);
}
const debouncedFilterSwitchesData = debounce(filterSwitchesData, 300);
export async function openSwitchModal(sw = null) {
    openModal("switch-modal");
    const title = elementCache.get("switch-modal-title");
    const form = elementCache.get("switch-form");
    const snmpVersionSelect = elementCache.get("switch-snmp-version");
    if (snmpVersionSelect && !snmpVersionSelect.dataset.bound) {
        snmpVersionSelect.addEventListener("change", toggleSnmpConfig);
        snmpVersionSelect.dataset.bound = "true";
    }
    if (positionSelector) {
        positionSelector.destroy();
    }
    positionSelector = new PositionSelector((regionId, regionName) => {
        updateNetworkRegion(regionId, regionName);
    });
    if (sw) {
        if (title)
            title.textContent = "编辑交换机";
        if (sw.ips && sw.ips.length > 0) {
            const firstIp = sw.ips[0];
            if (firstIp.network_region_id) {
                updateNetworkRegion(firstIp.network_region_id, firstIp.network_region || "");
            }
        }
        await positionSelector.init(sw);
        await setSwitchFormValues(sw);
    }
    else {
        if (title)
            title.textContent = "添加交换机";
        if (form)
            form.reset();
        await positionSelector.init();
    }
    toggleSnmpConfig();
}
export async function editSwitch(id) {
    const sw = await fetchSwitchById(String(id));
    if (sw) {
        await openSwitchModal(sw);
    }
    else {
        showToast("获取交换机数据失败", "error");
    }
}
export { deleteSwitch, deleteSwitchPort };
export function initSwitchEvents() {
    const searchInput = document.getElementById("switch-search");
    if (searchInput) {
        searchInput.addEventListener("input", () => {
            debouncedFilterSwitchesData(searchInput.value);
        });
    }
    const refreshBtn = document.getElementById("switch-refresh-btn");
    if (refreshBtn) {
        refreshBtn.addEventListener("click", () => {
            loadSwitchesData();
        });
    }
}
export { loadSwitchesData, loadSwitchPortsData, submitSwitchForm, toggleSnmpConfig, testSnmpConnection, syncPortsFromSnmp, viewArpTable, viewLldpNeighbors, SWITCH_PAGE_SIZE, SWITCH_PORT_PAGE_SIZE, };
//# sourceMappingURL=switchDevice.js.map