// 导入必要的模块
import { apiGet } from "./apiClient.js";
import { showToast } from "./ui.js";

// 配置对象
const CONFIG = {
  workstation: {
    containerId: 'workstation-ips-container',
    classPrefix: 'workstation',
    networksApi: (id) => `/api/resources/rooms/${id}/networks`,
    idSelector: 'workstation-room',
    idName: '房间'
  },
  'cabinet-position': {
    containerId: 'cabinet-position-ips-container',
    classPrefix: 'cabinet-position',
    networksApi: (id) => `/api/resources/cabinets/${id}/networks`,
    idSelector: 'cabinet-position-cabinet',
    idName: '机柜'
  },
  switch: {
    containerId: 'switch-ips-container',
    classPrefix: 'switch',
    networksApi: () => '/api/resources/networks',
    excludeSwitchId: null  // 用于编辑模式下排除当前交换机
  }
};

// 缓存交换机数据
let switchesCache = null;

export class IpConfigManager {
  constructor(resourceType) {
    this.resourceType = resourceType;
    this.config = { ...CONFIG[resourceType] };
    if (!this.config) {
      console.error(`Invalid resource type: ${resourceType}`);
    }
  }

  getContainer() {
    return document.getElementById(this.config.containerId);
  }

  // 清空容器
  clear() {
    const container = this.getContainer();
    if (container) {
      container.innerHTML = '';
    }
  }

  // 设置要排除的交换机ID（用于编辑模式下排除当前交换机）
  setExcludeSwitchId(switchId) {
    this.config.excludeSwitchId = switchId;
  }

  // 加载已有的IP列表
  async loadIps(ips) {
    this.clear();
    if (ips && ips.length > 0) {
      for (const ip of ips) {
        await this.addIpRow(ip);
      }
    } else {
      // 如果没有IP，默认添加一行
      await this.addIpRow();
    }
  }

  // 获取表单中的IP列表
  getIps() {
    const container = this.getContainer();
    if (!container) return [];

    const rows = container.querySelectorAll(`.${this.config.classPrefix}-ip-row`);
    const ips = [];

    rows.forEach(row => {
      const networkRegionSelect = row.querySelector(`.${this.config.classPrefix}-ip-network-region-select`);
      const networkSelect = row.querySelector(`.${this.config.classPrefix}-ip-network-select`);
      const ipAddressInput = row.querySelector(`.${this.config.classPrefix}-ip-address-input`);
      const macAddressInput = row.querySelector(`.${this.config.classPrefix}-ip-mac-address-input`);
      const switchSelect = row.querySelector(`.${this.config.classPrefix}-switch-select`);
      const switchPortSelect = row.querySelector(`.${this.config.classPrefix}-switch-port-select`);

      if (networkSelect && ipAddressInput) {
        const networkId = networkSelect.value;
        const ipAddress = ipAddressInput.value.trim();
        
        // 只有当网络ID和IP地址都存在时才收集
        if (networkId && ipAddress) {
          const ipData = {
            network_id: networkId,
            ip_address: ipAddress,
            mac_address: macAddressInput ? macAddressInput.value.trim() || null : null,
          };

          // 对于非switch类型，收集交换机端口信息
          if (this.resourceType !== 'switch' && switchSelect && switchPortSelect && switchPortSelect.value) {
            ipData.switch_port_id = switchPortSelect.value;
          }
          
          // 如果有网络区域信息（通常用于交换机）
          if (networkRegionSelect && networkRegionSelect.value) {
            ipData.network_region_id = networkRegionSelect.value;
          }

          ips.push(ipData);
        }
      }
    });

    return ips;
  }

