# IPMA 房间与机柜网段关联重构 — 功能逻辑文档

> 版本：0.8.74  
> 更新日期：2026-05-16

---

## 一、重构概述

本次重构建立了完整的层级关联体系：IP地址属于机位/工位，机位/工位属于机柜/房间，机柜属于房间，房间通过 `room_networks` 关联网段。IP地址的网段选择必须受限于其所属房间的可用网段。

### 重构目标

1. 通过 `room_networks` 关联表建立 rooms 与 network_cidrs 的多对多关系
2. 机柜直接继承所属房间的网段信息
3. IP地址通过 position/workstation 间接关联 room_networks，network_id 受房间网段约束
4. 确保层级结构：IP → 机位/工位 → 机柜/房间 → room_networks → network_cidrs

---

## 二、数据模型关系

### 2.1 层级关系图

```
network_regions ──1:N── network_cidrs ──N:M── room_networks ──N:1── rooms
                                                    │                    │
                                                    │              ┌─────┤
                                                    │              │     │
                                                    │              ▼     ▼
                                                    │         cabinets  workstations
                                                    │              │     │
                                                    │              ▼     │
                                                    │         positions  │
                                                    │              │     │
                                                    │              ▼     ▼
                                                    └──────────── ips
```

### 2.2 完整层级路径

**工位IP路径：**
```
ips.network_id → network_cidrs.id
ips.workstation_id → workstations.id → workstations.room_id → rooms.id → room_networks.room_id + room_networks.network_id
```

**机位IP路径：**
```
ips.network_id → network_cidrs.id
ips.position_id → positions.id → positions.cabinet_id → cabinets.id → cabinets.room_id → rooms.id → room_networks.room_id + room_networks.network_id
```

**交换机IP路径：**
```
ips.network_id → network_cidrs.id
ips.position_id → positions.id (device_type='switch', device_id=switch_id) → positions.cabinet_id → cabinets.room_id → room_networks
```

### 2.3 room_networks 关联表

| 列名 | 类型 | 约束 | 说明 |
|------|------|------|------|
| id | UUID | PK | 主键 |
| room_id | UUID | NOT NULL, FK → rooms(id) ON DELETE CASCADE | 房间ID |
| network_id | UUID | NOT NULL, FK → network_cidrs(id) ON DELETE CASCADE | 网段ID |
| created_at | TIMESTAMPTZ | NOT NULL | 创建时间 |
| updated_at | TIMESTAMPTZ | NOT NULL | 更新时间 |

**唯一约束：** `UNIQUE(room_id, network_id)` — 确保房间号与网段多行一一对应

---

## 三、网段继承机制

### 3.1 房间网段管理

房间通过 `room_networks` 表直接管理其关联网段：

- **创建房间**：同时指定 `network_ids` 数组，批量插入 `room_networks`
- **更新房间**：先 `DELETE FROM room_networks WHERE room_id = $1`，再批量插入新关联
- **删除房间**：`ON DELETE CASCADE` 自动删除关联的 `room_networks` 记录
- **查询房间网段**：`GET /api/resources/rooms/{id}/networks`

### 3.2 机柜网段继承

机柜不直接存储网段信息，而是通过 `room_id` 继承所属房间的网段：

```
cabinet.room_id → rooms.id → room_networks → network_cidrs
```

- **查询机柜网段**：`GET /api/resources/cabinets/{id}/networks`
- **SQL**：`SELECT n.* FROM room_networks rn JOIN network_cidrs n ON rn.network_id = n.id JOIN cabinets c ON rn.room_id = c.room_id WHERE c.id = $1`
- **前端展示**：机柜表单中选择房间后，自动显示继承的网段列表（只读）

### 3.3 工位网段继承

工位通过 `room_id` 直接关联房间，获取可用网段：

```
workstation.room_id → rooms.id → room_networks → network_cidrs
```

- **前端加载**：`/api/resources/rooms/${roomId}/networks`
- **IP配置**：工位IP的网段下拉框只显示所属房间的可用网段

---

## 四、IP网段约束验证

### 4.1 验证规则

创建或更新IP时，`ips.network_id` 必须满足以下约束：

| 设备类型 | 验证路径 | 验证SQL |
|----------|----------|---------|
| workstation | workstation.room_id → room_networks | `SELECT EXISTS(SELECT 1 FROM room_networks WHERE room_id = $room_id AND network_id = $network_id)` |
| cabinet_position | position.cabinet_id → cabinet.room_id → room_networks | `SELECT EXISTS(SELECT 1 FROM room_networks WHERE room_id = $room_id AND network_id = $network_id)` |
| switch | position.cabinet_id → cabinet.room_id → room_networks | 同 cabinet_position |

### 4.2 验证失败处理

- 返回 HTTP 400 Bad Request
- 错误信息：`"所选网段不属于该工位所在房间的可用网段"` 或 `"所选网段不属于该机位所在房间的可用网段"`
- 不执行任何数据库写入

### 4.3 特殊情况

