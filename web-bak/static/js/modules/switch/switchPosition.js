import { apiGet } from "../../utils/apiClient.js";
import { elementCache } from "../../utils/helpers.js";
import { showToast } from "../../utils/ui.js";
import {
  dispatchNetworkRegionChange
} from "../../utils/ipconfig.js";
import {
  positionData,
  networkRegion,
  updateNetworkRegion
} from "./switchState.js";

async function loadAllCabinets() {
  try {
    const result = await apiGet('/api/resources/cabinets?page_size=10000');
    if (result.success && result.data) {
      return result.data.items || result.data || [];
    }
    return [];
  } catch (error) {
    console.error('加载机柜失败:', error);
    return [];
  }
}

export class PositionSelector {
  constructor(onRegionChange) {
    this.onRegionChange = onRegionChange;
    this.eventController = null;
    this.cabinetsCache = [];
    this.boundHandleNetworkRegionChange = null;
    this.boundHandleNetworkChange = null;
    this.currentNetworkId = null;
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

    const cabinetSelect = elementCache.get('switch-cabinet-select');
    if (cabinetSelect) {
      cabinetSelect.innerHTML = '<option value="">请选择机柜</option>' +
        cabinets.map(c => `<option value="${c.id}" data-name="${c.name}" data-region-id="${c.network_region_id || ''}">${c.name}</option>`).join('');
    }

    if (sw) {
      await this.loadFromSwitch(sw);
    }
  }

  bindEvents(signal) {
    const cabinetSelect = elementCache.get('switch-cabinet-select');
    const startUInput = elementCache.get('switch-start-u');
    const endUInput = elementCache.get('switch-end-u');

    if (cabinetSelect) {
      cabinetSelect.addEventListener('change', (e) => {
        const selectedOption = cabinetSelect.options[cabinetSelect.selectedIndex];
        positionData.cabinetId = cabinetSelect.value || null;
        positionData.cabinetName = selectedOption?.dataset?.name || null;
        
        const regionId = selectedOption?.dataset?.regionId;
        if (regionId) {
          updateNetworkRegion(regionId, '');
          positionData.networkRegionId = regionId;
          dispatchNetworkRegionChange(regionId, '');
        }
      }, { signal });
    }

    if (startUInput) {
      startUInput.addEventListener('input', (e) => {
        positionData.startU = parseInt(e.target.value) || null;
      }, { signal });
    }

    if (endUInput) {
      endUInput.addEventListener('input', (e) => {
        positionData.endU = parseInt(e.target.value) || null;
      }, { signal });
    }
  }

  async loadFromSwitch(sw) {
    console.log('loadFromSwitch - switch data:', sw);
    const cabinetSelect = elementCache.get('switch-cabinet-select');
    const startUInput = elementCache.get('switch-start-u');
    const endUInput = elementCache.get('switch-end-u');

    let cabinetId = null;
    let cabinetName = null;
    let startU = null;
    let endU = null;

    if (sw.cabinet_id) {
      console.log('loadFromSwitch - found cabinet_id:', sw.cabinet_id);
      cabinetId = sw.cabinet_id;
      cabinetName = sw.cabinet_name;
      startU = sw.start_u;
      endU = sw.end_u;
    } else if (sw.position) {
      console.log('loadFromSwitch - found position:', sw.position);
      const pos = sw.position;
      if (pos.cabinet_id) {
        cabinetId = pos.cabinet_id;
        cabinetName = pos.cabinet_name;
        startU = pos.start_u;
        endU = pos.end_u;
      }
    }

    console.log('loadFromSwitch - extracted:', { cabinetId, cabinetName, startU, endU });

    if (cabinetId) {
      positionData.cabinetId = cabinetId;
      positionData.cabinetName = cabinetName;
      positionData.startU = startU;
      positionData.endU = endU;

      if (startUInput && startU) startUInput.value = startU;
      if (endUInput && endU) endUInput.value = endU;

      if (cabinetSelect && cabinetId) {
        await new Promise(resolve => setTimeout(resolve, 50));
        const option = cabinetSelect.querySelector(`option[value="${cabinetId}"]`);
        console.log('loadFromSwitch - option found:', !!option, 'cabinetsCache length:', this.cabinetsCache.length);
        if (option) {
          cabinetSelect.value = cabinetId;
          const regionId = option.dataset.regionId;
          if (regionId) {
            updateNetworkRegion(regionId, '');
            positionData.networkRegionId = regionId;
          }
        } else if (this.cabinetsCache.length > 0) {
          const cabinet = this.cabinetsCache.find(c => c.id === cabinetId);
          if (cabinet) {
            const newOption = document.createElement('option');
            newOption.value = cabinet.id;
            newOption.dataset.name = cabinet.name;
            newOption.dataset.regionId = cabinet.network_region_id || '';
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
      networkRegion.name = sw.network_region_name || '';
    } else if (sw.position?.network_region_id) {
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

    const cabinetSelect = elementCache.get('switch-cabinet-select');
    const startUInput = elementCache.get('switch-start-u');
    const endUInput = elementCache.get('switch-end-u');

    if (cabinetSelect) cabinetSelect.value = '';
    if (startUInput) startUInput.value = '';
    if (endUInput) endUInput.value = '';
  }

  destroy() {
    if (this.eventController) {
      this.eventController.abort();
      this.eventController = null;
    }
  }
}
