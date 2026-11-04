//! 厂商私有采集 OID 库（绑定后直连的定向 GET 目标，§6.2）：CPU/内存/温度
//! 三族，端口走公共 IF-MIB。识别规则见 mod.rs BUILTIN 与 community.rs——
//! 识别（OS 级 LibreNMS 模型）与采集库（品牌级）分层：社区规则经
//! vendor_kind() 归属到本表三家采集库。
//!
//! OID 来源：外部 MIB 资料池（不进仓库，仅开发期查证）；每条常量上方
//! 注释标注 MIB 文件名与 IANA 企业号。运行时零 MIB 依赖。

use async_snmp::{Oid, oid};

use super::{VendorMemOids, VendorOidSet};

// —— Cisco（IANA 企业号 9）——

// CISCO-PROCESS-MIB cpmCPUTotalTable：1min/5min 百分比列
fn cpm_cpu_1min() -> Oid {
    oid!(1, 3, 6, 1, 4, 1, 9, 9, 109, 1, 1, 1, 1, 7)
}
fn cpm_cpu_5min() -> Oid {
    oid!(1, 3, 6, 1, 4, 1, 9, 9, 109, 1, 1, 1, 1, 8)
}

// CISCO-MEMORY-POOL-MIB ciscoMemoryPoolTable：name/used/free 列
fn mem_pool_name() -> Oid {
    oid!(1, 3, 6, 1, 4, 1, 9, 9, 48, 1, 1, 1, 2)
}
fn mem_pool_used() -> Oid {
    oid!(1, 3, 6, 1, 4, 1, 9, 9, 48, 1, 1, 1, 5)
}
fn mem_pool_free() -> Oid {
    oid!(1, 3, 6, 1, 4, 1, 9, 9, 48, 1, 1, 1, 6)
}

// CISCO-ENVMON-MIB ciscoEnvMonTemperatureStatusValue（摄氏度）
fn envmon_temp() -> Oid {
    oid!(1, 3, 6, 1, 4, 1, 9, 9, 13, 1, 3, 1, 3)
}

pub(super) static CISCO: VendorOidSet = VendorOidSet {
    cpu_columns: &[cpm_cpu_1min, cpm_cpu_5min],
    mem: VendorMemOids::Pool {
        name: mem_pool_name,
        used: mem_pool_used,
        free: mem_pool_free,
    },
    temp_columns: &[envmon_temp],
};

// —— Huawei（IANA 企业号 2011）——

// HUAWEI-ENTITY-EXTENT-MIB hwEntityStateTable 各列。
// 注意 hwEntityMemSizeMega(.19，MB) 与 hwEntityMemSize(.9，字节) 并存，
// 探测期优先 MB 口径（大内存设备字节列会 32 位回绕）
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

pub(super) static HUAWEI: VendorOidSet = VendorOidSet {
    cpu_columns: &[cpu],
    mem: VendorMemOids::EntityPercent {
        usage: mem_usage,
        size_mega: Some((mem_size_mega, 1_048_576)),
        size_bytes: Some(mem_size),
    },
    temp_columns: &[temp],
};

// —— H3C（IANA 企业号 25506，原 3Com 归并）——

// HH3C-ENTITY-EXT-MIB hh3cEntityExtTable 各列。
// 注意部分 Comware 7 设备沿用惠普企业号 11，企业号 11 与描述词冲突时
// 描述词胜出（§6.1 冲突矩阵）
fn h3c_cpu() -> Oid {
    oid!(1, 3, 6, 1, 4, 1, 25506, 2, 6, 1, 1, 1, 1, 6)
}
fn h3c_mem_usage() -> Oid {
    oid!(1, 3, 6, 1, 4, 1, 25506, 2, 6, 1, 1, 1, 1, 8)
}
fn h3c_mem_size() -> Oid {
    oid!(1, 3, 6, 1, 4, 1, 25506, 2, 6, 1, 1, 1, 1, 10)
}
fn h3c_temp() -> Oid {
    oid!(1, 3, 6, 1, 4, 1, 25506, 2, 6, 1, 1, 1, 1, 12)
}

pub(super) static H3C: VendorOidSet = VendorOidSet {
    cpu_columns: &[h3c_cpu],
    mem: VendorMemOids::EntityPercent {
        usage: h3c_mem_usage,
        size_mega: None,
        size_bytes: Some(h3c_mem_size),
    },
    temp_columns: &[h3c_temp],
};
