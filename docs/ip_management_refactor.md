# IPMA IP管理模块重构 — 功能逻辑文档

> 版本：0.8.71  
> 更新日期：2026-05-16

---

## 一、模块概述

IP管理模块从 `ip_manager` 重构为 `ips`，核心变化是IP记录不再直接关联交换机（`switch_id`字段已移除），而是通过 `position_id` 间接关联。IP作为工位（workstation）和机位（position）的完整子元素，其管理权限严格限制在工位和机位页面。

---

## 二、数据库结构

### 2.1 ips 表

| 列名 | 类型 | 约束 | 说明 |
|------|------|------|------|
| id | UUID | PK | 主键 |
| workstation_id | UUID | FK → workstations(id) ON DELETE SET NULL | 关联工位 |
| position_id | UUID | FK → positions(id) ON DELETE SET NULL | 关联机位 |
| switch_port_id | UUID | FK → switch_ports(id) ON DELETE SET NULL | 关联交换机端口（非必填） |
| device_type | VARCHAR(20) | NOT NULL, CHECK IN ('workstation','cabinet_position','switch','unknown') | 设备类型 |
| network_id | UUID | NOT NULL, FK → network_cidrs(id) | 关联网段（必填） |
| ip_address | INET | NOT NULL | IP地址 |
| ip_version | SMALLINT | NOT NULL DEFAULT 4 | IP版本（4/6） |
| mac_address | VARCHAR(20) | | MAC地址 |
| hostname | VARCHAR(100) | | 主机名 |
| status | VARCHAR(20) | NOT NULL DEFAULT 'active' | 状态 |
| last_seen | TIMESTAMPTZ | NOT NULL DEFAULT NOW() | 最后发现时间 |
| last_mac | VARCHAR(20) | | 上次MAC地址（变更记录） |
| created_at | TIMESTAMPTZ | NOT NULL DEFAULT NOW() | 创建时间 |
| updated_at | TIMESTAMPTZ | NOT NULL DEFAULT NOW() | 更新时间 |

**约束 `chk_device_consistency`：**
- `switch` 类型：`position_id IS NOT NULL AND workstation_id IS NULL`
- `workstation` 类型：`workstation_id IS NOT NULL AND position_id IS NULL`
- `cabinet_position` 类型：`position_id IS NOT NULL AND workstation_id IS NULL`
- `unknown` 类型：`workstation_id IS NULL AND position_id IS NULL AND switch_port_id IS NULL`

### 2.2 ip_with_details 视图

提供IP记录的完整关联信息，用于列表展示和搜索：

| 列名 | 来源 | 说明 |
|------|------|------|
| id, workstation_id, position_id, switch_port_id | ips 表 | 基础字段 |
| device_type | ips 表 | 设备类型 |
| device_name | 计算字段 | 根据device_type显示设备名称 |
| network_id | ips 表 | 网段ID |
| workstation_name | workstations 表 | 工位名称 |
| cabinet_position_name | positions 表 | 机位名称 |
| switch_name | switches 表（通过 positions.device_type='switch' AND device_id 关联） | 交换机名称 |
| switch_port_number | switch_ports 表 | 交换机端口号 |
| room_name | rooms 表 | 房间名称 |
| cabinet_name | cabinets 表 | 机柜名称 |
| network_name | network_cidrs 表 | 网段名称 |
| network_region | network_regions 表 | 网络区域名称 |
| ip_address | host(ips.ip_address) | IP地址文本 |
| ip_version, mac_address, last_mac, hostname, status, last_seen, created_at, updated_at | ips 表 | 其他字段 |

### 2.3 关联关系图

```
network_regions ──1:N── network_cidrs ──1:N── ips
                                                │
                    workstations ──1:N──────────┤ (workstation_id)
                    positions ──1:N─────────────┤ (position_id)
                    switch_ports ──1:N──────────┘ (switch_port_id)
                         │
                         └── switches (通过 positions.device_type='switch' AND device_id)
```

---

## 三、API接口

### 3.1 IP查询接口（只读）

| 方法 | 路径 | 说明 | 权限 |
|------|------|------|------|
| GET | /api/resources/ip | 获取IP列表（分页+筛选） | 只读 |
| GET | /api/resources/ip/available/{network_id} | 获取网段可用IP | 只读 |
| GET | /api/resources/ip/workstation/{id} | 获取工位关联IP | 只读 |
| GET | /api/resources/ip/cabinet-position/{id} | 获取机位关联IP | 只读 |
| GET | /api/resources/ip/switch/{id} | 获取交换机关联IP | 只读 |

**GET /api/resources/ip 查询参数：**

| 参数 | 类型 | 说明 |
|------|------|------|
| search | string | 全局搜索（IP/MAC/主机名/设备名/网段名） |
| device_type | string | 设备类型过滤 |
| status | string | 状态过滤 |
| device_name | string | 设备名称过滤 |
| network | string | 网段名称过滤 |
| ip_address | string | IP地址过滤 |
| page | integer | 页码 |
| page_size | integer | 每页数量 |

### 3.2 MAC拉取接口

| 方法 | 路径 | 说明 | 权限 |
|------|------|------|------|
| POST | /api/resources/ip/pull | 从交换机MAC表拉取并更新MAC地址 | IP管理页面可用 |

**请求体：**
```json
{
  "switch_id": "uuid",
  "network_id": "uuid"
}
```

**处理逻辑：**
1. 查询 `switch_macs` 表获取指定交换机的MAC数据
2. 按 `network_id` 的CIDR范围过滤IP
3. 对每个IP：检查 `ips` 表中是否存在 → 检查MAC冲突 → 更新MAC地址
4. MAC变更时发送通知
5. 返回更新/跳过/未找到的统计

