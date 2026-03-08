import { apiGet } from "../../utils/apiClient.js";
import { elementCache } from "../../utils/helpers.js";
import { showToast } from "../../utils/ui.js";
import {
  onNetworkRegionChange,
  offNetworkRegionChange
} from "../../utils/ipconfig.js";

export function createPositionState() {
  return {
    cabinetId: null,
    cabinetName: null,
    startU: null,
    endU: null,
    positionId: null,
    networkRegionId: null
  };
}

export function createNetworkRegionState() {
  return {
    id: null,
    name: ''
  };
}

async function loadCabinetsByRegion(regionId) {
  try {
    const result = await apiGet(`/api/resources/network-regions/${regionId}/cabinets`);
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
  constructor(positionData, networkRegion, onRegionChange) {
    this.positionData = positionData;
    this.networkRegion = networkRegion;
    this.onRegionChange = onRegionChange;
    this.eventController = null;
    this.cabinetsCache = [];
  }

  async init(sw = null) {
    if (this.eventController) {
      this.eventController.abort();
    }
    this.eventController = new AbortController();
    const { signal } = this.eventController;

    this.bindEvents(signal);

    const regionId = this.networkRegion.id || (sw && (sw.network_region_id || sw.position?.network_region_id || (sw.ips && sw.ips[0]?.network_region_id)));
    
    if (regionId) {
      await this.handleNetworkRegionChange(regionId, this.networkRegion.name || sw?.network_region_name || sw?.ips?.[0]?.network_region || '');
    }

    if (sw) {
      await this.loadFromSwitch(sw);
    }

    onNetworkRegionChange((regionId, regionName) => {
      this.handleNetworkRegionChange(regionId, regionName);
    });
  }

  bindEvents(signal) {
    const cabinetSelect = elementCache.get('switch-cabinet-select');
    const startUInput = elementCache.get('switch-start-u');
    const endUInput = elementCache.get('switch-end-u');

    if (cabinetSelect) {
      cabinetSelect.addEventListener('change', (e) => {
        const selectedOption = cabinetSelect.options[cabinetSelect.selectedIndex];
        this.positionData.cabinetId = cabinetSelect.value || null;
        this.positionData.cabinetName = selectedOption?.dataset?.name || null;
      }, { signal });
    }

    if (startUInput) {
      startUInput.addEventListener('input', (e) => {
        this.positionData.startU = parseInt(e.target.value) || null;
      }, { signal });
    }

    if (endUInput) {
      endUInput.addEventListener('input', (e) => {
        this.positionData.endU = parseInt(e.target.value) || null;
      }, { signal });
    }
  }

  async loadFromSwitch(sw) {
    const cabinetSelect = elementCache.get('switch-cabinet-select');
    const startUInput = elementCache.get('switch-start-u');
    const endUInput = elementCache.get('switch-end-u');

    let cabinetId = null;
    let cabinetName = null;
    let startU = null;
    let endU = null;

    if (sw.cabinet_id) {
      cabinetId = sw.cabinet_id;
      cabinetName = sw.cabinet_name;
      startU = sw.start_u;
      endU = sw.end_u;
    } else if (sw.position) {
      const pos = sw.position;
      if (pos.cabinet_id) {
        cabinetId = pos.cabinet_id;
        cabinetName = pos.cabinet_name;
        startU = pos.start_u;
        endU = pos.end_u;
      }
    }

    if (cabinetId) {
      this.positionData.cabinetId = cabinetId;
      this.positionData.cabinetName = cabinetName;
      this.positionData.startU = startU;
      this.positionData.endU = endU;

      if (startUInput && startU) startUInput.value = startU;
      if (endUInput && endU) endUInput.value = endU;
      
      if (cabinetSelect && cabinetId) {
        await new Promise(resolve => setTimeout(resolve, 0));
        cabinetSelect.value = cabinetId;
      }
    }

    if (sw.network_region_id) {
      this.positionData.networkRegionId = sw.network_region_id;
      this.networkRegion.id = sw.network_region_id;
      this.networkRegion.name = sw.network_region_name || '';
    } else if (sw.position?.network_region_id) {
      this.positionData.networkRegionId = sw.position.network_region_id;
      this.networkRegion.id = sw.position.network_region_id;
    }
  }

  async handleNetworkRegionChange(regionId, regionName) {
    this.networkRegion.id = regionId;
    this.networkRegion.name = regionName;
    this.positionData.networkRegionId = regionId;

    if (this.onRegionChange) {
      this.onRegionChange(regionId, regionName);
    }

    const cabinetSelect = elementCache.get('switch-cabinet-select');
    const startUInput = elementCache.get('switch-start-u');
    const endUInput = elementCache.get('switch-end-u');

    this.positionData.cabinetId = null;
    this.positionData.cabinetName = null;
    this.positionData.startU = null;
    this.positionData.endU = null;
    this.positionData.positionId = null;

    if (startUInput) startUInput.value = '';
    if (endUInput) endUInput.value = '';

    if (regionId) {
      const cabinets = await loadCabinetsByRegion(regionId);
      this.cabinetsCache = cabinets;

      if (cabinetSelect) {
        cabinetSelect.disabled = false;
        if (cabinets.length === 0) {
          cabinetSelect.innerHTML = '<option value="">该网络区域暂无关联机柜</option>';
          showToast('该网络区域暂无关联机柜，请先在机柜管理中配置', 'warning');
        } else {
          cabinetSelect.innerHTML = '<option value="">选择机柜</option>' +
            cabinets.map(c => `<option value="${c.id}" data-name="${c.name}">${c.name}</option>`).join('');
        }
      }
      if (startUInput) startUInput.disabled = false;
      if (endUInput) endUInput.disabled = false;
    } else {
      if (cabinetSelect) {
        cabinetSelect.innerHTML = '<option value="">请先在IP配置中选择网络区域</option>';
        cabinetSelect.disabled = true;
      }
      if (startUInput) startUInput.disabled = true;
      if (endUInput) endUInput.disabled = true;
    }
  }

  clear() {
    this.positionData.cabinetId = null;
    this.positionData.cabinetName = null;
    this.positionData.startU = null;
    this.positionData.endU = null;
    this.positionData.positionId = null;

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
    offNetworkRegionChange(this.handleNetworkRegionChange);
  }
}
