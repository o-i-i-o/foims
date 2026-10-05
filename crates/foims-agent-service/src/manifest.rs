//! 构建期产物清单（manifest.json）解析与版本门控。
//!
//! manifest.json 由 `scripts/build-agent.sh` 在 CI 打包时生成，位于
//! `[agent].dist_dir`（默认 /opt/foims/agents），是运行期分发的唯一事实来源。
//! Schema（固定）：
//!
//! ```json
//! { "version": "0.21.15", "generated_at": "2026-10-05T12:00:00Z",
//!   "targets": { "x86_64-unknown-linux-musl": { "sha256": "<hex>", "size": 5432100 } } }
//! ```
//!
//! 版本门控（设计 §6.2）：agent 产物版本不得低于运行中的主程序版本，
//! 防止「升级主程序后产物目录残留旧版本」错配。

use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// manifest.json 顶层结构。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentManifest {
    /// 产物版本（CI 打包时的主程序版本）
    pub version: String,
    /// 生成时间（ISO 8601，可能缺省）
    #[serde(default)]
    pub generated_at: String,
    /// 各 rust target 的产物条目（target triple → sha256/size）
    pub targets: BTreeMap<String, AgentTargetEntry>,
}

/// 单个 target 的产物条目。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTargetEntry {
    /// 二进制 sha256（小写 hex，64 位）
    pub sha256: String,
    /// 二进制字节数
    pub size: u64,
}

/// 读取并解析 `{dist_dir}/manifest.json`；targets 为空视为无效清单。
pub fn load(dist_dir: &Path) -> Result<AgentManifest, String> {
    let path = dist_dir.join("manifest.json");
    let text = std::fs::read_to_string(&path)
        .map_err(|e| format!("读取清单失败 {}: {e}", path.display()))?;
    let manifest: AgentManifest =
        serde_json::from_str(&text).map_err(|e| format!("解析清单失败 {}: {e}", path.display()))?;
    if manifest.targets.is_empty() {
        return Err(format!(
            "清单 {} 的 targets 为空，产物目录不完整",
            path.display()
        ));
    }
    Ok(manifest)
}

/// 校验并读取指定 target 的 agent 二进制：流式读取 `{dist_dir}/{target}/foims-agent`，
/// 逐块计算 sha256 并累计大小，与清单记录一致才返回内容（约 5MB，内存可行）。
pub fn verify_binary(
    dist_dir: &Path,
    manifest: &AgentManifest,
    target: &str,
) -> Result<Vec<u8>, String> {
    let entry = manifest.targets.get(target).ok_or_else(|| {
        format!("清单中不存在目标平台 {target}，可用：{}", {
            let mut names: Vec<&str> = manifest.targets.keys().map(String::as_str).collect();
            names.sort_unstable();
            names.join(", ")
        })
    })?;

    let path = dist_dir.join(target).join("foims-agent");
    let mut file =
        std::fs::File::open(&path).map_err(|e| format!("打开产物失败 {}: {e}", path.display()))?;

    let mut hasher = Sha256::new();
    let mut content = Vec::with_capacity(entry.size as usize);
    let mut chunk = [0u8; 64 * 1024];
    let mut total: u64 = 0;
    loop {
        let n = file
            .read(&mut chunk)
            .map_err(|e| format!("读取产物失败 {}: {e}", path.display()))?;
        if n == 0 {
            break;
        }
        hasher.update(&chunk[..n]);
        content.extend_from_slice(&chunk[..n]);
        total = total.saturating_add(n as u64);
    }

    let actual: String = hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    if !actual.eq_ignore_ascii_case(&entry.sha256) {
        return Err(format!(
            "产物校验失败 {path:?}: sha256 不匹配（实际 {actual}，清单 {}）",
            entry.sha256
        ));
    }
    if total != entry.size {
        return Err(format!(
            "产物校验失败 {path:?}: 大小不匹配（实际 {total} 字节，清单 {} 字节）",
            entry.size
        ));
    }
    Ok(content)
}

