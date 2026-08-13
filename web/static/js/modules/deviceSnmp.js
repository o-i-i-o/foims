import { apiPost, apiGet } from "../utils/apiClient.js";
import { showToast } from "../utils/ui.js";
import { elementCache } from "../utils/helpers.js";
import { getManager } from "../utils/ipconfig.js";
import { t } from "../utils/i18n.js";

const PASSWORD_MASK = '••••••••';

function maskToNull(value) {
  if (!value || value === PASSWORD_MASK || value.trim() === '') {
    return null;
  }
  return value;
}

export function getDeviceFormValues() {
  const manager = getManager('device');
  const rawIps = manager ? manager.getIps() : [];

  let networkRegionId = null;

  const ips = rawIps.map(ip => {
    if (ip.network_region_id) networkRegionId = ip.network_region_id;
    return ip;
  });

  return {
    id: elementCache.getValue('device-id'),
    name: elementCache.getValue('device-name'),
    model: elementCache.getValue('device-model') || null,
    vendor: elementCache.getValue('device-vendor') || null,
    location: elementCache.getValue('device-location') || null,
    description: elementCache.getValue('device-description') || null,
    snmp_version: elementCache.getValue('device-snmp-version'),
    snmp_port: parseInt(elementCache.getValue('device-snmp-port')) || 161,
    snmp_community: maskToNull(elementCache.getValue('device-snmp-community')),
    snmp_username: elementCache.getValue('device-snmp-username') || null,
    snmp_auth_protocol: elementCache.getValue('device-snmp-auth-protocol') || null,
    snmp_auth_password: maskToNull(elementCache.getValue('device-snmp-auth-password')),
    snmp_priv_protocol: elementCache.getValue('device-snmp-priv-protocol') || null,
    snmp_priv_password: maskToNull(elementCache.getValue('device-snmp-priv-password')),
    ips: ips,
    network_region_id: networkRegionId
  };
}

function buildSnmpRequestData(formData) {
  const ip = formData.ips && formData.ips.length > 0 ? formData.ips[0].ip_address : null;
  const { snmp_port: port, snmp_version: version } = formData;
  const data = {
    ip_address: ip,
    snmp_port: port,
    snmp_version: version
  };

  if (version === 'v3') {
    data.snmp_username = formData.snmp_username;
    data.snmp_auth_protocol = formData.snmp_auth_protocol;
    if (formData.snmp_auth_password) {
      data.snmp_auth_password = formData.snmp_auth_password;
    }
    data.snmp_priv_protocol = formData.snmp_priv_protocol;
    if (formData.snmp_priv_password) {
      data.snmp_priv_password = formData.snmp_priv_password;
    }
  } else {
    if (formData.snmp_community) {
      data.snmp_community = formData.snmp_community;
    }
  }

  return data;
}

export function toggleSnmpConfig() {
  const version = elementCache.getValue('device-snmp-version');
  const v2cSection = elementCache.get('snmp-v2c-config');
  const v3Section = elementCache.get('snmp-v3-config');

  if (v2cSection && v3Section) {
    if (version === 'v3') {
      v2cSection.classList.add('hidden');
      v2cSection.style.display = 'none';
      v3Section.classList.remove('hidden');
      v3Section.style.display = 'block';
    } else {
      v2cSection.classList.remove('hidden');
      v2cSection.style.display = 'block';
      v3Section.classList.add('hidden');
      v3Section.style.display = 'none';
    }
  }
}

export async function testSnmpConnection() {
  const formData = getDeviceFormValues();
  const ip = formData.ips && formData.ips.length > 0 ? formData.ips[0].ip_address : null;

  if (!ip) {
    showToast(t('device.ip_required'), 'warning');
    return;
  }

  const testBtn = elementCache.get('test-snmp-btn');
  const originalText = testBtn?.textContent || t('device.test_snmp');
  if (testBtn) {
    testBtn.disabled = true;
    testBtn.textContent = t('common.testing');
  }

  try {
    const testData = buildSnmpRequestData(formData);
    const deviceId = formData.id;

    if (deviceId) {
      testData.device_id = deviceId;
    }

    const result = await apiPost('/api/resources/devices/test-snmp', testData);

    if (result.success) {
      showToast(t('device.snmp_test_success'), 'success');
    } else {
      showToast(t('device.snmp_test_failed') + ': ' + result.message, 'error');
    }
  } catch (error) {
    showToast(t('device.snmp_test_failed'), 'error');
  } finally {
    if (testBtn) {
      testBtn.disabled = false;
      testBtn.textContent = originalText;
    }
  }
}

export async function getDeviceInfoFromSnmp() {
  const formData = getDeviceFormValues();
  const deviceId = formData.id;

  if (!deviceId) {
    const ip = formData.ips && formData.ips.length > 0 ? formData.ips[0].ip_address : null;
    if (!ip) {
      showToast(t('device.ip_required'), 'warning');
      return;
    }
    showToast(t('device.save_before_snmp'), 'warning');
    return;
  }

  const getInfoBtn = elementCache.get('get-snmp-info-btn');
  const originalText = getInfoBtn?.textContent || t('device.get_snmp_info');
  if (getInfoBtn) {
    getInfoBtn.disabled = true;
    getInfoBtn.textContent = t('device.fetching_snmp');
  }

  try {
    const result = await apiGet(`/api/resources/devices/${deviceId}/snmp-info`);

    if (result.success && result.data) {
      const info = result.data;
      if (info.vendor) elementCache.setValue('device-vendor', info.vendor);
      if (info.model) elementCache.setValue('device-model', info.model);
      showToast(t('device.snmp_info_success'), 'success');
    } else {
      showToast(t('device.snmp_info_failed') + ': ' + result.message, 'error');
    }
  } catch (error) {
    showToast(t('device.snmp_info_failed'), 'error');
  } finally {
    if (getInfoBtn) {
      getInfoBtn.disabled = false;
      getInfoBtn.textContent = originalText;
    }
  }
}