### 3.3 IP写入接口（仅工位/机位页面调用）

| 方法 | 路径 | 说明 | 调用方 |
|------|------|------|--------|
| POST | /api/resources/ip/auto-assign | 自动分配IP | 工位/机位页面 |
| POST | /api/resources/ip/batch | 批量创建IP | 工位/机位页面 |

**自动分配IP请求体：**
```json
{
  "network_id": "uuid",
  "workstation_id": "uuid (可选)",
  "position_id": "uuid (可选)",
  "switch_port_id": "uuid (可选)",
  "mac_address": "string (可选)",
  "hostname": "string (可选)"
}
```

---

## 四、前端权限控制

### 4.1 IP管理页面（只读）

- **数据展示**：IP列表表格，支持搜索、过滤、分页
- **MAC拉取**：选择交换机和网段后拉取MAC地址
- **禁止操作**：无新增IP、编辑IP、删除IP按钮
- **导出功能**：保留IP数据导出

### 4.2 工位页面（完整管理）

- **创建工位**：可同时配置IP地址（网段、IP、MAC、主机名）
- **编辑工位**：可修改关联的IP地址
- **删除工位**：级联删除关联IP
- **IP配置**：通过 `IpConfigManager` 管理，device_type 自动设为 `workstation`

### 4.3 机位页面（完整管理）

- **创建机位**：可同时配置IP地址
- **编辑机位**：可修改关联IP地址
- **删除机位**：级联删除关联IP
- **IP配置**：通过 `IpConfigManager` 管理，device_type 自动设为 `cabinet_position`

### 4.4 仪表盘（只读）

- 显示IP总数统计
- 显示活跃IP数量
- 按设备类型统计
- 按状态统计

---

## 五、交换机IP关联机制

### 5.1 旧机制（已废弃）

```
ips.switch_id → switches.id  （直接关联）
```

### 5.2 新机制

```
ips.position_id → positions.id
positions.device_type = 'switch'
positions.device_id → switches.id  （间接关联）
```

### 5.3 查询交换机IP的流程

1. 通过 `positions` 表查找：`SELECT id FROM positions WHERE device_type = 'switch' AND device_id = $switch_id`
2. 使用 `position_id` 查询 `ip_with_details` 视图获取IP列表

### 5.4 创建交换机IP的流程

1. 创建交换机时自动创建 `positions` 记录（`device_type='switch'`, `device_id=switch_id`）
2. 创建IP记录时使用 `position_id` 和 `device_type='switch'`

---

## 六、网络区域信息获取

IP的网络区域信息通过 `network_cidrs` 表间接获取：

```
ips.network_id → network_cidrs.id
network_cidrs.network_region_id → network_regions.id
```

`ip_with_details` 视图已包含 `network_region` 字段，无需额外查询。

---

## 七、已删除的表和功能

| 已删除 | 替代方案 |
|--------|----------|
| ip_managers 表 | ips 表 |
| ip_managers_with_details 视图 | ip_with_details 视图 |
| mac_history 表 | ips.last_mac 字段 |
| workstation_ports 表 | ips.switch_port_id 字段 |
| position_ports 表 | ips.switch_port_id 字段 |
| svg_layouts 表 | workstation_layouts + cabinet_layouts 表 |
| cabinets.network_id 字段 | 通过 room_networks 关联 |
| positions.network_id 字段 | 通过 ips.network_id 关联 |
| switches.network_region_id 字段 | 通过 positions → ips → network_cidrs 关联 |
| switches.network_id 字段 | 通过 positions → ips → network_cidrs 关联 |

---

## 八、文件修改清单

### 后端文件

| 文件 | 修改内容 |
|------|----------|
| src/models.rs | IpManager/IpManagerWithNames 删除 switch_id，添加 last_mac；Switch 相关删除 network_region_id/network_id，添加 position_id；Cabinet 删除 network_id；Position 添加 device_type/device_id |
| src/resource/ip.rs | 所有 SQL 表名 ip_managers→ips，视图 ip_managers_with_details→ip_with_details，删除 switch_id 引用，添加 last_mac |
| src/resource/workstation.rs | ip_managers→ips，svg_layouts→workstation_layouts |
| src/resource/switch/device.rs | ip_managers→ips，switch_id 查询改为 position 关联 |
| src/resource/switch/snmp.rs | ip_managers→ips，switch_id 查询改为 position 关联 |
| src/resource/switch/port.rs | ip_managers→ips，workstation_ports/position_ports 改为 ips.switch_port_id |
| src/resource/switch/lldp.rs | ip_managers→ips |
| src/resource/switch/mac.rs | ip_managers→ips |
| src/resource/cabinet.rs | ip_managers→ips |
| src/resource/drawing.rs | svg_layouts→workstation_layouts/cabinet_layouts |
| src/resource/network.rs | ip_managers→ips |
| src/system/data.rs | ip_managers→ips，交换机IP查询改为 position 关联 |
| src/system/config.rs | ip_managers→ips |
| src/init/schema.rs | 建表/索引/视图/触发器全面重构 |
| src/init/check.rs | 健康检查表名更新 |

### 前端文件

| 文件 | 修改内容 |
|------|----------|
| js/utils/ipconfig.js | 删除 switch_id 相关表单字段和数据处理 |
| js/modules/visualization/SVGDataManager.js | fetchIpManager→fetchIps |
| js/modules/visualization/SVGVisualization.js | 更新方法调用名 |
