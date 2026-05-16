# IPMA 交换机管理与机位模块重构 — 功能逻辑文档

> 版本：0.8.72  
> 更新日期：2026-05-16

---

## 一、重构概述

本次重构将交换机（switches）和机位（positions）模块进行统一架构调整，核心原则是**交换机逻辑上视为一个机位**，交换机的位置信息（机柜、U位）统一由 positions 表管理，switches 表不再冗余存储 cabinet_id/start_u/end_u。

### 重构目标

1. 建立交换机与 positions 表的关联关系，交换机视为一个机位
2. 交换机的 IP 等网络信息通过机位模块写入 ips 表
3. 实现机位与机柜的关联关系，一个机柜可包含多个机位
4. 删除 switches 表中遗留的 cabinet_id、start_u、end_u 三个字段

---

## 二、数据模型关系

### 2.1 核心实体关系图

```
rooms ──1:N── cabinets ──1:N── positions ──1:N── ips
                                  │                    │
                                  ├── device_type =    ├── network_cidrs (network_id)
                                  │   'cabinet_position'│
                                  │   (普通机位)        └── switch_ports (switch_port_id)
                                  │
                                  └── device_type = 'switch'
                                      device_id → switches.id
                                      (交换机机位)

switches ──1:1── positions (position_id)
    │
    ├── switch_ports ──1:N── ips (switch_port_id)
    ├── switch_macs
    └── switch_lldps
```

### 2.2 switches 表结构（重构后）

| 列名 | 类型 | 约束 | 说明 |
|------|------|------|------|
| id | UUID | PK | 主键 |
| name | VARCHAR(100) | NOT NULL | 交换机名称 |
| model | VARCHAR(100) | | 型号 |
| vendor | VARCHAR(50) | | 厂商 |
| location | VARCHAR(100) | | 位置描述 |
| snmp_version | VARCHAR(3) | DEFAULT 'v2c' | SNMP版本 |
| snmp_community | VARCHAR(64) | | SNMP团体字 |
| snmp_username | VARCHAR(22) | | SNMP用户名 |
| snmp_auth_protocol | VARCHAR(10) | | 认证协议 |
| snmp_auth_password | VARCHAR(100) | | 认证密码（加密） |
| snmp_priv_protocol | VARCHAR(10) | | 加密协议 |
| snmp_priv_password | VARCHAR(100) | | 加密密码（加密） |
| snmp_port | INTEGER | DEFAULT 161 | SNMP端口 |
| parent_switch_id | UUID | FK → switches(id) | 上级交换机 |
| parent_port_id | UUID | FK → switch_ports(id) | 上级端口 |
| description | TEXT | | 描述 |
| created_at | TIMESTAMPTZ | NOT NULL | 创建时间 |
| updated_at | TIMESTAMPTZ | NOT NULL | 更新时间 |
| position_id | UUID | FK → positions(id) ON DELETE SET NULL | 关联机位 |

**已删除的列：** ~~cabinet_id~~, ~~start_u~~, ~~end_u~~

### 2.3 positions 表结构

| 列名 | 类型 | 约束 | 说明 |
|------|------|------|------|
| id | UUID | PK | 主键 |
| name | VARCHAR(50) | NOT NULL | 机位名称 |
| cabinet_id | UUID | FK → cabinets(id) ON DELETE CASCADE | 关联机柜（可空） |
| start_u | INTEGER | NOT NULL DEFAULT 1 | 起始U位 |
| end_u | INTEGER | NOT NULL DEFAULT 1 | 结束U位 |
| description | TEXT | | 描述 |
| created_at | TIMESTAMPTZ | NOT NULL | 创建时间 |
| updated_at | TIMESTAMPTZ | NOT NULL | 更新时间 |
| device_type | VARCHAR(20) | DEFAULT 'cabinet_position', CHECK IN ('cabinet_position','switch') | 设备类型 |
| device_id | UUID | | 关联设备ID（switch时为switches.id） |

**关键约束：**
- `chk_position_device_type`: device_type 只能是 'cabinet_position' 或 'switch'
- `trg_check_position_overlap`: 同一机柜内U位不允许重叠

### 2.4 switches_with_details 视图（重构后）

