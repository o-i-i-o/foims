import { apiPost } from "../../utils/apiClient.js";
import { showToast } from "../../utils/ui.js";
import { elementCache } from "../../utils/helpers.js";

function buildSnmpRequestData(formData) {
  const { name: ip, snmp_port: port, snmp_version: version } = formData;
  const data = { ip, port, version };
  
  if (version === 'v3') {
    data.username = formData.snmp_username;
    data.auth_protocol = formData.snmp_auth_protocol;
    data.auth_password = formData.snmp_auth_password;
    data.priv_protocol = formData.snmp_priv_protocol;
    data.priv_password = formData.snmp_priv_password;
  } else {
    data.community = formData.snmp_community || 'public';
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

export async function testSnmpConnection(getFormData) {
  const formData = getFormData();
  
  if (!formData.name) {
    showToast('请输入交换机IP地址', 'warning');
    return;
  }

  const testBtn = elementCache.get('test-snmp-btn');
  const originalText = testBtn.textContent;
  testBtn.disabled = true;
  testBtn.textContent = '测试中...';

  try {
    const testData = buildSnmpRequestData(formData);
    const result = await apiPost('/api/switches/snmp/test', testData);

    if (result.success) {
      showToast('SNMP连接测试成功', 'success');
    } else {
      showToast('SNMP连接测试失败: ' + result.message, 'error');
    }
  } catch (error) {
    showToast('SNMP连接测试失败', 'error');
  } finally {
    testBtn.disabled = false;
    testBtn.textContent = originalText;
  }
}

export async function getSwitchInfoFromSnmp(getFormData) {
  const formData = getFormData();
  
  if (!formData.name) {
    showToast('请输入交换机IP地址', 'warning');
    return;
  }

  const getInfoBtn = elementCache.get('get-snmp-info-btn');
  const originalText = getInfoBtn.textContent;
  getInfoBtn.disabled = true;
  getInfoBtn.textContent = '获取中...';

  try {
    const requestData = buildSnmpRequestData(formData);
    const result = await apiPost('/api/switches/snmp/info', requestData);

    if (result.success && result.data) {
      const info = result.data;
      if (info.vendor) elementCache.setValue('switch-vendor', info.vendor);
      if (info.model) elementCache.setValue('switch-model', info.model);
      if (info.description) elementCache.setValue('switch-description', info.description);
      showToast('交换机信息获取成功', 'success');
    } else {
      showToast('获取交换机信息失败: ' + result.message, 'error');
    }
  } catch (error) {
    showToast('获取交换机信息失败', 'error');
  } finally {
    getInfoBtn.disabled = false;
    getInfoBtn.textContent = originalText;
  }
}

export async function getSwitchPortsFromSnmp(switchId) {
  const getInfoBtn = elementCache.get('get-switch-ports-btn');
  const originalText = getInfoBtn?.textContent || '获取端口';
  if (getInfoBtn) {
    getInfoBtn.disabled = true;
    getInfoBtn.textContent = '获取中...';
  }

  try {
    const result = await apiPost(`/api/switches/${switchId}/snmp/ports`, {});

    if (result.success) {
      showToast(`成功获取 ${result.data?.length || 0} 个端口`, 'success');
      return result.data || [];
    } else {
      showToast('获取端口信息失败: ' + result.message, 'error');
      return [];
    }
  } catch (error) {
    showToast('获取端口信息失败', 'error');
    return [];
  } finally {
    if (getInfoBtn) {
      getInfoBtn.disabled = false;
      getInfoBtn.textContent = originalText;
    }
  }
}
