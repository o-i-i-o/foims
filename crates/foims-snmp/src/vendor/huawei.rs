//! 华为识别规则与私有采集 OID 集（自 live/profile.rs 迁入，数值不变）。
//!
//! OID 来源：外部 MIB 资料池 HUAWEI-ENTITY-EXTENT-MIB（IANA 企业号 2011）。
//! 注意 hwEntityMemSizeMega(.19，MB) 与 hwEntityMemSize(.9，字节) 并存，
//! 探测期优先 MB 口径（大内存设备字节列会 32 位回绕）。

use async_snmp::{Oid, oid};

use super::{VendorMemOids, VendorOidSet, VendorRule};

// HUAWEI-ENTITY-EXTENT-MIB hwEntityStateTable 各列
fn cpu() -> Oid {
    oid!(1, 3, 6, 1, 4, 1, 2011, 5, 25, 31, 1, 1, 1, 1, 5)
}
fn mem_usage() -> Oid {
    oid!(1, 3, 6, 1, 4, 1, 2011, 5, 25, 31, 1, 1, 1, 1, 7)
}
fn mem_size() -> Oid {
    oid!(1, 3, 6, 1, 4, 1, 2011, 5, 25, 31, 1, 1, 1, 1, 9)
}
fn mem_size_mega() -> Oid {
    oid!(1, 3, 6, 1, 4, 1, 2011, 5, 25, 31, 1, 1, 1, 1, 19)
}
fn temp() -> Oid {
    oid!(1, 3, 6, 1, 4, 1, 2011, 5, 25, 31, 1, 1, 1, 1, 11)
}

pub(super) static RULE: VendorRule = VendorRule {
    id: "huawei",
    display_name: "Huawei",
    aliases: &["huawei"],
    // IANA enterprise-numbers：2011 = Huawei Technologies
    sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 2011]],
    // "versatile routing platform" 为华为 VRP 系统 sysDescr 多词关键词
    descr_keywords: &["huawei", "versatile routing platform"],
    descr_keywords_except: &[],
    descr_regexes: &[],
    probe_checks: &[],
};

pub(super) static OID_SET: VendorOidSet = VendorOidSet {
    cpu_columns: &[cpu],
    mem: VendorMemOids::EntityPercent {
        usage: mem_usage,
        size_mega: Some((mem_size_mega, 1_048_576)),
        size_bytes: Some(mem_size),
    },
    temp_columns: &[temp],
};
