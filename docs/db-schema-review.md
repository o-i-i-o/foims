# ipma-init 数据库结构 vs Rust 数据模型一致性分析

> 生成时间：2026-08-23 ｜ 对照对象：`crates/ipma-init/src/schema/`（36 张表）、`src/models/`、各 resource 模块 SQL、`crates/ipma-visualization`、`crates/ipma-scheduler`
> 方法：schema 逐表审查 + Rust FromRow 结构与查询列逐一比对 + `check.rs` 校验清单核对 + API 实测验证（部分风险已实测确认）

## 一、总体结论

表结构与 Rust 模型**总体高度对齐**：字段命名、Option 可空语义、`TIMESTAMPTZ ↔ DateTime<Utc>`、`INET/CIDR ↔ String`（经 `host()`/`CAST` 转换）两侧一致；枚举除 `port_type` 外均双侧一致。层级链路（组织→房间→机柜/工位→设备→网口→IP/线路）通过 FK + 触发器 + 应用层校验形成基本闭环。

主要问题集中在四处：
1. **check.rs 校验清单 5 处遗漏**（升级自检盲区）；
2. **network_cidrs 缺唯一约束**（并发重复风险）；
3. **一批 VARCHAR 长度无 Rust 校验**（用户输入直通 DB 报 500）；
4. **删除链路存在一个实测确认的缺陷**（R1：删除被线路引用的设备 → 500 且不可删）。

## 二、表结构总览（36 表 + 6 视图 + 1 函数）

- 所有表 `id UUID PK DEFAULT uuid_generate_v4()`；时间列 `TIMESTAMPTZ NOT NULL DEF NOW()`；`updated_at` 触发器自动维护（`schema/triggers.rs:5-54`）。
- 关键约束亮点：
  - `organizations`：自引用 FK RESTRICT + `UNIQUE NULLS NOT DISTINCT (parent_id,name)`（**需 PostgreSQL ≥ 15**）+ level_index 触发器强制 = 父+1；
  - `devices`：触发器校验 room 一致性（workstation/position 必属于 device.room_id，`triggers.rs:94-136`）；
  - `cable_links`：CHECK 禁自环 + 规范序 + 端点存在性/形态触发器 + 四类端点防删触发器 + 递归 CTE 函数 `find_cable_path`；
  - `ips.ip_address INET` 全局 UNIQUE；`device_ports` UNIQUE(device_id,port_number)。

## 三、check.rs 校验清单遗漏（升级自检盲区）

| # | 遗漏 | 影响 |
|---|---|---|
| K-1 | `users.tokens_invalidated_at` 不在必需列清单（`check.rs:70-89`） | 旧库升级缺列时启动不报错，运行期认证功能异常 |
| K-2 | `element_layouts` 在必需表清单但**无列校验条目** | 缺列无法检出 |
| K-3 | `encryption_keys` 无列校验 | 同上 |
| K-4 | `workstation_layouts` 列清单漏 `room_id`（NOT NULL 列） | 同上 |
| K-5 | 6 个视图、`find_cable_path` 函数、全部触发器、`uq_cable_links_endpoint_pair`/`uq_topology_connections_logical` 唯一索引均不校验 | 视图缺失直接导致列表接口 42P01 报错；触发器缺失导致数据一致性静默失效 |

## 四、类型/可空性不一致（Rust ↔ DB）

| # | 不一致 | 位置 | 风险 |
|---|---|---|---|
| M-1 | `Device.snmp_version: String`、`snmp_port: i32`（非 Option）vs DB 两列可空（仅 DEFAULT） | `models/device.rs:318-343` vs `schema/devices.rs:19,26` | 外部写入 NULL 时 FromRow 解码报错（应用路径目前恒写值，潜在） |
| M-2 | `OperationLog.details: Option<Value>` vs DB `JSONB NOT NULL` | `models/log.rs:20` vs `logs.rs:11` | 宽于库，无害但语义不一致 |
| M-3 | `DeviceMac.ip_address: String` vs INET，依赖查询恒写 `host()` | `mac.rs:342,377` | 任何未来 `SELECT *` 直读该 struct 即类型不匹配 |
| M-4 | `NetworkRegion.ipv4/ipv6_cidrs` 用 `#[sqlx(json)] Option<Vec<String>>` + 查询侧 `json_agg(text(d))` | `network.rs:273-276` | 依赖查询形态，直读列即失败（脆约定） |

