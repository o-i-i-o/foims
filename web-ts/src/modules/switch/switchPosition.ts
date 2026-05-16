import { apiGet } from "../../utils/apiClient.js";
import { elementCache } from "../../utils/helpers.js";
import { dispatchNetworkRegionChange } from "../../utils/ipconfig.js";
import {
  positionData,
  networkRegion,
  updateNetworkRegion,
} from "./switchState.js";
import { t } from "../../utils/i18n.js";

interface Cabinet {
  id: string;
  name: string;
  network_region_id?: string;
}

interface SwitchData {
  position_id?: string;
  cabinet_id?: string;
  cabinet_name?: string;
  start_u?: number;
  end_u?: number;
  network_region_id?: string;
  network_region_name?: string;
  position?: {
    cabinet_id?: string;
    cabinet_name?: string;
    start_u?: number;
    end_u?: number;
    network_region_id?: string;
  };
}

async function loadAllCabinets(): Promise<Cabinet[]> {
  try {
    const result = await apiGet<Cabinet[] | PagedData<Cabinet>>("/api/resources/cabinets?page_size=10000");
    if (result.success && result.data) {
      const data = result.data;
      return (data as PagedData<Cabinet>).items || (Array.isArray(data) ? data : []);
    }
    return [];
  } catch (error) {
    console.error("加载机柜失败:", error);
    return [];
  }
}

export class PositionSelector {
  private onSelect: (regionId: string | null, regionName: string) => void;
  private eventController: AbortController | null = null;
  private cabinetsCache: Cabinet[] = [];

  constructor(onSelect: (regionId: string | null, regionName: string) => void) {
    this.onSelect = onSelect;
  }

  async init(sw: SwitchData | null = null): Promise<void> {
    if (this.eventController) {
      this.eventController.abort();
    }
    this.eventController = new AbortController();
    const { signal } = this.eventController;

    this.bindEvents(signal);

    const cabinets = await loadAllCabinets();
    this.cabinetsCache = cabinets;

    const cabinetSelect = elementCache.get("switch-cabinet-select") as HTMLSelectElement | null;
    if (cabinetSelect) {
      cabinetSelect.innerHTML = `<option value="">${t("switch.select_cabinet") || "请选择机柜"}</option>` +
        cabinets.map(c => `<option value="${c.id}" data-name="${c.name}" data-region-id="${c.network_region_id || ""}">${c.name}</option>`).join("");
    }

    if (sw) {
      await this.loadFromSwitch(sw);
    }
  }

  private bindEvents(signal: AbortSignal): void {
    const cabinetSelect = elementCache.get("switch-cabinet-select") as HTMLSelectElement | null;
    const startUInput = elementCache.get("switch-start-u") as HTMLInputElement | null;
    const endUInput = elementCache.get("switch-end-u") as HTMLInputElement | null;

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
        positionData.startU = parseInt((e.target as HTMLInputElement).value) || null;
      }, { signal });
    }

    if (endUInput) {
      endUInput.addEventListener("input", (e) => {
        positionData.endU = parseInt((e.target as HTMLInputElement).value) || null;
      }, { signal });
    }
  }

  async loadFromSwitch(sw: SwitchData): Promise<void> {
    const cabinetSelect = elementCache.get("switch-cabinet-select") as HTMLSelectElement | null;
    const startUInput = elementCache.get("switch-start-u") as HTMLInputElement | null;
    const endUInput = elementCache.get("switch-end-u") as HTMLInputElement | null;

    let cabinetId: string | null = null;
    let cabinetName: string | null = null;
    let startU: number | null = null;
    let endU: number | null = null;

    positionData.positionId = sw.position_id || null;

    if (sw.position && sw.position.cabinet_id) {
      const pos = sw.position;
      cabinetId = pos.cabinet_id;
      cabinetName = pos.cabinet_name || null;
      startU = pos.start_u ?? null;
      endU = pos.end_u ?? null;
    } else if (sw.cabinet_id) {
      cabinetId = sw.cabinet_id;
      cabinetName = sw.cabinet_name || null;
      startU = sw.start_u ?? null;
      endU = sw.end_u ?? null;
    }

    if (cabinetId) {
      positionData.cabinetId = cabinetId;
      positionData.cabinetName = cabinetName;
      positionData.startU = startU;
      positionData.endU = endU;

      if (startUInput && startU) startUInput.value = String(startU);
      if (endUInput && endU) endUInput.value = String(endU);

      if (cabinetSelect && cabinetId) {
        await new Promise(resolve => setTimeout(resolve, 50));
        const option = cabinetSelect.querySelector(`option[value="${cabinetId}"]`);
        if (option) {
          cabinetSelect.value = cabinetId;
          const regionId = (option as HTMLOptionElement).dataset.regionId;
          if (regionId) {
            updateNetworkRegion(regionId, "");
            positionData.networkRegionId = regionId;
          }
        } else if (this.cabinetsCache.length > 0) {
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
    } else if (sw.position?.network_region_id) {
      positionData.networkRegionId = sw.position.network_region_id;
      networkRegion.id = sw.position.network_region_id;
    }
  }

  clear(): void {
    positionData.cabinetId = null;
    positionData.cabinetName = null;
    positionData.startU = null;
    positionData.endU = null;
    positionData.positionId = null;

    const cabinetSelect = elementCache.get("switch-cabinet-select") as HTMLSelectElement | null;
    const startUInput = elementCache.get("switch-start-u") as HTMLInputElement | null;
    const endUInput = elementCache.get("switch-end-u") as HTMLInputElement | null;

    if (cabinetSelect) cabinetSelect.value = "";
    if (startUInput) startUInput.value = "";
    if (endUInput) endUInput.value = "";
  }

  destroy(): void {
    if (this.eventController) {
      this.eventController.abort();
      this.eventController = null;
    }
  }
}