/// 版本门控（设计 §6.2）：agent 产物版本不得低于主程序版本。
///
/// 两者均可按 semver 解析时做 `>=` 比较（`>` 允许现场先行更新产物的运维操作）；
/// 任一解析失败时退化为要求完全相等，避免错放不可比的版本。
pub fn gate_check(manifest_version: &str, server_version: &str) -> Result<(), String> {
    match (
        semver::Version::parse(manifest_version),
        semver::Version::parse(server_version),
    ) {
        (Ok(manifest), Ok(server)) => {
            if manifest < server {
                Err(format!(
                    "Agent 产物版本 {manifest_version} 低于主程序版本 {server_version}，\
                     请检查 agent 产物目录或重新部署分发产物"
                ))
            } else {
                Ok(())
            }
        }
        _ => {
            if manifest_version == server_version {
                Ok(())
            } else {
                Err(format!(
                    "Agent 产物版本 {manifest_version} 与主程序版本 {server_version} 不一致 \
                     （版本号无法按语义化版本比较）"
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    /// 唯一临时目录（时间戳 + 计数器），结束由调用方清理
    fn temp_dir(tag: &str) -> PathBuf {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("foims-agent-service-manifest-{tag}-{}-{n}", ts));
        std::fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("创建临时目录失败: {e}"));
        dir
    }

    /// 用已知内容计算 sha256 hex
    fn sha256_hex(data: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data);
        hasher
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    /// 在 dist_dir 下写入 manifest.json 与指定 target 的二进制
    fn setup_dist(
        dist_dir: &Path,
        manifest_version: &str,
        target: &str,
        binary: &[u8],
    ) -> AgentManifest {
        let targets = format!(
            r#"{{ "{target}": {{ "sha256": "{}", "size": {} }} }}"#,
            sha256_hex(binary),
            binary.len()
        );
        let manifest_text = format!(
            r#"{{ "version": "{manifest_version}", "generated_at": "2026-10-05T12:00:00Z", "targets": {targets} }}"#
        );
        std::fs::write(dist_dir.join("manifest.json"), manifest_text)
            .unwrap_or_else(|e| panic!("写入 manifest 失败: {e}"));
        let target_dir = dist_dir.join(target);
        std::fs::create_dir_all(&target_dir)
            .unwrap_or_else(|e| panic!("创建 target 目录失败: {e}"));
        std::fs::write(target_dir.join("foims-agent"), binary)
            .unwrap_or_else(|e| panic!("写入二进制失败: {e}"));
        load(dist_dir).unwrap_or_else(|e| panic!("加载清单失败: {e}"))
    }

    #[test]
    fn load_解析合法清单() {
        let dir = temp_dir("ok");
        let manifest = setup_dist(&dir, "0.21.15", "x86_64-unknown-linux-musl", b"binary");
        assert_eq!(manifest.version, "0.21.15");
        assert_eq!(manifest.generated_at, "2026-10-05T12:00:00Z");
        assert_eq!(manifest.targets.len(), 1);
        std::fs::remove_dir_all(&dir).unwrap_or_else(|e| panic!("清理临时目录失败: {e}"));
    }

    #[test]
    fn load_targets为空时报错() {
        let dir = temp_dir("empty");
        std::fs::write(
            dir.join("manifest.json"),
            r#"{ "version": "0.21.15", "targets": {} }"#,
        )
        .unwrap_or_else(|e| panic!("写入 manifest 失败: {e}"));
        assert!(load(&dir).is_err(), "空 targets 应视为无效清单");
        std::fs::remove_dir_all(&dir).unwrap_or_else(|e| panic!("清理临时目录失败: {e}"));
    }

    #[test]
    fn load_清单缺失时报错() {
        let dir = temp_dir("missing");
        assert!(load(&dir).is_err(), "manifest.json 缺失应报错");
        std::fs::remove_dir_all(&dir).unwrap_or_else(|e| panic!("清理临时目录失败: {e}"));
    }

    #[test]
    fn verify_binary_通过校验并返回内容() {
        let dir = temp_dir("verify-ok");
        let binary = b"foims-agent-binary-content";
        let manifest = setup_dist(&dir, "0.21.15", "aarch64-unknown-linux-musl", binary);
        let got = verify_binary(&dir, &manifest, "aarch64-unknown-linux-musl")
            .unwrap_or_else(|e| panic!("校验应通过: {e}"));
        assert_eq!(got, binary);
        std::fs::remove_dir_all(&dir).unwrap_or_else(|e| panic!("清理临时目录失败: {e}"));
    }

    #[test]
    fn verify_binary_sha256不匹配时报错() {
        let dir = temp_dir("sha-mismatch");
        let mut manifest = setup_dist(&dir, "0.21.15", "x86_64-unknown-linux-musl", b"binary");
        // 篡改清单中的 sha256
        let entry = manifest
            .targets
            .get_mut("x86_64-unknown-linux-musl")
            .unwrap_or_else(|| panic!("条目应存在"));
        entry.sha256 = "0".repeat(64);
        assert!(verify_binary(&dir, &manifest, "x86_64-unknown-linux-musl").is_err());
        std::fs::remove_dir_all(&dir).unwrap_or_else(|e| panic!("清理临时目录失败: {e}"));
    }

    #[test]
    fn verify_binary_大小不匹配时报错() {
        let dir = temp_dir("size-mismatch");
        let mut manifest = setup_dist(&dir, "0.21.15", "x86_64-unknown-linux-musl", b"binary");
        let entry = manifest
            .targets
            .get_mut("x86_64-unknown-linux-musl")
            .unwrap_or_else(|| panic!("条目应存在"));
        entry.size += 1;
        assert!(verify_binary(&dir, &manifest, "x86_64-unknown-linux-musl").is_err());
        std::fs::remove_dir_all(&dir).unwrap_or_else(|e| panic!("清理临时目录失败: {e}"));
    }

    #[test]
    fn verify_binary_目标不存在与文件缺失时报错() {
        let dir = temp_dir("no-target");
        let manifest = setup_dist(&dir, "0.21.15", "x86_64-unknown-linux-musl", b"binary");
        // 清单中不存在的 target
        assert!(verify_binary(&dir, &manifest, "i686-unknown-linux-musl").is_err());
        // 清单中存在但文件缺失
        std::fs::remove_file(dir.join("x86_64-unknown-linux-musl").join("foims-agent"))
            .unwrap_or_else(|e| panic!("删除产物失败: {e}"));
        assert!(verify_binary(&dir, &manifest, "x86_64-unknown-linux-musl").is_err());
        std::fs::remove_dir_all(&dir).unwrap_or_else(|e| panic!("清理临时目录失败: {e}"));
    }

    #[test]
    fn gate_产物低于主程序时拒绝() {
        assert!(gate_check("0.21.13", "0.21.14").is_err());
    }

    #[test]
    fn gate_相等时放行() {
        assert!(gate_check("0.21.14", "0.21.14").is_ok());
    }

    #[test]
    fn gate_产物高于主程序时放行() {
        assert!(gate_check("0.21.15", "0.21.14").is_ok());
    }

    #[test]
    fn gate_无法解析时要求完全相等() {
        // 完全相等 → 放行
        assert!(gate_check("dev", "dev").is_ok());
        // 不相等且无法比较 → 拒绝
        assert!(gate_check("dev", "0.21.14").is_err());
    }
}