## 五、VARCHAR 长度缺口（Rust 无校验 → DB 22001 → 500）

| 字段 | 长度 | 缺失校验位置 |
|---|---|---|
| `devices.snmp_version` | **VARCHAR(3)** | `models/device.rs:402` 无长度/枚举校验，传 "version3" 直接 DB 报错 |
| `devices.snmp_auth_protocol` / `snmp_priv_protocol` | VARCHAR(10) | `models/device.rs:407,413`（"SHA-256-AES" 11 字符即溢出） |
| `devices.brand/model/serial_number`、模板同名字段 | 50/100/100 | `models/device.rs:391-393,439-441,310-311` |
| `users.email` | 100 | `models/user.rs:30` 仅格式无长度 |
| `login_logs.username` | 50 | 邮箱验证码登录把整个 email 写入 username（`login.rs:365`），超长日志丢弃 |
| `login_logs/token_usage.user_agent` | 255 | `get_user_agent_from_parts` 不截断（`utils/common.rs:480-487`） |
| `organizations.type_path` | 50 | 索引段为任意 usize（`models/organization.rs:104-119`） |

另：validator 对 `Option<Option<String>>` 字段的 `#[validate(length)]` 完全不生效（见 security-review.md 第六节），使 `CableLinkUpdate.cable_label` 等已写的长度校验也形同虚设。

## 六、枚举/约束强度不对称

| 项 | DB CHECK | Rust 校验 | 结论 |
|---|---|---|---|
| device_type | 9 值 | 9 值一致 | ✅ |
| room_type | 6 值大写 | 小写比较 + 写前 `to_uppercase()` | ✅（隐式契约） |
| card_type/physical_type/interface_role/link_type/endpoint_type | 有 | 有（已有单元测试全覆盖） | ✅ |
| **device_ports.port_type** | 5 值 CHECK | **无校验，裸传** | ❌ 非法值 → 500 而非 422 |
| ips.status / port.status / users.role / notification_type / task_type | **无 CHECK** | role 有；其余仅长度或白名单 | ⚠️ 完全依赖应用层 |
| cabinets.capacity、U 位 1..=48 | 无 CHECK | 应用层 range | ⚠️ 直写 SQL 可绕过 |

## 七、数据一致性风险（层级链路）

链路：`organizations → rooms(org_id) → {workstations | cabinets → positions | net_outlets} → devices(room_id + workstation XOR position) → {device_nics → device_interfaces → ips; device_ports} → cable_links / topology`

