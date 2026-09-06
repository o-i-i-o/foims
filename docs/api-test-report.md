# FOIMS API 分组集成测试报告

- 测试时间：2026-09-06 16:05:09
- 运行标识：T0906160509（本批次创建的数据名均含该后缀，全部保留）
- 服务端点：UDS /tmp/foims-dev.sock


| 分组 | 用例 | 请求 | 状态码 | success | message | 判定 |
|---|---|---|---|---|---|---|
| 0 基础 | 健康检查 | `GET /health` | 200 | true | server.common.success | ✅ |
| 0 基础 | 管理员登录 | `POST /api/auth/login` | 200 | true | server.common.success | ✅ |
| 0 基础 | 当前用户信息 | `GET /api/auth/me` | 200 | true | server.common.success | ✅ |
| 0 基础 | 初始化状态（公开） | `GET /api/auth/init-status` | 200 | true | server.common.success | ✅ |
| 0 基础 | 未认证访问受保护接口（期望 401） | `GET /api/resources/organizations（无凭据）` | 401 |  | server.auth.auth_failed | ✅ |
| 0 基础 | 创建普通用户 apitest_T0906160509 | `POST /api/users` | 200 | true | server.user.created | ✅ |
| 0 基础 | 普通用户访问用户管理（RBAC）（期望 403） | `GET /api/users（user 角色）` | 403 |  | server.auth.secadmin_required | ✅ |
| 1 组织管理 | 创建组织模板 | `POST /api/resources/org-templates` | 200 | true | server.org_template.created | ✅ |
| 1 组织管理 | 模板列表 | `GET /api/resources/org-templates` | 200 | true | server.org_template.list_fetched | ✅ |
| 1 组织管理 | 可用组织类型 | `GET /api/resources/org-templates/available-types` | 200 | true | server.org_template.types_fetched | ✅ |
| 1 组织管理 | 模板详情 | `GET /api/resources/org-templates/{id}` | 200 | true | server.org_template.fetched | ✅ |
| 1 组织管理 | 创建根组织（园区） | `POST /api/resources/organizations` | 200 | true | server.organization.created | ✅ |
| 1 组织管理 | 创建子组织（楼宇） | `POST /api/resources/organizations` | 200 | true | server.organization.created | ✅ |
| 1 组织管理 | 创建孙组织（楼层） | `POST /api/resources/organizations` | 200 | true | server.organization.created | ✅ |
| 1 组织管理 | 组织列表（搜索） | `GET /api/resources/organizations?search=…` | 200 | true | server.organization.list_fetched | ✅ |
| 1 组织管理 | 组织树 | `GET /api/resources/organizations/tree` | 200 | true | server.organization.tree_fetched | ✅ |
| 1 组织管理 | 组织详情（含子级统计） | `GET /api/resources/organizations/{id}` | 200 | true | server.organization.fetched | ✅ |
| 1 组织管理 | 子组织列表 | `GET /api/resources/organizations/{id}/children` | 200 | true | server.organization.children_fetched | ✅ |
| 1 组织管理 | 允许的子类型 | `GET /api/resources/organizations/{id}/allowed-child-types` | 200 | true | server.organization.allowed_child_types_fetched | ✅ |
| 1 组织管理 | 更新组织 | `PUT /api/resources/organizations/{id}` | 200 | true | server.organization.updated | ✅ |
| 1 组织管理 | 组织下房间列表（空） | `GET /api/resources/organizations/{id}/rooms` | 200 | true | server.organization.rooms_fetched | ✅ |
| 1 组织管理 | 非法 type_path 被拒绝 | `POST /api/resources/organizations（type_path=abc）` | 400 |  | server.organization.validation.type_path_invalid | ✅ |
| 1 组织管理 | 同级重名被拒绝 | `POST /api/resources/organizations（重名）` | 409 |  | server.organization.name_exists | ✅ |
| 2 网络区域 | 创建网络区域 | `POST /api/resources/network-regions` | 200 | true | server.network.region_created | ✅ |
| 2 网络区域 | 区域列表（搜索） | `GET /api/resources/network-regions?search=…` | 200 | true | server.network.region_fetched | ✅ |
| 2 网络区域 | 区域详情 | `GET /api/resources/network-regions/{id}` | 200 | true | server.network.region_fetched | ✅ |
| 2 网络区域 | 更新区域 CIDR | `PUT /api/resources/network-regions/{id}` | 200 | true | server.network.region_updated | ✅ |
| 2 网络区域 | 区域下机柜（经网段/房间推导） | `GET /api/resources/network-regions/{id}/cabinets` | 200 | true | server.cabinet.fetched | ✅ |
| 2 网络区域 | 区域名超长（>20）被拒绝 | `POST /api/resources/network-regions（name 21+）` | 400 |  | server.network.validation.region_name_length | ✅ |
| 3 网段 | 创建 IPv4 网段 | `POST /api/resources/networks` | 200 | true | server.network.created | ✅ |
| 3 网段 | 创建第二个 IPv4 网段 | `POST /api/resources/networks` | 200 | true | server.network.created | ✅ |
| 3 网段 | 创建 IPv6 网段 | `POST /api/resources/networks` | 200 | true | server.network.created | ✅ |
| 3 网段 | 网段列表（搜索） | `GET /api/resources/networks?search=…` | 200 | true | server.network.fetched | ✅ |
| 3 网段 | 网段详情 | `GET /api/resources/networks/{id}` | 200 | true | server.network.fetched | ✅ |
| 3 网段 | 更新网段 | `PUT /api/resources/networks/{id}` | 200 | true | server.network.updated | ✅ |
| 3 网段 | 可用 IP 列表 | `GET /api/resources/ip/available/{network_id}` | 200 | true | server.ip.available_fetched | ✅ |
| 3 网段 | 非法 CIDR 被拒绝 | `POST /api/resources/networks（300.1.1.0/24）` | 400 |  | server.error.validation | ✅ |
| 3 网段 | 网关不在网段内被拒绝 | `POST /api/resources/networks（网关跨段）` | 400 |  | server.network.ipv4_gateway_not_in_cidr | ✅ |
| 4 房间 | 创建机房（DATA_CENTER） | `POST /api/resources/rooms` | 200 | true | server.room.created | ✅ |
| 4 房间 | 创建办公室（OFFICE） | `POST /api/resources/rooms` | 200 | true | server.room.created | ✅ |
| 4 房间 | 房间列表（搜索） | `GET /api/resources/rooms?search=…` | 200 | true | server.room.list_retrieved | ✅ |
| 4 房间 | 房间详情（含网段/机柜/工位） | `GET /api/resources/rooms/{id}` | 200 | true | server.room.fetched | ✅ |
| 4 房间 | 房间关联网段 | `GET /api/resources/rooms/{id}/networks` | 200 | true | server.room.networks_retrieved | ✅ |
| 4 房间 | 更新房间 | `PUT /api/resources/rooms/{id}` | 200 | true | server.room.updated | ✅ |
| 4 房间 | 同步房间子项-工位 | `PUT /api/resources/rooms/{id}/children` | 200 | true | server.room.children_synced | ✅ |
| 4 房间 | 同步房间子项-机柜 | `PUT /api/resources/rooms/{id}/children` | 200 | true | server.room.children_synced | ✅ |
| 4 房间 | 同步信息点 | `PUT /api/resources/rooms/{id}/net-outlets` | 200 | true | server.room.net_outlets_synced | ✅ |
| 4 房间 | 非法房型被拒绝 | `POST /api/resources/rooms（room_type=warehouse）` | 400 |  | server.common.missing_field | ✅ |
| 5 机柜 | 机柜列表（按房间过滤） | `GET /api/resources/cabinets?room_id=…` | 200 | true | server.cabinet.fetched | ✅ |
| 5 机柜 | 机柜详情（含机位/配线架） | `GET /api/resources/cabinets/{id}` | 200 | true | server.cabinet.fetched | ✅ |
| 5 机柜 | 更新机柜 | `PUT /api/resources/cabinets/{id}` | 200 | true | server.cabinet.updated | ✅ |
| 5 机柜 | 同步机位（U 位） | `PUT /api/resources/cabinets/{id}/positions` | 200 | true | server.cabinet.positions_synced | ✅ |
| 5 机柜 | 同步配线架 | `PUT /api/resources/cabinets/{id}/patch-panels` | 200 | true | server.patch_panel.synced | ✅ |
| 5 机柜 | 机柜可达网段 | `GET /api/resources/cabinets/{id}/networks` | 200 | true | server.cabinet.networks_fetched | ✅ |
| 5 机柜 | 机位列表 | `GET /api/resources/positions?room_id=…` | 200 | true | server.position.list_retrieved | ✅ |
| 5 机柜 | 配线架列表 | `GET /api/resources/patch-panels?cabinet_id=…` | 200 | true | server.patch_panel.list_retrieved | ✅ |
| 5 机柜 | 工位列表 | `GET /api/resources/workstations?room_id=…` | 200 | true | server.workstation.list_retrieved | ✅ |
| 5 机柜 | 信息点列表 | `GET /api/resources/net-outlets?room_id=…` | 200 | true | server.net_outlet.fetched | ✅ |
| 5 机柜 | 无机柜机位被拒绝（R8 风险已闭合）（400 = cabinet_id 必填校验生效） | `POST /api/resources/positions（无 cabinet_id）` | 400 |  | server.position.cabinet_required | ✅ |
| 5 机柜 | U 位重叠被数据库触发器拒绝（非 2xx 即约束生效） | `PUT positions（1-10 与 5-8 重叠）` | 400 |  | server.position.u_range_overlap | ✅ |
| 6 设备 | 创建核心交换机（机位安装） | `POST /api/resources/devices` | 200 | true | server.device.created | ✅ |
| 6 设备 | 创建接入交换机 | `POST /api/resources/devices` | 200 | true | server.device.created | ✅ |
| 6 设备 | 创建服务器（工位部署） | `POST /api/resources/devices` | 200 | true | server.device.created | ✅ |
| 6 设备 | 创建办公电脑 | `POST /api/resources/devices` | 200 | true | server.device.created | ✅ |
| 6 设备 | 设备列表（搜索） | `GET /api/resources/devices?search=…` | 200 | true | server.device.list_retrieved | ✅ |
| 6 设备 | 设备详情 | `GET /api/resources/devices/{id}` | 200 | true | server.device.fetched | ✅ |
| 6 设备 | 更新设备 | `PUT /api/resources/devices/{id}` | 200 | true | server.device.updated | ✅ |
| 6 设备 | 同步网卡/网口配置 | `PUT /api/resources/devices/{id}/network-config` | 200 | true | server.device.nic.synced | ✅ |
| 6 设备 | 同步网口并绑定固定 IP | `PUT /api/resources/devices/{id}/network-config（含 IP）` | 200 | true | server.device.nic.synced | ✅ |
| 6 设备 | 设备网卡树 | `GET /api/resources/devices/{id}/nics` | 200 | true | server.device.nic.fetched | ✅ |
| 6 设备 | 设备网口列表 | `GET /api/resources/devices/{id}/interfaces` | 200 | true | server.device.interface.list_retrieved | ✅ |
| 6 设备 | 设备 IP 列表 | `GET /api/resources/devices/{id}/ips` | 200 | true | server.ip.device_list_fetched | ✅ |
| 6 设备 | 全部设备网口（跨设备） | `GET /api/resources/devices/interfaces` | 200 | true | server.device.interface.list_all_retrieved | ✅ |
| 6 设备 | 创建交换机端口 24 | `POST /api/resources/devices/{id}/interfaces` | 200 | true | server.device.interface.created | ✅ |
| 6 设备 | 创建交换机端口 25 | `POST /api/resources/devices/{id}/interfaces` | 200 | true | server.device.interface.created | ✅ |
| 6 设备 | 创建接入交换机端口 1 | `POST /api/resources/devices/{id}/interfaces` | 200 | true | server.device.interface.created | ✅ |
| 6 设备 | 创建接入交换机端口 2 | `POST /api/resources/devices/{id}/interfaces` | 200 | true | server.device.interface.created | ✅ |
| 6 设备 | 设备端口列表 | `GET /api/resources/devices/{id}/interfaces` | 200 | true | server.device.interface.list_retrieved | ✅ |
| 6 设备 | 全部设备端口（跨设备） | `GET /api/resources/devices/interfaces` | 200 | true | server.device.interface.list_all_retrieved | ✅ |
| 6 设备 | 更新端口 | `PUT /api/resources/devices/interfaces/{port_id}` | 200 | true | server.device.interface.updated | ✅ |
| 6 设备 | 设备分配固定 IP | `POST /api/resources/devices/{id}/ips` | 200 | true | server.ip.created | ✅ |
| 6 设备 | IP 自动分配 | `POST /api/resources/ip/auto-assign` | 200 | true | server.ip.auto_assigned | ✅ |
| 6 设备 | IP 台账列表（按网段） | `GET /api/resources/ip?network=…` | 200 | true | server.ip.fetched | ✅ |
| 6 设备 | 设备模板列表 | `GET /api/resources/device-templates` | 200 | true | server.device_template.list_retrieved | ✅ |
| 6 设备 | 工位/机位互斥校验 | `POST /api/resources/devices（同时指定）` | 400 |  | server.device.workstation_position_exclusive | ✅ |
| 6 设备 | 设备房间一致性校验（触发器）（非 2xx 即约束生效） | `POST /api/resources/devices（工位属他房）` | 400 |  | server.device.workstation_room_mismatch | ✅ |
| 6 设备 | 非法设备类型被拒绝 | `POST /api/resources/devices（device_type=router）` | 400 |  | server.device.validation.type_invalid | ✅ |
| 6 设备 | SNMP 目标回环地址防护（SSRF 防护生效） | `POST /api/resources/devices/test-snmp（127.0.0.1）` | 400 |  | server.device.snmp.loopback_forbidden | ✅ |
| 6 设备 | SNMP 信息（无真实设备）（预期失败） | `GET /api/resources/devices/{id}/snmp-info` | 400 |  | server.device.no_ip_configured | ✅ |
| 6 设备 | MAC 表同步（无真实设备）（预期失败） | `POST /api/resources/devices/{id}/macs/sync` | 400 |  | server.device.no_ip_configured | ✅ |
| 7 线路 | 线路列表（端点过滤） | `GET /api/resources/cable-links?endpoint_type=…` | 200 | true | server.cable_link.fetched | ✅ |
| 7 线路 | 创建端口-端口链路 | `POST /api/resources/cable-links` | 200 | true | server.cable_link.created | ✅ |
| 7 线路 | 创建信息点-配线架链路 | `POST /api/resources/cable-links` | 200 | true | server.cable_link.created | ✅ |
| 7 线路 | 创建信息点-端口链路 | `POST /api/resources/cable-links` | 200 | true | server.cable_link.created | ✅ |
| 7 线路 | 线路详情 | `GET /api/resources/cable-links/{id}` | 200 | true | server.cable_link.fetched | ✅ |
| 7 线路 | 更新线路 | `PUT /api/resources/cable-links/{id}` | 200 | true | server.cable_link.updated | ✅ |
| 7 线路 | 线缆路径查询（find_cable_path） | `GET /api/resources/cable-links/path?…` | 200 | true | server.cable_link.path_fetched | ✅ |
| 7 线路 | 自环链路被拒绝（非 2xx 即约束生效） | `POST /api/resources/cable-links（a==b）` | 400 |  | server.cable_link.self_connection_forbidden | ✅ |
| 7 线路 | 非法端点类型被拒绝 | `POST /api/resources/cable-links（invalid_type）` | 400 |  | server.cable_link.invalid_endpoint_type | ✅ |
| 7 线路 | 删除被线路引用的设备（R1，线路级联清理）（线路随 device_interfaces 级联删除，无残留引用） | `DELETE /api/resources/devices/{id}（端口被引用）` | 200 | true | server.device.deleted | ✅ |
| 8 可视化 | 保存拓扑节点坐标 | `POST /api/resources/topology/nodes` | 200 | true | server.visualization.topology_nodes_saved | ✅ |
| 8 可视化 | 拓扑节点列表 | `GET /api/resources/topology/nodes` | 200 | true | server.visualization.topology_nodes_retrieved | ✅ |
| 8 可视化 | 创建物理拓扑连线 | `POST /api/resources/topology/connections` | 200 | true | server.visualization.topology_connection_created | ✅ |
| 8 可视化 | 创建逻辑拓扑连线（聚合） | `POST /api/resources/topology/connections` | 200 | true | server.visualization.topology_connection_created | ✅ |
| 8 可视化 | 拓扑连线列表（含派生） | `GET /api/resources/topology/connections` | 200 | true | server.visualization.topology_connections_retrieved | ✅ |
| 8 可视化 | 删除拓扑节点 | `DELETE /api/resources/topology/nodes/{device_id}` | 200 | true | server.visualization.topology_node_deleted | ✅ |
| 8 可视化 | 保存工位布局 | `POST /api/resources/layouts` | 200 | true | server.visualization.workstation_layout_saved | ✅ |
| 8 可视化 | 读取工位布局 | `GET /api/resources/layouts/workstation/{room_id}` | 200 | true | server.visualization.layout_retrieved | ✅ |
| 8 可视化 | 读取机位布局 | `GET /api/resources/layouts/positions/{room_id}` | 200 | true | server.visualization.cabinet_layout_retrieved | ✅ |
| 8 可视化 | 房间机柜聚合视图 | `GET /api/resources/layouts/room-cabinets/{room_id}` | 200 | true | server.visualization.room_cabinets_retrieved | ✅ |
| 8 可视化 | 自连拓扑被拒绝（非 2xx 即约束生效） | `POST /api/resources/topology/connections（自连）` | 400 |  | server.visualization.self_connection_forbidden | ✅ |
| 8 可视化 | 拓扑自动发现（无真实设备）（预期发现 0 条） | `POST /api/resources/topology/auto-discover` | 200 | true | server.visualization.auto_discover_completed | ✅ |
| 9 系统只读 | 仪表盘统计 | `GET /api/system/dashboard-stats` | 200 | true | server.system.dashboard_stats_retrieved | ✅ |
| 9 系统只读 | 系统信息（连接池指标） | `GET /api/system/info` | 200 | true | server.system.info_retrieved | ✅ |
| 9 系统只读 | 系统配置（脱敏检查） | `GET /api/system/config` | 200 | true | server.system.config_retrieved | ✅ |
| 9 系统只读 | 操作日志 | `GET /api/logs/operation` | 200 | true | server.logs.operation_retrieved | ✅ |
| 9 系统只读 | 登录日志 | `GET /api/logs/login` | 200 | true | server.logs.login_retrieved | ✅ |
| 9 系统只读 | 通知列表 | `GET /api/notifications` | 200 | true | server.notification.list_retrieved | ✅ |
| 9 系统只读 | 导入模板下载（二进制附件无 success 字段） | `GET /api/system/import-export/template?type=device` | 200 |  |  | ✅ |
| 9 系统只读 | 全量 CSV 导出（二进制附件无 success 字段） | `GET /api/system/import-export/export/csv?type=all` | 200 |  |  | ✅ |
| 9 系统只读 | 普通用户创建组织被拒绝（缺陷#16 已闭合）（403 = RBAC 生效） | `POST /api/resources/organizations（user 角色）` | 403 |  | server.auth.admin_required | ✅ |

