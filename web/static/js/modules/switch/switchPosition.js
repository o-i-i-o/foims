import { apiGet } from "../../utils/apiClient.js";
import { elementCache } from "../../utils/helpers.js";
import { dispatchNetworkRegionChange } from "../../utils/ipconfig.js";
import { positionData, networkRegion, updateNetworkRegion, } from "./switchState.js";
import { t } from "../../utils/i18n.js";
async function loadAllCabinets() {
    try {
        const result = await apiGet("/api/resources/cabinets?page_size=10000");
        if (result.success && result.data) {
            const data = result.data;
            return data.items || (Array.isArray(data) ? data : []);
        }
        return [];
    }
    catch (error) {
        console.error("加载机柜失败:", error);
        return [];
    }
}
export class PositionSelector {
    onSelect;
    eventController = null;
    cabinetsCache = [];
    constructor(onSelect) {
        this.onSelect = onSelect;
    }
    async init(sw = null) {
        if (this.eventController) {
            this.eventController.abort();
        }
        this.eventController = new AbortController();
        const { signal } = this.eventController;
        this.bindEvents(signal);
        const cabinets = await loadAllCabinets();
        this.cabinetsCache = cabinets;
        const cabinetSelect = elementCache.get("switch-cabinet-select");
        if (cabinetSelect) {
            cabinetSelect.innerHTML = `<option value="">${t("switch.select_cabinet") || "请选择机柜"}</option>` +
                cabinets.map(c => `<option value="${c.id}" data-name="${c.name}" data-region-id="${c.network_region_id || ""}">${c.name}</option>`).join("");
        }
        if (sw) {
            await this.loadFromSwitch(sw);
        }
    }
    bindEvents(signal) {
        const cabinetSelect = elementCache.get("switch-cabinet-select");
        const startUInput = elementCache.get("switch-start-u");
        const endUInput = elementCache.get("switch-end-u");
        if (cabinetSelect) {
            cabinetSelect.addEventListener("change", () => {
                const selectedOption = cabinetSelect.options[cabinetSelect.selectedIndex];
                positionData.cabinetId = cabinetSelect.value || null;
                positionData.cabinetName = selectedOption?.dataset?.name || null;
                const regionId = selectedOption?.dataset?.regionId;
                if (regionId) {
                    updateNetworkRegion(regionId, "");
                    positionData.networkRegionId = regionId;
                    dispatchNetworkRegionChange(regionId, "");
                    this.onSelect(regionId, selectedOption?.dataset?.name || "");
                }
            }, { signal });
        }
        if (startUInput) {
            startUInput.addEventListener("input", (e) => {
                positionData.startU = parseInt(e.target.value) || null;
            }, { signal });
        }
        if (endUInput) {
            endUInput.addEventListener("input", (e) => {
                positionData.endU = parseInt(e.target.value) || null;
            }, { signal });
        }
    }
    async loadFromSwitch(sw) {
        const cabinetSelect = elementCache.get("switch-cabinet-select");
        const startUInput = elementCache.get("switch-start-u");
        const endUInput = elementCache.get("switch-end-u");
        let cabinetId = null;
        let cabinetName = null;
        let startU = null;
        let endU = null;
        if (sw.cabinet_id) {
            cabinetId = sw.cabinet_id;
            cabinetName = sw.cabinet_name || null;
            startU = sw.start_u ?? null;
            endU = sw.end_u ?? null;
        }
        else if (sw.position) {
            const pos = sw.position;
            if (pos.cabinet_id) {
                cabinetId = pos.cabinet_id;
                cabinetName = pos.cabinet_name || null;
                startU = pos.start_u ?? null;
                endU = pos.end_u ?? null;
            }
        }
        if (cabinetId) {
            positionData.cabinetId = cabinetId;
            positionData.cabinetName = cabinetName;
            positionData.startU = startU;
            positionData.endU = endU;
            if (startUInput && startU)
                startUInput.value = String(startU);
            if (endUInput && endU)
                endUInput.value = String(endU);
            if (cabinetSelect && cabinetId) {
                await new Promise(resolve => setTimeout(resolve, 50));
                const option = cabinetSelect.querySelector(`option[value="${cabinetId}"]`);
                if (option) {
                    cabinetSelect.value = cabinetId;
                    const regionId = option.dataset.regionId;
                    if (regionId) {
                        updateNetworkRegion(regionId, "");
                        positionData.networkRegionId = regionId;
                    }
                }
                else if (this.cabinetsCache.length > 0) {
                    const cabinet = this.cabinetsCache.find(c => c.id === cabinetId);
                    if (cabinet) {
                        const newOption = document.createElement("option");
                        newOption.value = cabinet.id;
                        newOption.dataset.name = cabinet.name;
                        newOption.dataset.regionId = cabinet.network_region_id || "";
                        newOption.textContent = cabinet.name;
                        cabinetSelect.appendChild(newOption);
                        cabinetSelect.value = cabinetId;
                    }
                }
            }
        }
        if (sw.network_region_id) {
            positionData.networkRegionId = sw.network_region_id;
            networkRegion.id = sw.network_region_id;
            networkRegion.name = sw.network_region_name || "";
        }
        else if (sw.position?.network_region_id) {
            positionData.networkRegionId = sw.position.network_region_id;
            networkRegion.id = sw.position.network_region_id;
        }
    }
    clear() {
        positionData.cabinetId = null;
        positionData.cabinetName = null;
        positionData.startU = null;
        positionData.endU = null;
        positionData.positionId = null;
        const cabinetSelect = elementCache.get("switch-cabinet-select");
        const startUInput = elementCache.get("switch-start-u");
        const endUInput = elementCache.get("switch-end-u");
        if (cabinetSelect)
            cabinetSelect.value = "";
        if (startUInput)
            startUInput.value = "";
        if (endUInput)
            endUInput.value = "";
    }
    destroy() {
        if (this.eventController) {
            this.eventController.abort();
            this.eventController = null;
        }
    }
}
//# sourceMappingURL=switchPosition.js.map