  // 添加一行IP输入
  async addIpRow(initialData = null) {
    const container = this.getContainer();
    if (!container) return;

    // 检查前置ID（如房间ID或机柜ID）
    if (this.config.idSelector && !initialData) {
      const id = document.getElementById(this.config.idSelector)?.value;
      if (!id) {
        showToast(`请先选择${this.config.idName}`, "warning");
        return;
      }
    }

    try {
      // 1. 加载网络数据
      const networks = await this.loadNetworks(initialData); // 传入initialData以便在编辑模式下能加载正确的网络列表

      // 2. 从网络数据中提取唯一的网络区域
      const regionMap = new Map();
      networks.forEach(network => {
        if (network.network_region_id && network.network_region) {
          regionMap.set(network.network_region_id, {
            id: network.network_region_id,
            name: network.network_region
          });
        }
      });
      const regions = Array.from(regionMap.values());

      // 3. 创建DOM元素
      const row = this.createIpRowElement(regions, networks || []);
      container.appendChild(row);

      // 4. 绑定事件并填充数据
      await this.bindRowEvents(row, networks || [], initialData);

    } catch (error) {
      console.error("添加IP行失败:", error);
      showToast("加载选项失败", "error");
    }
  }

  // 加载网络列表
  async loadNetworks(initialData) {
    let url;
    if (this.config.idSelector) {
      // 如果有前置选择器，优先使用当前选择的值
      // 如果是编辑模式且有initialData，可能需要特殊处理，但通常编辑时前置字段已设置
      const id = document.getElementById(this.config.idSelector)?.value;
      if (id) {
        url = this.config.networksApi(id);
      } else {
        // 如果没有ID，可能无法加载网络（取决于API）
        return [];
      }
    } else {
      url = this.config.networksApi();
    }
    
    const result = await apiGet(url);
    return result.data || [];
  }

  // 创建HTML结构
  createIpRowElement(regions, networks) {
    const div = document.createElement("div");
    div.className = `${this.config.classPrefix}-ip-row form-row-container`;
    
    // 生成选项HTML
    const regionOptions = regions.map(r => `<option value="${r.id}">${r.name}</option>`).join('');
    const networkOptions = networks.map(n => `<option value="${n.id}">${n.name} (${n.ipv4_cidr || n.ipv6_cidr || '无CIDR'})</option>`).join('');

    div.innerHTML = `
      <div class="form-row">
        <div class="form-group">
          <label>网络区域</label>
          <select class="${this.config.classPrefix}-ip-network-region-select form-control">
            <option value="">请选择区域</option>
            ${regionOptions}
          </select>
        </div>
        <div class="form-group">
          <label>网络</label>
          <select class="${this.config.classPrefix}-ip-network-select form-control">
            <option value="">请选择网络</option>
            ${networkOptions}
          </select>
        </div>
      </div>
      <div class="form-row">
        <div class="form-group">
          <label>IP地址</label>
          <input type="text" class="${this.config.classPrefix}-ip-address-input form-control" placeholder="如: 192.168.1.100">
        </div>
        <div class="form-group">
          <label>MAC地址</label>
          <input type="text" class="${this.config.classPrefix}-ip-mac-address-input form-control" placeholder="如: 00:11:22:33:44:55">
        </div>
      </div>
      <div class="form-row">
        <div class="form-group">
          <label>交换机</label>
          <select class="${this.config.classPrefix}-switch-select form-control">
            <option value="">选择交换机</option>
          </select>
        </div>
        <div class="form-group">
          <label>端口</label>
          <select class="${this.config.classPrefix}-switch-port-select form-control">
            <option value="">选择端口</option>
          </select>
        </div>
      </div>
      <div class="form-row">
        <div class="form-group" style="display: flex; align-items: flex-end; gap: 8px;">
          <button type="button" class="btn btn-danger btn-sm remove-ip-btn">删除</button>
          <button type="button" class="btn btn-secondary btn-sm add-ip-btn" data-i18n="ip.add_ip">添加IP地址</button>
        </div>
      </div>
    `;
    return div;
  }