| 列名 | 来源 | 说明 |
|------|------|------|
| id, name, model, vendor, location | switches 表 | 基础信息 |
| snmp_version, snmp_community, snmp_username, snmp_auth_* | switches 表 | SNMP配置 |
| snmp_priv_*, snmp_port | switches 表 | SNMP配置 |
| parent_switch_id, parent_switch_name | switches 自关联 | 上级交换机 |
| parent_port_id, parent_port_number | switch_ports 表 | 上级端口 |
| position_id | switches 表 | 关联机位ID |
| **cabinet_id** | **positions 表** | **机柜ID（从position获取）** |
| **cabinet_name** | **cabinets 表** | **机柜名称（从position获取）** |
| **start_u** | **positions 表** | **起始U位（从position获取）** |
| **end_u** | **positions 表** | **结束U位（从position获取）** |
| position_network_id | ips → network_cidrs | 机位网络ID |
| network_region_id | network_cidrs | 网络区域ID |
| description | switches 表 | 描述 |
| device_type | 计算字段 'switch' | 设备类型 |
| ip_address | ips 表 | IP地址 |
| mac_address | ips 表 | MAC地址 |
| created_at, updated_at | switches 表 | 时间戳 |

---

## 三、业务流程

### 3.1 创建交换机流程

```
前端提交 → switchList.submitSwitchForm()
    │
    ├── 1. 收集交换机基本信息（name, model, snmp等）
    ├── 2. 收集位置信息（cabinet_id, start_u, end_u）
    │
    ├── 3. 如果选择了机柜：
    │   ├── 有 position_id → PUT /api/resources/positions/{id} 更新机位
    │   └── 无 position_id → POST /api/resources/positions 创建机位
    │       └── device_type='switch', device_id=待创建的switch_id
    │
    ├── 4. POST /api/switches 创建交换机
    │   └── body: { name, model, ..., position_id, ips: [...] }
    │
    └── 5. 后端 create_switch()：
        ├── 如果有 position_id，使用已有 position
        ├── 如果无 position_id，创建默认 position（仅含name和device_type/device_id）
        ├── INSERT INTO switches（不含cabinet_id/start_u/end_u）
        └── INSERT INTO ips（通过position_id关联）
```

### 3.2 更新交换机流程

```
前端提交 → switchList.submitSwitchForm()
    │
    ├── 1. 如果位置信息有变化：
    │   ├── 有 position_id → PUT /api/resources/positions/{id} 更新
    │   └── 无 position_id → POST /api/resources/positions 创建新机位
    │
    ├── 2. PUT /api/switches/{id} 更新交换机
    │   └── body: { name, model, ..., position_id, ips: [...] }
    │
    └── 3. 后端 update_switch()：
        ├── UPDATE switches SET position_id = $1, ...（不含cabinet_id/start_u/end_u）
        └── 如果有 ips，先删除旧IP再插入新IP
```

### 3.3 删除交换机流程

```
DELETE /api/switches/{id}
    │
    └── 后端 delete_switch()：
        ├── 检查是否有下级交换机
        ├── DELETE FROM ips WHERE position_id IN (SELECT id FROM positions WHERE device_type='switch' AND device_id=$1)
        ├── DELETE FROM positions WHERE device_type='switch' AND device_id=$1
        └── DELETE FROM switches WHERE id=$1
```

### 3.4 查询交换机位置信息流程

```
GET /api/switches → switches_with_details 视图
    │
    └── 视图 JOIN 链：
        switches s
        LEFT JOIN positions p ON s.position_id = p.id
        LEFT JOIN cabinets c ON p.cabinet_id = c.id
        → 返回 cabinet_id, cabinet_name, start_u, end_u
```

### 3.5 机位管理流程

```
创建机位：POST /api/resources/positions
    ├── INSERT INTO positions (name, cabinet_id, start_u, end_u, description, device_type='cabinet_position')
    └── INSERT INTO ips (position_id, device_type='cabinet_position', ...)

更新机位：PUT /api/resources/positions/{id}
    ├── UPDATE positions SET name=$1, cabinet_id=$2, start_u=$3, end_u=$4, ...
    └── 重建 ips 关联

删除机位：DELETE /api/resources/positions/{id}
    ├── device_type='switch' 的机位禁止直接删除（需通过交换机管理删除）
    ├── DELETE FROM ips WHERE position_id=$1
    └── DELETE FROM positions WHERE id=$1
```

---

## 四、数据流向

### 4.1 交换机位置信息数据流

