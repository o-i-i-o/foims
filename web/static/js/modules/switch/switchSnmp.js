import { apiPost, apiGet } from "../../utils/apiClient.js";
import { showToast } from "../../utils/ui.js";
import { elementCache } from "../../utils/helpers.js";
import { getSwitchFormValues } from "./switchForm.js";

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
  const version = elementCache.getValue('switch-snmp-version');
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
  const formData = getSwitchFormValues();
  const ip = formData.ips && formData.ips.length > 0 ? formData.ips[0].ip_address : null;

  if (!ip) {
    showToast('请输入交换机IP地址', 'warning');
    return;
  }

  const testBtn = elementCache.get('test-snmp-btn');
  const originalText = testBtn?.textContent || '测试连接';
  if (testBtn) {
    testBtn.disabled = true;
    testBtn.textContent = '测试中...';
  }

  try {
    const testData = buildSnmpRequestData(formData);
    const switchId = formData.id;

    if (switchId) {
      testData.switch_id = switchId;
    }

    const result = await apiPost('/api/switches/test-snmp', testData);

    if (result.success) {
      showToast('SNMP连接测试成功', 'success');
    } else {
      showToast('SNMP连接测试失败: ' + result.message, 'error');
    }
  } catch (error) {
    showToast('SNMP连接测试失败', 'error');
  } finally {
    if (testBtn) {
      testBtn.disabled = false;
      testBtn.textContent = originalText;
    }
  }
}

export async function getSwitchInfoFromSnmp() {
  const formData = getSwitchFormValues();
  const switchId = formData.id;

  if (!switchId) {
    const ip = formData.ips && formData.ips.length > 0 ? formData.ips[0].ip_address : null;
    if (!ip) {
      showToast('请输入交换机IP地址', 'warning');
      return;
    }
    showToast('请先保存交换机后再获取SNMP信息', 'warning');
    return;
  }

  const getInfoBtn = elementCache.get('get-snmp-info-btn');
  const originalText = getInfoBtn?.textContent || '获取信息';
  if (getInfoBtn) {
    getInfoBtn.disabled = true;
    getInfoBtn.textContent = '获取中...';
  }

  try {
    const result = await apiGet(`/api/switches/${switchId}/snmp-info`);

    if (result.success && result.data) {
      const info = result.data;
      if (info.vendor) elementCache.setValue('switch-vendor', info.vendor);
      if (info.model) elementCache.setValue('switch-model', info.model);
      showToast('交换机信息获取成功', 'success');
    } else {
      showToast('获取交换机信息失败: ' + result.message, 'error');
    }
  } catch (error) {
    showToast('获取交换机信息失败', 'error');
  } finally {
    if (getInfoBtn) {
      getInfoBtn.disabled = false;
      getInfoBtn.textContent = originalText;
    }
  }
}

export async function syncPortsFromSnmp(switchId) {
  const getPortsBtn = elementCache.get('get-snmp-ports-btn');
  const originalText = getPortsBtn?.textContent || '从SNMP获取端口';
  if (getPortsBtn) {
    getPortsBtn.disabled = true;
    getPortsBtn.textContent = '同步中...';
  }

  try {
    const result = await apiPost(`/api/switches/${switchId}/ports/sync-snmp`, {});

    if (result.success) {
      showToast(result.message || '端口同步成功', 'success');
      return result.data || [];
    } else {
      showToast('端口同步失败: ' + result.message, 'error');
      return [];
    }
  } catch (error) {
    showToast('端口同步失败', 'error');
    return [];
  } finally {
    if (getPortsBtn) {
      getPortsBtn.disabled = false;
      getPortsBtn.textContent = originalText;
    }
  }
}
