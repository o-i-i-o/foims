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

async function loadRoomsFromCabinets(cabinets) {
  const roomMap = new Map();
  cabinets.forEach(cabinet => {
    if (cabinet.room_id && cabinet.room_name && !roomMap.has(cabinet.room_id)) {
      roomMap.set(cabinet.room_id, cabinet.room_name);
    }
  });
  return roomMap;
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

    const roomMap = await loadRoomsFromCabinets(cabinets);
    const roomSelect = elementCache.get('switch-room-select');
    if (roomSelect) {
      roomSelect.innerHTML = '<option value="">请选择机房</option>' +
        Array.from(roomMap.entries()).map(([id, name]) => 
          `<option value="${id}">${name}</option>`
        ).join('');
    }

    const cabinetSelect = elementCache.get('switch-cabinet-select');
    if (cabinetSelect) {
      cabinetSelect.innerHTML = '<option value="">请先选择机房</option>';
    }

    if (sw) {
      await this.loadFromSwitch(sw);
    }
  }

  bindEvents(signal) {
    const roomSelect = elementCache.get('switch-room-select');
    const cabinetSelect = elementCache.get('switch-cabinet-select');
    const startUInput = elementCache.get('switch-start-u');
    const endUInput = elementCache.get('switch-end-u');

    if (roomSelect) {
      roomSelect.addEventListener('change', (e) => {
        const roomId = e.target.value;
        positionData.roomId = roomId || null;
        
        cabinetSelect.innerHTML = '<option value="">请选择机柜</option>';
        
        if (roomId) {
          const filteredCabinets = this.cabinetsCache.filter(c => c.room_id === roomId);
          filteredCabinets.forEach(cabinet => {
            const option = document.createElement('option');
            option.value = cabinet.id;
            option.dataset.name = cabinet.name;
            option.dataset.regionId = cabinet.network_region_id || '';
            option.textContent = cabinet.name;
            cabinetSelect.appendChild(option);
          });
        }
        
        positionData.cabinetId = null;
        positionData.cabinetName = null;
      }, { signal });
    }

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
    const roomSelect = elementCache.get('switch-room-select');
    const cabinetSelect = elementCache.get('switch-cabinet-select');
    const startUInput = elementCache.get('switch-start-u');
    const endUInput = elementCache.get('switch-end-u');

    let cabinetId = null;
    let cabinetName = null;
    let roomId = null;
    let startU = null;
    let endU = null;

    positionData.positionId = sw.position_id || null;

    if (sw.position && sw.position.cabinet_id) {
      const pos = sw.position;
      cabinetId = pos.cabinet_id;
      cabinetName = pos.cabinet_name;
      roomId = pos.room_id;
      startU = pos.start_u;
      endU = pos.end_u;
    } else if (sw.cabinet_id) {
      cabinetId = sw.cabinet_id;
      cabinetName = sw.cabinet_name;
      roomId = sw.room_id;
      startU = sw.start_u;
      endU = sw.end_u;
    }

    if (roomId && roomSelect) {
      positionData.roomId = roomId;
      roomSelect.value = roomId;
      
      cabinetSelect.innerHTML = '<option value="">请选择机柜</option>';
      const filteredCabinets = this.cabinetsCache.filter(c => c.room_id === roomId);
      filteredCabinets.forEach(cabinet => {
        const option = document.createElement('option');
        option.value = cabinet.id;
        option.dataset.name = cabinet.name;
        option.dataset.regionId = cabinet.network_region_id || '';
        option.textContent = cabinet.name;
        cabinetSelect.appendChild(option);
      });
    }

    if (cabinetId) {
      positionData.cabinetId = cabinetId;
      positionData.cabinetName = cabinetName;
      positionData.startU = startU;
      positionData.endU = endU;

      if (startUInput && startU) startUInput.value = startU;
      if (endUInput && endU) endUInput.value = endU;

      if (cabinetSelect && cabinetId) {
        cabinetSelect.value = cabinetId;
        const selectedOption = cabinetSelect.options[cabinetSelect.selectedIndex];
        const regionId = selectedOption?.dataset?.regionId;
        if (regionId) {
          updateNetworkRegion(regionId, '');
          positionData.networkRegionId = regionId;
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
