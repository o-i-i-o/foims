# IPMA MAC地址同步逻辑 — 功能文档

> 版本：0.8.73  
> 更新日期：2026-05-16

---

## 一、功能概述

MAC地址同步功能通过 `ips` 表的 `ip_address` 字段与 `switch_macs` 表的 `ip_address` 字段进行**精确匹配**，实现从交换机MAC表自动同步MAC地址到IP管理表。同步过程包含三级判断逻辑，确保数据一致性和变更可追溯。

---

## 二、数据模型

### 2.1 相关表结构

**ips 表（关键字段）：**

| 字段 | 类型 | 说明 |
|------|------|------|
| ip_address | INET | IP地址（匹配键） |
| mac_address | VARCHAR(20) | 当前MAC地址 |
| last_mac | VARCHAR(20) | 上次MAC地址（变更记录） |
| device_type | VARCHAR(20) | 设备类型 |
| workstation_id | UUID | 关联工位 |
| position_id | UUID | 关联机位 |
| last_seen | TIMESTAMPTZ | 最后发现时间 |
| updated_at | TIMESTAMPTZ | 更新时间 |

**switch_macs 表（关键字段）：**

| 字段 | 类型 | 说明 |
|------|------|------|
| switch_id | UUID | 交换机ID |
| ip_address | INET | IP地址（匹配键） |
| mac_address | VARCHAR(20) | MAC地址 |

### 2.2 匹配关系

```
switch_macs.ip_address = ips.ip_address  （精确匹配，INET类型比较）
```

匹配通过 SQL `INNER JOIN` 实现：

```sql
SELECT host(sm.ip_address), sm.mac_address 
FROM switch_macs sm 
INNER JOIN ips i ON sm.ip_address = i.ip_address 
WHERE sm.switch_id = $1 AND i.network_id = $2
```

---

## 三、同步流程

### 3.1 整体流程图

```
用户/定时任务触发
    │
    ├── POST /api/resources/ip/pull  (HTTP接口)
    │   └── pull_ip_managers()
    │
    └── 定时任务调度
        └── pull_ip_managers_internal()
    │
    ▼
1. 验证网段存在
    │
    ▼
2. 精确匹配：switch_macs INNER JOIN ips ON ip_address
    │
    ▼
3. 遍历匹配结果，逐条执行三级判断
    │
    ▼
4. 返回同步结果统计
```

### 3.2 三级判断逻辑

对每条匹配成功的记录，根据 `ips.mac_address` 的当前状态执行不同操作：

#### 情况A：ips.mac_address 为空

```
条件：ips.mac_address IS NULL OR ips.mac_address = ''
操作：直接写入
SQL：  UPDATE ips SET mac_address = $new_mac, last_seen = NOW(), updated_at = NOW()
       WHERE ip_address = $ip
日志：  "MAC地址写入: IP={ip}, MAC={new_mac}"
```

#### 情况B：ips.mac_address 与 switch_macs.mac_address 相同

```
条件：ips.mac_address = switch_macs.mac_address
操作：仅更新 last_seen 时间戳（无数据变更）
SQL：  UPDATE ips SET last_seen = NOW(), updated_at = NOW()
       WHERE ip_address = $ip
统计：  计入 "MAC无变化" 计数
```

#### 情况C：ips.mac_address 与 switch_macs.mac_address 不同

```
条件：ips.mac_address != switch_macs.mac_address AND ips.mac_address IS NOT NULL
操作：三步执行
  i)   将 ips.mac_address 当前值移动到 ips.last_mac
  ii)  将 switch_macs.mac_address 更新到 ips.mac_address
  iii) 触发系统通知

SQL：  UPDATE ips 
       SET last_mac = $old_mac, 
           mac_address = $new_mac, 
           last_seen = NOW(), 
           updated_at = NOW()
       WHERE ip_address = $ip

通知：  send_mac_change_notification(workstation_id, ip, old_mac, new_mac)
日志：  "检测到MAC地址变更: IP={ip}, 旧MAC={old_mac}, 新MAC={new_mac}"
```

