# 实施计划：线路/设备标签调整、端点类型整合、机柜配线架

## 已确认的设计决策
- **端点类型整合方式 = UI 层整合**：模态框端点类型只保留一个「设备接口」选项；后端 `device_port` / `device_interface` 两种类型都保留不变。下拉值用前缀编码（`device_interface:<id>` / `device_port:<id>`），提交时拆回真实类型。无需数据迁移。
- **列表标签统一**：线路列表中 `device_port` 与 `device_interface` 都显示为「设备接口」。
- 配线架在数据层即 `net_outlets` 表中 `outlet_type='patch_panel'` 且带 `cabinet_id` 的行（沿用既有设计）。

---

## A. 数据库 schema

### A1. `crates/ipma-init/src/schema/tables/cable_links.rs`
- **行 7、行 9** 的列级 `CHECK ... IN ('device_port','net_outlet','device_interface')` → 增加 `'patch_panel'`（供全新建库）。
- 在 `CREATE TABLE` 之后追加**幂等迁移**（适配已存在的库，因项目无 migration 系统）：删除自动命名的列级约束并重建为命名约束，包含 `patch_panel`：
  ```sql
  ALTER TABLE cable_links DROP CONSTRAINT IF EXISTS cable_links_a_endpoint_type_check;
  ALTER TABLE cable_links DROP CONSTRAINT IF EXISTS cable_links_b_endpoint_type_check;
  ALTER TABLE cable_links ADD CONSTRAINT chk_cl_a_type CHECK (a_endpoint_type IN ('device_port','net_outlet','device_interface','patch_panel'));
  ALTER TABLE cable_links ADD CONSTRAINT chk_cl_b_type CHECK (b_endpoint_type IN ('device_port','net_outlet','device_interface','patch_panel'));
  ```
  （`chk_endpoint_order` 用字符串字典序排序，新增 patch_panel 后顺序自动为 device_interface < device_port < net_outlet < patch_panel，无需改。）
- **`validate_cable_link_endpoints` 触发器**（行 54-68 A 侧、76-90 B 侧）：各新增分支
  ```sql
  WHEN 'patch_panel' THEN
      SELECT EXISTS(SELECT 1 FROM net_outlets WHERE id = NEW.<side>_endpoint_id AND outlet_type='patch_panel') INTO endpoint_exists;
  ```
  不新增跨类型禁止规则（patch_panel 可与任意类型相连，仅保留既有自连接禁止与 device_interface↔device_interface 禁止）。
- **`prevent_net_outlet_deletion_if_linked`**（行 142-153）：当前只检查 `endpoint_type='net_outlet'`。追加对 `endpoint_type='patch_panel'` 的 OR 检查，防止被引用的配线架被删除（因配线架行也在 net_outlets 表）。
- **`find_cable_path` 的 node_label CASE**（行 255-259）：新增
  ```sql
  WHEN 'patch_panel' THEN (SELECT name FROM net_outlets WHERE id = p.node_id AND outlet_type='patch_panel')
  ```

### A2. `crates/ipma-init/src/schema/tables/views.rs`
- `cable_links_with_details` 视图的 `endpoint_labels` CTE（行 181-190）新增 patch_panel 分支：
  ```sql
  UNION ALL
  SELECT no.id, 'patch_panel'::VARCHAR, no.name::text
  FROM net_outlets no WHERE no.outlet_type = 'patch_panel'
  ```
  （现有 `net_outlet` 分支无需改，按 etype 精确连接不会冲突。视图每次 schema 运行时 DROP+CREATE。）

> **重要提示（适用 A1/A2 全部 schema 改动）**：项目无 migration 系统，`create_tables()` 仅在「初始化/建库/导入库」流程中执行。对已运行的既有库，需重新走一次建库/导入初始化（或手动执行上述 ALTER/重建视图与函数）才会生效。

---

## B. 后端 Rust

### B1. `src/resource/cable_link.rs`
- 行 26 `VALID_ENDPOINT_TYPES` 增加 `"patch_panel"`（`validate_endpoint_type` 自动生效）。其余（排序、过滤、自连接禁止）无需改动。

### B2. `src/resource/net_outlet.rs` — 新增 `cabinet_id` 过滤
- `get_net_outlets`（行 18-137）仿照 `room_id` 处理（行 43-48、72-74、104-105、117-118）增加 `cabinet_id` 查询参数：读取 → 解析 Uuid → 追加 `ap.cabinet_id = $n` 条件并绑定。用于线路模态框「配线架」按机柜过滤。

