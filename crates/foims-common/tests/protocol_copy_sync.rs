//! 双副本漂移校验：主仓 `foims-common` 与 agent 工作空间 `foims-protocol`
//! 之间的受控双副本必须逐字同源（agent 拆为独立工作空间后无法共享 path
//! 依赖，属各副本文件模块头「双副本同步义务」所声明的受控复制）。
//!
//! 同步义务此前仅靠注释约定，本测试将其固化：对受控副本逐一比对代码行
//! （去注释、去空行，允许注释差异、禁止代码差异），不一致即失败；主仓
//! `cargo test` 与 CI 自动拦截漂移。新增受控副本时在 `CONTROLLED_COPIES`
//! 登记一对路径即可复用同一校验。

// 测试代码允许 unwrap/expect（AGENTS.md「测试代码除外」），与各 crate
// lib.rs 同款口径；生产代码仍由 workspace lints + CI `-D warnings` 强制禁止。
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::path::Path;
use std::path::PathBuf;

/// 受控双副本清单：`(主仓相对 foims-common 的路径, agent 侧相对 foims-agent 的路径)`
const CONTROLLED_COPIES: &[(&str, &str)] = &[
    ("src/report.rs", "crates/foims-protocol/src/report.rs"),
    ("src/x509.rs", "crates/foims-protocol/src/x509.rs"),
];

/// 取代码行：剔除注释与空行（副本间允许注释差异，禁止代码差异）
fn code_lines(path: &Path) -> Vec<String> {
    let text =
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("读取双副本失败 {path:?}: {e}"));
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with("//"))
        .map(String::from)
        .collect()
}

/// 逐一断言受控双副本代码行同源
#[test]
fn controlled_copies_in_sync() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let agent_root = manifest.join("../../foims-agent");
    for (main_rel, agent_rel) in CONTROLLED_COPIES {
        let main = manifest.join(main_rel);
        let agent = agent_root.join(agent_rel);
        assert_eq!(
            code_lines(&main),
            code_lines(&agent),
            "双副本已漂移：请同步修改主仓 {main_rel} 与 foims-protocol 副本 {agent_rel}（见模块头同步义务声明）"
        );
    }
}