  // 绑定事件并初始化数据
  async bindRowEvents(row, networks, initialData) {
    // 删除按钮
    row.querySelector(".remove-ip-btn")?.addEventListener("click", () => row.remove());
    
    // 添加IP地址按钮
    row.querySelector(".add-ip-btn")?.addEventListener("click", () => this.addIpRow());

    const regionSelect = row.querySelector(`.${this.config.classPrefix}-ip-network-region-select`);
    const networkSelect = row.querySelector(`.${this.config.classPrefix}-ip-network-select`);
    const ipInput = row.querySelector(`.${this.config.classPrefix}-ip-address-input`);
    const macInput = row.querySelector(`.${this.config.classPrefix}-ip-mac-address-input`);
    const switchSelect = row.querySelector(`.${this.config.classPrefix}-switch-select`);
    const portSelect = row.querySelector(`.${this.config.classPrefix}-switch-port-select`);

    // 区域与网络的联动
    if (regionSelect && networkSelect) {
      regionSelect.addEventListener("change", () => {
        const regionId = regionSelect.value;
        const filtered = networks.filter(n => n.network_region_id === regionId);
        
        networkSelect.innerHTML = '<option value="">请选择网络</option>' + 
          filtered.map(n => `<option value="${n.id}">${n.name} (${n.ipv4_cidr || n.ipv6_cidr || '无CIDR'})</option>`).join('');
        
        // 如果只有一个选项，自动选择
        if (filtered.length === 1) {
            networkSelect.value = filtered[0].id;
        }
      });
    }

    // 加载交换机列表
    await this.loadSwitches(switchSelect, portSelect);

    // 填充初始数据
    if (initialData) {
      // 1. 设置区域
      let regionId = initialData.network_region_id;
      if (!regionId && initialData.network_id) {
        const network = networks.find(n => n.id === initialData.network_id);
        if (network) {
          regionId = network.network_region_id;
        }
      }

      if (regionId && regionSelect) {
        regionSelect.value = regionId;
        regionSelect.dispatchEvent(new Event('change'));
      }

      // 2. 设置网络
      if (initialData.network_id && networkSelect) {
        networkSelect.value = initialData.network_id;
      }

      // 3. 设置IP和MAC
      if (initialData.ip_address && ipInput) ipInput.value = initialData.ip_address;
      if (initialData.mac_address && macInput) macInput.value = initialData.mac_address;

      // 4. 设置交换机和端口
      let switchId = initialData.switch_id;
      // 兼容直接从交换机信息获取parent信息的情况
      if (this.resourceType === 'switch' && !switchId && initialData.parent_switch_id) {
          switchId = initialData.parent_switch_id;
      }
      
      let portId = initialData.switch_port_id || initialData.port_id;
      // 兼容直接从交换机信息获取parent信息的情况
      if (this.resourceType === 'switch' && !portId && initialData.parent_port_id) {
          portId = initialData.parent_port_id;
      }

      if (switchId && switchSelect) {
        // 确保UUID格式一致（转为小写）
        const normalizedSwitchId = typeof switchId === 'string' ? switchId.toLowerCase() : switchId;
        
        // 查找匹配的选项
        const switchOptions = switchSelect.querySelectorAll('option');
        let found = false;
        for (const opt of switchOptions) {
          if (opt.value && opt.value.toLowerCase() === normalizedSwitchId) {
            switchSelect.value = opt.value;
            found = true;
            break;
          }
        }
        
        if (found) {
          // 等待端口加载完成
          await this.handleSwitchChange(switchSelect, portSelect);
          
          // 端口加载完成后再设置端口值
          if (portId && portSelect) {
            const normalizedPortId = typeof portId === 'string' ? portId.toLowerCase() : portId;
            const portOptions = portSelect.querySelectorAll('option');
            for (const opt of portOptions) {
              if (opt.value && opt.value.toLowerCase() === normalizedPortId) {
                portSelect.value = opt.value;
                break;
              }
            }
          }
        }
      }
    } else {
        if (regionSelect) {
             regionSelect.dispatchEvent(new Event('change'));
        }
    }
  }

