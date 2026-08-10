# 资源管理 - 设备模块 API 文档

本文档详细列出 IPMA 项目"资源管理 - 设备"模块的前后端 API 及其数据结构。

## 目录

- [通用说明](#通用说明)
- [1. 设备管理（CRUD）](#1-设备管理crud)
- [2. 设备模板管理](#2-设备模板管理)
- [3. 设备网卡配置（网卡 → 网口 → IP）](#3-设备网卡配置网卡--网口--ip)
- [4. 设备三层接口（网口）管理](#4-设备三层接口网口管理)
- [5. 交换机端口（二层口）管理](#5-交换机端口二层口管理)
- [6. 设备 IP 管理](#6-设备-ip-管理)
- [7. SNMP 相关功能](#7-snmp-相关功能)
- [8. MAC 表（ARP 表）管理](#8-mac-表arp-表管理)
- [9. LLDP 邻居管理](#9-lldp-邻居管理)
- [数据模型汇总](#数据模型汇总)

---

## 通用说明

### 响应格式

所有 API 统一返回 [`ApiResponse<T>`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/models.rs#L110-L115) 结构：

```json
{
  "success": true,
  "message": "操作描述",
  "data": { ... }
}
```

成功响应 HTTP 状态码为 `200`，错误响应参见 [`AppError`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/error.rs)。SNMP 凭据（`snmp_community`、`snmp_auth_password`、`snmp_priv_password`）在数据库中加密存储，详情接口会解密后返回，列表接口会置为 `null`。

### 认证

所有 `/api/resources/devices/...` 路由均挂载 [`auth_middleware`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/routes/mod.rs#L546-L549)，需要在请求头携带 `Authorization: Bearer <access_token>`。

### 路由汇总

完整路由定义见 [src/routes/mod.rs:362-443](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/routes/mod.rs#L362-L443)。

---

## 1. 设备管理（CRUD）

后端实现：[src/resource/device/crud.rs](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/crud.rs)

### 1.1 获取设备列表

| 项目 | 内容 |
|---|---|
| Method | `GET` |
| URL | `/api/resources/devices` |
| 前端调用 | [device.js:82](file:///media/oi-io/AA709DF48A7AD5C2/ipma/web/static/js/modules/device.js#L82)、[ipmanager.js:35](file:///media/oi-io/AA709DF48A7AD5C2/ipma/web/static/js/modules/ipmanager.js#L35) |
| 后端处理 | [`get_devices`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/crud.rs#L22-L176) |

**Query 参数：**

| 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|
| page | i64 | 否 | 页码，默认 1 |
| page_size | i64 | 否 | 每页数量，默认 20 |
| search | String | 否 | 在 name/brand/model/serial_number/description 中模糊搜索 |
| workstation_id | UUID | 否 | 按工位过滤 |
| position_id | UUID | 否 | 按机位过滤 |
| device_type | String | 否 | 按设备类型过滤（pc/laptop/printer/server/network_device/switch/camera/phone/other） |
| room_id | UUID | 否 | 按房间过滤 |
| sort_by | String | 否 | 排序字段：name/device_type/created_at，默认 name |
| sort_order | String | 否 | asc/desc，默认 asc |

**响应 data：**

```json
{
  "items": [ DeviceWithDetails, ... ],
  "total": 100,
  "page": 1,
  "page_size": 20,
  "total_pages": 5
}
```

> 注意：列表中的 `snmp_community`、`snmp_auth_password`、`snmp_priv_password` 字段会被置为 `null`（参见 [crud.rs:159-162](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/crud.rs#L159-L162)）。

### 1.2 创建设备

| 项目 | 内容 |
|---|---|
| Method | `POST` |
| URL | `/api/resources/devices` |
| 前端调用 | [device.js:555](file:///media/oi-io/AA709DF48A7AD5C2/ipma/web/static/js/modules/device.js#L555)（通过 `handleFormSubmit`） |
| 后端处理 | [`create_device`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/crud.rs#L178-L411) |

**请求体：[`DeviceCreate`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/models.rs#L1508-L1546)**

```json
{
  "name": "核心交换机A",          // 必填，1-100 字符
  "device_type": "switch",        // 可选，未提供时使用 "other"
  "brand": "Cisco",
  "model": "Catalyst 9300",
  "serial_number": "SN123456",
  "workstation_id": null,         // 工位 ID，与 position_id 互斥
  "position_id": null,            // 机位 ID，与 workstation_id 互斥
  "room_id": "uuid-xxx",          // 必填
  "template_id": null,            // 引用设备模板
  "vendor": "Cisco",
  "location": "机柜A-1U",
  "snmp_version": "v2c",          // 默认 v2c
  "snmp_community": "public",
  "snmp_username": null,
  "snmp_auth_protocol": null,
  "snmp_auth_password": null,
  "snmp_priv_protocol": null,
  "snmp_priv_password": null,
  "snmp_port": 161,               // 默认 161
  "cards": [ NetworkCardSyncItem, ... ],  // 可选，网卡配置
  "description": "核心交换机",
  "save_as_template": false,      // 是否同时保存为模板
  "template_name": null           // 模板名称（save_as_template=true 时生效）
}
```

**响应 data：[`Device`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/models.rs#L1447-L1472)**，message = "设备创建成功"

**业务逻辑：**
- 验证 `workstation_id` 和 `position_id` 互斥（[crud.rs:193-195](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/crud.rs#L193-L195)）
- 若提供 `template_id`，从模板填充 `device_type`/`brand`/`model` 的默认值（[crud.rs:221-259](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/crud.rs#L221-L259)）
- 通过 [`apply_network_config`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/nic.rs#L83-L239) 应用网卡配置；`cards` 为空时自动生成默认网卡+网口
- SNMP 凭据使用 [`encrypt_password_async`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/crypto.rs) 加密存储

### 1.3 获取设备详情

| 项目 | 内容 |
|---|---|
| Method | `GET` |
| URL | `/api/resources/devices/{id}` |
| 前端调用 | [device.js:158](file:///media/oi-io/AA709DF48A7AD5C2/ipma/web/static/js/modules/device.js#L158)、[deviceMacLldp.js:86](file:///media/oi-io/AA709DF48A7AD5C2/ipma/web/static/js/modules/deviceMacLldp.js#L86)、[deviceMacLldp.js:360](file:///media/oi-io/AA709DF48A7AD5C2/ipma/web/static/js/modules/deviceMacLldp.js#L360)、[devicePorts.js:163](file:///media/oi-io/AA709DF48A7AD5C2/ipma/web/static/js/modules/devicePorts.js#L163)、[unifiedDevicePorts.js:57](file:///media/oi-io/AA709DF48A7AD5C2/ipma/web/static/js/modules/unifiedDevicePorts.js#L57) |
| 后端处理 | [`get_device`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/crud.rs#L413-L459) |

**路径参数：** `id` - 设备 UUID

**响应 data：** [`DeviceWithDetails`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/models.rs#L1474-L1506) + `cards` 字段（嵌套的网卡/网口/IP 配置，通过 [`fetch_device_network_config`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/nic.rs#L242-L293) 获取）

> 详情接口会解密返回 SNMP 凭据（[crud.rs:445-456](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/crud.rs#L445-L456)）。

### 1.4 更新设备

| 项目 | 内容 |
|---|---|
| Method | `PUT` |
| URL | `/api/resources/devices/{id}` |
| 前端调用 | [device.js:555](file:///media/oi-io/AA709DF48A7AD5C2/ipma/web/static/js/modules/device.js#L555)（通过 `handleFormSubmit`） |
| 后端处理 | [`update_device`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/crud.rs#L461-L724) |

**请求体：[`DeviceUpdate`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/models.rs#L1548-L1587)**

```json
{
  "name": "新名称",                       // 可选
  "device_type": "switch",                // 可选
  "brand": "Cisco",                       // 可选
  "model": null,                          // 可选
  "serial_number": null,                  // 可选
  "workstation_id": null,                 // Option<Option<Uuid>>：null=不变，Some(null)=清空，Some(uuid)=设置
  "position_id": null,                    // Option<Option<Uuid>>：同上
  "room_id": null,                        // 可选
  "vendor": null,
  "location": null,
  "snmp_version": null,
  "snmp_community": null,                 // 提供空字符串=清空
  "snmp_username": null,
  "snmp_auth_protocol": null,
  "snmp_auth_password": null,
  "snmp_priv_protocol": null,
  "snmp_priv_password": null,
  "snmp_port": null,
  "cards": null,                          // 提供时整体替换网卡配置
  "description": null,
  "save_as_template": false,
  "template_name": null
}
```

**响应 data：** 更新后的 `DeviceWithDetails` + `cards`，message = "设备更新成功"

### 1.5 删除设备

| 项目 | 内容 |
|---|---|
| Method | `DELETE` |
| URL | `/api/resources/devices/{id}` |
| 前端调用 | [device.js:170](file:///media/oi-io/AA709DF48A7AD5C2/ipma/web/static/js/modules/device.js#L170)（通过 `handleDelete`） |
| 后端处理 | [`delete_device`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/crud.rs#L726-L771) |

**响应 data：** `null`，message = "设备删除成功"

---

## 2. 设备模板管理

后端实现：[src/resource/device/template.rs](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/template.rs)

### 2.1 获取设备模板列表

| 项目 | 内容 |
|---|---|
| Method | `GET` |
| URL | `/api/resources/device-templates` |
| 前端调用 | [device.js:267](file:///media/oi-io/AA709DF48A7AD5C2/ipma/web/static/js/modules/device.js#L267) |
| 后端处理 | [`get_device_templates`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/template.rs#L16-L29) |

**响应 data：**

```json
{
  "items": [ DeviceTemplateSummary, ... ]
}
```

模板按 `created_at ASC` 排序。

### 2.2 获取设备模板详情

| 项目 | 内容 |
|---|---|
| Method | `GET` |
| URL | `/api/resources/device-templates/{id}` |
| 前端调用 | [device.js:205](file:///media/oi-io/AA709DF48A7AD5C2/ipma/web/static/js/modules/device.js#L205)、[device.js:321](file:///media/oi-io/AA709DF48A7AD5C2/ipma/web/static/js/modules/device.js#L321) |
| 后端处理 | [`get_device_template`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/template.rs#L32-L46) |

**响应 data：[`DeviceTemplate`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/models.rs#L1413-L1423)**，message = "设备模板获取成功"

### 2.3 更新设备模板

| 项目 | 内容 |
|---|---|
| Method | `PUT` |
| URL | `/api/resources/device-templates/{id}` |
| 前端调用 | [device.js:381](file:///media/oi-io/AA709DF48A7AD5C2/ipma/web/static/js/modules/device.js#L381) |
| 后端处理 | [`update_device_template`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/template.rs#L105-L163) |

**请求体：[`UpdateDeviceTemplateRequest`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/models.rs#L1434-L1443)**

```json
{
  "name": "模板名称",          // 必填，1-100 字符
  "device_type": "switch",     // 必填，1-30 字符
  "brand": "Cisco",            // 可选
  "model": "Catalyst 9300",    // 可选
  "description": "..."         // 可选
}
```

**响应 data：** `null`，message = "设备模板更新成功"

**业务逻辑：** 名称冲突时返回 409 Conflict。

### 2.4 删除设备模板

| 项目 | 内容 |
|---|---|
| Method | `DELETE` |
| URL | `/api/resources/device-templates/{id}` |
| 前端调用 | [device.js:405](file:///media/oi-io/AA709DF48A7AD5C2/ipma/web/static/js/modules/device.js#L405) |
| 后端处理 | [`delete_device_template`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/template.rs#L49-L102) |

**响应 data：** `null`，message = "设备模板删除成功"

**业务逻辑：** 若有设备正在使用该模板，返回 400 Validation（含使用数量）。

---

## 3. 设备网卡配置（网卡 → 网口 → IP）

后端实现：[src/resource/device/nic.rs](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/nic.rs)

### 3.1 同步设备网卡配置（整体替换）

| 项目 | 内容 |
|---|---|
| Method | `PUT` |
| URL | `/api/resources/devices/{id}/network-config` |
| 后端处理 | [`sync_device_network_config`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/nic.rs#L28-L79) |

**请求体：[`DeviceNetworkConfigSync`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/models.rs#L793-L797)**

```json
{
  "cards": [ NetworkCardSyncItem, ... ]   // 为空时自动生成默认网卡+网口
}
```

**[`NetworkCardSyncItem`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/models.rs#L781-L791)：**

```json
{
  "id": null,                    // 可选，提供时复用 ID
  "name": "网卡1",                // 必填，1-50 字符
  "card_type": "pcie",           // 可选，pcie/onboard/usb/virtual/wwan/wifi/other
  "description": null,
  "ports": [ PortSyncItem, ... ]
}
```

**[`PortSyncItem`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/models.rs#L762-L779)：**

```json
{
  "id": null,
  "name": "eth0",                // 必填，1-50 字符
  "interface_type": "physical",  // 可选，physical/svi/management/loopback/wifi
  "mac_address": null,           // 最长 20 字符
  "vlan_id": null,
  "description": null,
  "switch_id": null,             // 上级交换机设备 ID
  "uplink_interface_id": null,   // 上级接口 ID
  "ips": [ IpSyncItem, ... ]
}
```

**[`IpSyncItem`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/models.rs#L752-L760)：**

```json
{
  "id": null,
  "network_id": null,            // 不提供时按 room_id 自动匹配
  "ip_address": "192.168.1.10",  // 必填，需为合法 IP
  "description": null
}
```

**响应 data：** 同步后的网卡配置（`Vec<serde_json::Value>`，结构同下"获取网卡配置"），message = "网卡配置同步成功"

**业务逻辑：**
- 整体替换：先删除设备所有 IP / cable_links / device_interfaces / nics，再按 `cards` 重建（[nic.rs:91-111](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/nic.rs#L91-L111)）
- `cards` 为空时，自动生成默认网卡（"网卡1" + pcie 类型）和默认网口（"eth0" + physical 类型）
- IP 重复时返回 409 Conflict
- 通过 [`validate_and_resolve_outlet_chain`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/interface.rs#L140-L202) 验证信息点链并自动推导上级端口

### 3.2 获取设备网卡配置（嵌入设备详情）

前端在 [unifiedDevicePorts.js:124](file:///media/oi-io/AA709DF48A7AD5C2/ipma/web/static/js/modules/unifiedDevicePorts.js#L124) 调用 `GET /api/resources/devices/{deviceId}/nics`，但**该路由在后端 [routes/mod.rs](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/routes/mod.rs#L362-L443) 中未定义**。

实际获取网卡配置的方式：通过 `GET /api/resources/devices/{id}` 设备详情接口返回的 `cards` 字段（由 [`fetch_device_network_config`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/nic.rs#L242-L293) 在 [crud.rs:438](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/crud.rs#L438) 拼装）。

**`cards` 字段结构（[`fetch_device_network_config`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/nic.rs#L242-L293) 返回）：**

```json
[
  {
    "id": "uuid",
    "device_id": "uuid",
    "name": "网卡1",
    "card_type": "pcie",
    "description": null,
    "sort_order": 0,
    "created_at": "2026-...",
    "updated_at": "2026-...",
    "ports": [
      {
        "id": "uuid",
        "device_id": "uuid",
        "nic_id": "uuid",
        "name": "eth0",
        "interface_type": "physical",
        "mac_address": null,
        "vlan_id": null,
        "description": null,
        "switch_id": null,
        "uplink_interface_id": null,
        "sort_order": 0,
        "created_at": "...",
        "updated_at": "...",
        "ips": [ IpManager, ... ]
      }
    ]
  }
]
```

---

## 4. 设备三层接口（网口）管理

后端实现：[src/resource/device/interface.rs](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/interface.rs)

### 4.1 获取设备接口列表（分页）

| 项目 | 内容 |
|---|---|
| Method | `GET` |
| URL | `/api/resources/devices/{id}/interfaces` |
| 后端处理 | [`get_device_interfaces`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/interface.rs#L21-L56) |

**Query 参数：** `page`、`page_size`

**响应 data：**

```json
{
  "items": [ DeviceInterface, ... ],
  "total": 10,
  "page": 1,
  "page_size": 20,
  "total_pages": 1
}
```

### 4.2 获取所有设备接口（分页，跨设备）

| 项目 | 内容 |
|---|---|
| Method | `GET` |
| URL | `/api/resources/devices/interfaces` |
| 后端处理 | [`get_all_device_interfaces`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/interface.rs#L58-L135) |

**Query 参数：** `page`、`page_size`、`search`（在设备名/接口名/MAC/描述中模糊搜索）

**响应 data：**

```json
{
  "items": [ DeviceInterfaceWithDevice, ... ],
  "total": 10,
  "page": 1,
  "page_size": 20,
  "total_pages": 1
}
```

### 4.3 创建设备接口

| 项目 | 内容 |
|---|---|
| Method | `POST` |
| URL | `/api/resources/devices/{id}/interfaces` |
| 后端处理 | [`create_device_interface`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/interface.rs#L204-L310) |

**请求体：[`DeviceInterfaceCreate`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/models.rs#L964-L978)**

```json
{
  "name": "GigabitEthernet0/1",  // 必填，1-50 字符
  "interface_type": "physical",  // 可选，physical/svi/management/loopback/wifi，默认 physical
  "mac_address": null,           // 最长 20 字符
  "vlan_id": null,
  "description": null,
  "switch_id": null,
  "uplink_interface_id": null
}
```

**响应 data：[`DeviceInterface`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/models.rs#L925-L942)**，message = "创建接口成功"

**业务逻辑：**
- 同设备下接口名唯一，冲突返回 409
- 信息点与交换机端口/设备接口之间的物理连线通过"线路"（cable_links）模块表达，不再在设备接口上维护信息点链

### 4.4 获取接口详情

| 项目 | 内容 |
|---|---|
| Method | `GET` |
| URL | `/api/resources/devices/interfaces/{interface_id}` |
| 后端处理 | [`get_device_interface`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/interface.rs#L312-L332) |

**响应 data：[`DeviceInterfaceWithDevice`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/models.rs#L944-L962)**，message = "获取接口成功"

### 4.5 更新接口

| 项目 | 内容 |
|---|---|
| Method | `PUT` |
| URL | `/api/resources/devices/interfaces/{interface_id}` |
| 后端处理 | [`update_device_interface`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/interface.rs#L334-L562) |

**请求体：[`DeviceInterfaceUpdate`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/models.rs#L980-L996)**

```json
{
  "name": null,
  "interface_type": null,
  "mac_address": null,             // Option<Option<String>>：Some(null)=清空
  "vlan_id": null,
  "description": null,             // Option<Option<String>>
  "switch_id": null,               // Option<Option<Uuid>>
  "uplink_interface_id": null      // Option<Option<Uuid>>
}
```

**响应 data：[`DeviceInterface`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/models.rs#L925-L942)**，message = "更新接口成功"

### 4.6 删除接口

| 项目 | 内容 |
|---|---|
| Method | `DELETE` |
| URL | `/api/resources/devices/interfaces/{interface_id}` |
| 后端处理 | [`delete_device_interface`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/interface.rs#L564-L619) |

**响应 data：** `null`，message = "删除接口成功"

**业务逻辑：** 事务删除：先删除关联 IP → 再删除关联 cable_links → 最后删除接口本身。

---

## 5. 交换机端口（二层口）管理

后端实现：[src/resource/device/switch_port.rs](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/switch_port.rs)

### 5.1 获取设备的交换机端口

| 项目 | 内容 |
|---|---|
| Method | `GET` |
| URL | `/api/resources/devices/{id}/switch-ports` |
| 前端调用 | [devicePorts.js:83](file:///media/oi-io/AA709DF48A7AD5C2/ipma/web/static/js/modules/devicePorts.js#L83)、[devicePorts.js:175](file:///media/oi-io/AA709DF48A7AD5C2/ipma/web/static/js/modules/devicePorts.js#L175)、[devicePorts.js:490](file:///media/oi-io/AA709DF48A7AD5C2/ipma/web/static/js/modules/devicePorts.js#L490)、[unifiedDevicePorts.js:151](file:///media/oi-io/AA709DF48A7AD5C2/ipma/web/static/js/modules/unifiedDevicePorts.js#L151) |
| 后端处理 | [`get_switch_ports`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/switch_port.rs#L20-L54) |

**Query 参数：** `page`、`page_size`（前端常传 `page_size=1000`）

**响应 data：**

```json
{
  "items": [ SwitchPort, ... ],
  "total": 10,
  "page": 1,
  "page_size": 1000,
  "total_pages": 1
}
```

### 5.2 获取所有交换机端口（分页，跨设备）

| 项目 | 内容 |
|---|---|
| Method | `GET` |
| URL | `/api/resources/devices/switch-ports` |
| 前端调用 | [devicePorts.js:48](file:///media/oi-io/AA709DF48A7AD5C2/ipma/web/static/js/modules/devicePorts.js#L48) |
| 后端处理 | [`get_all_switch_ports`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/switch_port.rs#L56-L155) |

**Query 参数：** `page`、`page_size`、`search`（设备名/端口号/端口名/描述）、`room_id`

**响应 data：**

```json
{
  "items": [ SwitchPortWithDevice, ... ],
  "total": 100,
  "page": 1,
  "page_size": 20,
  "total_pages": 5
}
```

### 5.3 创建交换机端口

| 项目 | 内容 |
|---|---|
| Method | `POST` |
| URL | `/api/resources/devices/{id}/switch-ports` |
| 前端调用 | [devicePorts.js:478](file:///media/oi-io/AA709DF48A7AD5C2/ipma/web/static/js/modules/devicePorts.js#L478) |
| 后端处理 | [`create_switch_port`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/switch_port.rs#L157-L240) |

**请求体：[`SwitchPortCreate`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/models.rs#L860-L873)**

```json
{
  "port_number": "1",            // 必填，1-30 字符
  "port_name": "GigabitEthernet1/0/1",  // 可选，最长 50 字符
  "port_type": "access",         // 可选，默认 access
  "vlan_id": 100,                // 可选
  "status": "up",                // 可选，默认 up
  "speed": "1Gbps",              // 可选，最长 20 字符
  "description": null            // 可选，最长 255 字符
}
```

**响应 data：[`SwitchPort`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/models.rs#L828-L841)**，message = "创建端口成功"

**业务逻辑：** 同设备下 `port_number` 唯一，冲突返回 409。

### 5.4 获取交换机端口详情

| 项目 | 内容 |
|---|---|
| Method | `GET` |
| URL | `/api/resources/devices/switch-ports/{port_id}` |
| 前端调用 | [devicePorts.js:519](file:///media/oi-io/AA709DF48A7AD5C2/ipma/web/static/js/modules/devicePorts.js#L519) |
| 后端处理 | [`get_switch_port`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/switch_port.rs#L242-L265) |

**响应 data：[`SwitchPortWithDevice`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/models.rs#L843-L858)**，message = "获取端口成功"

### 5.5 更新交换机端口

| 项目 | 内容 |
|---|---|
| Method | `PUT` |
| URL | `/api/resources/devices/switch-ports/{port_id}` |
| 前端调用 | [devicePorts.js:476](file:///media/oi-io/AA709DF48A7AD5C2/ipma/web/static/js/modules/devicePorts.js#L476) |
| 后端处理 | [`update_switch_port`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/switch_port.rs#L267-L335) |

**请求体：[`SwitchPortUpdate`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/models.rs#L875-L889)**

```json
{
  "port_number": null,
  "port_name": null,
  "port_type": null,
  "vlan_id": null,
  "status": null,
  "speed": null,
  "description": null
}
```

**响应 data：[`SwitchPort`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/models.rs#L828-L841)**，message = "更新端口成功"

### 5.6 删除交换机端口

| 项目 | 内容 |
|---|---|
| Method | `DELETE` |
| URL | `/api/resources/devices/switch-ports/{port_id}` |
| 前端调用 | [devicePorts.js:403](file:///media/oi-io/AA709DF48A7AD5C2/ipma/web/static/js/modules/devicePorts.js#L403)、[devicePorts.js:539](file:///media/oi-io/AA709DF48A7AD5C2/ipma/web/static/js/modules/devicePorts.js#L539)（通过 `handleDelete`） |
| 后端处理 | [`delete_switch_port`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/switch_port.rs#L337-L380) |

**响应 data：** `null`，message = "删除端口成功"

**业务逻辑：** 端口被 `cable_links` 引用时返回 400 Validation。

### 5.7 从 SNMP 同步交换机端口

| 项目 | 内容 |
|---|---|
| Method | `POST` |
| URL | `/api/resources/devices/{id}/switch-ports/sync-snmp` |
| 前端调用 | [devicePorts.js:551](file:///media/oi-io/AA709DF48A7AD5C2/ipma/web/static/js/modules/devicePorts.js#L551)、[unifiedDevicePorts.js:301](file:///media/oi-io/AA709DF48A7AD5C2/ipma/web/static/js/modules/unifiedDevicePorts.js#L301) |
| 后端处理 | [`sync_ports_from_snmp`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/switch_port.rs#L382-L494) |

**请求体：** 无需（前端传 `{}`）

**响应 data：** `[ SwitchPort, ... ]`（同步后该设备的所有端口），message 描述同步统计（如 "成功保存 5 个端口到数据库"）

**业务逻辑：**
- 通过设备 ID 查询 SNMP 配置和首个 IP，构建 [`SnmpParamsLegacy`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/snmp.rs#L200-L212)
- 通过 SNMP walk `1.3.6.1.2.1.2.2.1.2`（ifDescr）获取端口列表（[`get_switch_ports_via_snmp`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/snmp.rs#L444-L499)）
- 已存在的 `port_number` 跳过，未存在的插入

---

## 6. 设备 IP 管理

后端实现：[src/resource/ip.rs](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/ip.rs)（部分接口复用全局 IP 管理）

### 6.1 获取设备 IP 列表

| 项目 | 内容 |
|---|---|
| Method | `GET` |
| URL | `/api/resources/devices/{id}/ips` |
| 后端处理 | [`get_device_ips`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/ip.rs#L184-L215) |

**响应 data：**

```json
{
  "items": [ IpManager, ... ]
}
```

### 6.2 为设备添加 IP

| 项目 | 内容 |
|---|---|
| Method | `POST` |
| URL | `/api/resources/devices/{id}/ips` |
| 后端处理 | [`create_device_ip`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/ip.rs#L217-L339) |

**请求体：[`IpManagerCreate`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/models.rs#L739-L748)**

```json
{
  "device_interface_id": null,   // 可选，未提供时使用设备的首个 physical 接口
  "device_id": null,             // 路径参数已覆盖，可不传
  "network_id": null,            // 可选，未提供时按 room_id 自动匹配
  "ip_address": "192.168.1.10",  // 必填，合法 IP
  "description": null
}
```

**响应 data：[`IpManager`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/models.rs#L692-L708)**，message = "IP地址创建成功"

**业务逻辑：**
- IP 重复返回 409 Conflict
- 设备无可用 physical 接口时返回 400 Validation

### 6.3 自动分配设备 IP

| 项目 | 内容 |
|---|---|
| Method | `POST` |
| URL | `/api/resources/devices/{id}/auto-assign-ip` |
| 后端处理 | [`auto_assign_device_ip`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/ip.rs#L873-L882)（委托给全局 `auto_assign_ip`） |

**请求体：[`AutoAssignIpRequest`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/models.rs#L799-L806)**

```json
{
  "network_id": "uuid-xxx",          // 必填，从该网段分配
  "device_interface_id": null,       // 可选
  "device_id": null,                 // 路径参数已覆盖
  "description": null
}
```

**响应 data：[`IpManager`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/models.rs#L692-L708)**，message = "IP地址自动分配成功"

---

## 7. SNMP 相关功能

后端实现：[src/resource/device/snmp.rs](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/snmp.rs)

### 7.1 测试 SNMP 连接（按设备 ID 或独立参数）

| 项目 | 内容 |
|---|---|
| Method | `POST` |
| URL | `/api/resources/devices/test-snmp`、`/api/resources/devices/{id}/test-snmp` |
| 前端调用 | [deviceSnmp.js:119](file:///media/oi-io/AA709DF48A7AD5C2/ipma/web/static/js/modules/deviceSnmp.js#L119) |
| 后端处理 | [`test_snmp_connection`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/snmp.rs#L511-L630)、[`test_snmp_connection_by_id`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/snmp.rs#L501-L509) |

**请求体：[`SnmpTestRequest`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/models.rs#L1068-L1080)**

```json
{
  "device_id": null,             // 提供时从数据库读取默认值
  "ip_address": "192.168.1.1",   // 必填（若 device_id 未配置 IP）
  "snmp_version": "v2c",         // 可选，v1/v2c/v3，默认 v2c
  "snmp_community": "public",    // v1/v2c 必填
  "snmp_username": null,         // v3 必填
  "snmp_auth_protocol": null,    // MD5/SHA/SHA-1/SHA-224/SHA-256/SHA-384/SHA-512
  "snmp_auth_password": null,
  "snmp_priv_protocol": null,    // DES/3DES/AES/AES-128/AES-192/AES-256
  "snmp_priv_password": null,
  "snmp_port": 161               // 默认 161
}
```

**响应 data：**

```json
{ "sysDescr": "Cisco IOS Software..." }
```

message = "SNMP连接测试成功"

**业务逻辑：**
- SSRF 防护：禁止连接回环/组播/链路本地/云元数据端点（[snmp.rs:570-599](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/snmp.rs#L570-L599)）
- 通过 `get` `1.3.6.1.2.1.1.1.0`（sysDescr）测试连通性（[`test_snmp`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/snmp.rs#L303-L358)）

### 7.2 通过 SNMP 获取设备厂商/型号

| 项目 | 内容 |
|---|---|
| Method | `GET` |
| URL | `/api/resources/devices/{id}/snmp-info` |
| 前端调用 | [deviceSnmp.js:158](file:///media/oi-io/AA709DF48A7AD5C2/ipma/web/static/js/modules/deviceSnmp.js#L158) |
| 后端处理 | [`get_device_info_snmp`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/snmp.rs#L632-L651) |

**响应 data：**

```json
{ "vendor": "Cisco", "model": "Catalyst 9300" }
```

message = "获取交换机信息成功"

**业务逻辑：** 通过 sysDescr 字符串匹配识别厂商（[`identify_vendor`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/snmp.rs#L371-L424)），提取型号（[`extract_model`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/snmp.rs#L426-L442)）。

### 7.3 通过 SNMP 获取端口列表（不落库）

| 项目 | 内容 |
|---|---|
| Method | `GET` |
| URL | `/api/resources/devices/{id}/snmp-ports` |
| 后端处理 | [`get_device_ports_snmp`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/snmp.rs#L653-L669) |

**响应 data：** `[ SwitchPortCreate, ... ]`，message = "获取交换机端口信息成功"

---

## 8. MAC 表（ARP 表）管理

后端实现：[src/resource/device/mac.rs](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/mac.rs)

### 8.1 获取设备 MAC 表（从数据库）

| 项目 | 内容 |
|---|---|
| Method | `GET` |
| URL | `/api/resources/devices/{id}/macs` |
| 前端调用 | [deviceMacLldp.js:95](file:///media/oi-io/AA709DF48A7AD5C2/ipma/web/static/js/modules/deviceMacLldp.js#L95) |
| 后端处理 | [`get_device_macs_from_db`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/mac.rs#L517-L545) |

**响应 data：** `[ DeviceMac, ... ]`，message = "获取MAC表成功"

### 8.2 从 SNMP 同步 MAC 表

| 项目 | 内容 |
|---|---|
| Method | `POST` |
| URL | `/api/resources/devices/{id}/macs/sync` |
| 前端调用 | [deviceMacLldp.js:186](file:///media/oi-io/AA709DF48A7AD5C2/ipma/web/static/js/modules/deviceMacLldp.js#L186) |
| 后端处理 | [`get_device_mac_table`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/mac.rs#L424-L515) |

**请求体：** 无需（前端传 `{}`）

**响应 data：** `[ DeviceMac, ... ]`（同步后的所有记录），message 描述同步结果（如 "同步 50 条 MAC 记录"）

**业务逻辑：**
- 通过 [`get_arp_table_via_snmp`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/mac.rs#L113-L255) 同时获取 IPv4 ARP（`1.3.6.1.2.1.4.22.1.2`）和 IPv6 邻居（`1.3.6.1.2.1.4.35.1.4`）
- 通过 `ifName` 表（`1.3.6.1.2.1.31.1.1.1.1`）和 `dot1qTpFdbPort` 表（`1.3.6.1.2.1.17.7.1.2.2.1.2`）补充接口名和 VLAN
- 使用 `ON CONFLICT (device_id, ip_address) DO UPDATE` upsert 写入 `device_macs`
- 删除本次同步未出现的陈旧记录

---

## 9. LLDP 邻居管理

后端实现：[src/resource/device/lldp.rs](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/lldp.rs)

### 9.1 获取设备 LLDP 邻居（从数据库）

| 项目 | 内容 |
|---|---|
| Method | `GET` |
| URL | `/api/resources/devices/{id}/lldp-neighbors` |
| 前端调用 | [deviceMacLldp.js:369](file:///media/oi-io/AA709DF48A7AD5C2/ipma/web/static/js/modules/deviceMacLldp.js#L369) |
| 后端处理 | [`get_device_lldp_neighbors`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/lldp.rs#L382-L405) |

**响应 data：** `[ DeviceLldp, ... ]`，message = "获取LLDP邻居成功"

### 9.2 从 SNMP 同步 LLDP 邻居

| 项目 | 内容 |
|---|---|
| Method | `POST` |
| URL | `/api/resources/devices/{id}/lldp/sync` |
| 前端调用 | [deviceMacLldp.js:411](file:///media/oi-io/AA709DF48A7AD5C2/ipma/web/static/js/modules/deviceMacLldp.js#L411) |
| 后端处理 | [`sync_lldp_from_snmp`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/lldp.rs#L407-L509) |

**请求体：** 无需（前端传 `{}`）

**响应 data：** `[ DeviceLldp, ... ]`（同步后的所有记录），message 描述同步结果（如 "新增 5 条，更新 2 条 LLDP 记录"）

**业务逻辑：**
- 通过 [`get_lldp_neighbors_via_snmp`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/lldp.rs#L87-L305) walk 多个 LLDP MIB：
  - `ifName`（`1.3.6.1.2.1.2.2.1.2`）→ 接口名映射
  - `lldpLocPortSubtype`（`1.0.8802.1.1.2.1.3.7.1.2`）、`lldpLocPortId`（...3）、`lldpLocPortDesc`（...4）→ 本地端口信息
  - `lldpRemTable`（`1.0.8802.1.1.2.1.4.1.1`）→ 邻居信息（chassis/port/sysName/sysDesc）
- 按 `local_port` upsert 到 `device_lldps`

---

## 数据模型汇总

### 设备相关

| 模型 | 定义位置 | 说明 |
|---|---|---|
| [`Device`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/models.rs#L1447-L1472) | models.rs:1447 | 设备基础结构（数据库原始字段） |
| [`DeviceWithDetails`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/models.rs#L1474-L1506) | models.rs:1474 | 设备带关联名称（工位/房间/机柜/模板名等） |
| [`DeviceCreate`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/models.rs#L1508-L1546) | models.rs:1508 | 创建设备请求体 |
| [`DeviceUpdate`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/models.rs#L1548-L1587) | models.rs:1548 | 更新设备请求体 |
| [`DeviceTemplate`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/models.rs#L1413-L1423) | models.rs:1413 | 设备模板 |
| [`DeviceTemplateSummary`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/models.rs#L1425-L1432) | models.rs:1425 | 设备模板摘要（列表用） |
| [`UpdateDeviceTemplateRequest`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/models.rs#L1434-L1443) | models.rs:1434 | 更新模板请求体 |

### 网卡/接口/端口相关

| 模型 | 定义位置 | 说明 |
|---|---|---|
| [`NetworkCard`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/models.rs#L893-L903) | models.rs:893 | 网卡（nics 表） |
| [`NetworkCardSyncItem`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/models.rs#L781-L791) | models.rs:781 | 网卡同步项（含嵌套 ports） |
| [`DeviceInterface`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/models.rs#L925-L942) | models.rs:925 | 三层接口/网口（device_interfaces 表） |
| [`DeviceInterfaceWithDevice`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/models.rs#L944-L962) | models.rs:944 | 接口带设备名 |
| [`DeviceInterfaceCreate`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/models.rs#L964-L978) | models.rs:964 | 创建接口请求体 |
| [`DeviceInterfaceUpdate`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/models.rs#L980-L996) | models.rs:980 | 更新接口请求体 |
| [`SwitchPort`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/models.rs#L828-L841) | models.rs:828 | 交换机二层端口（switch_ports 表） |
| [`SwitchPortWithDevice`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/models.rs#L843-L858) | models.rs:843 | 端口带设备名/IP |
| [`SwitchPortCreate`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/models.rs#L860-L873) | models.rs:860 | 创建端口请求体 |
| [`SwitchPortUpdate`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/models.rs#L875-L889) | models.rs:875 | 更新端口请求体 |
| [`PortSyncItem`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/models.rs#L762-L779) | models.rs:762 | 网口同步项（含嵌套 ips） |
| [`IpSyncItem`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/models.rs#L752-L760) | models.rs:752 | IP 同步项 |
| [`DeviceNetworkConfigSync`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/models.rs#L793-L797) | models.rs:793 | 网卡配置整体同步请求体 |

### IP 相关

| 模型 | 定义位置 | 说明 |
|---|---|---|
| [`IpManager`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/models.rs#L692-L708) | models.rs:692 | IP 记录（ips 表） |
| [`IpManagerCreate`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/models.rs#L739-L748) | models.rs:739 | 创建 IP 请求体 |
| [`AutoAssignIpRequest`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/models.rs#L799-L806) | models.rs:799 | 自动分配 IP 请求体 |

### SNMP/MAC/LLDP 相关

| 模型 | 定义位置 | 说明 |
|---|---|---|
| [`SnmpTestRequest`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/models.rs#L1068-L1080) | models.rs:1068 | SNMP 测试请求体 |
| [`ArpEntry`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/models.rs#L1082-L1088) | models.rs:1082 | ARP 表项（SNMP 返回） |
| [`DeviceMac`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/models.rs#L1100-L1110) | models.rs:1100 | 设备 MAC 记录（device_macs 表） |
| [`LldpNeighbor`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/models.rs#L1090-L1098) | models.rs:1090 | LLDP 邻居（SNMP 返回） |
| [`DeviceLldp`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/models.rs#L1120-L1133) | models.rs:1120 | 设备 LLDP 记录（device_lldps 表） |

### 通用

| 模型 | 定义位置 | 说明 |
|---|---|---|
| [`ApiResponse<T>`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/models.rs#L110-L115) | models.rs:110 | 统一响应包装 |

---

## 路由总览表

| Method | URL | 处理函数 | 前端调用位置 |
|---|---|---|---|
| GET | `/api/resources/devices` | [`get_devices`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/crud.rs#L22) | [device.js:82](file:///media/oi-io/AA709DF48A7AD5C2/ipma/web/static/js/modules/device.js#L82)、[ipmanager.js:35](file:///media/oi-io/AA709DF48A7AD5C2/ipma/web/static/js/modules/ipmanager.js#L35) |
| POST | `/api/resources/devices` | [`create_device`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/crud.rs#L178) | [device.js:555](file:///media/oi-io/AA709DF48A7AD5C2/ipma/web/static/js/modules/device.js#L555) |
| GET | `/api/resources/devices/{id}` | [`get_device`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/crud.rs#L413) | device.js:158、deviceMacLldp.js:86/360、devicePorts.js:163、unifiedDevicePorts.js:57 |
| PUT | `/api/resources/devices/{id}` | [`update_device`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/crud.rs#L461) | [device.js:555](file:///media/oi-io/AA709DF48A7AD5C2/ipma/web/static/js/modules/device.js#L555) |
| DELETE | `/api/resources/devices/{id}` | [`delete_device`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/crud.rs#L726) | [device.js:170](file:///media/oi-io/AA709DF48A7AD5C2/ipma/web/static/js/modules/device.js#L170) |
| PUT | `/api/resources/devices/{id}/network-config` | [`sync_device_network_config`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/nic.rs#L28) | （由 `cards` 字段随设备创建/更新提交） |
| GET | `/api/resources/devices/{id}/ips` | [`get_device_ips`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/ip.rs#L184) | （IP 管理模块） |
| POST | `/api/resources/devices/{id}/ips` | [`create_device_ip`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/ip.rs#L217) | （IP 管理模块） |
| POST | `/api/resources/devices/{id}/auto-assign-ip` | [`auto_assign_device_ip`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/ip.rs#L873) | （IP 管理模块） |
| GET | `/api/resources/devices/{id}/interfaces` | [`get_device_interfaces`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/interface.rs#L21) | - |
| POST | `/api/resources/devices/{id}/interfaces` | [`create_device_interface`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/interface.rs#L204) | - |
| GET | `/api/resources/devices/{id}/switch-ports` | [`get_switch_ports`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/switch_port.rs#L20) | devicePorts.js:83/175/490、unifiedDevicePorts.js:151 |
| POST | `/api/resources/devices/{id}/switch-ports` | [`create_switch_port`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/switch_port.rs#L157) | [devicePorts.js:478](file:///media/oi-io/AA709DF48A7AD5C2/ipma/web/static/js/modules/devicePorts.js#L478) |
| POST | `/api/resources/devices/{id}/switch-ports/sync-snmp` | [`sync_ports_from_snmp`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/switch_port.rs#L382) | devicePorts.js:551、unifiedDevicePorts.js:301 |
| GET | `/api/resources/devices/{id}/macs` | [`get_device_macs_from_db`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/mac.rs#L517) | [deviceMacLldp.js:95](file:///media/oi-io/AA709DF48A7AD5C2/ipma/web/static/js/modules/deviceMacLldp.js#L95) |
| POST | `/api/resources/devices/{id}/macs/sync` | [`get_device_mac_table`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/mac.rs#L424) | [deviceMacLldp.js:186](file:///media/oi-io/AA709DF48A7AD5C2/ipma/web/static/js/modules/deviceMacLldp.js#L186) |
| GET | `/api/resources/devices/{id}/lldp-neighbors` | [`get_device_lldp_neighbors`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/lldp.rs#L382) | [deviceMacLldp.js:369](file:///media/oi-io/AA709DF48A7AD5C2/ipma/web/static/js/modules/deviceMacLldp.js#L369) |
| POST | `/api/resources/devices/{id}/lldp/sync` | [`sync_lldp_from_snmp`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/lldp.rs#L407) | [deviceMacLldp.js:411](file:///media/oi-io/AA709DF48A7AD5C2/ipma/web/static/js/modules/deviceMacLldp.js#L411) |
| GET | `/api/resources/devices/{id}/snmp-info` | [`get_device_info_snmp`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/snmp.rs#L632) | [deviceSnmp.js:158](file:///media/oi-io/AA709DF48A7AD5C2/ipma/web/static/js/modules/deviceSnmp.js#L158) |
| GET | `/api/resources/devices/{id}/snmp-ports` | [`get_device_ports_snmp`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/snmp.rs#L653) | - |
| GET | `/api/resources/devices/{id}/test-snmp` | [`test_snmp_connection_by_id`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/snmp.rs#L501) | - |
| GET | `/api/resources/devices/interfaces` | [`get_all_device_interfaces`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/interface.rs#L58) | - |
| GET | `/api/resources/devices/interfaces/{interface_id}` | [`get_device_interface`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/interface.rs#L312) | - |
| PUT | `/api/resources/devices/interfaces/{interface_id}` | [`update_device_interface`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/interface.rs#L334) | - |
| DELETE | `/api/resources/devices/interfaces/{interface_id}` | [`delete_device_interface`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/interface.rs#L564) | - |
| GET | `/api/resources/devices/switch-ports` | [`get_all_switch_ports`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/switch_port.rs#L56) | [devicePorts.js:48](file:///media/oi-io/AA709DF48A7AD5C2/ipma/web/static/js/modules/devicePorts.js#L48) |
| GET | `/api/resources/devices/switch-ports/{port_id}` | [`get_switch_port`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/switch_port.rs#L242) | [devicePorts.js:519](file:///media/oi-io/AA709DF48A7AD5C2/ipma/web/static/js/modules/devicePorts.js#L519) |
| PUT | `/api/resources/devices/switch-ports/{port_id}` | [`update_switch_port`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/switch_port.rs#L267) | [devicePorts.js:476](file:///media/oi-io/AA709DF48A7AD5C2/ipma/web/static/js/modules/devicePorts.js#L476) |
| DELETE | `/api/resources/devices/switch-ports/{port_id}` | [`delete_switch_port`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/switch_port.rs#L337) | devicePorts.js:403/539 |
| POST | `/api/resources/devices/test-snmp` | [`test_snmp_connection`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/snmp.rs#L511) | [deviceSnmp.js:119](file:///media/oi-io/AA709DF48A7AD5C2/ipma/web/static/js/modules/deviceSnmp.js#L119) |
| GET | `/api/resources/device-templates` | [`get_device_templates`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/template.rs#L16) | [device.js:267](file:///media/oi-io/AA709DF48A7AD5C2/ipma/web/static/js/modules/device.js#L267) |
| GET | `/api/resources/device-templates/{id}` | [`get_device_template`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/template.rs#L32) | device.js:205/321 |
| PUT | `/api/resources/device-templates/{id}` | [`update_device_template`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/template.rs#L105) | [device.js:381](file:///media/oi-io/AA709DF48A7AD5C2/ipma/web/static/js/modules/device.js#L381) |
| DELETE | `/api/resources/device-templates/{id}` | [`delete_device_template`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/resource/device/template.rs#L49) | [device.js:405](file:///media/oi-io/AA709DF48A7AD5C2/ipma/web/static/js/modules/device.js#L405) |

---

## 备注

1. **前端 `/api/resources/devices/{id}/nics` 调用问题**：[unifiedDevicePorts.js:124](file:///media/oi-io/AA709DF48A7AD5C2/ipma/web/static/js/modules/unifiedDevicePorts.js#L124) 调用此 URL，但后端 [routes/mod.rs](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/routes/mod.rs#L362-L443) 未定义该路由。实际网卡数据通过 `GET /api/resources/devices/{id}` 响应的 `cards` 字段获取。
2. **SNMP 凭据加密**：所有 SNMP 凭据（`snmp_community`、`snmp_auth_password`、`snmp_priv_password`）通过 [`crypto::encrypt_password_async`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/crypto.rs) 加密存储，详情接口解密返回，列表接口置 `null`。
3. **操作日志**：所有写操作（create/update/delete/sync）均通过 [`log_system_operation`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/utils/mod.rs) 记录到 `operation_logs` 表，日志失败仅警告不影响主流程。
4. **IPv6 支持友好**：MAC 表同步同时获取 IPv4 ARP 和 IPv6 邻居表，IP 自动分配按 `ip_version` 区分。

---

文档版本：1.0  
最后更新：2026 年 8 月
