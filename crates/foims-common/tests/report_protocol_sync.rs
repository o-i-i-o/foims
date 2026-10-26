//! 双副本漂移校验：`foims_common::report` 与 agent 工作空间
//! `foims-protocol/src/report.rs` 必须逐字同源（独立 workspace 无法共享
//! path 依赖，属已声明的受控双副本，见主仓 report.rs 模块头）。
//!
//! 同步义务此前仅靠注释约定，本测试将其固化：比对两侧代码行
//! （去注释、去空行），不一致即失败；主仓 `cargo test` 与 CI 自动拦截漂移。

// 测试代码允许 unwrap/expect（AGENTS.md「测试代码除外」），与各 crate
// lib.rs 同款口径；生产代码仍由 workspace lints + CI `-D warnings` 强制禁止。
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::path::PathBuf;

/// 取代码行：剔除注释与空行（副本间允许注释差异，禁止代码差异）
fn code_lines(path: &PathBuf) -> Vec<String> {
    let text =
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("读取双副本失败 {path:?}: {e}"));
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with("//"))
        .map(String::from)
        .collect()
}

#[test]
fn report_protocol_copy_in_sync() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let main = manifest.join("src/report.rs");
    let agent = manifest.join("../../foims-agent/crates/foims-protocol/src/report.rs");
    assert_eq!(
        code_lines(&main),
        code_lines(&agent),
        "report.rs 双副本已漂移：请同步修改主仓与 foims-protocol 副本（见模块头同步义务声明）"
    );
}
