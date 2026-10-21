#!/usr/bin/env bash
# 生成前端 TS 类型：从 Rust API 载荷类型（#[derive(TS)]）导出到
# frontend/src/types/api/generated/，生成物提交入库，CI 做漂移校验。
# 导出目录与大整数映射由 .cargo/config.toml 的 [env] 配置
# （TS_RS_EXPORT_DIR / TS_RS_LARGE_INT）。
set -euo pipefail
cd "$(dirname "$0")/.."
cargo test --workspace export_bindings