### B3. `src/resource/cabinets.rs` — 配线架同步与读取
- **新增 `sync_cabinet_net_outlets`**：仿照 `room.rs::sync_room_net_outlets`（行 710-842）与 `sync_cabinet_positions`（行 486-602）：
  - 校验机柜存在；取 `cabinet.room_id`（配线架的 `room_id` 必须取自机柜所在房间，因 net_outlets.room_id NOT NULL）。
  - 既有 id 范围：`SELECT id FROM net_outlets WHERE cabinet_id=$1 AND outlet_type='patch_panel'`。
  - 删除请求中不存在的行，映射 cable_links 引用错误为「配线架已被线路引用，无法删除」。
  - 新增/更新：强制 `outlet_type='patch_panel'`、`cabinet_id=<机柜id>`、`room_id=<机柜room_id>`；名称唯一性复用 `validate_net_outlet_name(room_id)`。
  - 日志 action=`sync_net_outlets`，resource_type=`cabinet`。
- **`get_cabinet`（行 281-333）**：额外查询该机柜的配线架 `SELECT id,name FROM net_outlets WHERE cabinet_id=$1 AND outlet_type='patch_panel' ORDER BY name`，挂到响应。

### B4. `src/models.rs`
- `CabinetWithNetworks` 增加 `pub patch_panels: Option<Vec<NetOutletBrief>>`（复用 `NetOutletBrief`，含 id/name/outlet_type/cabinet_id/cabinet_name），并在 `get_cabinet`、`get_cabinets` 构造处补 `Some(vec)`/`None`。
- 新增同步 DTO（语义清晰）：
  ```rust
  pub struct PatchPanelSyncItem { pub id: Option<Uuid>, pub name: String }
  pub struct CabinetPatchPanelsSync { pub patch_panels: Vec<PatchPanelSyncItem> }
  ```
  （含 `validate()`：name 非空。）

### B5. `src/routes/mod.rs`
- 机柜路由区（约行 236 旁）新增：
  ```rust
  .route("/api/resources/cabinets/{id}/net-outlets", put(sync_cabinet_net_outlets))
  ```
  并在 imports 中引入 `sync_cabinet_net_outlets`。

---

## C. 前端 — 线路模态框与标签

### C1. `web/static/main.html` — 标签顺序
- 将「设备」(`devices`) 标签按钮与面板 移到「线路」(`cable-links`) **之前**。最终顺序：网络区域、网段、房间、机柜、**设备、线路**。仅调整 `<button class="tab-btn">` 与对应 `.tab-content` 的 DOM 顺序（标签顺序纯由 DOM 决定，无需改 JS）。

### C2. `web/static/modals/cable-link-modal.html` — 端点类型与级联选择器
- 两端（A/B）的 `*-type` `<select>`：移除 `device_port` 选项；保留 `net_outlet`(信息点)、`device_interface`(设备接口)；新增 `patch_panel`(配线架)。
- 每端增加一个**级联「范围」选择器** `cable-link-a-scope` / `cable-link-b-scope`，其 label 与可选项由 JS 依类型动态设置（信息点→房间、设备接口→设备、配线架→机柜）。
- 布局调整：每端改为两行 —— 第 1 行 `{类型, 范围}`，第 2 行 `{端点 id}`（端点下拉依范围选择后填充）。

### C3. `web/static/js/modules/cableLink.js` — 端点加载与提交
- `ENDPOINT_TYPE_LABELS`：`device_port` 与 `device_interface` 均映射「设备接口」（列表统一显示）；新增 `patch_panel: '配线架'`。
- `loadEndpointOptions` 改造为「按类型 + 范围」加载：
  - `net_outlet`：范围=房间（复用 `loadRoomsForSelect` 或内联获取 rooms）；选房间后 `GET /api/resources/net-outlets?room_id=<id>&page_size=1000`，**客户端过滤掉 `outlet_type==='patch_panel'`**，填充 id 下拉。
  - `device_interface`（整合）：范围=设备（`GET /api/resources/devices?page_size=1000`）；选设备后并发 `GET /devices/{id}/interfaces` 与 `GET /devices/{id}/device-ports`，用 `<optgroup>` 分组（「设备接口」「设备端口」），值前缀编码：`device_interface:<id>`、`device_port:<id>`。
  - `patch_panel`：范围=机柜（`GET /api/resources/cabinets?page_size=1000`）；选机柜后 `GET /api/resources/net-outlets?outlet_type=patch_panel&cabinet_id=<id>&page_size=1000`，填充 id 下拉。
