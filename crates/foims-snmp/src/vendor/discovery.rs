//! LibreNMS os_discovery 采集规则（CPU/内存/温度）。
//!
//! 数据由 `da-scripts/gen_vendor_discovery.py` 生成到
//! [`super::os_discovery`]，本文件只放运行时结构与查表入口。与识别域
//!（community/）分层：识别得到 OS id 后，用该 id 查本表；查不到或某族
//! 规则探测为空时，回退到品牌级 [`super::VendorOidSet`] 与公共 MIB 链。
//!
//! 运行时仍然零 MIB 依赖：生成期已把 `num_oid` / `snmptranslate` 结果
//! 固化为数字 OID 弧，采集只做数字 OID 的定向 GET。

use async_snmp::Oid;

use super::os_discovery::PROFILES;

/// 单列 OID：`indexed = true` 表示表列，采集期按探测到的单弧索引 GET；
/// `false` 表示标量，`base` 已包含完整实例（生成期补 `.0`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OidColumn {
    pub base: &'static [u32],
    pub indexed: bool,
}

impl OidColumn {
    pub fn oid(&self) -> Oid {
        Oid::from_slice(self.base)
    }

    pub fn indexed_oid(&self, index: u32) -> Oid {
        self.oid().child(index)
    }
}

/// CPU 规则：整数百分比列 + LibreNMS `processor_precision` 语义。
///
/// `precision < 0` 表示上游上报的是空闲率：`usage = 100 + raw / precision`；
/// 否则 `usage = raw / precision`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuRule {
    pub column: OidColumn,
    pub precision: i32,
}

/// 内存规则。同一 OS 可有多条（主内存/交换/各板卡），按生成顺序探测，
/// 首个可用方案生效。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemRule {
    /// used + free，total = used + free。
    Pool {
        used: OidColumn,
        free: OidColumn,
        precision: u64,
    },
    /// percent_used + total：total 在探测期缓存，采集期只读百分比。
    PercentTotal {
        percent: OidColumn,
        total: OidColumn,
        precision: u64,
    },
    /// 只有 percent_used：只报使用率，总量/已用为空。
    Percent { percent: OidColumn },
    /// total + free，used = total - free。
    TotalFree {
        total: OidColumn,
        free: OidColumn,
        precision: u64,
    },
    /// total + used，free = total - used。
    UsedTotal {
        total: OidColumn,
        used: OidColumn,
        precision: u64,
    },
}

/// 温度规则：原始值 × multiplier / divisor（摄氏度）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TempRule {
    pub column: OidColumn,
    pub divisor: u64,
    pub multiplier: u64,
}

/// 单个 OS 的采集规则集（各字段为空表示该族无 LibreNMS 规则）。
#[derive(Debug, Clone, Copy)]
pub struct DiscoveryProfile {
    pub cpu: &'static [CpuRule],
    pub mem: &'static [MemRule],
    pub temp: &'static [TempRule],
}

/// OS id → 采集规则集（生成的 PROFILES 按 id 排序，二分查找）。
pub fn discovery_profile(id: Option<&str>) -> Option<&'static DiscoveryProfile> {
    let id = id?;
    PROFILES
        .binary_search_by(|(os, _)| (*os).cmp(id))
        .ok()
        .map(|idx| &PROFILES[idx].1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 查表_命中与未命中() {
        // fortigate 是 os_discovery 中稳定存在的 OS 条目
        let profile = discovery_profile(Some("fortigate")).expect("fortigate 应有采集规则");
        assert!(!profile.cpu.is_empty() || !profile.mem.is_empty() || !profile.temp.is_empty());
        assert!(discovery_profile(None).is_none());
        assert!(discovery_profile(Some("__not_an_os__")).is_none());
    }

    #[test]
    fn 标量列构造() {
        let column = OidColumn {
            base: &[1, 3, 6, 1, 4, 1, 9, 9, 1, 0],
            indexed: false,
        };
        assert_eq!(column.oid().to_string(), "1.3.6.1.4.1.9.9.1.0");
        assert_eq!(column.indexed_oid(7).to_string(), "1.3.6.1.4.1.9.9.1.0.7");
    }
}
