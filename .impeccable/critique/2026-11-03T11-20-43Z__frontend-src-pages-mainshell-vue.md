---
target: 前端布局（MainShell 应用壳及代表页面）
total_score: 30
max_score: 36
na_heuristics: 10
p0_count: 0
p1_count: 1
target_identity: "file:/media/oi-io/data/foims/frontend/src/pages/MainShell.vue"
target_fingerprint: "sha256:ea008f99247f40694077942dc9b2d54d8247b33f2a18489455926e19eb6c5d48"
target_path: /media/oi-io/data/foims/frontend/src/pages/MainShell.vue
timestamp: 2026-11-03T11-20-43Z
slug: frontend-src-pages-mainshell-vue
---
# FOIMS 前端布局复审（v0.73.0 修复批次后）

Method: dual-agent (A: aad58e60 设计评审 · B: 3ba361d8 CLI 检测 + 8c8aa8c0 浏览器证据)

## 设计健康得分（Nielsen 10 启发式，0-4）

| # | 启发式 | 分 | 变化 |
|---|---|---|---|
| 1 | 状态可见 | 3 | ↑ 自动保存徽标可感知、aria-sort 播报到位 |
| 2 | 贴近真实 | 3 | = 图标语义更准（mail/bell/shield） |
| 3 | 用户控制 | 3 | ↑ 删除布局带数量确认、焦点还原 |
| 4 | 一致性 | 3 | ↑ NavIcon/排序全站统一（span 尾巴已收） |
| 5 | 错误预防 | 3 | ↑ 危险按钮恒右+间距+数量确认 |
| 6 | 识别而非回忆 | 3.5 | ↑ 全站页头补全位置感 |
| 7 | 灵活效率 | 3 | = |
| 8 | 美学极简 | 3.5 | ↑ emoji 清零、stat 卡入卡片体系 |
| 9 | 错误恢复 | 3 | = |
| 10 | 帮助文档 | 2 | = 未涉及 |
| **合计** | | **30/36** | **Good 档下缘（基线 25/36 Acceptable，+5）** |

## 修复核验

10 项修复 9 项完整落地、1 项部分（ResourcesPage cable_label span 排序尾巴，已在本轮收口）。核验无问题项：页头×100vh 画布、分页贴底数学（border-box）、徽标计时器 unmount 清理、焦点陷阱桌面无副作用、NavIcon 新容器尺寸。

## 新引入问题（已在 v0.73.1 修复）

- P1 移动端页头被 fixed 汉堡按钮遮挡 → ≤768px .page-header padding-left:3.25rem
- P2 ResourcesPage.vue:1075 span role="button" 排序尾巴 → 语义上移 th 本体，打印按钮 @click.stop 防冒泡
- P3 cards.css:2 注释漂移 → 改指 DashboardPage.vue scoped
- P3 「已保存」徽标引发工具栏按钮瞬时位移（约一个徽标宽）→ 评估后暂不处理（2s 瞬时、纯装饰性，修复复杂度大于收益）

## 确定性扫描

6 条 side-tab 发现与基线持平（4 真实 + 2 边界误报，前次已判定），无新增无消除。

## 浏览器证据

admin 登录成功，仪表盘/可视化/资源/系统 4 页截图（/tmp/trae/screenshots/）。可见证实：每页顶部出现页级标题栏；仪表盘统计卡为白底+线稿图标；可视化工具栏仅 自动绘制/删除布局（保存布局按钮已撤下）。overlay 注入沿用前次结论（:8400 脚本在页面上下文加载失败），以 CLI + 截图为证据。

## 总评

工程质量高于典型修补批次：计时器清理、flush:post 时序、贴底数学验证、toast/flash 双轨语义划分均符合项目纪律。增益集中在状态可见、错误预防、美学极简。剩余风险集中在响应式交叉区与一致性长尾；建议下一批以「响应式回归专检」为主题。遗留大项：筛选三轨收敛、表格 td 对齐体系（均需逐页产品决策+浏览器回归的独立批次）。

## 启发性问题

1. 移动端页头是保留（现已避让）还是直接隐藏、由侧栏导航承担定位？
2. 三个画布页签的保存模型能否完全统一（机柜补自动保存，撤最后一个手动按钮）？
3. side-tab 6 条旧发现（4 真 2 误报）是否值得顺手改掉以清零检测器？