### 3.3 MAC冲突检查

在执行任何写入操作前，先检查MAC冲突：

```sql
SELECT host(ip_address) FROM ips
WHERE mac_address = $new_mac 
  AND ip_address != $current_ip
  AND (
      device_type != $device_type
      OR workstation_id IS DISTINCT FROM $workstation_id
      OR position_id IS DISTINCT FROM $position_id
  )
LIMIT 1
```

如果发现冲突（同一MAC被不同设备的IP使用），跳过该条记录并计入"MAC冲突跳过"统计。

---

## 四、接口定义

### 4.1 HTTP接口

**POST /api/resources/ip/pull**

请求体：
```json
{
  "switch_id": "uuid",
  "network_id": "uuid"
}
```

响应体：
```json
{
  "success": true,
  "message": "更新 3 条MAC地址，2 条MAC无变化，1 条MAC冲突跳过",
  "data": [/* 更新后的IP列表 */]
}
```

### 4.2 内部接口（定时任务）

```rust
pull_ip_managers_internal(pool: &PgPool, switch_id: Uuid, network_id: Uuid) -> Result<(), String>
```

逻辑与HTTP接口完全一致，仅返回值不同。

---

## 五、通知机制

### 5.1 触发条件

仅在**情况C**（MAC地址变更）时触发通知。

### 5.2 通知流程

```
1. 查询变更IP关联的 workstation_id
   SQL: SELECT COALESCE(i.workstation_id, p.workstation_id) 
        FROM ips i LEFT JOIN positions p ON i.position_id = p.id 
        WHERE i.ip_address = $ip

2. 如果找到 workstation_id，调用通知函数
   send_mac_change_notification(pool, workstation_id, ip, old_mac, new_mac)

3. 通知内容：
   - IP地址
   - 旧MAC地址
   - 新MAC地址
   - 关联工位/机位信息
```

### 5.3 通知失败处理

通知发送失败不影响MAC地址更新，仅记录错误日志：
```
"MAC地址变更通知发送失败: IP={ip}, 错误: {error}"
```

---

## 六、数据流向

```
switch_macs 表                    ips 表
┌─────────────────┐              ┌─────────────────────────┐
│ switch_id       │              │ ip_address              │
│ ip_address ─────┼──INNER JOIN──┼─ ip_address             │
│ mac_address     │              │ mac_address ←── 写入    │
│                 │              │ last_mac  ←── 旧值迁移  │
└─────────────────┘              │ last_seen ←── 更新时间  │
                                 │ updated_at ←─ 更新时间  │
                                 └─────────────────────────┘
                                          │
                                          ▼
                                 notifications 表
                                 （MAC变更通知）
```

---

## 七、关键业务规则

| 规则 | 说明 |
|------|------|
| 精确IP匹配 | 使用 `INNER JOIN ON ip_address` 确保只处理ips表中已存在的IP |
| 网段过滤 | 通过 `i.network_id = $2` 限制同步范围 |
| MAC冲突保护 | 同一MAC不允许被不同设备的IP使用，冲突时跳过 |
| last_mac追溯 | MAC变更时旧值写入last_mac，保留变更历史 |
| 空MAC直接写入 | ips.mac_address为空时不写入last_mac |
| 相同MAC更新时间 | MAC未变化时仅更新last_seen，不修改数据 |
| 通知不阻塞 | 通知失败不影响数据更新 |

---

## 八、代码变更记录

| 文件 | 变更内容 |
|------|----------|
| src/resource/ip.rs | `pull_ip_managers` 函数：改为INNER JOIN精确匹配+三级判断+last_mac写入+通知 |
| src/resource/ip.rs | `pull_ip_managers_internal` 函数：同上逻辑同步 |
| src/resource/ip.rs | 删除 `ip_belongs_to_cidr` 函数（精确匹配不再需要CIDR范围过滤） |
| src/resource/ip.rs | 新增 `IpMacCurrentInfo` 类型别名（消除clippy类型复杂度警告） |
| Cargo.toml | 版本号 0.8.72 → 0.8.73 |