| 情况 | 处理方式 |
|------|----------|
| workstation 无 room_id | 跳过验证（允许创建） |
| position 无 cabinet_id | 跳过验证（允许创建） |
| cabinet 无 room_id | 跳过验证（允许创建） |
| room 无 room_networks | 验证失败（无可用网段） |

### 4.4 验证位置

| 文件 | 函数 | 验证位置 |
|------|------|----------|
| ip.rs | create_ip_manager | 设备类型验证之后、IP重复检查之前 |
| ip.rs | update_ip_manager | 设备类型验证之后、UPDATE之前 |
| ip.rs | auto_assign_ip | device_type确定之后、网络查询之前 |
| ip.rs | batch_create_ip_managers | IP重复检查之后、INSERT之前 |
| workstation.rs | create_workstation | IP插入之前 |
| workstation.rs | update_workstation | IP重建之前 |
| cabinet.rs | create_cabinet_position | IP插入之前 |
| cabinet.rs | update_cabinet_position | IP重建之前 |
| switch/device.rs | create_switch | IP插入之前 |
| switch/device.rs | update_switch | IP重建之前 |

---

## 五、前端网段加载逻辑

### 5.1 工位IP配置

```
用户选择房间 → workstation-room 下拉框变更
    │
    ▼
handleWorkstationRoomChange() → ipManager.clear() + ipManager.addIpRow()
    │
    ▼
loadNetworks() → document.getElementById('workstation-room')?.value → roomId
    │
    ▼
apiGet(`/api/resources/rooms/${roomId}/networks`)
    │
    ▼
网段下拉框只显示该房间的可用网段
```

### 5.2 机位IP配置

```
用户选择机柜 → cabinet-position-cabinet 下拉框变更
    │
    ▼
handleCabinetPositionCabinetChange() → ipManager.clear() + ipManager.addIpRow()
    │
    ▼
loadNetworks() → document.getElementById('cabinet-position-cabinet')?.value → cabinetId
    │
    ▼
apiGet(`/api/resources/cabinets/${cabinetId}/networks`)
    │
    ▼
网段下拉框只显示该机柜继承自房间的可用网段
```

### 5.3 交换机IP配置

```
用户选择网络区域 → 按 region_id 加载网段
    │
    ▼
apiGet(`/api/resources/networks?region_id=${regionId}&page_size=1000`)
    │
    ▼
网段下拉框显示该区域的所有网段
    │
    ▼
提交时后端验证 network_id 是否属于 position 所在房间的 room_networks
```

---

## 六、数据流向

### 6.1 网段分配流向

```
管理员配置房间网段
    │
    ▼
room_networks 表（room_id, network_id）
    │
    ├── 机柜继承 → GET /api/resources/cabinets/{id}/networks
    │
    ├── 工位使用 → GET /api/resources/rooms/{id}/networks
    │
    └── IP约束 → 创建/更新IP时验证 network_id ∈ room_networks
```

### 6.2 IP创建数据流

```
前端提交 IP 数据（含 network_id）
    │
    ▼
后端验证：
1. 查找 workstation.room_id 或 position → cabinet.room_id
2. 验证 network_id ∈ room_networks(room_id)
3. 验证 IP 不重复
4. 验证 IP 在 CIDR 范围内
    │
    ▼
INSERT INTO ips (network_id, ...)
```

---

## 七、关键业务规则

| 规则 | 说明 |
|------|------|
| 房间多网段 | 一个房间可关联多个网段，room_networks 多行一一对应 |
| 机柜继承 | 机柜不独立存储网段，完全继承所属房间的网段 |
| IP网段约束 | IP的network_id必须属于其父实体（工位/机位）所在房间的room_networks |
| 级联删除 | 删除房间时级联删除room_networks；删除网段时级联删除room_networks |
| 前端联动 | 房间/机柜选择变更时，IP网段下拉框自动刷新 |
| 验证跳过 | 当room_id为空时跳过网段验证（兼容无房间关联的情况） |

---

## 八、API接口

### 8.1 房间网段管理

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | /api/resources/rooms/{id}/networks | 获取房间的关联网段列表 |
| POST | /api/resources/rooms | 创建房间（含 network_ids 数组） |
| PUT | /api/resources/rooms/{id} | 更新房间（含 network_ids 数组，全量替换） |

### 8.2 机柜网段继承

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | /api/resources/cabinets/{id}/networks | 获取机柜继承的网段列表 |

---

## 九、文件修改清单

### 后端文件

| 文件 | 修改内容 |
|------|----------|
| src/resource/ip.rs | create/update/auto_assign/batch_create 添加 room_networks 验证 |
| src/resource/workstation.rs | create/update 添加 room_networks 验证 |
| src/resource/cabinet.rs | create/update_cabinet_position 添加 room_networks 验证 |
| src/resource/switch/device.rs | create/update_switch 添加 room_networks 验证 |
| Cargo.toml | 版本号 0.8.73 → 0.8.74 |

### 前端文件

无需修改（已有逻辑正确：工位通过房间加载网段，机位通过机柜加载网段）