## 汇总

- 用例总数：121
- 通过：121
- 失败：0
- 起止时间：2026-09-06 16:05:09 ~ 2026-09-06 16:05:13

### 本批次保留的业务数据

| 资源 | 名称 | ID |
|---|---|---|
| 组织模板 | 模板-园区层级-T0906160509 | `32edfd09-f01f-4fac-b0b7-a64263f51c3c` |
| 组织（根/楼宇/楼层） | 测试园区-T0906160509 / 一号楼-T0906160509 / 一层-T0906160509 | `d65558a7-c469-46b7-b7a9-b9a907c30796` / `fa3f4ee6-faa3-4099-a6ca-08d40c0b7be8` / `6a5594ef-45b0-4c0b-9383-d2d9a1d053f8` |
| 网络区域 | 核心区域-T0906160509 | `b0421bf8-cc5a-45de-9cce-f9b8bc21fe24` |
| 网段 | 办公/服务器/IPv6 | `5fe0e09a-8be2-44ee-a02a-6b9630dc295a` / `a3d32121-efb4-472f-ada6-61d199ff7a3e` / `a5e6d34b-2b7d-426d-84e5-3a387a1e4bd2` |
| 房间 | 测试机房/测试办公室 | `9a76c69a-be80-4fb6-b0d3-03b02309200a` / `6227b424-72a6-4a08-9eac-0b0a4d06e35d` |
| 机柜 | 机柜A/机柜B | `4e2ffeff-5c4b-4a70-ad47-22e852e02de7` / `cb31c401-6cca-4efa-b69d-d443e6341113` |
| 设备 | 核心/接入交换机、服务器、电脑 | `264b2168-8ab6-4794-9e78-cb92c01dfb08` `7cb5a288-7c8f-440c-9762-7662694e54a6` `ec46d056-61bf-44b0-b6a9-a2a4a4bae402` `f0936440-a84b-474d-a494-4328cc63c79f` |
| 线路 | 3 条 + 1 条防删验证 | `3f1ce7ad-f34d-4982-81b9-4ed00b0e8b42` `7c47e6fa-04a6-4422-8aa9-498e4ebd4363` `c4917183-2dc2-4a45-ab91-f0666e30f78e` |