| # | 严重度 | 风险 | 状态 |
|---|---|---|---|
| R1 | **高** | **删除被 cable_links 引用的设备**：`delete_device`（`crud.rs:751`）直接 DELETE，级联删端口时被防删触发器 `RAISE EXCEPTION`，无错误映射 | **已实测**：DELETE 返回 500 `server.error.database`，设备无法删除。对比 `delete_device_interface`（`interface.rs:317-330`）有预清理，设备级删除缺失 |
| R2 | **高** | `network_cidrs` 无任何唯一约束（name/CIDR 均无）：应用层查重（`network.rs:283-293,364-394`）并发下有竞态。注意应用层对 CIDR 查重存在（实测重复 CIDR 返回 409 `ipv4_cidr_exists`），但 DB 兜底缺失 | 代码审查 + 部分实测 |
| R3 | 中 | `ips.network_id` 与设备所在房间网段的一致性仅应用层校验（`validate_network_in_room`），无触发器；绕过 API 写入即产生跨房挂载脏数据 | 代码审查 |
| R4 | 中 | `topology_connection_members` 无 (device_id, device_port_id) 一致性约束；`device_port_id FK CASCADE` 使端口删除时逻辑连线成员静默消失，无告警清理 | 代码审查 |
| R5 | 中 | `element_layouts UNIQUE(room_id, element_type)` = 每类型每房间仅一行；保存图纸同类型多元素逐条 upsert 后写覆盖先写（`visualization/layout.rs:187-210`），前端传多元素会丢数据 | 代码审查 |
| R6 | 中 | cable_links 端点为多态 UUID 无 FK，闭环完全依赖触发器；`session_replication_role='replica'`（init 清库时）或手工禁用触发器会产生悬挂端点；视图对悬挂端点 LEFT JOIN 静默降级 | 代码审查 |
| R7 | 低 | `get_cabinet_networks` 硬编码 `room_type IN ('DATA_CENTER','TELECOM_CLOSET')`（`cabinets.rs:489`）与 `sync_room_children` 分支（含 'OTHER'，`room.rs:594-601`）口径不同：OTHER 房型机柜不参与机柜网络查询 | 代码审查 |
| R8 | 低 | `positions.cabinet_id` 可空：NULL 机位不受 UNIQUE(cabinet_id,name) 与 U 位重叠触发器保护 | **已实测**：POST /api/resources/positions 无 cabinet_id 返回 200 成功创建 |
| R9 | 低 | `UNIQUE NULLS NOT DISTINCT` 需 PG ≥ 15；层级闭环本身完整（RESTRICT + child_count/room_count 检查 + 深度限制 + 环检测） | 代码审查 |
| R10 | 低 | `update_cabinet/update_workstation` 允许改 room_id，但**不级联重校验**已挂设备的房间一致性（触发器仅监听 devices 行变更）→ 设备挂 A 房工位、工位搬 B 房的不一致 | 代码审查 |
| R11 | 低 | `room_networks.network_id`/`ips.network_id` NO ACTION + 应用层删前计数检查（`network.rs:737-755`）：闭环成立 | 代码审查 ✅ |
| — | ✅ | devices 与 workstation/position 互斥 + 房间一致性：应用层 XOR 校验 + DB 触发器双保险 | **已实测**（同时指定/跨房工位均被拒） |

## 八、死结构（建议清理或注释保留原因）

| 对象 | 说明 |
|---|---|
| `encryption_keys` 表 | 除建表/check 外全仓库无任何读写；密钥实际存于 `/etc/ipma/` 与配置 |
| `token_usage` 表 | 只有清理 DELETE，无任何 INSERT，审计用途落空 |
| `mac_comparison` 视图 | 无消费者 |
| `RoomNetworkDetail` 模型 | `models/room.rs:34-45` 定义后无引用 |

## 九、其他观察

- `ip_with_details.ip_address` 是 `host()` 输出 TEXT：`ip.rs:154-163` 排序注释称"按 inet 数值排序"但实际是**字典序**（"10.0.0.10" < "10.0.0.2"）——注释与行为不符。
- `operation_logs.ip_address`/`login_logs.ip_address` 用 VARCHAR(50) 而非 INET：长度足够（IPv6 最长 45），但无法做网段运算。
- seed 仅 1 条（初始管理员）；角色级 statement_timeout=30s、lock_timeout=5s 合理。
- 视图 `GRANT SELECT TO ipma` 在角色不存在时容忍失败（`views.rs:195-202`），属预期。

## 十、修复优先级建议

1. **R1**：`delete_device` 前置清理 cable_links（或映射触发器异常为友好 409）
2. **R2**：`network_cidrs` 补 UNIQUE(name, network_region_id) 与 UNIQUE(ipv4_cidr)/UNIQUE(ipv6_cidr)
3. **K-1~K-4**：check.rs 补 `tokens_invalidated_at`、`element_layouts`、`encryption_keys`、`workstation_layouts.room_id`
4. **第五节长度缺口**：请求模型补 length 校验（含修复双层 Option 校验失效）
5. **port_type** 等 Rust 侧补枚举校验，非法输入返回 422 而非 500
