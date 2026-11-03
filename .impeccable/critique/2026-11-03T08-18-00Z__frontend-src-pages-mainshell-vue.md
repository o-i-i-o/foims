---
target: 前端布局（MainShell 应用壳及代表页面）
total_score: 25
max_score: 36
na_heuristics: 10
p0_count: 1
p1_count: 3
target_identity: "file:/media/oi-io/data/foims/frontend/src/pages/MainShell.vue"
target_fingerprint: "sha256:8096085b3f98e55d6a2c57c131b5f1d56599fccfbfc8504ade9e64c988197a11"
target_path: /media/oi-io/data/foims/frontend/src/pages/MainShell.vue
timestamp: 2026-11-03T08-18-00Z
slug: frontend-src-pages-mainshell-vue
---
# FOIMS 前端布局评审（critique 快照）

Method: dual-agent (A: 28d2e459 设计评审 · B: f09da4ae CLI 检测 + a1077b95/aa67e5a0 浏览器证据)

## 设计健康得分（Nielsen 10 启发式，0-4）

| # | 启发式 | 分 | 关键问题 |
|---|---|---|---|
| 1 | 状态可见 | 3 | 工位拖动 600ms 静默自动保存，保存与否不可见（VisualizationPage.vue:343） |
| 2 | 贴近真实 | 3 | U 位/机位/门隐喻准确；"IP 详情"一级菜单名偏技术 |
| 3 | 用户控制 | 2 | 删除布局一键清空无撤销；拖拽误触即落库无 undo |
| 4 | 一致性 | 2 | 筛选三模式并存；两套弹窗体系（ModalShell vs DeviceDetailPanel 自绘） |
| 5 | 错误预防 | 3 | danger ConfirmModal、渲染中拒保存、Number("")===0 显式拦截 |
| 6 | 识别而非回忆 | 3 | tooltip 全覆盖；但各页无标题，仅靠 section aria-label |
| 7 | 灵活效率 | 3 | 分页 hover 预取、页签 localStorage 记忆、aria-sort 键盘可达 |
| 8 | 美学极简 | 3 | Agents 页 12 列略挤；其余克制 |
| 9 | 错误恢复 | 3 | 空态/错误/加载三态齐备 |
| 10 | 帮助文档 | n/a | 源码层面无文档入口 |
| **合计** | | **25/36** | **Acceptable（69%，接近 Good）** |

## 设计具体性判定

画布层高度量身（Open Color token + AA 对比注释、机柜 U 位/网格标尺/线缆标签色、工位 IP 状态染色）≈9/10；页面骨架层是通用后台模板（sidebar+tabs+table+pagination 换个 CRM 也成立；仪表盘 emoji 卡片与侧边栏 SVG 线稿体系割裂）≈5/10。综合：半量身。

## 确定性扫描（detector）

6 条发现，全部为 side-tab 规则（卡片单侧彩色边条）：network-card.css:12/22/32、tables.css:806、system.css:594、toasts.css:29。抽查判定：network-card 与 import-result 为真实命中；.network-group-header（表格分组行）与 .toast（状态色条为平台通用惯例）疑似规则范围外误报。检测器未发现评审遗漏的布局问题（其职责不覆盖布局结构）。

## 浏览器证据

admin 登录成功，仪表盘/可视化/资源/系统 4 页截图（/tmp/trae/screenshots/01~04.png）。可见证据：仪表盘 emoji 大色块卡片与线稿图标体系冲突；可视化工具栏按钮文字两行换行（拥挤）；资源页表头内嵌搜索；系统页 emoji 分节图标。detect.js overlay 注入两页均失败（加载 http://localhost:8400/detect.js 报 ERR_ABORTED），无用户可见 overlay；以 CLI 扫描 + 截图为浏览器证据。附加：vite 编译告警 ArpModal.vue `<tr>` 直接作为 `<table>` 子元素（缺 thead/tbody）。

## 整体印象

骨架合格但通用，画布层是产品亮点；最大机会是统一页面骨架（页头/留白/工具栏规范），让画布层的工艺水准延伸到全站。

## 做得好

1. 收起态动效工程（sidebar.css max-width/opacity 延迟淡入、图标中心精确计算）
2. 移动端抽屉（汉堡+scrim+Esc+路由即收，aria-expanded 完备，与桌面收起态解耦）
3. Pagination 组件（省略页码、跳页 >7 页才出现、hover 预取）

## 优先问题

- P0 工具栏窄视口溢出：.toolbar 无 flex-wrap（main-content.css:139-143），Agents 8 控件/拓扑 7 控件横向溢出，截图中按钮已两行换行。改法：flex-wrap + 低频操作收进"更多"下拉。→ /impeccable adapt
- P1 无页面页头与统一留白：main-content 无内边距贴边 vs system.css 自带 1.5rem，全站无页标题区。改法：统一 page-header（h2+主操作右置）+ 全局 content gutter。→ /impeccable layout
- P1 筛选三轨 + 双轨保存：表头内嵌/工具栏/下拉筛选并存；自动保存与手动保存按钮双真相。改法：统一筛选区组件，自动保存加瞬时"已保存"徽标或撤下手动按钮。→ /impeccable distill
- P1 危险操作位置不一致：机柜页签"删除布局"居中、工位页签居最右紧邻"保存布局"；DeviceDetailPanel"移除出拓扑"紧邻"保存坐标"。改法：危险按钮恒右置+视觉分隔+带数量二次确认。→ /impeccable audit
- P2 双保存心智模型：静默自动保存 + 手动保存按钮并存，用户须记住哪条是真相。→ /impeccable distill

## 认知负荷

3 项失败（中等）：选项>4（Agents 工具栏 8 控件、System 7 页签、拓扑工具栏 7 控件）；分组失败（Agents 12 列、IP 页 10 列、IpPage 位置单元格三重复合）；工作记忆（双轨保存）。

## 情感旅程

高峰在画布（对齐参考线+流畅落库）；低谷在初始化向导第 3 步要求去服务器控制台抄验证码且无辅助说明；工程兜底扎实但缺"已保存"正反馈承接。

## Persona 红旗

Alex（高级运维）：12 列主机表无列显隐/密度切换；删布局无快照恢复；三画布交互模型不一（fit-only/scroll-x/pan+zoom）肌肉记忆断裂。
Sam（无障碍）：移动抽屉无焦点陷阱（弹窗有 useFocusTrap）；全站唯一 h1 是侧边栏 FOIMS，各页无标题；--btn-height:32px、sidebar-toggle 30px 低于 44px 触摸目标；IpPage.vue:129 排序 th 同时挂 role="button" 与 aria-sort 角色冲突。

## 次要观察

visualization.css/tables.css 注释引用已不存在的 responsive.css；表格 td 全居中不利长文本扫读；zoom 下拉用 'current' 假 option 回显是 hack；MIN_ZOOM_SCALE=0.05 时网格文字不可读无提示；ArpModal.vue `<tr>` 直属 `<table>`。

## 启发性问题

1. 工位/机柜/拓扑若合并为单画布+图层切换，一级导航可否减至 7 项？
2. 资源页 6 页签与 IP 页重复暴露网络信息，是否该以"资产为中心"重组 IA？
3. 布局这类强创作型数据，用"版本快照"替代删除确认是否更安心？
