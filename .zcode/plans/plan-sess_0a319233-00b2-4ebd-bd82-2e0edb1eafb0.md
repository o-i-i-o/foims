## 总体方案（已确认：复用 net_outlets；配线架仅名称）

配线架 = `net_outlets`(outlet_type='patch_panel', cabinet_id=机柜)。按 outlet_type 划分归属：**房间信息点管理器只管非 patch_panel；机柜管 patch_panel**。线路里"配线架"端点在后端仍存为 `net_outlet` 类型。**因此无需任何 DB DDL 迁移**（patch_panel outlet_type 与 cabinet_id 列已存在），只改查询过滤 + 新增同步接口 + 前端。

---

### A. 标签页顺序（main.html）
把"线缆链路"标签按钮 + 其 tab-content 面板，从"机柜"之后移到"设备"之后。
新顺序：网络区域 → 网段 → 房间 → 机柜 → **设备 → 线缆链路**。

### B. 机柜配线架（参考房间信息点内联模式）
- **cabinet-modal.html**：在"机位"区与"描述"之间，新增配线架区：`#cabinet-patch-panels-container` + `#add-patch-panel-row-btn`。
- **cabinet.js**：新增 `CabinetPatchPanelsManager` 类（仿 `CabinetPositionsManager`，但每行仅一个名称输入框 + 删除按钮）；`openCabinetModal` 编辑时 `loadExisting(cabinet.patch_panels)`、新增时 `init()`；`submitCabinetForm` 在同步机位后再 `PUT /api/resources/cabinets/{id}/patch-panels`。
- **room.js**：`outletTypeOptionsHtml` 去掉 `patch_panel` 选项（配线架改由机柜管理）。

### C. 后端
- **room.rs `get_room`**：信息点 SELECT 加 `AND no.outlet_type != 'patch_panel'`（房间模态不再显示机柜配线架）。
- **room.rs `sync_room_net_outlets`**：existing-ids 查询加 `AND outlet_type != 'patch_panel'`，使房间同步只作用于非配线架信息点（不会误删机柜配线架）。
- **cabinets.rs `get_cabinet`**：返回 `patch_panels`（`SELECT id,name FROM net_outlets WHERE cabinet_id=$1 AND outlet_type='patch_panel'`）。
- **cabinets.rs 新增 `sync_cabinet_patch_panels`**：`PUT /api/resources/cabinets/{id}/patch-panels`，body `{patch_panels:[{id?,name}]}`。查 cabinet 的 room_id（net_outlets.room_id 非空），existing=`WHERE cabinet_id=$1 AND outlet_type='patch_panel'`；删请求中不存在的（cable_links 触发器拦截→友好报错）；逐条 UPDATE/INSERT（固定 outlet_type='patch_panel'、cabinet_id=本机柜、room_id=机柜房间）；唯一约束 `UNIQUE(room_id,name)` 冲突→友好报错。仿 `sync_cabinet_positions`。
- **net_outlet.rs `get_net_outlets`**：新增 `cabinet_id` 查询过滤（供线路模态"配线架→机柜"级联用）。
- **models.rs**：新增 `PatchPanelBrief{id,name}`、`PatchPanelSyncItem{id:Option<Uuid>,name:String}`、`CabinetPatchPanelsSync{patch_panels:Vec<PatchPanelSyncItem>}`；`get_cabinet` 返回结构体加 `patch_panels: Option<Vec<PatchPanelBrief>>`。
- **routes/mod.rs**：注册 `PUT /api/resources/cabinets/{id}/patch-panels`。
- **views.rs `cable_links_with_details`**：endpoint_labels CTE 对 net_outlets 一行多带 `outlet_type`，视图新增 `a_endpoint_outlet_type`/`b_endpoint_outlet_type`（供前端区分信息点/配线架）。相应 `CableLinkWithDetails` 加两个 `Option<String>` 字段。

### D. 线路模态框重构（cable-link-modal.html + cableLink.js）
端点类型收敛为 3 个（UI 值）：`net_outlet`(信息点) / `device`(设备接口) / `patch_panel`(配线架)。
- **HTML**：A/B 各自一行 = 类型 select + 范围 select(`cable-link-a-scope`/`-b-scope`) + 端点 select。范围框由 JS 按 类型 显示/隐藏与填充。
- **JS 级联**：
  - 类型=信息点 → 范围=房间(rooms)；端点=`GET /net-outlets?room_id=X`，前端过滤掉 patch_panel；每项 `data-et="net_outlet"`。
  - 类型=设备接口 → 范围=设备(devices)；端点=合并该设备的 `device-ports` + `interfaces`(仅 physical/wifi)，用 `<optgroup>` 分"端口/接口"两组；每项 `data-et="device_port"` 或 `"device_interface"`。
  - 类型=配线架 → 范围=机柜(cabinets)；端点=`GET /net-outlets?cabinet_id=X&outlet_type=patch_panel`；每项 `data-et="net_outlet"`。
- **提交**：读取所选端点 option 的 `dataset.et` 作为后端 `*_endpoint_type`，value 作为 id（设备接口项按实际子类型 device_port/device_interface 提交；配线架提交 net_outlet）。**后端 cable_links 仍只存 device_port/net_outlet/device_interface，无需改 CHECK/触发器/视图语义。**
- **编辑态**（端点不可改）：后端类型映射回 UI 类型（device_port/device_interface→设备接口；net_outlet 看 `a_endpoint_outlet_type` 区分信息点/配线架）；范围框隐藏，端点框只读显示已存 label。
- **列表显示**：device_port/device_interface → "设备接口"；net_outlet 看 outlet_type → 信息点/配线架。
- ENDPOINT_TYPE_LABELS 同步更新。

### E. i18n + 缓存
- zh.json/en.json 新增键：`cable_link.endpoint_device`(设备接口)、`cable_link.endpoint_patch_panel`(配线架)、`cable_link.scope`/scope_room/scope_device/scope_cabinet、`cabinet.patch_panels`/add_patch_panel/no_patch_panels_hint 等。
- 缓存版本 `01268 → 01269`（resourceLoader.js + main.html app.js?v=）。

### F. 构建与验证
- `cargo build --release` → 重启服务（init.enabled=false，但本方案无 DDL，无需手动迁移；仅需重启加载新代码）。
- API 验证：创建带配线架的机柜并 GET 回显；线路分别用信息点/设备接口/配线架三种端点创建并列表回显。
- 浏览器：硬刷新后验证（上一轮已把 nginx 开发环境改 no-store）。

### 风险/注意
- 约束 `UNIQUE(room_id,name)`：同一房间内不同机柜的配线架不能重名，冲突时给清晰提示。
- 房间信息点查询加 `outlet_type != 'patch_panel'` 过滤；若库里已有"通过房间创建的 patch_panel 且 cabinet_id 为空"的历史数据，会变得不可见——实施时会先查库，若有则报告处理。
- 设备接口收敛是纯前端 UI（后端仍区分 device_port/device_interface），不影响既有数据与拓扑发现。