  // 加载交换机数据到Select
  async loadSwitches(switchSelect, portSelect) {
    if (!switchSelect || !portSelect) return;

    try {
      if (!switchesCache) {
        const result = await apiGet("/api/switches");
        switchesCache = result.data || [];
      }
      
      // 清空并添加默认选项
      switchSelect.innerHTML = '<option value="">选择交换机</option>';
      
      // 对于switch类型，过滤掉当前编辑的交换机
      let filteredSwitches = switchesCache;
      if (this.resourceType === 'switch' && this.config.excludeSwitchId) {
        filteredSwitches = switchesCache.filter(sw => sw.id !== this.config.excludeSwitchId);
      }
      
      filteredSwitches.forEach(sw => {
        const option = document.createElement("option");
        option.value = sw.id;
        option.textContent = sw.name;
        switchSelect.appendChild(option);
      });

      // 绑定交换机变更事件
      switchSelect.addEventListener("change", () => this.handleSwitchChange(switchSelect, portSelect));
    } catch (error) {
      console.error("加载交换机失败:", error);
    }
  }

  // 处理交换机变更
  async handleSwitchChange(switchSelect, portSelect) {
    const switchId = switchSelect.value;
    portSelect.innerHTML = '<option value="">选择端口</option>';
    
    if (!switchId) return;
    
    try {
      const result = await apiGet(`/api/switches/${switchId}/ports`);
      if (result.success && result.data) {
          // 排序端口
          const ports = result.data.sort((a, b) => {
              // 简单的数字提取排序
              const getNum = (s) => {
                  const m = s.match(/\d+/g);
                  return m ? parseInt(m[m.length-1]) : 0;
              };
              return getNum(a.port_number) - getNum(b.port_number);
          });

          ports.forEach(port => {
            const option = document.createElement("option");
            option.value = port.id;
            // 显示端口状态
            const statusMark = port.status === 'up' ? '🟢' : '🔴';
            option.textContent = `${statusMark} ${port.port_number}${port.port_name ? ` (${port.port_name})` : ""}`;
            portSelect.appendChild(option);
          });
      }
    } catch (error) {
      console.error("加载交换机端口失败:", error);
    }
  }
}

// ==========================================
// 兼容性导出 (Wrapper Functions)
// ==========================================

const managers = {};

function getManager(type) {
  if (!managers[type]) {
    managers[type] = new IpConfigManager(type);
  }
  return managers[type];
}

export async function addIpAddressField(resourceType) {
  const manager = getManager(resourceType);
  await manager.addIpRow();
}

// 绑定按钮
export function bindButton(buttonId, resourceType) {
  const button = document.getElementById(buttonId);
  if (!button) return;

  const newButton = button.cloneNode(true);
  button.replaceWith(newButton);
  newButton.addEventListener("click", () => addIpAddressField(resourceType));
}

// 特定资源导出
export const handleWorkstationRoomChange = async () => {
  const manager = getManager('workstation');
  manager.clear();
  // 房间改变后自动添加一行IP输入
  await manager.addIpRow();
};

export const handleCabinetPositionCabinetChange = async () => {
  const manager = getManager('cabinet-position');
  manager.clear();
  // 机柜改变后自动添加一行IP输入
  await manager.addIpRow();
};

export const addIpAddressFieldToWorkstationForm = () => addIpAddressField('workstation');
export const addIpAddressFieldToCabinetPositionForm = () => addIpAddressField('cabinet-position');
export const addIpAddressFieldToSwitchForm = () => addIpAddressField('switch');

export const bindWorkstationAddIpButton = () => {};
export const bindCabinetPositionAddIpButton = () => {};
export const bindSwitchAddIpButton = () => {};

// 导出 getManager 函数
export { getManager };
