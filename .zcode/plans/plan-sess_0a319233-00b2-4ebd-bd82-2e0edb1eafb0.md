## 任务 1：彻底删除「信息点(net_outlets)」的 description 字段（代码 + 数据库）

数据库列 `net_outlets.description` 同时被「房间模态框」和「独立信息点模态框(网络出口)」共用。按确认的方案彻底删除该列并清理所有引用。**注意：该列已有数据将被删除（不可恢复）。**

### 数据库 / Schema（Rust）
- `crates/ipma-init/src/schema/tables/net_outlets.rs`：CREATE TABLE 中移除 `description TEXT,`；新增 `ALTER TABLE net_outlets DROP COLUMN IF EXISTS description`（保证已有数据库在服务重启时自动迁移）。
- `crates/ipma-init/src/schema/tables/views.rs`：`net_outlets_with_details` 视图 SELECT 中移除 `ap.description,`（视图在 `create_all_tables` 中每次启动重建，顺序在 net_outlets 之后，安全）。

### 后端 handler
- `src/resource/room.rs`：
  - `sync_room_net_outlets`：UPDATE/INSERT 语句移除 `description` 占位符与 `.bind(&item.description)`。
  - `get_room`：信息点 SELECT 移除 `no.description`，`NetOutletBrief` 映射移除 description 字段。
- `src/resource/net_outlet.rs`：
  - `get_net_outlets`：SELECT 移除 `ap.description`；搜索 ILIKE 子句移除 `ap.description`。
  - `create_net_outlet`：INSERT 移除 description 列与 bind；日志 details 移除 description。
  - `get_net_outlet` / `update_net_outlet` 末尾 SELECT 移除 description；update 中移除 description set 子句、bind 与日志 details。

### Models（`src/models.rs`）
移除 `description` 字段：`NetOutletSyncItem`、`NetOutletBrief`、`NetOutlet`、`NetOutletWithDetails`、`NetOutletCreate`、`NetOutletUpdate`（含对应 `#[validate]` 注解）。

### 前端
- `web/static/js/modules/room.js`：`createRow` 移除描述 `<input class="net-outlet-description">` 的 `<div class="form-group">`；`collectData` 移除 `descInput` 读取与 `description` 字段。
- `web/static/modals/net-outlet-modal.html`：移除 `net-outlet-description` 的 `<div class="form-group">`（label+textarea）。
- `web/static/js/modules/netOutlet.js`：列表表格移除 description 列；`submitNetOutletForm` 移除 description 读取与字段；`openNetOutletModal` 移除 `setValue('net-outlet-description', ...)`。

### 应用 & 验证
- `cargo build`（release）重建服务，重启后 `create_all_tables` 自动执行 `DROP COLUMN`。
- 用 ipma/admin123 连接数据库确认 `net_outlets` 表已无 description 列。

---

## 任务 2：刷新浏览器后停留在当前子标签页（覆盖全部子标签系统）

顶层页面已用 URL hash 持久化，刷新后页面正确；问题在子标签（仅 DOM 内存态、默认硬编码）。方案：用 localStorage 记住每个页面的子标签，初始化时恢复。**副作用：从侧边栏进入某页面时也会回到上次子标签（符合"记住上次位置"的预期）。**

### 实现
- `web/static/js/utils/helpers.js`：新增 `setActiveSubtab(pageId, tabId)` / `getActiveSubtab(pageId)`，localStorage key `ipma_subtab_<pageId>`。
- 在 4 个子标签系统中接入：
  - `web/static/js/modules/resourceTabs.js`（#resources）：`bindTabClickHandlers` 点击时保存；`loadDefaultTabData` 改为先读 storage，命中且按钮存在则激活它，否则沿用原默认。
  - `web/static/js/modules/log.js`（#logs）：点击处理函数保存；初始化末尾的默认分支改为先读 storage（保留现有 `?tab=` URL 参数逻辑作为更高优先级，不破坏既有跳转）。
  - `web/static/js/modules/systemManager.js`（#system）：点击保存；初始化默认分支改为先读 storage。
  - `web/static/js/modules/visualization/visualizationManager.js`（#visualization）：`initTabSwitching` 点击保存；初始化时先读 storage 激活对应按钮。
- 与现有 dashboard 跨级导航（`dashboard.js` 模拟点击 `.tab-btn`）兼容：模拟点击会触发点击处理函数从而写入 storage，行为一致。

### 验证
浏览器进入资源管理→切到"设备"→F5 刷新，应仍停留在"设备"而非"网络区域"；其它页面同理。

---

### 风险与回滚
- 任务1 删列不可逆：迁移前我会先确认现有数据是否需要备份；可通过 git 还原代码，但 DB 列数据需手动备份。
- 任务2 纯前端、可安全回滚。