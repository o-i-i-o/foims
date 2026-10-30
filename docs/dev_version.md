# 本文件为开发用版本号控制规则

# 开发版本号格式为vxx.yy.zz
# z 每次修改更新无条件z+1，z>99则y+1，z归零
# y 每次为模块增加小功能、功能优化 则y+1，z归零
# x 每次增加模块（级功能）x+1，y归零，z归零
# 普通小bug修复仅执行z规则，功能bug更新执行y规则，模块级功能更新执行x规则

# 主 Cargo.toml（根 crate foims 即 lib）version = "aa.bb.cc" 关联本文件 vxx.yy.zz
# version版本号改为 "aa.bb.cc"的形式 ，aa bb cc的首个0省略 06.21.10 > 6.21.10
# 映射关系：本文件 x→Cargo.toml 的 b，本文件 y→Cargo.toml 的 c
# 联动仅以下两种触发，本文件 z 段变化（普通修改）一律不触碰 Cargo.toml：
# 1. 本文件 y 段变化（小功能/功能优化，含 z>99 进位）→ Cargo.toml 仅 c+1
# 2. 本文件 x 段变化（模块级功能）→ Cargo.toml b+1，c 归零
# a由开发者根据实际情况手动变更
# 示例：本文件 v0.0.13→v0.0.14（仅 z+1）时 Cargo.toml 保持 0.21.36 不动；本文件 v0.1.0（y+1）时 Cargo.toml 才 0.21.36→0.21.37
# 目的：z 段作为高频修改审计计数，避免智能体高频次修改推高 Cargo.toml 版本号
# crate版本各自递增，某crate版本变更主Cargo.toml的版本都要关联变更（本次修改只涉及crate a则修改crate a 和lib的版本号，crate b版本号不变）
# 前端自切换 Vue 3 + Vite 工程起由 frontend/package.json 的 version 字段承载版本号，复用本文件同一套 x/y/z 判级与联动机制、与 Cargo.toml 相互独立：仅前端修改联动 package.json（z 段不动 / y 段 c+1 / x 段 b+1 且 c 归零，a 段手动），不再 bump 根 Cargo.toml；仅后端修改照旧只联动 Cargo.toml；前后端混合修改按各自维度分别联动。版本计数自 0.1.0（Vue 3 工程确立）起独立递增；旧 bump_static_version.sh/?v=/MODULE_VERSION 机制已废弃，Vite 内容指纹（assets/ 哈希文件名）仅承担构建产物缓存命名


# v0.0.1

# 2026100712171501
# 2026年10月7日12点17分15秒 第1次修改 （01用来规避极端情况，通常不可能1秒修改几次）
# 版本采用文本累加机制，每次版本更新新增此版的版本号 时间序号 更新内容。
# 此版本机制自2026年10月7日实行

### 重要！本文件只作为版本记录，智能体阅读到此处请直接跳到页尾，禁止智能体全量引用本文档造成上下文超限 

v0.43.0
2026103015120501
将开发版本记录文档归档到docs/dev_version_achived.md，智能体请勿检索此文件

v0.43.1
2026103015361201
注释审计阶段五（auth/init/data-management/x509/scheduler/visualization/services）：auth 35 个、init 11 个、visualization 13 个、data-management 6 个及主 crate 证书/服务 11 个 handler 统一为路径优先文档；删除复述式注释，保留 why/安全/并发说明；修正 scheduler、topology、test-connection 等不准确注释。仅注释改动，未改代码/错误码/日志；cargo fmt、workspace clippy -D warnings、受影响 7 crate 测试（279 项）全部通过；仅 dev_version z+1，Cargo.toml/package.json 不联动

v0.43.2
2026103016273701
注释审计阶段六（前端纯 TS，41 文件，排除 ts-rs 生成物）：删除 6 文件 10 条名称直译注释（useToast/usePagedList/useSubTabs/pagedFetch/types.visualization/viewModel），其余注释为契约/单位/浏览器坑等 why 说明，保留；未动并行在途的 3 个 SFC 与 i18n JSON。验证：仅注释守卫为空、npm run typecheck、npm run build、biome check、oxlint 均通过；仅 dev_version z+1，frontend/package.json 与 Cargo.toml 不联动
