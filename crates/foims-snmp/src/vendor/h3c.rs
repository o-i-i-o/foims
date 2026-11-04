//! 新华三（H3C）识别规则与私有采集 OID 集（自 live/profile.rs 迁入，数值不变）。
//!
//! OID 来源：外部 MIB 资料池 HH3C-ENTITY-EXT-MIB（IANA 企业号 25506）。
//! 注意部分 Comware 7 设备沿用惠普企业号 11，企业号 11 与描述词冲突时
//! 描述词胜出（§6.1 冲突矩阵）。

use async_snmp::{Oid, oid};

use super::{VendorMemOids, VendorOidSet, VendorRule};

// HH3C-ENTITY-EXT-MIB hh3cEntityExtTable 各列
fn cpu() -> Oid {
    oid!(1, 3, 6, 1, 4, 1, 25506, 2, 6, 1, 1, 1, 1, 6)
}
fn mem_usage() -> Oid {
    oid!(1, 3, 6, 1, 4, 1, 25506, 2, 6, 1, 1, 1, 1, 8)
}
fn mem_size() -> Oid {
    oid!(1, 3, 6, 1, 4, 1, 25506, 2, 6, 1, 1, 1, 1, 10)
}
fn temp() -> Oid {
    oid!(1, 3, 6, 1, 4, 1, 25506, 2, 6, 1, 1, 1, 1, 12)
}

pub(super) static RULE: VendorRule = VendorRule {
    id: "h3c",
    display_name: "H3C",
    aliases: &["h3c", "3com"],
    // IANA enterprise-numbers：25506 = H3C（原 3Com 归并）
    sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 25506]],
    descr_keywords: &["h3c", "comware", "3com"],
    descr_keywords_except: &[],
};

pub(super) static OID_SET: VendorOidSet = VendorOidSet {
    cpu_columns: &[cpu],
    mem: VendorMemOids::EntityPercent {
        usage: mem_usage,
        size_mega: None,
        size_bytes: Some(mem_size),
    },
    temp_columns: &[temp],
};
