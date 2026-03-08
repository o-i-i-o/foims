import { elementCache } from "../../utils/helpers.js";
import { getManager } from "../../utils/ipconfig.js";

export const SWITCH_FORM_FIELDS = [
  'switch-id', 'switch-name', 'switch-model', 'switch-vendor',
  'switch-location', 'switch-description', 'switch-snmp-version',
  'switch-snmp-port', 'switch-snmp-community', 'switch-snmp-username',
  'switch-snmp-auth-protocol', 'switch-snmp-auth-password',
  'switch-snmp-priv-protocol', 'switch-snmp-priv-password'
];

export function getSwitchFormValues(positionData) {
  const manager = getManager('switch');
  const rawIps = manager ? manager.getIps() : [];
  
  let parentSwitchId = null;
  let parentPortId = null;
  let networkRegionId = null;
  
  const ips = rawIps.map(ip => {
    if (ip.parent_switch_id) parentSwitchId = ip.parent_switch_id;
    if (ip.parent_port_id) parentPortId = ip.parent_port_id;
    if (ip.network_region_id) networkRegionId = ip.network_region_id;
    const { parent_switch_id, parent_port_id, ...rest } = ip;
    return rest;
  });
  
  const cabinetSelect = elementCache.get('switch-cabinet-select');
  const startUInput = elementCache.get('switch-start-u');
  const endUInput = elementCache.get('switch-end-u');
  
  const cabinetId = cabinetSelect?.value || null;
  const cabinetName = cabinetSelect?.selectedOptions?.[0]?.dataset?.name || null;
  const startU = startUInput?.value ? parseInt(startUInput.value) : null;
  const endU = endUInput?.value ? parseInt(endUInput.value) : null;
  
  return {
    id: elementCache.getValue('switch-id'),
    name: elementCache.getValue('switch-name'),
    model: elementCache.getValue('switch-model') || null,
    vendor: elementCache.getValue('switch-vendor') || null,
    location: elementCache.getValue('switch-location') || null,
    description: elementCache.getValue('switch-description') || null,
    snmp_version: elementCache.getValue('switch-snmp-version'),
    snmp_port: parseInt(elementCache.getValue('switch-snmp-port')) || 161,
    snmp_community: elementCache.getValue('switch-snmp-community') || null,
    snmp_username: elementCache.getValue('switch-snmp-username') || null,
    snmp_auth_protocol: elementCache.getValue('switch-snmp-auth-protocol') || null,
    snmp_auth_password: elementCache.getValue('switch-snmp-auth-password') || null,
    snmp_priv_protocol: elementCache.getValue('switch-snmp-priv-protocol') || null,
    snmp_priv_password: elementCache.getValue('switch-snmp-priv-password') || null,
    parent_switch_id: parentSwitchId,
    parent_port_id: parentPortId,
    ips: ips,
    network_region_id: networkRegionId || positionData.networkRegionId,
    cabinet_id: cabinetId,
    start_u: startU,
    end_u: endU
  };
}

export async function setSwitchFormValues(sw, positionData, updateNetworkRegion) {
  elementCache.setValue('switch-id', sw.id || '');
  elementCache.setValue('switch-name', sw.name || '');
  elementCache.setValue('switch-model', sw.model || '');
  elementCache.setValue('switch-vendor', sw.vendor || '');
  elementCache.setValue('switch-location', sw.location || '');
  elementCache.setValue('switch-description', sw.description || '');
  elementCache.setValue('switch-snmp-version', sw.snmp_version || 'v2c');
  elementCache.setValue('switch-snmp-port', sw.snmp_port || 161);
  elementCache.setValue('switch-snmp-community', sw.snmp_community || '');
  elementCache.setValue('switch-snmp-username', sw.snmp_username || '');
  elementCache.setValue('switch-snmp-auth-protocol', sw.snmp_auth_protocol || '');
  elementCache.setValue('switch-snmp-auth-password', sw.snmp_auth_password || '');
  elementCache.setValue('switch-snmp-priv-protocol', sw.snmp_priv_protocol || '');
  elementCache.setValue('switch-snmp-priv-password', sw.snmp_priv_password || '');

  const pos = sw.position || sw;
  if (pos.cabinet_id || pos.id) {
    positionData.cabinetId = pos.cabinet_id;
    positionData.cabinetName = pos.cabinet_name || '';
    positionData.positionId = pos.id || pos.position_id;
    positionData.startU = pos.start_u;
    positionData.endU = pos.end_u;
    if (pos.network_region_id) {
      positionData.networkRegionId = pos.network_region_id;
    }
  }

  if (sw.ips && sw.ips.length > 0) {
    const firstIp = sw.ips[0];
    if (firstIp.network_region_id) {
      positionData.networkRegionId = firstIp.network_region_id;
      updateNetworkRegion(firstIp.network_region_id, firstIp.network_region);
    }
  }

  const manager = getManager('switch');
  if (manager) {
    manager.setExcludeSwitchId(sw.id || null);
    const ipsWithParent = (sw.ips || []).map(ip => ({
      ...ip,
      parent_switch_id: sw.parent_switch_id,
      parent_port_id: sw.parent_port_id
    }));
    await manager.loadIps(ipsWithParent);
  }
}

export async function resetSwitchForm(positionData, updateNetworkRegion) {
  SWITCH_FORM_FIELDS.forEach(field => elementCache.setValue(field, ''));
  Object.assign(positionData, {
    cabinetId: null,
    cabinetName: null,
    startU: null,
    endU: null,
    positionId: null,
    networkRegionId: null
  });
  updateNetworkRegion(null);
  
  const cabinetSelect = elementCache.get('switch-cabinet-select');
  const startUInput = elementCache.get('switch-start-u');
  const endUInput = elementCache.get('switch-end-u');
  
  if (cabinetSelect) {
    cabinetSelect.innerHTML = '<option value="">请先在IP配置中选择网络区域</option>';
    cabinetSelect.disabled = true;
  }
  if (startUInput) {
    startUInput.value = '';
    startUInput.disabled = true;
  }
  if (endUInput) {
    endUInput.value = '';
    endUInput.disabled = true;
  }
  
  const manager = getManager('switch');
  if (manager) {
    manager.setExcludeSwitchId(null);
    manager.clear();
    await manager.addIpRow();
  }
}
