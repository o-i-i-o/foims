//! Cisco 识别规则与私有采集 OID 集（自 live/profile.rs 迁入，数值不变）。
//!
//! OID 来源：外部 MIB 资料池 cisco_v2/ 下 CISCO-PROCESS-MIB.my /
//! CISCO-MEMORY-POOL-MIB / CISCO-ENVMON-MIB（IANA 企业号 9）。

use async_snmp::{Oid, oid};

use super::{VendorMemOids, VendorOidSet, VendorRule};

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

pub(super) static RULE: VendorRule = VendorRule {
    id: "cisco",
    display_name: "Cisco",
    aliases: &["cisco"],
    // IANA enterprise-numbers：9 = Cisco Systems
    sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 9]],
    descr_keywords: &["cisco"],
    descr_keywords_except: &[],
    descr_regexes: &[],
    probe_checks: &[],
};

pub(super) static OID_SET: VendorOidSet = VendorOidSet {
    cpu_columns: &[cpm_cpu_1min, cpm_cpu_5min],
    mem: VendorMemOids::Pool {
        name: mem_pool_name,
        used: mem_pool_used,
        free: mem_pool_free,
    },
    temp_columns: &[envmon_temp],
};
