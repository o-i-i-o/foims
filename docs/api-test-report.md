# IPMA API 分组集成测试报告

- 测试时间：2026-08-24 00:07:50
- 运行标识：T08240007（本批次创建的数据名均含该后缀，全部保留）
- 服务端点：UDS /tmp/ipma-dev.sock


| 分组 | 用例 | 请求 | 状态码 | success | message | 判定 |
|---|---|---|---|---|---|---|
| 0 基础 | 健康检查 | `GET /health` | 200 | true | server.common.success | ✅ |
| 0 基础 | 管理员登录 | `POST /api/auth/login` | 200 | true | server.common.success | ✅ |
| 0 基础 | 当前用户信息 | `GET /api/auth/me` | 200 | true | server.common.success | ✅ |
| 0 基础 | 初始化状态（公开） | `GET /api/auth/init-status` | 200 | true | server.common.success | ✅ |
| 0 基础 | 未认证访问受保护接口（期望 401） | `GET /api/resources/organizations（无凭据）` | 401 |  | server.auth.auth_failed | ✅ |
| 0 基础 | 创建普通用户 apitest_T08240007 | `POST /api/users` | 200 | true | server.user.created | ✅ |
| 0 基础 | 普通用户访问用户管理（RBAC）（期望 403） | `GET /api/users（user 角色）` | 403 |  | server.auth.admin_required | ✅ |
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
| 3 网段 | 非法 CIDR 被拒绝 | `POST /api/resources/networks（300.1.1.0/24）` | 400 |  | server.network.ipv4_cidr_invalid | ✅ |
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
| 5 机柜 | 风险验证：无机柜机位（R8）（2xx 即确认库列可空风险） | `POST /api/resources/positions（无 cabinet_id）` | 200 | true | server.position.created | ✅ |
| 5 机柜 | U 位重叠被数据库触发器拒绝（非 2xx 即约束生效） | `PUT positions（1-10 与 5-8 重叠）` | 500 |  | server.error.database | ✅ |
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
| 6 设备 | 创建交换机端口 24 | `POST /api/resources/devices/{id}/device-ports` | 200 | true | server.device.port.created | ✅ |
| 6 设备 | 创建交换机端口 25 | `POST /api/resources/devices/{id}/device-ports` | 200 | true | server.device.port.created | ✅ |
| 6 设备 | 创建接入交换机端口 1 | `POST /api/resources/devices/{id}/device-ports` | 200 | true | server.device.port.created | ✅ |
| 6 设备 | 创建接入交换机端口 2 | `POST /api/resources/devices/{id}/device-ports` | 200 | true | server.device.port.created | ✅ |
| 6 设备 | 设备端口列表 | `GET /api/resources/devices/{id}/device-ports` | 200 | true | server.device.port.list_retrieved | ✅ |
| 6 设备 | 全部设备端口（跨设备） | `GET /api/resources/devices/device-ports` | 200 | true | server.device.port.list_all_retrieved | ✅ |
| 6 设备 | 更新端口 | `PUT /api/resources/devices/device-ports/{port_id}` | 200 | true | server.device.port.updated | ✅ |
| 6 设备 | 设备分配固定 IP | `POST /api/resources/devices/{id}/ips` | 200 | true | server.ip.created | ✅ |
| 6 设备 | IP 自动分配 | `POST /api/resources/ip/auto-assign` | 200 | true | server.ip.auto_assigned | ✅ |
| 6 设备 | IP 台账列表（按网段） | `GET /api/resources/ip?network=…` | 200 | true | server.ip.fetched | ✅ |
| 6 设备 | 设备模板列表 | `GET /api/resources/device-templates` | 200 | true | server.device_template.list_retrieved | ✅ |
| 6 设备 | 工位/机位互斥校验 | `POST /api/resources/devices（同时指定）` | 400 |  | server.device.workstation_position_exclusive | ✅ |
| 6 设备 | 设备房间一致性校验（触发器）（非 2xx 即约束生效） | `POST /api/resources/devices（工位属他房）` | 500 |  | server.error.database | ✅ |
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
| 7 线路 | 风险验证：删除被线路引用的设备（R1）（非 2xx 即触发器拦截） | `DELETE /api/resources/devices/{id}（端口被引用）` | 500 |  | server.error.database | ✅ |
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
| 9 系统只读 | 风险验证：普通用户创建组织（缺陷#16）（2xx 即确认资源 RBAC 缺失） | `POST /api/resources/organizations（user 角色）` | 200 | true | server.organization.created | ✅ |

## 汇总

- 用例总数：121
- 通过：121
- 失败：0
- 起止时间：2026-08-24 00:07:50 ~ 2026-08-24 00:07:54

### 本批次保留的业务数据

| 资源 | 名称 | ID |
|---|---|---|
| 组织模板 | 模板-园区层级-T08240007 | `c7577f93-67f9-4da7-8c61-a87fa059bd85` |
| 组织（根/楼宇/楼层） | 测试园区-T08240007 / 一号楼-T08240007 / 一层-T08240007 | `1076a243-9a5d-490f-81f9-9e52649fc997` / `beb856ad-e878-4627-bf09-cc0f2317f6cf` / `ba292533-9193-4256-93a9-91f87ea4cd13` |
| 网络区域 | 核心区域-T08240007 | `cf2025f3-226b-4b92-8b86-2a2cc31fd3ae` |
| 网段 | 办公/服务器/IPv6 | `0c6ae72c-f8d6-42f0-bf14-530e35f27f05` / `c450950f-1939-4734-add2-b430f7e72241` / `825c74b8-c613-47db-8e74-7f6b0ab74f31` |
| 房间 | 测试机房/测试办公室 | `1cb1c61e-49cc-4e30-89ba-133400df8959` / `e4b7bfd9-05e6-4dbf-a1b1-8fe5b158a348` |
| 机柜 | 机柜A/机柜B | `291a2c95-d2da-41f3-9acf-026ac6742851` / `96714488-3d7b-4480-9d77-a85e826d416b` |
| 设备 | 核心/接入交换机、服务器、电脑 | `9038bb32-83d6-4ad2-a0ec-92b64f58b3ae` `6b2c30ce-69c7-4d70-9faa-ce2d02276834` `23a2c048-4001-4ba4-b42a-78119af0e822` `c3c63794-2409-4e82-9479-3e8f426b2a43` |
| 线路 | 3 条 + 1 条防删验证 | `a9ac5893-9713-4a8d-8329-6ad9443f7d88` `ecbcdf87-d391-4e82-bc6f-bbf4f0f82799` `f3946e3a-ad05-403e-bb3c-cf3c04d757fb` |