```
前端表单
  │
  ├── cabinet_id, start_u, end_u ──→ positions 表
  │                                    │
  │                                    ├── positions.cabinet_id
  │                                    ├── positions.start_u
  │                                    └── positions.end_u
  │
  └── position_id ──→ switches 表
                        │
                        └── switches.position_id → positions.id

查询时：
  switches.position_id → positions → cabinet_id, start_u, end_u → cabinets.cabinet_name
```

### 4.2 交换机IP数据流

```
前端表单
  │
  └── ips[] ──→ ips 表
                 │
                 ├── ips.position_id → positions.id (device_type='switch')
                 ├── ips.network_id → network_cidrs.id
                 └── ips.switch_port_id → switch_ports.id
```

### 4.3 机位IP数据流

```
前端表单
  │
  └── ips[] ──→ ips 表
                 │
                 ├── ips.position_id → positions.id (device_type='cabinet_position')
                 ├── ips.network_id → network_cidrs.id
                 └── ips.switch_port_id → switch_ports.id
```

---

## 五、关键业务规则

### 5.1 交换机即机位

| 规则 | 说明 |
|------|------|
| 交换机必须通过 position 关联机柜 | switches 表不再直接存储 cabinet_id/start_u/end_u |
| 一个交换机对应一个 position | positions.device_type='switch', device_id=switches.id |
| 交换机 position 禁止直接删除 | 必须通过交换机删除接口级联删除 |
| 交换机 position 的名称同步 | 创建/更新交换机时，position.name = switch.name |

### 5.2 机位与机柜关联

| 规则 | 说明 |
|------|------|
| 一个机柜可包含多个机位 | positions.cabinet_id → cabinets.id |
| 机位U位不可重叠 | trg_check_position_overlap 触发器保证 |
| 删除机柜级联删除机位 | positions.cabinet_id FK ON DELETE CASCADE |
| 机位可以不属于任何机柜 | cabinet_id 可为空 |

### 5.3 IP管理权限

| 页面 | 权限 | 说明 |
|------|------|------|
| 工位页面 | 完整管理 | 创建/编辑/删除工位IP |
| 机位页面 | 完整管理 | 创建/编辑/删除机位IP |
| 交换机页面 | 完整管理 | 创建/编辑/删除交换机IP（通过position_id） |
| IP管理页面 | 只读+MAC拉取 | 仅查看和MAC地址更新 |
| 仪表盘 | 只读 | 统计展示 |

---

## 六、表结构变更记录

### 6.1 switches 表变更

| 变更类型 | 字段 | 说明 |
|----------|------|------|
| 删除 | cabinet_id | 位置信息统一由 positions 表管理 |
| 删除 | start_u | 位置信息统一由 positions 表管理 |
| 删除 | end_u | 位置信息统一由 positions 表管理 |
| 保留 | position_id | 通过 positions 表获取位置信息 |

### 6.2 迁移函数

`migrate_drop_switch_cabinet_columns`:
1. 检测 switches 表是否仍有 cabinet_id 列
2. 若有则执行 `ALTER TABLE switches DROP COLUMN cabinet_id, DROP COLUMN start_u, DROP COLUMN end_u`
3. 重建 `switches_with_details` 视图，通过 positions JOIN 获取位置信息
4. 记录迁移到 schema_migrations 表

---

## 七、文件修改清单

### 后端文件

| 文件 | 修改内容 |
|------|----------|
| src/models.rs | Switch/SwitchCreate/SwitchUpdate 删除 cabinet_id/start_u/end_u |
| src/resource/switch/device.rs | create_switch/update_switch 删除位置字段处理，position 创建逻辑简化 |
| src/init/schema.rs | switches 建表删除三列，新增迁移函数 |
| src/init/check.rs | switches 列检查更新 |

### 前端文件

| 文件 | 修改内容 |
|------|----------|
| js/modules/switch/switchForm.js | 删除 cabinet_id/start_u/end_u 直接提交，改为 position_id |
| js/modules/switch/switchPosition.js | 位置信息从 position 对象获取 |
| js/modules/switch/switchList.js | 提交时先创建/更新 position，再提交交换机 |
| web-ts/src/modules/switch/switchForm.ts | TypeScript 同步更新 |
| web-ts/src/modules/switch/switchPosition.ts | TypeScript 同步更新 |
| web-ts/src/modules/switch/switchList.ts | TypeScript 同步更新 |