- 类型 change 处理：切换时显示对应范围选择器、设置其 label、重置范围与 id 下拉。
- 范围 change 处理：触发对应 id 下拉加载。
- `submitCableLinkForm`（新建分支）：对整合类型解析前缀 `value.split(':')` 还原 `{type, id}` 后再组装 payload；net_outlet/patch_panel 的 value 即纯 id。
- 编辑模式（端点只读）：将存储的 `device_port`/`device_interface` 都映射为选中「设备接口」选项作展示（仅展示，编辑不提交端点）；id 下拉显示服务端解析的 label（既有逻辑）。范围选择器隐藏/禁用。

### C4. i18n（`web/static/js/i18n/zh.json` 与 `en.json`）
- `cable_link` 下新增：`endpoint_patch_panel`(配线架/Patch Panel)、`scope_room`(房间/Room)、`scope_device`(设备/Device)、`scope_cabinet`(机柜/Cabinet)、以及占位「选择房间/设备/机柜」。
- `cabinet` 下新增：`patch_panels`(配线架/Patch Panels)、`add_patch_panel`(添加配线架/Add Patch Panel)。（`net_outlet.type_patch_panel`='配线架' 已存在，复用。）

---

## D. 前端 — 机柜模态框配线架（参照房间信息点）

### D1. `web/static/modals/cabinet-modal.html`
- 仿 `room-modal.html` 行 46-52 的信息点区块，在机柜表单内（机位区块之后）新增：
  ```html
  <div class="form-row"><div class="form-group">
    <label id="cabinet-patch-panels-label" data-i18n="cabinet.patch_panels">Patch Panels</label>
    <div id="cabinet-patch-panels-container"></div>
    <div class="cabinet-patch-panels-actions">
      <button type="button" id="add-patch-panel-row-btn" class="btn btn-secondary btn-sm" data-i18n="cabinet.add_patch_panel">Add Patch Panel</button>
    </div>
  </div></div>
  ```

### D2. `web/static/js/modules/cabinet.js`
- 新增 `CabinetPatchPanelsManager` 类（精简版 `RoomNetOutletsManager`）：每行仅 `name` 输入 + 删除/添加按钮（无类型下拉、无机柜下拉——父级即机柜）。方法：`init/loadExisting/createRow/addItem/removeItem/collectData`（collect 返回 `{id,name}` 数组）。
- `openCabinetModal`：编辑模式 `loadExisting(cabinet.patch_panels||[])`；新增模式 `init()`。
- `submitCabinetForm`：在机位同步（行 435）之后追加：
  ```js
  const ppData = cabinetPatchPanelsManager.collectData();
  const ppResult = await apiPut(`/api/resources/cabinets/${cabinetId}/net-outlets`, { patch_panels: ppData });
  ```
  失败提示并刷新列表。

---

## E. 验证
1. `cargo check`（或既有构建命令）确保后端编译通过。
2. 前端：确认标签顺序、线路模态框三类端点的级联选择器与下拉填充、整合后 optgroup 分组与前缀编码提交正确、编辑只读展示正常；机柜模态框配线架增删改与同步生效。
3. 列表显示：device_port/device_interface 端点统一显示「设备接口」，patch_panel 显示「配线架」且 label 正确解析。
4. DB：对既有库需重新初始化/导入以使 CHECK、触发器、视图与函数生效（或手动执行 ALTER）；删除被线路引用的配线架应被阻止。
5. 如项目前端有缓存版本号（提交历史见「更新缓存版本」），构建/部署时一并 bump。

## 涉及文件清单
- DB：`crates/ipma-init/src/schema/tables/cable_links.rs`、`crates/ipma-init/src/schema/tables/views.rs`
- 后端：`src/resource/cable_link.rs`、`src/resource/net_outlet.rs`、`src/resource/cabinets.rs`、`src/models.rs`、`src/routes/mod.rs`
- 前端：`web/static/main.html`、`web/static/modals/cable-link-modal.html`、`web/static/js/modules/cableLink.js`、`web/static/modals/cabinet-modal.html`、`web/static/js/modules/cabinet.js`、`web/static/js/i18n/zh.json`、`web/static/js/i18n/en.json`