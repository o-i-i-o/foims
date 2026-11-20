//! LibreNMS os_discovery 采集规则（CPU/内存/温度）。
//!
//! 由 `da-scripts/gen_vendor_discovery.py` 生成，**勿手改**。
//! 运行时零 MIB 依赖：所有 OID 已固化为数字弧。

use super::discovery::{CpuRule, DiscoveryProfile, MemRule, OidColumn, TempRule};

/// 按 OS id 升序排列（运行时二分查找）。
#[rustfmt::skip]
pub(super) static PROFILES: &[(&str, DiscoveryProfile)] = &[
    (
        "3com",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 43, 45, 1, 6, 1, 1, 1, 3], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::TotalFree { total: OidColumn { base: &[1, 3, 6, 1, 4, 1, 43, 45, 1, 6, 1, 2, 1, 1, 2, 0], indexed: false }, free: OidColumn { base: &[1, 3, 6, 1, 4, 1, 43, 45, 1, 6, 1, 2, 1, 1, 3, 0], indexed: false }, precision: 1 },
            ],
            temp: &[
            ],
        },
    ),
    (
        "acos",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 22610, 2, 4, 1, 3, 2, 1, 3], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::UsedTotal { total: OidColumn { base: &[1, 3, 6, 1, 4, 1, 22610, 2, 4, 1, 2, 1, 0], indexed: false }, used: OidColumn { base: &[1, 3, 6, 1, 4, 1, 22610, 2, 4, 1, 2, 2, 0], indexed: false }, precision: 1024 },
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 22610, 2, 4, 1, 5, 1], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "adtran-aos",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 664, 5, 53, 1, 4, 1], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::TotalFree { total: OidColumn { base: &[1, 3, 6, 1, 4, 1, 664, 5, 53, 1, 4, 6, 0], indexed: false }, free: OidColumn { base: &[1, 3, 6, 1, 4, 1, 664, 5, 53, 1, 4, 8, 0], indexed: false }, precision: 1 },
            ],
            temp: &[
            ],
        },
    ),
    (
        "adva-alm",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2544, 1, 14, 2, 1, 11], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2544, 1, 14, 2, 1, 13], indexed: true }, divisor: 10, multiplier: 1 },
            ],
        },
    ),
    (
        "adva-aos",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2544, 1, 20, 1, 5, 2, 2, 1, 2], indexed: true }, divisor: 10, multiplier: 1 },
            ],
        },
    ),
    (
        "adva_xg300",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 629, 1, 50, 11, 1, 13, 1], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 629, 1, 50, 11, 1, 13, 2], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 6926, 1, 18, 1, 1, 3, 1, 3], indexed: true }, divisor: 10, multiplier: 1 },
            ],
        },
    ),
    (
        "advantech",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 10297, 202, 7000, 2, 5, 2, 1, 2], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 10297, 202, 7000, 7, 3, 3, 1, 5], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "aen",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 22420, 1, 1, 20], indexed: true }, precision: 1 },
            ],
            mem: &[
            ],
            temp: &[
            ],
        },
    ),
    (
        "airos",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 41112, 1, 4, 8, 3], indexed: true }, precision: 100 },
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 41112, 1, 4, 8, 4], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 41112, 1, 4, 8, 4], indexed: true }, divisor: 10000000, multiplier: 1 },
            ],
        },
    ),
    (
        "airos-af",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 41112, 1, 4, 8, 3], indexed: true }, precision: 100 },
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 41112, 1, 3, 2, 1, 8], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 41112, 1, 3, 2, 1, 10], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "airos-af-ltu",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 41112, 1, 10, 1, 3, 6], indexed: true }, precision: 1 },
            ],
            mem: &[
            ],
            temp: &[
            ],
        },
    ),
    (
        "airos-af60",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 41112, 1, 11, 1, 2, 6], indexed: true }, precision: 1 },
            ],
            mem: &[
            ],
            temp: &[
            ],
        },
    ),
    (
        "akcp",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 3854, 3, 5, 2, 1, 20], indexed: true }, divisor: 10, multiplier: 1 },
            ],
        },
    ),
    (
        "alaxala",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 21839, 2, 2, 7, 2, 1, 3, 1, 3], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 21839, 2, 2, 11, 2, 1, 3, 1, 3], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 21839, 2, 2, 15, 2, 1, 3, 1, 3], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 21839, 2, 2, 17, 2, 1, 3, 1, 3], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 21839, 2, 2, 6, 2, 1, 3, 1, 3], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 21839, 2, 2, 10, 2, 1, 3, 1, 3], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 21839, 2, 2, 8, 2, 2, 2, 1, 3], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 21839, 2, 2, 9, 2, 2, 2, 1, 3], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 21839, 2, 2, 12, 2, 2, 2, 1, 3], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 21839, 2, 2, 13, 2, 1, 3, 1, 3], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 21839, 2, 2, 14, 2, 1, 3, 1, 3], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 21839, 2, 2, 18, 2, 1, 3, 1, 3], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 21839, 2, 2, 25, 2, 1, 3, 1, 3], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 21839, 2, 2, 23, 2, 1, 3, 1, 3], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 21839, 2, 2, 16, 2, 1, 3, 1, 3], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 21839, 2, 2, 24, 2, 1, 3, 1, 3], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 21839, 2, 2, 20, 2, 1, 3, 1, 3], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 21839, 2, 2, 2, 2, 2, 3, 1, 3], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 21839, 2, 2, 3, 2, 2, 3, 1, 3], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 21839, 2, 1, 2, 2, 2, 3, 1, 3], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 21839, 2, 1, 5, 2, 2, 3, 1, 3], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 21839, 2, 1, 4, 2, 2, 3, 1, 3], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "albentiaaos",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28087, 12, 10, 10, 1, 1, 5], indexed: true }, divisor: 100, multiplier: 1 },
            ],
        },
    ),
    (
        "alcoma-almp",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 12140, 2, 3, 9], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "algcom-dc-ups",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 49136, 1, 4, 1], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 49136, 1, 4, 2], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 49136, 1, 4, 3], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "alpha-power",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 5591, 1, 4, 6, 1, 3], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "alpineoe-tdcmedfa",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 52326, 2, 1, 1, 1, 8], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 52326, 1, 1, 1, 1, 8], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "alteonos",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 1872, 2, 5, 1, 2, 2, 1], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::TotalFree { total: OidColumn { base: &[1, 3, 6, 1, 4, 1, 1872, 2, 5, 1, 2, 8, 1, 0], indexed: false }, free: OidColumn { base: &[1, 3, 6, 1, 4, 1, 1872, 2, 5, 1, 2, 8, 3, 0], indexed: false }, precision: 1 },
            ],
            temp: &[
            ],
        },
    ),
    (
        "aos",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 89, 1, 8], indexed: true }, precision: 1 },
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 6486, 800, 1, 2, 1, 16, 1, 1, 1, 13], indexed: true }, precision: 1 },
            ],
            mem: &[
            ],
            temp: &[
            ],
        },
    ),
    (
        "aos-emu2",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 318, 1, 1, 10, 3, 13, 1, 1, 3], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "aos6",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 6486, 800, 1, 2, 1, 16, 1, 1, 1, 14], indexed: true }, precision: 1 },
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 6486, 800, 1, 2, 1, 5, 1, 1, 2, 5, 1, 1], indexed: true }, divisor: 1000, multiplier: 1 },
            ],
        },
    ),
    (
        "aos7",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 6486, 801, 1, 2, 1, 16, 1, 1, 1, 1, 1, 11], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::PercentTotal { percent: OidColumn { base: &[1, 3, 6, 1, 4, 1, 6486, 801, 1, 2, 1, 16, 1, 1, 1, 1, 1, 8, 0], indexed: false }, total: OidColumn { base: &[1, 3, 6, 1, 4, 1, 6486, 801, 1, 1, 1, 2, 1, 1, 3, 4, 0], indexed: false }, precision: 1024 },
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 6486, 801, 1, 1, 1, 3, 1, 1, 3, 1, 8], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 6486, 801, 1, 2, 1, 5, 1, 1, 2, 6, 1, 2], indexed: true }, divisor: 1000, multiplier: 1 },
            ],
        },
    ),
    (
        "apc",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 318, 1, 1, 13, 3, 3, 1, 2, 2, 3], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 318, 1, 1, 13, 3, 3, 1, 2, 2, 5], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 318, 1, 1, 13, 3, 3, 1, 2, 2, 7], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 318, 1, 1, 13, 3, 3, 1, 2, 2, 24], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 318, 1, 1, 13, 3, 3, 1, 2, 2, 26], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 318, 1, 1, 13, 3, 3, 1, 2, 2, 28], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 318, 1, 1, 13, 3, 3, 1, 2, 2, 41], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 318, 1, 1, 13, 3, 3, 1, 2, 2, 43], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 318, 1, 1, 13, 3, 3, 2, 2, 2, 5], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 318, 1, 1, 13, 3, 3, 2, 2, 2, 7], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 318, 1, 1, 13, 3, 3, 2, 2, 2, 24], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 318, 1, 1, 13, 3, 3, 2, 2, 2, 26], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 318, 1, 1, 13, 3, 3, 2, 2, 2, 28], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 318, 1, 1, 13, 4, 5, 2, 1, 7], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 318, 1, 1, 13, 4, 5, 2, 1, 9], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 318, 1, 1, 13, 4, 5, 2, 1, 11], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 318, 1, 1, 13, 4, 5, 2, 1, 13], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 318, 1, 1, 26, 10, 2, 2, 1, 8], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 318, 1, 1, 13, 1, 5, 2, 2, 1, 2], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 318, 1, 1, 13, 1, 5, 2, 2, 1, 5], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 318, 1, 1, 13, 1, 4, 5, 10], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 318, 1, 1, 13, 1, 4, 8, 3], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 318, 1, 1, 10, 5, 1, 1, 1, 5], indexed: true }, divisor: 10, multiplier: 1 },
            ],
        },
    ),
    (
        "apc-inrow",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 318, 1, 1, 13, 3, 2, 2, 2, 7], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 318, 1, 1, 13, 3, 2, 2, 2, 9], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 318, 1, 1, 13, 3, 2, 2, 2, 11], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 318, 1, 1, 13, 3, 2, 2, 2, 24], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 318, 1, 1, 13, 3, 2, 2, 2, 26], indexed: true }, divisor: 10, multiplier: 1 },
            ],
        },
    ),
    (
        "apc-mgeups",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 705, 1, 5, 7], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "apc-netbotz",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 52674, 500, 4, 1, 1, 1, 8], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "apsoluteos",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 89, 35, 1, 54], indexed: true }, precision: 1 },
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 89, 35, 1, 165, 1, 2], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "arbos",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 9694, 1, 4, 2, 1, 2], indexed: true }, precision: 100 },
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 9694, 1, 5, 2, 4], indexed: true }, precision: 100 },
            ],
            mem: &[
                MemRule::Percent { percent: OidColumn { base: &[1, 3, 6, 1, 4, 1, 9694, 1, 5, 2, 7, 0], indexed: false } },
                MemRule::Percent { percent: OidColumn { base: &[1, 3, 6, 1, 4, 1, 9694, 1, 5, 2, 8, 1], indexed: false } },
                MemRule::PercentTotal { percent: OidColumn { base: &[1, 3, 6, 1, 4, 1, 9694, 1, 4, 2, 1, 7, 0], indexed: false }, total: OidColumn { base: &[1, 3, 6, 1, 4, 1, 9694, 1, 4, 2, 1, 5, 0], indexed: false }, precision: 1024 },
                MemRule::PercentTotal { percent: OidColumn { base: &[1, 3, 6, 1, 4, 1, 9694, 1, 4, 2, 1, 10, 1], indexed: false }, total: OidColumn { base: &[1, 3, 6, 1, 4, 1, 9694, 1, 4, 2, 1, 8, 1], indexed: false }, precision: 1024 },
            ],
            temp: &[
            ],
        },
    ),
    (
        "arris-apex",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 1166, 1, 31, 3, 2, 2, 1], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 1166, 1, 31, 3, 2, 2, 2], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 1166, 1, 31, 3, 2, 2, 3], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 1166, 1, 31, 3, 2, 2, 4], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "arris-c3",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 4115, 1, 4, 3, 3, 1, 2, 4], indexed: true }, divisor: 100, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 4115, 1, 4, 3, 3, 1, 2, 5], indexed: true }, divisor: 100, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 4115, 1, 4, 3, 3, 1, 2, 6], indexed: true }, divisor: 100, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 4115, 1, 4, 3, 3, 1, 2, 7], indexed: true }, divisor: 100, multiplier: 1 },
            ],
        },
    ),
    (
        "arubaos",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 14823, 2, 2, 1, 2, 1, 13, 1, 3], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::Pool { used: OidColumn { base: &[1, 3, 6, 1, 4, 1, 14823, 2, 2, 1, 1, 1, 11, 1, 3, 0], indexed: false }, free: OidColumn { base: &[1, 3, 6, 1, 4, 1, 14823, 2, 2, 1, 1, 1, 11, 1, 4, 0], indexed: false }, precision: 1024 },
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 14823, 2, 2, 1, 2, 1, 10], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "arubaos-cx",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 47196, 4, 1, 1, 3, 11, 3, 1, 1, 7], indexed: true }, divisor: 1000, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 47196, 4, 1, 1, 3, 27, 1, 2, 1, 1, 2], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "asrockrack",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 49622, 6, 24, 5], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 49622, 6, 25, 5], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 49622, 6, 26, 5], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 49622, 6, 27, 5], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 49622, 6, 28, 5], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 49622, 6, 29, 5], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 49622, 6, 30, 5], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 49622, 6, 32, 5], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 49622, 6, 33, 5], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 49622, 6, 34, 5], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 49622, 6, 35, 5], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 49622, 6, 36, 5], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 49622, 6, 37, 5], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 49622, 6, 38, 5], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 49622, 6, 39, 5], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 49622, 6, 40, 5], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 49622, 6, 41, 5], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 49622, 6, 42, 5], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 49622, 6, 43, 5], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 49622, 6, 44, 5], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 49622, 6, 45, 5], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 49622, 6, 46, 5], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 49622, 6, 47, 5], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 49622, 6, 48, 5], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 49622, 6, 49, 5], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 49622, 6, 50, 5], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 49622, 6, 55, 5], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 49622, 6, 60, 5], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "avocent",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 10418, 17, 2, 5, 3, 1, 17], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 10418, 17, 2, 5, 3, 1, 19], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "avtech",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 20916, 1, 9, 1, 1, 1, 1], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 20916, 1, 9, 1, 1, 2, 1], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 20916, 1, 13, 1, 1, 1, 1], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 20916, 1, 6, 1, 1, 1, 2], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 20916, 1, 6, 1, 2, 1, 1], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 20916, 1, 6, 1, 2, 2, 1], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 20916, 1, 3, 1, 2, 1], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 20916, 1, 3, 1, 3, 1], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 20916, 1, 3, 1, 4, 1], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 20916, 1, 10, 1, 1, 1], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 20916, 1, 10, 1, 2, 1], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 20916, 1, 10, 1, 3, 1], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 20916, 1, 10, 1, 4, 1], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 20916, 1, 8, 1, 1, 1, 2], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 20916, 1, 8, 1, 1, 4, 2], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 20916, 1, 8, 1, 2], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 20916, 1, 8, 1, 4], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 20916, 1, 11, 1, 1, 1, 2], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 20916, 1, 11, 1, 1, 4, 2], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 20916, 1, 11, 1, 2], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 20916, 1, 7, 1, 1, 1, 1], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 20916, 1, 7, 1, 2, 1, 1], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 20916, 1, 7, 1, 2, 2, 1], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 20916, 1, 1, 1, 1, 1], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 20916, 1, 1, 1, 1, 2], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 20916, 1, 1, 1, 1, 3], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 20916, 1, 1, 1, 1, 4], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 20916, 1, 12, 1, 1, 1, 2], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 20916, 1, 12, 1, 2], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "awplus",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 207, 8, 4, 4, 3, 3, 8, 1, 4], indexed: true }, precision: 1 },
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 207, 8, 4, 4, 3, 12, 3, 1, 5], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 207, 8, 4, 4, 3, 28, 1, 1, 1, 3], indexed: true }, divisor: 1000, multiplier: 1 },
            ],
        },
    ),
    (
        "axiscam",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 368, 4, 1, 100, 3, 1], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::UsedTotal { total: OidColumn { base: &[1, 3, 6, 1, 4, 1, 368, 4, 1, 100, 1, 2, 0, 0], indexed: false }, used: OidColumn { base: &[1, 3, 6, 1, 4, 1, 368, 4, 1, 100, 1, 3, 0, 0], indexed: false }, precision: 1024 },
            ],
            temp: &[
            ],
        },
    ),
    (
        "axos",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 6321, 1, 2, 4, 2, 4, 2, 1, 4], indexed: true }, divisor: 1000, multiplier: 1 },
            ],
        },
    ),
    (
        "bats",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 37069, 1, 1, 4], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "bdcom",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 3320, 3, 6, 10, 1, 11], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::Percent { percent: OidColumn { base: &[1, 3, 6, 1, 4, 1, 11606, 10, 3, 6, 10, 1, 12, 0], indexed: false } },
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 3320, 3, 6, 10, 1, 13], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 3320, 9, 63, 1, 7, 1, 4], indexed: true }, divisor: 256, multiplier: 1 },
            ],
        },
    ),
    (
        "benuos",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 39406, 1, 5, 1, 8], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::UsedTotal { total: OidColumn { base: &[1, 3, 6, 1, 4, 1, 39406, 1, 5, 1, 2, 0], indexed: false }, used: OidColumn { base: &[1, 3, 6, 1, 4, 1, 39406, 1, 5, 1, 3, 0], indexed: false }, precision: 1024 },
            ],
            temp: &[
            ],
        },
    ),
    (
        "binos",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 738, 1, 111, 3, 1, 2, 1], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::Pool { used: OidColumn { base: &[1, 3, 6, 1, 4, 1, 738, 10, 111, 2, 1, 1, 5, 0], indexed: false }, free: OidColumn { base: &[1, 3, 6, 1, 4, 1, 738, 10, 111, 2, 1, 1, 1, 0], indexed: false }, precision: 1 },
            ],
            temp: &[
            ],
        },
    ),
    (
        "binox",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 738, 10, 111, 1, 1, 3, 1, 1], indexed: true }, precision: 1 },
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 738, 10, 111, 3, 1, 1], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::Percent { percent: OidColumn { base: &[1, 3, 6, 1, 4, 1, 738, 10, 111, 3, 1, 3, 0], indexed: false } },
            ],
            temp: &[
            ],
        },
    ),
    (
        "bintec-beip-plus",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 272, 4, 17, 4, 1, 1, 18], indexed: true }, precision: -1 },
            ],
            mem: &[
                MemRule::UsedTotal { total: OidColumn { base: &[1, 3, 6, 1, 4, 1, 272, 4, 17, 4, 2, 1, 4], indexed: true }, used: OidColumn { base: &[1, 3, 6, 1, 4, 1, 272, 4, 17, 4, 2, 1, 5], indexed: true }, precision: 1 },
            ],
            temp: &[
            ],
        },
    ),
    (
        "bke",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 27533, 5, 3, 3], indexed: true }, divisor: 10, multiplier: 1 },
            ],
        },
    ),
    (
        "bktel-oa",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 7501, 1, 2, 116, 5, 1, 1, 3], indexed: true }, divisor: 10, multiplier: 1 },
            ],
        },
    ),
    (
        "boschcpp",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 3967, 1, 1, 9, 1, 1], indexed: true }, precision: -1 },
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 3967, 1, 1, 7, 1, 1], indexed: true }, divisor: 10, multiplier: 1 },
            ],
        },
    ),
    (
        "boss",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 45, 1, 6, 3, 8, 1, 1, 6], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::TotalFree { total: OidColumn { base: &[1, 3, 6, 1, 4, 1, 45, 1, 6, 3, 8, 1, 1, 12], indexed: true }, free: OidColumn { base: &[1, 3, 6, 1, 4, 1, 45, 1, 6, 3, 8, 1, 1, 13], indexed: true }, precision: 1048576 },
            ],
            temp: &[
            ],
        },
    ),
    (
        "bti800",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 30005, 1, 7, 100, 1, 2, 1, 1, 1, 7], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 30005, 1, 7, 100, 1, 2, 6, 3, 1, 4], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "cdata",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 34592, 1, 3, 100, 1, 8, 1], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::TotalFree { total: OidColumn { base: &[1, 3, 6, 1, 4, 1, 34592, 1, 3, 100, 1, 8, 2, 0], indexed: false }, free: OidColumn { base: &[1, 3, 6, 1, 4, 1, 34592, 1, 3, 100, 1, 8, 3, 0], indexed: false }, precision: 1 },
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 34592, 1, 3, 100, 1, 8, 6], indexed: true }, divisor: 10, multiplier: 1 },
            ],
        },
    ),
    (
        "ceraos",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2281, 10, 1, 1, 9], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2281, 10, 5, 1, 1, 24], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "cetpower",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 12551, 4, 1, 1, 1, 1, 28], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "ciena-rls",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 1271, 4, 1, 1, 1, 1, 1, 1, 29], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "ciena-saos",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 1271, 2, 1, 9, 1, 1, 1, 1, 3], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "ciena-sds",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 1271, 2, 1, 5, 1, 2, 1, 4, 5, 1, 4], indexed: true }, precision: 1 },
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 6141, 2, 60, 12, 1, 11, 1], indexed: true }, precision: 1 },
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 6141, 2, 60, 12, 1, 11, 5], indexed: true }, precision: 1 },
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 6141, 2, 60, 12, 1, 11, 9], indexed: true }, precision: 1 },
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 1271, 2, 1, 5, 1, 2, 1, 3, 2, 1, 4], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 1271, 2, 1, 5, 1, 2, 1, 4, 35, 1, 4], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 1271, 2, 1, 2, 1, 2, 3, 1, 3], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 1271, 2, 1, 9, 1, 1, 1, 1, 3], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 6141, 2, 60, 4, 1, 1, 1, 1, 16], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "ciena-waveserver",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 1271, 3, 4, 15, 13, 1, 2], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "cimc",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 9, 9, 719, 1, 9, 44, 1, 4], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 9, 9, 719, 1, 9, 44, 1, 13], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 9, 9, 719, 1, 9, 44, 1, 17], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 9, 9, 719, 1, 9, 44, 1, 8], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 9, 9, 719, 1, 9, 44, 1, 21], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 9, 9, 719, 1, 30, 12, 1, 6], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 9, 9, 719, 1, 41, 2, 1, 10], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "ciscosb",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 9, 6, 1, 101, 1, 8], indexed: true }, precision: 1 },
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 9, 6, 1, 101, 83, 5, 1, 1, 2], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 9, 6, 1, 101, 53, 15, 1, 10], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "ciscowlc",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 14179, 1, 1, 5, 1], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::TotalFree { total: OidColumn { base: &[1, 3, 6, 1, 4, 1, 14179, 1, 1, 5, 2, 0], indexed: false }, free: OidColumn { base: &[1, 3, 6, 1, 4, 1, 14179, 1, 1, 5, 3, 0], indexed: false }, precision: 1024 },
            ],
            temp: &[
            ],
        },
    ),
    (
        "clickshare",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 7312, 2468, 1, 2, 3], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "cnmatrix",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2076, 81, 1, 68], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::Percent { percent: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2076, 81, 1, 73, 0], indexed: false } },
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2076, 81, 1, 66], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "cnos",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 19046, 2, 3, 11, 1, 3, 1, 6], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "compas",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 26854, 3, 2, 1, 20, 1, 20, 1, 13, 3, 71], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "comware",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 25506, 2, 6, 1, 1, 1, 1, 6], indexed: true }, precision: 1 },
            ],
            mem: &[
            ],
            temp: &[
            ],
        },
    ),
    (
        "conteg-pdu",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28402, 12, 1, 1, 2, 8, 1, 1, 1], indexed: true }, divisor: 100, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28402, 12, 1, 1, 2, 8, 1, 1, 2], indexed: true }, divisor: 100, multiplier: 1 },
            ],
        },
    ),
    (
        "controlbox",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 17095, 2, 1, 3], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "ctm",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 25868, 1, 9], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "cts",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 9304, 100, 51122, 5, 45, 1, 2], indexed: true }, precision: 1 },
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 9304, 100, 5106, 5, 45, 1, 2], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::Pool { used: OidColumn { base: &[1, 3, 6, 1, 4, 1, 9304, 100, 51122, 5, 45, 2, 6, 0], indexed: false }, free: OidColumn { base: &[1, 3, 6, 1, 4, 1, 9304, 100, 51122, 5, 45, 2, 7, 0], indexed: false }, precision: 1024 },
                MemRule::Pool { used: OidColumn { base: &[1, 3, 6, 1, 4, 1, 9304, 100, 5106, 5, 45, 2, 6, 0], indexed: false }, free: OidColumn { base: &[1, 3, 6, 1, 4, 1, 9304, 100, 5106, 5, 45, 2, 7, 0], indexed: false }, precision: 1024 },
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 9304, 100, 3110, 5, 34, 2, 1, 1, 5], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 9304, 100, 51122, 5, 34, 2, 1, 1, 5], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 9304, 100, 51122, 1, 2, 20], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 9304, 100, 5106, 5, 34, 2, 1, 1, 5], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 9304, 100, 5106, 1, 2, 20], indexed: true }, divisor: 10, multiplier: 1 },
            ],
        },
    ),
    (
        "cubo",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 27894, 11, 241, 1, 9, 5, 1, 4], indexed: true }, divisor: 10, multiplier: 1 },
            ],
        },
    ),
    (
        "cxc",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 7309, 5, 3, 1, 14], indexed: true }, divisor: 1000, multiplier: 1 },
            ],
        },
    ),
    (
        "cxrc",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 7309, 4, 1, 6, 2, 2, 1, 3], indexed: true }, divisor: 100, multiplier: 1 },
            ],
        },
    ),
    (
        "cyberoam-utm",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 21067, 2, 1, 2, 2, 1], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::PercentTotal { percent: OidColumn { base: &[1, 3, 6, 1, 4, 1, 21067, 2, 1, 2, 4, 2, 0], indexed: false }, total: OidColumn { base: &[1, 3, 6, 1, 4, 1, 21067, 2, 1, 2, 4, 1, 0], indexed: false }, precision: 1048576 },
                MemRule::PercentTotal { percent: OidColumn { base: &[1, 3, 6, 1, 4, 1, 21067, 2, 1, 2, 4, 4, 1], indexed: false }, total: OidColumn { base: &[1, 3, 6, 1, 4, 1, 21067, 2, 1, 2, 4, 3, 1], indexed: false }, precision: 1048576 },
            ],
            temp: &[
            ],
        },
    ),
    (
        "cyberpower",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 3808, 1, 1, 1, 2, 2, 3], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "dahua-nvr",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 1004849, 2, 1, 3], indexed: true }, precision: 1 },
            ],
            mem: &[
            ],
            temp: &[
            ],
        },
    ),
    (
        "dantherm-os",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 46651, 1, 1, 1], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 46651, 1, 1, 2], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 46651, 1, 1, 3], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 46651, 1, 1, 6], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 46651, 1, 1, 29], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "dasan-nos",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 6296, 9, 1, 1, 1, 8], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::Pool { used: OidColumn { base: &[1, 3, 6, 1, 4, 1, 6296, 9, 1, 1, 1, 15, 0], indexed: false }, free: OidColumn { base: &[1, 3, 6, 1, 4, 1, 6296, 9, 1, 1, 1, 16, 0], indexed: false }, precision: 1 },
            ],
            temp: &[
            ],
        },
    ),
    (
        "datacom",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 3709, 3, 5, 201, 1, 1, 10], indexed: true }, precision: 100 },
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 3709, 3, 5, 201, 1, 1, 2, 1, 12], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "datacom-dmos",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 3709, 3, 6, 4, 1, 2, 1, 4], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::Pool { used: OidColumn { base: &[1, 3, 6, 1, 4, 1, 3709, 3, 6, 4, 2, 2, 1, 27], indexed: true }, free: OidColumn { base: &[1, 3, 6, 1, 4, 1, 3709, 3, 6, 4, 2, 2, 1, 28], indexed: true }, precision: 1 },
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 3709, 3, 6, 8, 1, 1, 1, 1], indexed: true }, divisor: 100, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 3709, 3, 6, 6, 1, 3, 1, 6], indexed: true }, divisor: 10, multiplier: 1 },
            ],
        },
    ),
    (
        "dcn-software",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 6339, 100, 1, 11, 10], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::UsedTotal { total: OidColumn { base: &[1, 3, 6, 1, 4, 1, 6339, 100, 1, 11, 6, 0], indexed: false }, used: OidColumn { base: &[1, 3, 6, 1, 4, 1, 6339, 100, 1, 11, 7, 0], indexed: false }, precision: 1 },
            ],
            temp: &[
            ],
        },
    ),
    (
        "dell-compellent",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 674, 11000, 2000, 500, 1, 2, 19, 1, 5], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 674, 11000, 2000, 500, 1, 2, 23, 1, 5], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "dell-ome-m",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 674, 10892, 6, 3, 1, 8], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "dell-os10",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 674, 11000, 5000, 100, 4, 1, 1, 3, 1, 11], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 674, 11000, 5000, 100, 4, 1, 1, 4, 1, 5], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "dell-rpdu",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 674, 10903, 200, 2, 200, 150, 2, 2, 1, 5], indexed: true }, divisor: 10, multiplier: 1 },
            ],
        },
    ),
    (
        "deltaorion",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 20246, 2, 3, 1, 1, 1, 2, 9, 1, 1, 3], indexed: true }, divisor: 10, multiplier: 1 },
            ],
        },
    ),
    (
        "deva-db7001",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 35833, 37, 3, 39], indexed: true }, divisor: 256, multiplier: 1 },
            ],
        },
    ),
    (
        "didactum",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 46501, 3, 5, 1, 25], indexed: true }, divisor: 100, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 46501, 5, 2, 1, 25], indexed: true }, divisor: 100, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 46501, 6, 2, 1, 25], indexed: true }, divisor: 100, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 46501, 7, 2, 1, 25], indexed: true }, divisor: 100, multiplier: 1 },
            ],
        },
    ),
    (
        "dlink",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 171, 12, 1, 1, 6, 1], indexed: true }, precision: 1 },
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 171, 14, 5, 1, 7, 1, 4], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::Percent { percent: OidColumn { base: &[1, 3, 6, 1, 4, 1, 171, 12, 1, 1, 9, 1, 4, 0], indexed: false } },
                MemRule::Pool { used: OidColumn { base: &[1, 3, 6, 1, 4, 1, 171, 11, 165, 1000, 5, 1, 4, 1, 4], indexed: true }, free: OidColumn { base: &[1, 3, 6, 1, 4, 1, 171, 11, 165, 1000, 5, 1, 4, 1, 5], indexed: true }, precision: 1 },
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 171, 12, 11, 1, 8, 1, 2], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 171, 14, 5, 1, 1, 1, 1, 4], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "dlink-dgs1250",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 171, 11, 165, 1000, 5, 1, 7, 1, 5], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::Pool { used: OidColumn { base: &[1, 3, 6, 1, 4, 1, 171, 11, 165, 1000, 5, 1, 4, 1, 4], indexed: true }, free: OidColumn { base: &[1, 3, 6, 1, 4, 1, 171, 11, 165, 1000, 5, 1, 4, 1, 5], indexed: true }, precision: 1024 },
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 171, 11, 165, 1000, 5, 1, 1, 1, 1, 4], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 171, 11, 165, 1000, 72, 1, 4, 1, 1, 1], indexed: true }, divisor: 1000, multiplier: 1 },
            ],
        },
    ),
    (
        "dnos",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 6027, 3, 26, 1, 4, 4, 1, 4], indexed: true }, precision: 1 },
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 6027, 3, 26, 1, 4, 4, 1, 5], indexed: true }, precision: 1 },
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 6027, 3, 10, 1, 2, 9, 1, 2], indexed: true }, precision: 1 },
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 6027, 3, 8, 1, 3, 7, 1, 3], indexed: true }, precision: 1 },
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 6027, 3, 8, 1, 5, 1, 1, 1], indexed: true }, precision: 1 },
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 6027, 3, 19, 1, 2, 8, 1, 2], indexed: true }, precision: 1 },
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 6027, 3, 25, 1, 2, 3, 1, 1], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::PercentTotal { percent: OidColumn { base: &[1, 3, 6, 1, 4, 1, 6027, 3, 26, 1, 4, 4, 1, 6], indexed: true }, total: OidColumn { base: &[1, 3, 6, 1, 4, 1, 6027, 3, 26, 1, 4, 3, 1, 6], indexed: true }, precision: 1048576 },
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 6027, 3, 26, 1, 3, 4, 1, 13], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 6027, 3, 11, 1, 3, 1, 1, 16], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "drac",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 674, 10892, 2, 3, 1, 10], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 674, 10892, 2, 3, 1, 11], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 674, 10892, 2, 3, 1, 12], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 674, 10892, 5, 4, 700, 20, 1, 6], indexed: true }, divisor: 10, multiplier: 1 },
            ],
        },
    ),
    (
        "dsm",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 6574, 1, 2], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 6574, 2, 1, 1, 6], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "dxmos",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 21150, 1, 1, 3, 1, 10], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 21150, 1, 1, 13, 1, 3], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 21150, 1, 1, 17, 1, 26], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 21150, 1, 1, 17, 1, 28], indexed: true }, divisor: 10, multiplier: 1 },
            ],
        },
    ),
    (
        "e3meter",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 21695, 1, 10, 7, 3, 1, 4], indexed: true }, divisor: 10, multiplier: 1 },
            ],
        },
    ),
    (
        "e3meterdc",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 21695, 1, 10, 5, 1, 4], indexed: true }, divisor: 10, multiplier: 1 },
            ],
        },
    ),
    (
        "eaton-matrix",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 935, 10, 2, 1, 3, 2, 1, 16], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 935, 10, 2, 1, 1, 4], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 935, 10, 2, 1, 2, 2, 1, 13], indexed: true }, divisor: 10, multiplier: 1 },
            ],
        },
    ),
    (
        "eaton-mgeups",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 705, 1, 5, 7], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 534, 1, 6, 5], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 534, 1, 6, 1], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "eaton-sc200",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 1918, 2, 13, 10, 100, 30], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 1918, 2, 13, 10, 60, 29], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 1918, 2, 13, 10, 60, 30, 1, 110], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "eaton-sc300",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 1918, 2, 14, 10, 100, 30], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 1918, 2, 14, 10, 60, 29], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 1918, 2, 14, 10, 60, 30, 1, 110], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "eatonpdu",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 534, 6, 6, 7, 7, 1, 1, 4], indexed: true }, divisor: 10, multiplier: 1 },
            ],
        },
    ),
    (
        "eatonups",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 534, 1, 6, 1], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "eatonupsm2",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 534, 1, 6, 1], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 534, 1, 6, 5], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "ecosdn",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 45555, 3, 3, 2, 1, 2], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::TotalFree { total: OidColumn { base: &[1, 3, 6, 1, 4, 1, 45555, 3, 3, 3, 1, 0], indexed: false }, free: OidColumn { base: &[1, 3, 6, 1, 4, 1, 45555, 3, 3, 3, 2, 0], indexed: false }, precision: 1 },
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 45555, 3, 3, 7, 1, 3], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "ecresofm-os",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 6404, 3, 1, 2, 11, 1, 3, 1, 1], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 6404, 3, 1, 2, 11, 1, 3, 1, 10], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 6404, 3, 1, 2, 11, 1, 3, 1, 11], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 6404, 3, 1, 2, 11, 1, 3, 1, 9], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "edfa",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 17409, 1, 3, 3, 2, 2, 1, 12], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 17409, 1, 11, 4, 1, 4], indexed: true }, divisor: 10, multiplier: 1 },
            ],
        },
    ),
    (
        "edfamux",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 51628, 1, 2], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 51628, 1, 3], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "edgeos-ep",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 41112, 1, 4, 8, 3], indexed: true }, precision: 100 },
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 41112, 1, 5, 4, 2, 1, 3], indexed: true }, divisor: 1000, multiplier: 1 },
            ],
        },
    ),
    (
        "edgeosolt",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 41112, 1, 5, 4, 2, 1, 3], indexed: true }, divisor: 1000, multiplier: 1 },
            ],
        },
    ),
    (
        "edgeswitch",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
                MemRule::TotalFree { total: OidColumn { base: &[1, 3, 6, 1, 4, 1, 4413, 1, 1, 1, 1, 4, 2, 0], indexed: false }, free: OidColumn { base: &[1, 3, 6, 1, 4, 1, 4413, 1, 1, 1, 1, 4, 1, 0], indexed: false }, precision: 1024 },
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 4413, 1, 1, 43, 1, 8, 1, 5], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "eds",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 31440, 10, 5, 1, 1], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "ees",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 6302, 2, 1, 2, 7, 3, 1, 2], indexed: true }, divisor: 1000, multiplier: 1 },
            ],
        },
    ),
    (
        "eltek-webpower",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 12148, 9, 5, 5, 2, 1, 5], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "eltex-mes21xx",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 89, 1, 9], indexed: true }, precision: 1 },
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 89, 53, 15, 1, 9], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "eltex-mes23xx",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 89, 1, 9], indexed: true }, precision: 1 },
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 89, 53, 15, 1, 10], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 89, 108, 3, 1, 6], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "eltex-mes24xx",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 35265, 1, 139, 6, 1, 1, 2, 2], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::UsedTotal { total: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2076, 81, 1, 72, 0], indexed: false }, used: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2076, 81, 1, 73, 0], indexed: false }, precision: 1 },
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2076, 81, 1, 66], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "eltex-olt",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 35265, 1, 22, 1, 10, 4], indexed: true }, precision: 10 },
            ],
            mem: &[
            ],
            temp: &[
            ],
        },
    ),
    (
        "enexus",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 12148, 10, 10, 7, 5], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 12148, 10, 10, 18, 3, 1, 6], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 12148, 10, 13, 13, 5], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "enterasys",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 5624, 1, 2, 49, 1, 1, 1, 1, 4], indexed: true }, precision: 10 },
            ],
            mem: &[
            ],
            temp: &[
            ],
        },
    ),
    (
        "epmp",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 17713, 21, 2, 1, 64], indexed: true }, precision: 10 },
            ],
            mem: &[
            ],
            temp: &[
            ],
        },
    ),
    (
        "ericsson-6600",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 193, 81, 3, 4, 3, 1, 1, 1, 8], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "ericsson-ipos",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 193, 218, 2, 6, 1, 3], indexed: true }, precision: 1 },
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 193, 218, 2, 6, 1, 6], indexed: true }, precision: 1 },
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 193, 218, 2, 4, 1, 6, 1, 3], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 193, 218, 2, 50, 1, 2, 1, 3], indexed: true }, divisor: 1000, multiplier: 1 },
            ],
        },
    ),
    (
        "ericsson-tn",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 193, 81, 3, 4, 3, 1, 1, 1, 8], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "esphome",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
                MemRule::TotalFree { total: OidColumn { base: &[1, 3, 9999, 32, 1], indexed: true }, free: OidColumn { base: &[1, 3, 9999, 32, 2], indexed: true }, precision: 1 },
                MemRule::UsedTotal { total: OidColumn { base: &[1, 3, 6, 1, 2, 1, 25, 2, 3, 1, 5], indexed: true }, used: OidColumn { base: &[1, 3, 6, 1, 2, 1, 25, 2, 3, 1, 6], indexed: true }, precision: 1 },
            ],
            temp: &[
            ],
        },
    ),
    (
        "eurostor",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 14752, 1, 3, 2, 1, 10, 1, 3], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 14752, 1, 3, 2, 2, 10, 1, 3], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 14752, 1, 3, 2, 3, 10, 1, 3], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 14752, 1, 3, 2, 4, 10, 1, 3], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 14752, 1, 3, 2, 5, 10, 1, 3], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 14752, 1, 3, 2, 6, 10, 1, 3], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 14752, 1, 3, 2, 7, 10, 1, 3], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 14752, 1, 3, 2, 8, 10, 1, 3], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 14752, 1, 3, 2, 9, 10, 1, 3], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "ewc",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 5624, 1, 2, 49, 1, 1, 1, 1, 4], indexed: true }, precision: 10 },
            ],
            mem: &[
            ],
            temp: &[
            ],
        },
    ),
    (
        "exalink-fusion",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 43296, 3, 3, 1, 4], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 43296, 3, 2, 1, 1, 3], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "exeltech-xfersw",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 30320, 1, 0, 1, 8], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 30320, 1, 0, 2, 8], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 30320, 1, 0, 3, 8], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "f5",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
                MemRule::UsedTotal { total: OidColumn { base: &[1, 3, 6, 1, 4, 1, 3375, 2, 1, 1, 2, 1, 44, 0], indexed: false }, used: OidColumn { base: &[1, 3, 6, 1, 4, 1, 3375, 2, 1, 1, 2, 1, 45, 0], indexed: false }, precision: 1 },
                MemRule::UsedTotal { total: OidColumn { base: &[1, 3, 6, 1, 4, 1, 3375, 2, 1, 1, 2, 20, 44, 1], indexed: false }, used: OidColumn { base: &[1, 3, 6, 1, 4, 1, 3375, 2, 1, 1, 2, 20, 45, 1], indexed: false }, precision: 1 },
                MemRule::UsedTotal { total: OidColumn { base: &[1, 3, 6, 1, 4, 1, 3375, 2, 1, 1, 2, 20, 46, 2], indexed: false }, used: OidColumn { base: &[1, 3, 6, 1, 4, 1, 3375, 2, 1, 1, 2, 20, 47, 2], indexed: false }, precision: 1 },
            ],
            temp: &[
            ],
        },
    ),
    (
        "f5os",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 12276, 1, 2, 1, 1, 2, 1, 4], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::TotalFree { total: OidColumn { base: &[1, 3, 6, 1, 4, 1, 12276, 1, 2, 1, 4, 1, 1, 5], indexed: true }, free: OidColumn { base: &[1, 3, 6, 1, 4, 1, 12276, 1, 2, 1, 4, 1, 1, 6], indexed: true }, precision: 1 },
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 12276, 1, 2, 1, 3, 1, 1, 2], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "fabos",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 1588, 2, 1, 1, 1, 26, 1], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::Percent { percent: OidColumn { base: &[1, 3, 6, 1, 4, 1, 1588, 2, 1, 1, 1, 26, 6, 0], indexed: false } },
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 1588, 2, 1, 1, 1, 1, 22, 1, 4], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 1588, 2, 1, 1, 1, 28, 1, 1, 1], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "feos",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 25597, 11, 1, 1, 4], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "fiberhome",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 5875, 800, 3, 9, 8, 1, 1, 5], indexed: true }, precision: 100 },
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 5875, 800, 3, 9, 4, 5], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "fiberhome-switch",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 3807, 1, 8012, 1, 4, 1, 1, 2], indexed: true }, precision: 100 },
            ],
            mem: &[
                MemRule::UsedTotal { total: OidColumn { base: &[1, 3, 6, 1, 4, 1, 3807, 1, 8012, 1, 5, 4, 1, 11], indexed: true }, used: OidColumn { base: &[1, 3, 6, 1, 4, 1, 3807, 1, 8012, 1, 5, 4, 1, 7], indexed: true }, precision: 1 },
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 3807, 1, 8012, 1, 6, 2, 1, 5], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "fibernet-xmux4-plus",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 29003, 2, 44, 1, 1, 9, 1, 4], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 29003, 2, 44, 1, 1, 9, 1, 5], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "fibrolan-falcon",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 4467, 1000, 10, 1, 120, 1, 2], indexed: true }, precision: 1 },
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 4467, 1000, 50, 1, 2, 1, 3], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "fibrolan-metrostar",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 4467, 1000, 10, 1, 120, 1, 2], indexed: true }, precision: 1 },
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 4467, 100, 500, 20, 1, 4], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "firebrick",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 24693, 100, 2, 1, 1, 2], indexed: true }, precision: 100 },
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 24693, 100, 1, 1, 1, 4], indexed: true }, divisor: 1000, multiplier: 1 },
            ],
        },
    ),
    (
        "flexbng",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 54268, 1, 8, 1, 1], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::Percent { percent: OidColumn { base: &[1, 3, 6, 1, 4, 1, 54268, 1, 8, 1, 2, 0], indexed: false } },
            ],
            temp: &[
            ],
        },
    ),
    (
        "forcepoint",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 1369, 5, 2, 1, 11, 1, 1, 3], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::Pool { used: OidColumn { base: &[1, 3, 6, 1, 4, 1, 1369, 5, 2, 1, 11, 2, 5, 0], indexed: false }, free: OidColumn { base: &[1, 3, 6, 1, 4, 1, 1369, 5, 2, 1, 11, 2, 6, 0], indexed: false }, precision: 1 },
            ],
            temp: &[
            ],
        },
    ),
    (
        "forcepoint-fe",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 47565, 1, 1, 1, 11, 1, 1, 3], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::Pool { used: OidColumn { base: &[1, 3, 6, 1, 4, 1, 47565, 1, 1, 1, 11, 2, 5, 0], indexed: false }, free: OidColumn { base: &[1, 3, 6, 1, 4, 1, 47565, 1, 1, 1, 11, 2, 6, 0], indexed: false }, precision: 1 },
            ],
            temp: &[
            ],
        },
    ),
    (
        "fortiadc",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 12356, 112, 1, 4], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::Percent { percent: OidColumn { base: &[1, 3, 6, 1, 4, 1, 12356, 112, 1, 5, 0], indexed: false } },
            ],
            temp: &[
            ],
        },
    ),
    (
        "fortiap",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 12356, 120, 3, 41], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::Percent { percent: OidColumn { base: &[1, 3, 6, 1, 4, 1, 12356, 120, 3, 42, 0], indexed: false } },
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 12356, 120, 3, 44], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "fortiauthenticator",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 12356, 113, 1, 4], indexed: true }, precision: 1 },
            ],
            mem: &[
            ],
            temp: &[
            ],
        },
    ),
    (
        "fortiextender",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 12356, 121, 21, 1, 2, 1], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::Percent { percent: OidColumn { base: &[1, 3, 6, 1, 4, 1, 12356, 121, 21, 1, 2, 2, 0], indexed: false } },
            ],
            temp: &[
            ],
        },
    ),
    (
        "fortigate",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 12356, 101, 4, 1, 3], indexed: true }, precision: 1 },
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 12356, 101, 4, 4, 2, 1, 2], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::PercentTotal { percent: OidColumn { base: &[1, 3, 6, 1, 4, 1, 12356, 101, 4, 1, 4, 0], indexed: false }, total: OidColumn { base: &[1, 3, 6, 1, 4, 1, 12356, 101, 4, 1, 5, 0], indexed: false }, precision: 1024 },
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 12356, 101, 4, 3, 2, 1, 3], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "fortimail",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 12356, 105, 1, 6], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::Percent { percent: OidColumn { base: &[1, 3, 6, 1, 4, 1, 12356, 105, 1, 7, 0], indexed: false } },
            ],
            temp: &[
            ],
        },
    ),
    (
        "fortios",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
                MemRule::UsedTotal { total: OidColumn { base: &[1, 3, 6, 1, 4, 1, 12356, 103, 2, 1, 3, 0], indexed: false }, used: OidColumn { base: &[1, 3, 6, 1, 4, 1, 12356, 103, 2, 1, 2, 0], indexed: false }, precision: 1024 },
            ],
            temp: &[
            ],
        },
    ),
    (
        "fortisandbox",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 12356, 118, 3, 1, 2], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::PercentTotal { percent: OidColumn { base: &[1, 3, 6, 1, 4, 1, 12356, 118, 3, 1, 3, 0], indexed: false }, total: OidColumn { base: &[1, 3, 6, 1, 4, 1, 12356, 118, 3, 1, 4, 0], indexed: false }, precision: 1024 },
            ],
            temp: &[
            ],
        },
    ),
    (
        "fortiswitch",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 12356, 106, 4, 1, 2], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::UsedTotal { total: OidColumn { base: &[1, 3, 6, 1, 4, 1, 12356, 106, 4, 1, 4, 0], indexed: false }, used: OidColumn { base: &[1, 3, 6, 1, 4, 1, 12356, 106, 4, 1, 3, 0], indexed: false }, precision: 1024 },
            ],
            temp: &[
            ],
        },
    ),
    (
        "fortivoice",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 12356, 115, 1, 6], indexed: true }, precision: 1 },
            ],
            mem: &[
            ],
            temp: &[
            ],
        },
    ),
    (
        "fortiweb",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 12356, 107, 2, 1, 5], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::PercentTotal { percent: OidColumn { base: &[1, 3, 6, 1, 4, 1, 12356, 107, 2, 1, 7, 0], indexed: false }, total: OidColumn { base: &[1, 3, 6, 1, 4, 1, 12356, 107, 2, 1, 8, 0], indexed: false }, precision: 1048576 },
            ],
            temp: &[
            ],
        },
    ),
    (
        "fortiwlc",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 15983, 1, 1, 3, 1, 14, 13], indexed: true }, precision: 1 },
            ],
            mem: &[
            ],
            temp: &[
            ],
        },
    ),
    (
        "fs-bdcom",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 52642, 3, 6, 10, 1, 11], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::Percent { percent: OidColumn { base: &[1, 3, 6, 1, 4, 1, 11606, 10, 3, 6, 10, 1, 12, 0], indexed: false } },
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 52642, 3, 6, 10, 1, 13], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 52642, 9, 63, 1, 7, 1, 4], indexed: true }, divisor: 256, multiplier: 1 },
            ],
        },
    ),
    (
        "fs-centec",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 52642, 1, 1, 9, 2], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::Pool { used: OidColumn { base: &[1, 3, 6, 1, 4, 1, 52642, 1, 1, 1, 12], indexed: true }, free: OidColumn { base: &[1, 3, 6, 1, 4, 1, 52642, 1, 1, 1, 11], indexed: true }, precision: 1024 },
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 52642, 1, 37, 1, 3, 1, 4], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "fs-gbn",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
                MemRule::TotalFree { total: OidColumn { base: &[1, 3, 6, 1, 4, 1, 65268, 1, 2, 1, 1, 2, 12, 0], indexed: false }, free: OidColumn { base: &[1, 3, 6, 1, 4, 1, 65268, 1, 2, 1, 1, 2, 13, 0], indexed: false }, precision: 1048576 },
            ],
            temp: &[
            ],
        },
    ),
    (
        "fs-ruijie",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 52642, 1, 1, 10, 2, 36, 1, 1, 3], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::UsedTotal { total: OidColumn { base: &[1, 3, 6, 1, 4, 1, 52642, 1, 1, 10, 2, 35, 1, 1, 1, 6], indexed: true }, used: OidColumn { base: &[1, 3, 6, 1, 4, 1, 52642, 1, 1, 10, 2, 35, 1, 1, 1, 7], indexed: true }, precision: 1024 },
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 52642, 1, 1, 10, 2, 1, 1, 44, 1, 5], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 52642, 1, 1, 10, 2, 105, 1, 1, 1, 17], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "fs-switch",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 52642, 2, 1, 45, 1, 39, 2, 1], indexed: true }, precision: 1 },
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 27975, 1, 2, 11], indexed: true }, precision: -1 },
            ],
            mem: &[
                MemRule::Pool { used: OidColumn { base: &[1, 3, 6, 1, 4, 1, 52642, 1, 1, 1, 12, 0], indexed: false }, free: OidColumn { base: &[1, 3, 6, 1, 4, 1, 52642, 1, 1, 1, 11, 0], indexed: false }, precision: 1024 },
                MemRule::Pool { used: OidColumn { base: &[1, 3, 6, 1, 4, 1, 52642, 2, 1, 45, 1, 39, 3, 2, 0], indexed: false }, free: OidColumn { base: &[1, 3, 6, 1, 4, 1, 52642, 2, 1, 45, 1, 39, 3, 3, 0], indexed: false }, precision: 1024 },
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 52642, 2, 1, 45, 1, 1, 11, 1, 3], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "fs-switch-dc",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 52642, 1, 1, 10, 2, 36, 1, 2, 1, 5], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::UsedTotal { total: OidColumn { base: &[1, 3, 6, 1, 4, 1, 52642, 1, 1, 10, 2, 35, 1, 2, 1, 6], indexed: true }, used: OidColumn { base: &[1, 3, 6, 1, 4, 1, 52642, 1, 1, 10, 2, 35, 1, 2, 1, 7], indexed: true }, precision: 1024 },
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 52642, 1, 1, 10, 2, 1, 1, 44, 1, 5], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 52642, 1, 1, 10, 2, 105, 1, 1, 1, 17], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "ftos",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 6027, 3, 10, 1, 2, 9, 1, 2], indexed: true }, precision: 1 },
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 6027, 3, 8, 1, 3, 7, 1, 3], indexed: true }, precision: 1 },
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 6027, 3, 8, 1, 5, 1, 1, 1], indexed: true }, precision: 1 },
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 6027, 3, 19, 1, 2, 8, 1, 2], indexed: true }, precision: 1 },
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 6027, 3, 25, 1, 2, 3, 1, 1], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::PercentTotal { percent: OidColumn { base: &[1, 3, 6, 1, 4, 1, 6027, 3, 1, 1, 3, 7, 1, 6, 0], indexed: false }, total: OidColumn { base: &[1, 3, 6, 1, 4, 1, 6027, 3, 1, 1, 2, 6, 1, 6, 0], indexed: false }, precision: 1 },
                MemRule::PercentTotal { percent: OidColumn { base: &[1, 3, 6, 1, 4, 1, 6027, 3, 10, 1, 2, 9, 1, 5, 0], indexed: false }, total: OidColumn { base: &[1, 3, 6, 1, 4, 1, 6027, 3, 10, 1, 2, 7, 1, 4, 0], indexed: false }, precision: 1 },
                MemRule::PercentTotal { percent: OidColumn { base: &[1, 3, 6, 1, 4, 1, 6027, 3, 8, 1, 2, 3, 1, 6, 0], indexed: false }, total: OidColumn { base: &[1, 3, 6, 1, 4, 1, 6027, 3, 8, 1, 2, 3, 1, 6, 0], indexed: false }, precision: 1 },
                MemRule::PercentTotal { percent: OidColumn { base: &[1, 3, 6, 1, 4, 1, 6027, 3, 19, 1, 2, 6, 1, 4, 0], indexed: false }, total: OidColumn { base: &[1, 3, 6, 1, 4, 1, 6027, 3, 19, 1, 2, 6, 1, 4, 0], indexed: false }, precision: 1 },
                MemRule::PercentTotal { percent: OidColumn { base: &[1, 3, 6, 1, 4, 1, 6027, 3, 25, 1, 2, 3, 1, 4, 0], indexed: false }, total: OidColumn { base: &[1, 3, 6, 1, 4, 1, 6027, 3, 25, 1, 2, 1, 1, 4, 0], indexed: false }, precision: 1 },
            ],
            temp: &[
            ],
        },
    ),
    (
        "fujitsu-irmc",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 231, 2, 10, 2, 2, 10, 5, 1, 1, 6], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "fusion",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 259, 6, 10, 120, 1, 2, 11, 1, 2], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 259, 6, 10, 120, 1, 1, 11, 1, 3], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "fxm",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 7309, 6, 1, 6, 4], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 7309, 6, 1, 2, 6], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "gaia",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2620, 1, 6, 7, 5, 1, 5], indexed: true }, precision: 1 },
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2620, 1, 6, 7, 8, 1, 1, 3], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "gam",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 20095, 2001, 24, 1, 3, 1, 3], indexed: true }, precision: 1 },
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 20095, 2001, 24, 1, 3, 7, 1, 3], indexed: true }, divisor: 10, multiplier: 1 },
            ],
        },
    ),
    (
        "geist-watchdog",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 21239, 5, 1, 2, 1, 5], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 21239, 5, 1, 4, 1, 5], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 21239, 5, 1, 5, 1, 5], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 21239, 5, 1, 6, 1, 5], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 21239, 5, 1, 8, 1, 6], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 21239, 5, 1, 9, 1, 5], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 21239, 2, 32, 1, 5], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 21239, 2, 31, 1, 6], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 21239, 2, 26, 1, 5], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 21239, 2, 17, 1, 5], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 21239, 2, 13, 1, 8], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 21239, 2, 5, 1, 5], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 21239, 2, 4, 1, 5], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 21239, 2, 2, 1, 5], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 21239, 2, 19, 1, 13], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 21239, 2, 19, 1, 14], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "gemds",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 4130, 10, 1, 1, 1, 2, 6], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "gepulsar",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 10520, 2, 1, 3, 7, 1, 12], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "grandstream-sw",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 42397, 1, 4, 12, 1, 1], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "gude-epc",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28507, 43, 1, 6, 1, 1, 2], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28507, 68, 1, 6, 1, 1, 2], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28507, 69, 1, 6, 1, 1, 2], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28507, 107, 1, 6, 1, 1, 2], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28507, 106, 1, 6, 1, 1, 2], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28507, 100, 1, 6, 1, 1, 2], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28507, 77, 1, 6, 1, 1, 2], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28507, 81, 1, 6, 1, 1, 2], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28507, 82, 1, 6, 1, 1, 2], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28507, 83, 1, 6, 1, 1, 2], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28507, 85, 1, 6, 1, 1, 2], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28507, 87, 1, 6, 1, 1, 2], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28507, 56, 1, 6, 1, 1, 2], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28507, 58, 1, 6, 1, 1, 2], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28507, 98, 1, 6, 1, 1, 2], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28507, 97, 1, 6, 1, 1, 2], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28507, 64, 1, 6, 1, 1, 2], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28507, 111, 1, 6, 1, 1, 2], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28507, 112, 1, 6, 1, 1, 2], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28507, 43, 1, 6, 1, 1, 6], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28507, 68, 1, 6, 1, 1, 6], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28507, 69, 1, 6, 1, 1, 6], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28507, 107, 1, 6, 1, 1, 6], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28507, 106, 1, 6, 1, 1, 6], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28507, 100, 1, 6, 1, 1, 6], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28507, 77, 1, 6, 1, 1, 6], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28507, 81, 1, 6, 1, 1, 6], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28507, 82, 1, 6, 1, 1, 6], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28507, 83, 1, 6, 1, 1, 6], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28507, 85, 1, 6, 1, 1, 6], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28507, 87, 1, 6, 1, 1, 6], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28507, 56, 1, 6, 1, 1, 6], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28507, 58, 1, 6, 1, 1, 6], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28507, 98, 1, 6, 1, 1, 6], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28507, 97, 1, 6, 1, 1, 6], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28507, 64, 1, 6, 1, 1, 6], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28507, 111, 1, 6, 1, 1, 6], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28507, 112, 1, 6, 1, 1, 6], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28507, 43, 1, 6, 1, 1, 7], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28507, 68, 1, 6, 1, 1, 7], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28507, 69, 1, 6, 1, 1, 7], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28507, 107, 1, 6, 1, 1, 7], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28507, 106, 1, 6, 1, 1, 7], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28507, 100, 1, 6, 1, 1, 7], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28507, 77, 1, 6, 1, 1, 7], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28507, 81, 1, 6, 1, 1, 7], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28507, 82, 1, 6, 1, 1, 7], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28507, 83, 1, 6, 1, 1, 7], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28507, 85, 1, 6, 1, 1, 7], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28507, 87, 1, 6, 1, 1, 7], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28507, 56, 1, 6, 1, 1, 7], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28507, 58, 1, 6, 1, 1, 7], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28507, 98, 1, 6, 1, 1, 7], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28507, 97, 1, 6, 1, 1, 7], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28507, 64, 1, 6, 1, 1, 7], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28507, 111, 1, 6, 1, 1, 7], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28507, 112, 1, 6, 1, 1, 7], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28507, 100, 1, 5, 14, 4], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28507, 77, 1, 5, 14, 4], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28507, 81, 1, 5, 14, 4], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28507, 82, 1, 5, 14, 4], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28507, 83, 1, 5, 14, 4], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28507, 85, 1, 5, 14, 4], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28507, 87, 1, 5, 14, 4], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28507, 98, 1, 5, 14, 4], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28507, 111, 1, 5, 14, 4], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28507, 112, 1, 5, 14, 4], indexed: true }, divisor: 10, multiplier: 1 },
            ],
        },
    ),
    (
        "gude-esb",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28507, 66, 1, 6, 1, 1, 2], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28507, 66, 1, 6, 1, 1, 6], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28507, 66, 1, 6, 1, 1, 7], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28507, 67, 1, 6, 1, 1, 2], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28507, 67, 1, 6, 1, 1, 6], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28507, 67, 1, 6, 1, 1, 7], indexed: true }, divisor: 10, multiplier: 1 },
            ],
        },
    ),
    (
        "gwd",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 10072, 2, 20, 1, 1, 2, 1, 1, 19], indexed: true }, precision: 1 },
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 10072, 2, 15, 1, 6, 1, 1, 2], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 10072, 2, 15, 1, 5, 1, 1, 3], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "harmony-enhanced",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 7262, 4, 5, 2, 2, 3], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "hikvision-cam",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 39165, 1, 7], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::PercentTotal { percent: OidColumn { base: &[1, 3, 6, 1, 4, 1, 39165, 1, 11, 0], indexed: false }, total: OidColumn { base: &[1, 3, 6, 1, 4, 1, 39165, 1, 10, 0], indexed: false }, precision: 1048576 },
            ],
            temp: &[
            ],
        },
    ),
    (
        "hikvision-nvr",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
                MemRule::PercentTotal { percent: OidColumn { base: &[1, 3, 6, 1, 4, 1, 50001, 1, 221, 0], indexed: false }, total: OidColumn { base: &[1, 3, 6, 1, 4, 1, 50001, 1, 220, 0], indexed: false }, precision: 1048576 },
            ],
            temp: &[
            ],
        },
    ),
    (
        "hirschmann",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 248, 14, 2, 15, 2, 1], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::Pool { used: OidColumn { base: &[1, 3, 6, 1, 4, 1, 248, 14, 2, 15, 3, 1, 0], indexed: false }, free: OidColumn { base: &[1, 3, 6, 1, 4, 1, 248, 14, 2, 15, 3, 2, 0], indexed: false }, precision: 1024 },
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 248, 14, 1, 1, 13, 1, 12], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 248, 14, 2, 5, 1], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "hiveos-wireless",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 26928, 1, 2, 3], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::Percent { percent: OidColumn { base: &[1, 3, 6, 1, 4, 1, 26928, 1, 2, 4, 0], indexed: false } },
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 26928, 1, 2, 10], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "horizon-compact",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 7262, 2, 2, 5, 1, 3, 6], indexed: true }, divisor: 10, multiplier: 1 },
            ],
        },
    ),
    (
        "horizon-compactplus",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 7262, 2, 5, 4, 4, 1, 1, 11], indexed: true }, divisor: 10, multiplier: 1 },
            ],
        },
    ),
    (
        "horizon-quantum",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 7262, 2, 4, 1, 1, 8], indexed: true }, divisor: 100, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 7262, 2, 4, 5, 4, 1, 1, 14], indexed: true }, divisor: 10, multiplier: 1 },
            ],
        },
    ),
    (
        "hpe-ilo",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
                MemRule::TotalFree { total: OidColumn { base: &[1, 3, 6, 1, 4, 1, 232, 11, 2, 13, 1, 0], indexed: false }, free: OidColumn { base: &[1, 3, 6, 1, 4, 1, 232, 11, 2, 13, 2, 0], indexed: false }, precision: 1048576 },
                MemRule::TotalFree { total: OidColumn { base: &[1, 3, 6, 1, 4, 1, 232, 11, 2, 13, 3, 1], indexed: false }, free: OidColumn { base: &[1, 3, 6, 1, 4, 1, 232, 11, 2, 13, 4, 1], indexed: false }, precision: 1048576 },
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 232, 6, 2, 6, 8, 1, 4], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 232, 3, 2, 5, 1, 1, 70], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "hpe-mapdu",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 232, 165, 7, 4, 2, 1, 4], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 232, 165, 11, 4, 2, 1, 4], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "hpmsm",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 8744, 5, 21, 1, 1, 5], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::TotalFree { total: OidColumn { base: &[1, 3, 6, 1, 4, 1, 8744, 5, 21, 1, 1, 9, 0], indexed: false }, free: OidColumn { base: &[1, 3, 6, 1, 4, 1, 8744, 5, 21, 1, 1, 10, 0], indexed: false }, precision: 1024 },
            ],
            temp: &[
            ],
        },
    ),
    (
        "huawei-optixrtn",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2011, 2, 25, 4, 30, 14, 1, 1, 1, 5], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "hwg-poseidon",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 21796, 3, 3, 3, 1, 6], indexed: true }, divisor: 10, multiplier: 1 },
            ],
        },
    ),
    (
        "hwg-ste",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 21796, 4, 1, 3, 1, 5], indexed: true }, divisor: 10, multiplier: 1 },
            ],
        },
    ),
    (
        "hwg-ste2",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 21796, 4, 9, 3, 1, 5], indexed: true }, divisor: 10, multiplier: 1 },
            ],
        },
    ),
    (
        "ibm-amm",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2, 3, 51, 2, 2, 1, 1, 2], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2, 3, 51, 2, 2, 1, 5, 1], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "ibm-imm",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2, 3, 51, 3, 1, 1, 2, 1, 3], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "ibmc",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2011, 2, 235, 1, 1, 1, 23], indexed: true }, precision: 1 },
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2011, 2, 235, 1, 1, 26, 50, 1, 3], indexed: true }, divisor: 10, multiplier: 1 },
            ],
        },
    ),
    (
        "iboot",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 1418, 6, 6, 5], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 1418, 6, 6, 6], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "ibos",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 9303, 4, 1, 2, 1, 1, 2], indexed: true }, divisor: 100, multiplier: 1 },
            ],
        },
    ),
    (
        "icotera",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 29865, 11, 3, 1, 3, 1], indexed: true }, divisor: 256, multiplier: 1 },
            ],
        },
    ),
    (
        "icr-os",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 30140, 3, 6], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::UsedTotal { total: OidColumn { base: &[1, 3, 6, 1, 4, 1, 30140, 3, 10, 0], indexed: false }, used: OidColumn { base: &[1, 3, 6, 1, 4, 1, 30140, 3, 8, 0], indexed: false }, precision: 1 },
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 30140, 3, 3], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "ict-mps",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 39145, 13, 12], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "ies5000",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 890, 1, 5, 13, 5, 11, 3, 4, 1, 2], indexed: true }, precision: 1 },
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 890, 1, 5, 13, 5, 11, 3, 3, 1, 2], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "ies52xxM",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 890, 1, 5, 13, 13, 5, 9, 1, 3], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::Percent { percent: OidColumn { base: &[1, 3, 6, 1, 4, 1, 890, 1, 5, 13, 13, 5, 10, 1, 3], indexed: true } },
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 890, 1, 5, 13, 13, 5, 5, 1, 1], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "ihse-draco",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 38034, 1, 4, 2], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "imcopower-big",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2185, 1, 3, 11, 2, 1, 7, 11], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2185, 1, 3, 11, 2, 1, 8, 11], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2185, 1, 3, 11, 2, 1, 9, 11], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2185, 1, 3, 11, 2, 1, 3, 31], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2185, 1, 3, 11, 2, 1, 4, 31], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2185, 1, 3, 11, 2, 1, 5, 31], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "imcopower-ls110",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 33283, 1, 30, 5, 1, 3], indexed: true }, divisor: 100, multiplier: 1 },
            ],
        },
    ),
    (
        "infinera-groove",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 42229, 1, 2, 3, 1, 1, 1, 3], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 42229, 1, 2, 3, 1, 1, 1, 4], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "infinera-xtm",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 8708, 2, 71, 2, 2, 1, 1, 3], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::PercentTotal { percent: OidColumn { base: &[1, 3, 6, 1, 4, 1, 8708, 2, 71, 2, 3, 1, 1, 6], indexed: true }, total: OidColumn { base: &[1, 3, 6, 1, 4, 1, 8708, 2, 71, 2, 3, 1, 1, 3], indexed: true }, precision: 1 },
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 8708, 2, 11, 2, 2, 1, 1, 11], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 8708, 2, 11, 2, 3, 1, 1, 8], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 8708, 2, 19, 2, 2, 1, 1, 43], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 8708, 2, 47, 2, 2, 1, 1, 5], indexed: true }, divisor: 10, multiplier: 1 },
            ],
        },
    ),
    (
        "infinity",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
                MemRule::TotalFree { total: OidColumn { base: &[1, 3, 6, 1, 4, 1, 10002, 1, 1, 1, 1, 1, 0], indexed: false }, free: OidColumn { base: &[1, 3, 6, 1, 4, 1, 10002, 1, 1, 1, 1, 2, 0], indexed: false }, precision: 1024 },
            ],
            temp: &[
            ],
        },
    ),
    (
        "infortrend",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 1714, 1, 1, 9, 1, 9], indexed: true }, divisor: 10, multiplier: 1 },
            ],
        },
    ),
    (
        "inteno",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 12919, 6, 5], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "interseptor",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 19011, 1, 3, 1, 1, 3, 2, 1, 3], indexed: true }, divisor: 10, multiplier: 1 },
            ],
        },
    ),
    (
        "ionodes",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 40748, 2, 1], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::Percent { percent: OidColumn { base: &[1, 3, 6, 1, 4, 1, 40748, 2, 2, 0], indexed: false } },
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 40748, 2, 3], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "iqnos",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 21296, 2, 2, 2, 1, 13, 1, 1, 9], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "ironware",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
                MemRule::TotalFree { total: OidColumn { base: &[1, 3, 6, 1, 4, 1, 1991, 1, 1, 2, 12, 4, 2, 0], indexed: false }, free: OidColumn { base: &[1, 3, 6, 1, 4, 1, 1991, 1, 1, 2, 12, 4, 3, 0], indexed: false }, precision: 1 },
                MemRule::TotalFree { total: OidColumn { base: &[1, 3, 6, 1, 4, 1, 1991, 1, 1, 2, 2, 1, 1, 24, 0], indexed: false }, free: OidColumn { base: &[1, 3, 6, 1, 4, 1, 1991, 1, 1, 2, 2, 1, 1, 25, 0], indexed: false }, precision: 1 },
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 1991, 1, 1, 2, 13, 1, 1, 4], indexed: true }, divisor: 2, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 1991, 1, 1, 2, 13, 3, 1, 5], indexed: true }, divisor: 2, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 1991, 1, 1, 3, 3, 6, 1, 1], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 1991, 1, 1, 3, 3, 10, 1, 2], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "iskratel-lumia",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 1332, 1, 1, 5, 3, 14, 4, 1, 3], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 1332, 1, 1, 5, 3, 32, 3, 1, 2], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 1332, 1, 1, 5, 3, 111, 3, 1, 41], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "jetstream",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 11863, 6, 4, 1, 1, 1, 1, 2], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::Percent { percent: OidColumn { base: &[1, 3, 6, 1, 4, 1, 11863, 6, 4, 1, 2, 1, 1, 2, 0], indexed: false } },
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 11863, 6, 96, 1, 7, 1, 1, 2], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "juniper-mss",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 14525, 4, 8, 1, 1, 11, 1], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::UsedTotal { total: OidColumn { base: &[1, 3, 6, 1, 4, 1, 14525, 4, 8, 1, 1, 6, 0], indexed: false }, used: OidColumn { base: &[1, 3, 6, 1, 4, 1, 14525, 4, 8, 1, 1, 12, 1, 0], indexed: false }, precision: 1024 },
            ],
            temp: &[
            ],
        },
    ),
    (
        "junos",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2636, 3, 1, 13, 1, 8], indexed: true }, precision: 1 },
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2636, 3, 39, 1, 12, 1, 1, 1, 4], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::PercentTotal { percent: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2636, 3, 1, 13, 1, 11], indexed: true }, total: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2636, 3, 1, 13, 1, 15], indexed: true }, precision: 1048576 },
                MemRule::Percent { percent: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2636, 3, 39, 1, 12, 1, 1, 1, 5], indexed: true } },
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2636, 3, 1, 13, 1, 7], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2636, 3, 60, 1, 1, 1, 1, 8], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2636, 3, 71, 1, 2, 1, 1, 39], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "junose",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 4874, 2, 2, 2, 1, 3, 5, 1, 3], indexed: true }, precision: 1 },
            ],
            mem: &[
            ],
            temp: &[
            ],
        },
    ),
    (
        "kymata",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 60429, 1, 3, 10], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "lambdatrail",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 35616, 5, 1, 26, 1, 15], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 35616, 5, 3, 1, 1, 15], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 35616, 5, 3, 1, 1, 28], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 35616, 5, 2, 1], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 35616, 2, 1, 1, 12, 1, 17], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 35616, 2, 1, 1, 1], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 35616, 9, 3, 2, 1, 10], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 35616, 9, 3, 2, 1, 20], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 35616, 9, 2, 1], indexed: true }, divisor: 100, multiplier: 1 },
            ],
        },
    ),
    (
        "lantronix-slc",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 244, 1, 1, 6, 25], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "lcos",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2356, 11, 1, 47, 20], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "lcoslx",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
                MemRule::TotalFree { total: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2356, 13, 1, 47, 4, 0], indexed: false }, free: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2356, 13, 1, 47, 5, 0], indexed: false }, precision: 1024 },
            ],
            temp: &[
            ],
        },
    ),
    (
        "lcossx",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
                MemRule::TotalFree { total: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2356, 14, 1, 1, 1, 15, 0, 0], indexed: false }, free: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2356, 14, 1, 1, 1, 204, 0, 0], indexed: false }, precision: 1048576 },
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2356, 100, 1, 1, 1, 1, 6], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2356, 14, 1, 1, 1, 22], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2356, 14, 1, 1, 1, 23], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "liebert",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 476, 1, 42, 3, 8, 60, 15, 1, 70], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 476, 1, 42, 3, 4, 1, 3, 3, 1, 6], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "linksys-ss",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 3955, 1000, 201, 1, 9], indexed: true }, precision: 1 },
            ],
            mem: &[
            ],
            temp: &[
            ],
        },
    ),
    (
        "linux",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 3582, 4, 1, 4, 1, 2, 1, 42], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 3582, 4, 1, 4, 1, 6, 2, 1, 16], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 3582, 4, 1, 4, 2, 1, 2, 1, 36], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "logmaster",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 935, 1, 1, 1, 2, 2, 3], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 935, 1, 1, 1, 9, 1, 1], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "loop-telecom",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 823, 34441, 1, 10, 11], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 823, 34441, 1, 10, 16], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "luminato",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 3715, 99, 2, 2, 1, 1, 3], indexed: true }, divisor: 10, multiplier: 1 },
            ],
        },
    ),
    (
        "mediornet",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 23056, 2, 12], indexed: true }, precision: 10 },
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 23056, 2, 11], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 23056, 2, 20, 1, 5], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "microsens-g6",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 3181, 10, 6, 1, 30, 104, 0, 0], indexed: false }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 3181, 10, 6, 1, 34, 100, 1, 20], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "mlos-atd",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 8962, 4, 1, 1, 1], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::Percent { percent: OidColumn { base: &[1, 3, 6, 1, 4, 1, 8962, 4, 1, 1, 2, 0], indexed: false } },
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 8962, 4, 1, 1, 8], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "mlos-nsp",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 8962, 2, 1, 3, 1, 1, 5, 1, 1], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::Percent { percent: OidColumn { base: &[1, 3, 6, 1, 4, 1, 8962, 2, 1, 3, 1, 4, 1, 1, 4, 0], indexed: false } },
            ],
            temp: &[
            ],
        },
    ),
    (
        "mni",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 3323, 13, 1, 4, 1, 1, 20], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "montclair-edfa",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 1385, 12, 2], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 1385, 77, 7, 2, 16], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 1385, 77, 7, 2, 17], indexed: true }, divisor: 10, multiplier: 1 },
            ],
        },
    ),
    (
        "morningstar-mppt",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 33333, 2, 47], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 33333, 2, 48], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 33333, 2, 49], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 33333, 3, 35], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 33333, 3, 36], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 33333, 3, 37], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 33333, 3, 38], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 33333, 5, 38], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 33333, 5, 39], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 33333, 5, 40], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 33333, 5, 41], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 33333, 5, 42], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 33333, 5, 43], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 33333, 5, 44], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 33333, 7, 47], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 33333, 7, 48], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 33333, 7, 49], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 33333, 10, 36], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 33333, 10, 37], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 33333, 10, 38], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "morningstar-pwm",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 33333, 6, 38], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 33333, 6, 39], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 33333, 6, 40], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 33333, 6, 41], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 33333, 8, 36], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 33333, 8, 37], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "moxa-awk",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 8691, 15, 34, 1, 11, 25, 1], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::Pool { used: OidColumn { base: &[1, 3, 6, 1, 4, 1, 8691, 15, 34, 1, 11, 25, 3, 0], indexed: false }, free: OidColumn { base: &[1, 3, 6, 1, 4, 1, 8691, 15, 34, 1, 11, 25, 4, 0], indexed: false }, precision: 1 },
            ],
            temp: &[
            ],
        },
    ),
    (
        "moxa-etherdevice",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
                MemRule::Pool { used: OidColumn { base: &[1, 3, 6, 1, 4, 1, 8691, 7, 116, 1, 58, 0], indexed: false }, free: OidColumn { base: &[1, 3, 6, 1, 4, 1, 8691, 7, 116, 1, 57, 0], indexed: false }, precision: 1 },
                MemRule::Pool { used: OidColumn { base: &[1, 3, 6, 1, 4, 1, 8691, 7, 69, 1, 58, 0], indexed: false }, free: OidColumn { base: &[1, 3, 6, 1, 4, 1, 8691, 7, 69, 1, 57, 0], indexed: false }, precision: 1 },
                MemRule::Pool { used: OidColumn { base: &[1, 3, 6, 1, 4, 1, 8691, 7, 70, 1, 58, 0], indexed: false }, free: OidColumn { base: &[1, 3, 6, 1, 4, 1, 8691, 7, 70, 1, 57, 0], indexed: false }, precision: 1 },
                MemRule::Pool { used: OidColumn { base: &[1, 3, 6, 1, 4, 1, 8691, 7, 71, 1, 58, 0], indexed: false }, free: OidColumn { base: &[1, 3, 6, 1, 4, 1, 8691, 7, 71, 1, 57, 0], indexed: false }, precision: 1 },
                MemRule::Pool { used: OidColumn { base: &[1, 3, 6, 1, 4, 1, 8691, 7, 162, 1, 58, 0], indexed: false }, free: OidColumn { base: &[1, 3, 6, 1, 4, 1, 8691, 7, 162, 1, 57, 0], indexed: false }, precision: 1 },
                MemRule::Pool { used: OidColumn { base: &[1, 3, 6, 1, 4, 1, 8691, 7, 86, 1, 58, 0], indexed: false }, free: OidColumn { base: &[1, 3, 6, 1, 4, 1, 8691, 7, 86, 1, 57, 0], indexed: false }, precision: 1 },
                MemRule::Pool { used: OidColumn { base: &[1, 3, 6, 1, 4, 1, 8691, 7, 108, 1, 58, 0], indexed: false }, free: OidColumn { base: &[1, 3, 6, 1, 4, 1, 8691, 7, 108, 1, 57, 0], indexed: false }, precision: 1 },
                MemRule::Pool { used: OidColumn { base: &[1, 3, 6, 1, 4, 1, 8691, 7, 85, 1, 58, 0], indexed: false }, free: OidColumn { base: &[1, 3, 6, 1, 4, 1, 8691, 7, 85, 1, 57, 0], indexed: false }, precision: 1 },
                MemRule::Pool { used: OidColumn { base: &[1, 3, 6, 1, 4, 1, 8691, 7, 114, 1, 58, 0], indexed: false }, free: OidColumn { base: &[1, 3, 6, 1, 4, 1, 8691, 7, 114, 1, 57, 0], indexed: false }, precision: 1 },
                MemRule::Pool { used: OidColumn { base: &[1, 3, 6, 1, 4, 1, 8691, 7, 117, 1, 58, 0], indexed: false }, free: OidColumn { base: &[1, 3, 6, 1, 4, 1, 8691, 7, 117, 1, 57, 0], indexed: false }, precision: 1 },
                MemRule::Pool { used: OidColumn { base: &[1, 3, 6, 1, 4, 1, 8691, 7, 142, 1, 58, 0], indexed: false }, free: OidColumn { base: &[1, 3, 6, 1, 4, 1, 8691, 7, 142, 1, 57, 0], indexed: false }, precision: 1 },
                MemRule::Pool { used: OidColumn { base: &[1, 3, 6, 1, 4, 1, 8691, 7, 84, 1, 58, 0], indexed: false }, free: OidColumn { base: &[1, 3, 6, 1, 4, 1, 8691, 7, 84, 1, 57, 0], indexed: false }, precision: 1 },
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 8691, 7, 86, 1, 10, 7, 1, 3], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 8691, 7, 108, 1, 10, 11, 1, 5], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 8691, 7, 71, 1, 10, 11, 1, 5], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 8691, 7, 162, 1, 10, 11, 1, 5], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 8691, 7, 85, 1, 10, 11, 1, 5], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 8691, 7, 114, 1, 10, 11, 1, 5], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 8691, 7, 117, 1, 10, 11, 1, 5], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 8691, 7, 70, 1, 10, 11, 1, 5], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 8691, 7, 142, 1, 10, 11, 1, 5], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 8691, 7, 84, 1, 10, 11, 1, 5], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "mpb-srp",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 4464, 2, 2, 8, 2, 5], indexed: true }, divisor: 10, multiplier: 1 },
            ],
        },
    ),
    (
        "mrv-od",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 629, 200, 6, 1, 1, 15], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 629, 200, 7, 1, 1, 34], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 629, 200, 8, 1, 1, 30], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "mx-nos",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 8691, 602, 1, 3, 2, 1], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::PercentTotal { percent: OidColumn { base: &[1, 3, 6, 1, 4, 1, 8691, 602, 1, 3, 2, 3, 0], indexed: false }, total: OidColumn { base: &[1, 3, 6, 1, 4, 1, 8691, 602, 1, 3, 2, 2, 0], indexed: false }, precision: 1 },
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 8691, 603, 5, 3, 2, 1, 1, 1, 5], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "mypoweros",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 5651, 3, 20, 1, 1, 3, 5, 1, 10], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::Pool { used: OidColumn { base: &[1, 3, 6, 1, 4, 1, 5651, 3, 20, 1, 1, 1, 5, 0], indexed: false }, free: OidColumn { base: &[1, 3, 6, 1, 4, 1, 5651, 3, 20, 1, 1, 1, 1, 0], indexed: false }, precision: 1 },
            ],
            temp: &[
            ],
        },
    ),
    (
        "neptune",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 1286, 5, 1, 1, 2, 1, 5, 1, 18], indexed: true }, precision: -10 },
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 1286, 5, 1, 1, 2, 1, 10, 3, 1, 4], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 1286, 5, 1, 1, 2, 1, 8, 4, 1, 1, 5], indexed: true }, divisor: 10, multiplier: 1 },
            ],
        },
    ),
    (
        "netagent2",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 13409, 1, 1, 1, 0, 0], indexed: false }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 935, 1, 1, 1, 2, 2, 3], indexed: true }, divisor: 10, multiplier: 1 },
            ],
        },
    ),
    (
        "netbotz",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 5528, 100, 4, 1, 3, 1, 2], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 5528, 100, 4, 1, 1, 1, 8], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "netgear",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
                MemRule::TotalFree { total: OidColumn { base: &[1, 3, 6, 1, 4, 1, 4413, 1, 1, 1, 1, 4, 2, 0], indexed: false }, free: OidColumn { base: &[1, 3, 6, 1, 4, 1, 4413, 1, 1, 1, 1, 4, 1, 0], indexed: false }, precision: 1024 },
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 4526, 10, 43, 1, 8, 1, 5], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "netguardian",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2682, 2, 1, 1, 6], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2682, 2, 1, 1, 7], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2682, 2, 2, 1, 5], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "netmanplus",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 5491, 10, 1, 10, 8], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 5491, 10, 1, 10, 5], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "netonix",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 46242, 3, 1, 3], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "netscaler",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 5951, 4, 1, 1, 41, 6, 1, 2], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::PercentTotal { percent: OidColumn { base: &[1, 3, 6, 1, 4, 1, 5951, 4, 1, 1, 41, 2, 0], indexed: false }, total: OidColumn { base: &[1, 3, 6, 1, 4, 1, 5951, 4, 1, 1, 41, 4, 0], indexed: false }, precision: 1048576 },
            ],
            temp: &[
            ],
        },
    ),
    (
        "netspire",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 1732, 2, 1, 9], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::Percent { percent: OidColumn { base: &[1, 3, 6, 1, 4, 1, 1732, 2, 1, 8, 0], indexed: false } },
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 1732, 2, 1, 10], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "nexans",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 266, 20, 1, 12], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "nokia-1830",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 7483, 2, 2, 3, 1, 2, 1, 2, 1, 13], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 7483, 2, 2, 4, 3, 4, 76, 1, 2], indexed: true }, divisor: 100, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 7483, 2, 2, 4, 3, 5, 71, 1, 4], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "nokia-isam",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 637, 61, 1, 9, 29, 1, 1, 4], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::UsedTotal { total: OidColumn { base: &[1, 3, 6, 1, 4, 1, 637, 61, 1, 9, 29, 2, 1, 1, 0], indexed: false }, used: OidColumn { base: &[1, 3, 6, 1, 4, 1, 637, 61, 1, 9, 29, 2, 1, 2, 0], indexed: false }, precision: 1048576 },
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 637, 61, 1, 23, 10, 1, 2], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "nos",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 1588, 2, 1, 1, 1, 26, 1], indexed: true }, precision: 1 },
            ],
            mem: &[
            ],
            temp: &[
            ],
        },
    ),
    (
        "nti",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 3699, 1, 1, 12, 1, 1, 1, 1, 4], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 3699, 1, 1, 12, 1, 2, 1, 1, 4], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 3699, 1, 1, 9, 1, 3, 1, 1, 6], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 3699, 1, 1, 10, 1, 3, 1, 1, 6], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 3699, 1, 1, 11, 1, 3, 1, 1, 6], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 3699, 1, 1, 9, 1, 5, 1, 1, 7], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 3699, 1, 1, 10, 1, 5, 1, 1, 7], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 3699, 1, 1, 11, 1, 5, 1, 1, 7], indexed: true }, divisor: 10, multiplier: 1 },
            ],
        },
    ),
    (
        "occamos",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 6066, 2, 1, 3, 2, 1, 1, 1, 1, 4], indexed: true }, divisor: 1000, multiplier: 1 },
            ],
        },
    ),
    (
        "oceanstor",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 34774, 4, 1, 23, 5, 1, 1, 11], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "ocnos",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 36673, 100, 1, 5, 1, 1, 3], indexed: true }, divisor: 1000, multiplier: 1 },
            ],
        },
    ),
    (
        "omnitron-iconverter",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 7342, 2, 1, 3, 1, 38], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 7342, 2, 1, 11, 1, 9], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "oneaccess",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 13191, 10, 3, 3, 1, 2, 3, 1, 3], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::PercentTotal { percent: OidColumn { base: &[1, 3, 6, 1, 4, 1, 13191, 10, 3, 3, 1, 1, 4, 0], indexed: false }, total: OidColumn { base: &[1, 3, 6, 1, 4, 1, 13191, 10, 3, 3, 1, 1, 3, 0], indexed: false }, precision: 1 },
            ],
            temp: &[
            ],
        },
    ),
    (
        "ons",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 3607, 2, 80, 20, 1, 30], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "opengear",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 25049, 17, 13, 1, 5], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 25049, 17, 9, 1, 5], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 25049, 10, 19, 9, 1, 1, 4], indexed: true }, divisor: 1000, multiplier: 1 },
            ],
        },
    ),
    (
        "orolia-securesync",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 18837, 3, 2, 2, 1, 17], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 18837, 3, 2, 2, 1, 18], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 18837, 3, 2, 2, 1, 19], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "osm",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 18418, 16, 1, 4], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 18418, 2, 1, 2, 3, 1, 9], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "packetlight",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2011, 5, 25, 31, 1, 1, 1, 1, 5], indexed: true }, precision: 1 },
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 4515, 1, 3, 1, 43], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "packetpower-eg",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 33688, 4, 1, 1, 12], indexed: true }, divisor: 10, multiplier: 1 },
            ],
        },
    ),
    (
        "pandacom",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 3652, 3, 3, 1, 8, 1, 9], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 3652, 3, 3, 6, 1, 1, 7], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 3652, 3, 3, 6, 4, 1, 7], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 3652, 3, 3, 3, 1, 1, 7], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 3652, 3, 3, 3, 7, 1, 7], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 3652, 3, 4, 1, 1, 1, 7], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 3652, 3, 4, 1, 7, 1, 19], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 3652, 3, 4, 1, 6, 1, 16], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "panduit",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 19536, 10, 1, 4, 2, 1, 4], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "panduit-eagle",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 3711, 24, 1, 1, 1, 2, 2, 1, 7], indexed: true }, divisor: 10, multiplier: 1 },
            ],
        },
    ),
    (
        "papouch-quido",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 18248, 16, 1, 1], indexed: true }, divisor: 10, multiplier: 1 },
            ],
        },
    ),
    (
        "parks-switch",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 50224, 3, 1, 1, 17], indexed: true }, precision: 1 },
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 50224, 3, 1, 1, 23], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 50224, 3, 2, 4, 1, 8], indexed: true }, divisor: 100, multiplier: 1 },
            ],
        },
    ),
    (
        "patton-sn",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 1768, 100, 70, 10, 2, 1, 2], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::Pool { used: OidColumn { base: &[1, 3, 6, 1, 4, 1, 1768, 100, 70, 20, 2, 1, 3, 0], indexed: false }, free: OidColumn { base: &[1, 3, 6, 1, 4, 1, 1768, 100, 70, 20, 2, 1, 4, 0], indexed: false }, precision: 1 },
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 1768, 100, 70, 30, 2, 1, 2], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "pbn",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 11606, 10, 9, 109, 1, 1, 1, 1, 5], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::PercentTotal { percent: OidColumn { base: &[1, 3, 6, 1, 4, 1, 11606, 10, 9, 48, 1, 0], indexed: false }, total: OidColumn { base: &[1, 3, 6, 1, 4, 1, 11606, 10, 9, 48, 2, 0], indexed: false }, precision: 1 },
            ],
            temp: &[
            ],
        },
    ),
    (
        "pcoweb",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 9839, 2, 1, 2, 1], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 9839, 2, 1, 2, 2], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 9839, 2, 1, 2, 3], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 9839, 2, 1, 2, 4], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 9839, 2, 1, 2, 5], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 9839, 2, 1, 2, 7], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 9839, 2, 1, 2, 10], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 9839, 2, 1, 2, 11], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 9839, 2, 1, 2, 12], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 9839, 2, 1, 2, 13], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 9839, 2, 1, 2, 14], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 9839, 2, 1, 2, 15], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 9839, 2, 1, 2, 20], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 9839, 2, 1, 2, 21], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 9839, 2, 1, 2, 22], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 9839, 2, 1, 2, 23], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 9839, 2, 1, 2, 24], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 9839, 2, 1, 2, 25], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 9839, 2, 1, 2, 8], indexed: true }, divisor: 10, multiplier: 1 },
            ],
        },
    ),
    (
        "pcoweb-rittal-lcp-3311",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 9839, 2606, 2, 1, 2, 48], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 9839, 2606, 2, 1, 2, 21], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 9839, 2606, 2, 1, 2, 22], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 9839, 2606, 2, 1, 2, 6], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 9839, 2606, 2, 1, 2, 7], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 9839, 2606, 2, 1, 2, 8], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 9839, 2606, 2, 1, 2, 2], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 9839, 2606, 2, 1, 2, 3], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 9839, 2606, 2, 1, 2, 4], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 9839, 2606, 2, 1, 3, 4], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 9839, 2606, 2, 1, 2, 9], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 9839, 2606, 2, 1, 2, 10], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 9839, 2606, 2, 1, 2, 13], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 9839, 2606, 2, 1, 2, 14], indexed: true }, divisor: 10, multiplier: 1 },
            ],
        },
    ),
    (
        "pcoweb-rittalchiller",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 9839, 2, 1, 2], indexed: true }, divisor: 10, multiplier: 1 },
            ],
        },
    ),
    (
        "pepwave",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 27662, 200, 1, 1, 1, 4, 4, 1], indexed: true }, divisor: 1000, multiplier: 1 },
            ],
        },
    ),
    (
        "picos",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 35098, 1, 1], indexed: true }, precision: 1 },
            ],
            mem: &[
            ],
            temp: &[
            ],
        },
    ),
    (
        "planet-pdu",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 10456, 3, 3, 5, 1, 3], indexed: true }, divisor: 10, multiplier: 1 },
            ],
        },
    ),
    (
        "pmp",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 161, 19, 3, 3, 1, 35], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "poweralert",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 850, 1, 1, 3, 3, 3, 1, 1, 1], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 850, 101, 1, 1, 1], indexed: true }, divisor: 10, multiplier: 10 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 850, 1, 1, 3, 1, 2, 1, 1, 7], indexed: true }, divisor: 10, multiplier: 1 },
            ],
        },
    ),
    (
        "powerwalker",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 935, 10, 1, 1, 2, 2], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 935, 10, 1, 1, 3, 8], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 935, 10, 1, 1, 3, 12, 1, 8], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 935, 10, 1, 1, 4, 2, 1, 5], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 935, 10, 1, 1, 6, 1], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "powerwalker-vfi",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 2, 1, 33, 1, 2, 7], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "primekey",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 22408, 1, 1, 2, 1, 3, 99, 112, 117, 1, 0], indexed: false }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "prismasdwan",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 50114, 10, 3, 10, 31, 1, 4], indexed: true }, divisor: 1000, multiplier: 1 },
            ],
        },
    ),
    (
        "procurve",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 11, 2, 14, 11, 5, 1, 9, 6, 1], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::Pool { used: OidColumn { base: &[1, 3, 6, 1, 4, 1, 11, 2, 14, 11, 5, 1, 1, 2, 1, 1, 1, 7, 0], indexed: false }, free: OidColumn { base: &[1, 3, 6, 1, 4, 1, 11, 2, 14, 11, 5, 1, 1, 2, 1, 1, 1, 6, 0], indexed: false }, precision: 1 },
                MemRule::Pool { used: OidColumn { base: &[1, 3, 6, 1, 4, 1, 11, 2, 14, 11, 5, 1, 1, 2, 2, 1, 1, 7, 0], indexed: false }, free: OidColumn { base: &[1, 3, 6, 1, 4, 1, 11, 2, 14, 11, 5, 1, 1, 2, 2, 1, 1, 6, 0], indexed: false }, precision: 1 },
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 11, 2, 14, 11, 5, 1, 55, 1, 1, 1, 4], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 11, 2, 14, 11, 5, 1, 82, 1, 1, 1, 1, 11], indexed: true }, divisor: 1000, multiplier: 1 },
            ],
        },
    ),
    (
        "pulse",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 12532, 10], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::PercentTotal { percent: OidColumn { base: &[1, 3, 6, 1, 4, 1, 12532, 11, 0], indexed: false }, total: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2021, 4, 5, 0], indexed: false }, precision: 1024 },
            ],
            temp: &[
            ],
        },
    ),
    (
        "qnap",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 24681, 1, 4, 1, 1, 1, 1, 1, 2, 1, 7], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 24681, 1, 4, 1, 1, 1, 1, 4, 2], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 24681, 1, 4, 1, 1, 1, 1, 3, 2, 1, 6], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 24681, 1, 4, 1, 1, 1, 1, 5, 2, 1, 6], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "qtech",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 27514, 100, 1, 11, 10], indexed: true }, precision: 1 },
            ],
            mem: &[
            ],
            temp: &[
            ],
        },
    ),
    (
        "quanta",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
                MemRule::TotalFree { total: OidColumn { base: &[1, 3, 6, 1, 4, 1, 4413, 1, 1, 1, 1, 4, 2, 0], indexed: false }, free: OidColumn { base: &[1, 3, 6, 1, 4, 1, 4413, 1, 1, 1, 1, 4, 1, 0], indexed: false }, precision: 1024 },
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 4413, 1, 1, 43, 1, 8, 1, 4], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "quantastor",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 39324, 1, 1, 48, 1, 1, 30], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2021, 13, 16, 2, 1, 3], indexed: true }, divisor: 1000, multiplier: 1 },
            ],
        },
    ),
    (
        "quantum",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 3764, 1, 1, 200, 200, 30, 1, 4], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "radlan",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 89, 1, 9], indexed: true }, precision: 1 },
            ],
            mem: &[
            ],
            temp: &[
            ],
        },
    ),
    (
        "raisecom",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 8886, 1, 1, 1, 3, 1, 2, 1, 2], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::TotalFree { total: OidColumn { base: &[1, 3, 6, 1, 4, 1, 8886, 1, 1, 3, 2, 1, 0], indexed: false }, free: OidColumn { base: &[1, 3, 6, 1, 4, 1, 8886, 1, 1, 3, 2, 2, 0], indexed: false }, precision: 1 },
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 8886, 1, 18, 2, 2, 1, 1, 2], indexed: true }, divisor: 1000, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 8886, 1, 1, 4, 2, 1], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "raisecom-ros",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 8886, 60, 1, 1, 1, 2, 1, 1, 1, 3], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::TotalFree { total: OidColumn { base: &[1, 3, 6, 1, 4, 1, 8886, 60, 1, 3, 1, 1, 1, 0], indexed: false }, free: OidColumn { base: &[1, 3, 6, 1, 4, 1, 8886, 60, 1, 3, 1, 1, 2, 0], indexed: false }, precision: 1 },
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 8886, 60, 18, 1, 2, 2, 1, 1, 2], indexed: true }, divisor: 1000, multiplier: 1 },
            ],
        },
    ),
    (
        "rajant",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 34861, 1, 2, 0], indexed: false }, divisor: 100, multiplier: 1 },
            ],
        },
    ),
    (
        "raritan-kvm",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 13742, 3, 1, 2], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::Percent { percent: OidColumn { base: &[1, 3, 6, 1, 4, 1, 13742, 3, 1, 1, 0], indexed: false } },
            ],
            temp: &[
            ],
        },
    ),
    (
        "raritan-pdu",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 13742, 6, 5, 5, 3, 1, 4], indexed: true }, divisor: 10, multiplier: 1 },
            ],
        },
    ),
    (
        "ray",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 33555, 1, 1, 5, 1], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::Percent { percent: OidColumn { base: &[1, 3, 6, 1, 4, 1, 33555, 1, 1, 5, 2, 0], indexed: false } },
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 33555, 1, 1, 4, 2], indexed: true }, divisor: 100, multiplier: 1 },
            ],
        },
    ),
    (
        "ray3",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 33555, 4, 1, 5, 1], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::Percent { percent: OidColumn { base: &[1, 3, 6, 1, 4, 1, 33555, 4, 1, 5, 2, 0], indexed: false } },
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 33555, 4, 1, 4, 2], indexed: true }, divisor: 100, multiplier: 1 },
            ],
        },
    ),
    (
        "rittal-cmc",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2606, 4, 2, 3, 5, 2, 1, 5], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2606, 4, 2, 4, 5, 2, 1, 5], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2606, 4, 2, 5, 5, 2, 1, 5], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2606, 4, 2, 6, 5, 2, 1, 5], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "riverbed",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 17163, 1, 1, 5, 1, 4], indexed: true }, precision: 1 },
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 17163, 1, 1, 2, 9], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "rnx-updu",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 55108, 1, 3, 2, 1, 4], indexed: true }, divisor: 10, multiplier: 1 },
            ],
        },
    ),
    (
        "ros",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 15004, 4, 2, 2, 3], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "routeros",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 14988, 1, 1, 19, 1, 1, 6], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 14988, 1, 1, 3, 100, 1, 3], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "ruckuswireless",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 25053, 1, 2, 1, 1, 1, 5, 58], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::Percent { percent: OidColumn { base: &[1, 3, 6, 1, 4, 1, 25053, 1, 2, 1, 1, 1, 5, 59, 0], indexed: false } },
            ],
            temp: &[
            ],
        },
    ),
    (
        "ruckuswireless-hotzone",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 25053, 1, 1, 11, 1, 1, 1, 1], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::Percent { percent: OidColumn { base: &[1, 3, 6, 1, 4, 1, 25053, 1, 1, 11, 1, 1, 1, 2, 0], indexed: false } },
            ],
            temp: &[
            ],
        },
    ),
    (
        "ruckuswireless-unleashed",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 25053, 1, 15, 1, 1, 1, 15, 13], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::Percent { percent: OidColumn { base: &[1, 3, 6, 1, 4, 1, 25053, 1, 15, 1, 1, 1, 15, 14, 0], indexed: false } },
            ],
            temp: &[
            ],
        },
    ),
    (
        "ruijie",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 4881, 1, 1, 10, 2, 36, 1, 2, 1, 5], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::Pool { used: OidColumn { base: &[1, 3, 6, 1, 4, 1, 4881, 1, 1, 10, 2, 35, 1, 1, 1, 7, 0], indexed: false }, free: OidColumn { base: &[1, 3, 6, 1, 4, 1, 4881, 1, 1, 10, 2, 35, 1, 1, 1, 8, 0], indexed: false }, precision: 1024 },
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 4881, 1, 1, 10, 2, 1, 1, 44, 1, 5], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 4881, 1, 1, 10, 2, 105, 1, 1, 1, 17], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "rutos",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 48690, 2, 9], indexed: true }, divisor: 10, multiplier: 1 },
            ],
        },
    ),
    (
        "rutos-2xx",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 48690, 2, 9], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 48690, 2, 2, 1, 17], indexed: true }, divisor: 10, multiplier: 1 },
            ],
        },
    ),
    (
        "rutos-otd",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 48690, 2, 2, 1, 17], indexed: true }, divisor: 10, multiplier: 1 },
            ],
        },
    ),
    (
        "rutos-rutm",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 48690, 2, 2, 1, 17], indexed: true }, divisor: 10, multiplier: 1 },
            ],
        },
    ),
    (
        "rutos-rutx",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 48690, 2, 2, 1, 17], indexed: true }, divisor: 10, multiplier: 1 },
            ],
        },
    ),
    (
        "saf",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 7571, 100, 1, 1, 5, 1, 1, 1, 5, 0], indexed: false }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 7571, 100, 1, 1, 12, 3, 3], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 7571, 100, 1, 1, 12, 2, 2, 1, 10], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 7571, 100, 1, 1, 12, 2, 2, 1, 11], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "saf-cfm",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 7571, 100, 1, 1, 2, 22, 1, 28], indexed: true }, precision: 1 },
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 7571, 100, 1, 1, 2, 22, 1, 6], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 7571, 100, 1, 1, 2, 22, 3, 15], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 7571, 100, 1, 1, 2, 22, 4, 15], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "saf-integra-b",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 7571, 100, 1, 1, 7, 1, 4, 10], indexed: true }, precision: -10 },
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 7571, 100, 1, 1, 7, 1, 4, 2], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 7571, 100, 1, 1, 7, 1, 4, 8], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "saf-integra-e",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 7571, 100, 1, 1, 7, 9, 4, 10], indexed: true }, precision: -10 },
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 7571, 100, 1, 1, 7, 9, 4, 2], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 7571, 100, 1, 1, 7, 9, 4, 8], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "saf-integra-w",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 7571, 100, 1, 1, 7, 2, 4, 10], indexed: true }, precision: -10 },
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 7571, 100, 1, 1, 7, 2, 4, 2], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 7571, 100, 1, 1, 7, 2, 4, 8], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "saf-integra-x",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 7571, 100, 1, 1, 7, 10, 4, 10], indexed: true }, precision: -10 },
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 7571, 100, 1, 1, 7, 10, 4, 2], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 7571, 100, 1, 1, 7, 10, 4, 8], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "samlex",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 49075, 1, 3, 3, 8], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 49075, 1, 3, 3, 9], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "scalance",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 4329, 20, 1, 1, 1, 1, 79, 3, 1, 13], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::Percent { percent: OidColumn { base: &[1, 3, 6, 1, 4, 1, 4329, 20, 1, 1, 1, 1, 79, 3, 1, 13], indexed: true } },
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 4329, 20, 1, 1, 1, 1, 79, 7, 1, 16], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "screenos",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 3224, 16, 1, 3], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::Pool { used: OidColumn { base: &[1, 3, 6, 1, 4, 1, 3224, 16, 2, 1, 0], indexed: false }, free: OidColumn { base: &[1, 3, 6, 1, 4, 1, 3224, 16, 2, 2, 0], indexed: false }, precision: 1 },
            ],
            temp: &[
            ],
        },
    ),
    (
        "scs-ks",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 9839, 1, 2, 15], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 9839, 1, 2, 16], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 9839, 1, 2, 17], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 9839, 1, 2, 18], indexed: true }, divisor: 10, multiplier: 1 },
            ],
        },
    ),
    (
        "secureplatform",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2620, 1, 6, 7, 2, 4], indexed: true }, precision: 1 },
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2620, 1, 6, 7, 8, 1, 1, 3], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "sensatronics-em1",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 16174, 1, 1, 3, 3, 1, 3], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 16174, 1, 1, 3, 3, 2, 3], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 16174, 1, 1, 3, 3, 3, 3], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 16174, 1, 1, 3, 3, 4, 3], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "sensatronics-ittm",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 16174, 1, 1, 1, 3, 1, 2], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 16174, 1, 1, 1, 3, 2, 2], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 16174, 1, 1, 1, 3, 3, 2], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 16174, 1, 1, 1, 3, 4, 2], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 16174, 1, 1, 1, 3, 5, 2], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 16174, 1, 1, 1, 3, 6, 2], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 16174, 1, 1, 1, 3, 7, 2], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 16174, 1, 1, 1, 3, 8, 2], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 16174, 1, 1, 1, 3, 9, 2], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 16174, 1, 1, 1, 3, 10, 2], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 16174, 1, 1, 1, 3, 11, 2], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 16174, 1, 1, 1, 3, 12, 2], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 16174, 1, 1, 1, 3, 13, 2], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 16174, 1, 1, 1, 3, 14, 2], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 16174, 1, 1, 1, 3, 15, 2], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 16174, 1, 1, 1, 3, 16, 2], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "sentry3",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 1718, 3, 2, 5, 1, 6], indexed: true }, divisor: 10, multiplier: 1 },
            ],
        },
    ),
    (
        "sentry4",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 1718, 4, 1, 9, 3, 1, 1], indexed: true }, divisor: 10, multiplier: 1 },
            ],
        },
    ),
    (
        "seos",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2352, 2, 6, 1, 1], indexed: true }, precision: 1 },
            ],
            mem: &[
            ],
            temp: &[
            ],
        },
    ),
    (
        "serveriron",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
                MemRule::TotalFree { total: OidColumn { base: &[1, 3, 6, 1, 4, 1, 1991, 1, 1, 2, 1, 54, 0], indexed: false }, free: OidColumn { base: &[1, 3, 6, 1, 4, 1, 1991, 1, 1, 2, 1, 55, 0], indexed: false }, precision: 1 },
            ],
            temp: &[
            ],
        },
    ),
    (
        "sgos",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 3417, 2, 11, 2, 4, 1, 8], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::Pool { used: OidColumn { base: &[1, 3, 6, 1, 4, 1, 3417, 2, 11, 2, 3, 3, 0], indexed: false }, free: OidColumn { base: &[1, 3, 6, 1, 4, 1, 3417, 2, 11, 2, 3, 1, 0], indexed: false }, precision: 1 },
            ],
            temp: &[
            ],
        },
    ),
    (
        "siklu",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 31926, 1, 2], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "siteboss",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 3052, 12, 1, 1, 1, 1, 7], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "siteboss360",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 3052, 23, 1, 1, 1, 1, 7], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "siteboss550",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 3052, 10, 1, 1, 1, 1, 7], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "siteboss571",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 3052, 14, 1, 1, 1, 1, 7], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "siteboss572",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 3052, 20, 1, 1, 1, 1, 7], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "sitemonitor3",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 32050, 3, 2, 3, 1, 4], indexed: true }, divisor: 100, multiplier: 1 },
            ],
        },
    ),
    (
        "slms",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 5504, 3, 3, 3, 1, 5], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::TotalFree { total: OidColumn { base: &[1, 3, 6, 1, 4, 1, 5504, 3, 3, 3, 1, 3], indexed: true }, free: OidColumn { base: &[1, 3, 6, 1, 4, 1, 5504, 3, 3, 3, 1, 2], indexed: true }, precision: 1024 },
            ],
            temp: &[
            ],
        },
    ),
    (
        "slxos",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 1588, 2, 1, 1, 1, 26, 1], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::TotalFree { total: OidColumn { base: &[1, 3, 6, 1, 4, 1, 1588, 3, 1, 13, 1, 1, 1, 2, 0], indexed: false }, free: OidColumn { base: &[1, 3, 6, 1, 4, 1, 1588, 3, 1, 13, 1, 1, 1, 3, 0], indexed: false }, precision: 1024 },
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 1588, 2, 1, 1, 1, 1, 22, 1, 4], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "sm-os",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 3373, 1103, 74, 7, 1, 6], indexed: true }, divisor: 10, multiplier: 1 },
            ],
        },
    ),
    (
        "smartax",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2011, 2, 6, 7, 1, 1, 2, 1, 5], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::Percent { percent: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2011, 2, 6, 7, 1, 1, 2, 1, 6, 0], indexed: false } },
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2011, 6, 128, 1, 1, 2, 23, 1, 1], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "smartbyte",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 56166, 1, 2, 4, 1, 2], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 56166, 1, 8, 2, 1, 10], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "smartos",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 30826, 1, 4, 1, 1, 1, 3], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "snr",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 40418, 7, 100, 1, 8, 1, 11], indexed: true }, precision: 1 },
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 40418, 7, 100, 1, 11, 10], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::UsedTotal { total: OidColumn { base: &[1, 3, 6, 1, 4, 1, 40418, 7, 100, 1, 8, 1, 7, 0], indexed: false }, used: OidColumn { base: &[1, 3, 6, 1, 4, 1, 40418, 7, 100, 1, 8, 1, 8, 0], indexed: false }, precision: 1 },
                MemRule::UsedTotal { total: OidColumn { base: &[1, 3, 6, 1, 4, 1, 40418, 7, 100, 1, 11, 6, 0], indexed: false }, used: OidColumn { base: &[1, 3, 6, 1, 4, 1, 40418, 7, 100, 1, 11, 7, 0], indexed: false }, precision: 1 },
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 40418, 7, 100, 1, 8, 1, 10], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 40418, 7, 100, 1, 11, 9], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "snr-erd",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 40418, 2, 6, 1, 30, 1, 1, 3], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "snr-olt",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 65268, 1, 14, 2, 1, 3, 1], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::TotalFree { total: OidColumn { base: &[1, 3, 6, 1, 4, 1, 65268, 1, 2, 1, 1, 2, 12, 0], indexed: false }, free: OidColumn { base: &[1, 3, 6, 1, 4, 1, 65268, 1, 2, 1, 1, 2, 13, 0], indexed: false }, precision: 1048576 },
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 65268, 1, 14, 4, 4], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 65268, 1, 14, 2, 1, 3, 3], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 65268, 1, 14, 2, 1, 3, 4], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 65268, 1, 14, 2, 3, 3, 1, 12], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "socomec-ups",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 4555, 1, 1, 7, 1, 2, 6], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 4555, 1, 1, 7, 1, 2, 7], indexed: true }, divisor: 10, multiplier: 1 },
            ],
        },
    ),
    (
        "solidserver",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2440, 1, 14, 3, 6], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2440, 1, 14, 3, 4], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2440, 1, 14, 3, 1], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2440, 1, 14, 3, 5], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "sonicwall",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
                MemRule::Percent { percent: OidColumn { base: &[1, 3, 6, 1, 4, 1, 8741, 6, 2, 1, 5, 0], indexed: false } },
                MemRule::Percent { percent: OidColumn { base: &[1, 3, 6, 1, 4, 1, 8741, 1, 3, 1, 4, 0], indexed: false } },
            ],
            temp: &[
            ],
        },
    ),
    (
        "sophos-xg",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2604, 5, 1, 9, 1, 0, 0], indexed: false }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2604, 5, 1, 9, 2, 0, 0], indexed: false }, divisor: 10, multiplier: 1 },
            ],
        },
    ),
    (
        "spectracool",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 26609, 2, 1, 5, 2], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 26609, 2, 1, 8, 2], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 26609, 2, 4, 1, 2], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 26609, 2, 4, 2, 2], indexed: true }, divisor: 10, multiplier: 1 },
            ],
        },
    ),
    (
        "srlinux",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 6527, 3, 1, 2, 1, 1, 1], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::Pool { used: OidColumn { base: &[1, 3, 6, 1, 4, 1, 6527, 3, 1, 2, 1, 1, 9, 0], indexed: false }, free: OidColumn { base: &[1, 3, 6, 1, 4, 1, 6527, 3, 1, 2, 1, 1, 10, 0], indexed: false }, precision: 1024 },
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 6527, 3, 1, 2, 2, 1, 8, 1, 18], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "stoneos",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28557, 2, 2, 1, 3], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::UsedTotal { total: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28557, 2, 2, 1, 4, 0], indexed: false }, used: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28557, 2, 2, 1, 5, 0], indexed: false }, precision: 1024 },
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 28557, 2, 28, 1, 2, 1, 3], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "stulz",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 29462, 10, 2, 1, 1, 1, 1, 1, 1, 1, 1170], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 29462, 10, 2, 1, 1, 1, 1, 1, 1, 1, 1193], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 29462, 10, 2, 1, 1, 1, 1, 1, 1, 1, 1192], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 29462, 10, 2, 1, 1, 1, 1, 1, 1, 1, 1196], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 29462, 10, 2, 1, 1, 1, 2, 1, 1, 1191], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 29462, 10, 2, 1, 1, 1, 1, 1, 1, 1, 1175], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 29462, 10, 2, 1, 1, 1, 2, 1, 1, 1202], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 29462, 10, 2, 1, 1, 1, 2, 1, 1, 1206], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 29462, 10, 2, 1, 1, 1, 2, 1, 1, 1207], indexed: true }, divisor: 10, multiplier: 1 },
            ],
        },
    ),
    (
        "supercap",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 2, 1, 555, 1, 1, 1, 2, 0], indexed: false }, divisor: 100, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 2, 1, 555, 1, 1, 1, 3, 0], indexed: false }, divisor: 100, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 2, 1, 555, 1, 1, 1, 15, 0], indexed: false }, divisor: 100, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 2, 1, 555, 1, 1, 1, 16, 0], indexed: false }, divisor: 100, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 2, 1, 555, 1, 1, 1, 28, 0], indexed: false }, divisor: 100, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 2, 1, 555, 1, 1, 1, 29, 0], indexed: false }, divisor: 100, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 2, 1, 555, 1, 1, 1, 41, 0], indexed: false }, divisor: 100, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 2, 1, 555, 1, 1, 1, 42, 0], indexed: false }, divisor: 100, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 2, 1, 555, 1, 1, 1, 54, 0], indexed: false }, divisor: 100, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 2, 1, 555, 1, 1, 1, 55, 0], indexed: false }, divisor: 100, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 2, 1, 555, 1, 1, 1, 67, 0], indexed: false }, divisor: 100, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 2, 1, 555, 1, 1, 1, 68, 0], indexed: false }, divisor: 100, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 2, 1, 555, 1, 1, 1, 80, 0], indexed: false }, divisor: 100, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 2, 1, 555, 1, 1, 1, 81, 0], indexed: false }, divisor: 100, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 2, 1, 555, 1, 1, 1, 93, 0], indexed: false }, divisor: 100, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 2, 1, 555, 1, 1, 1, 94, 0], indexed: false }, divisor: 100, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 2, 1, 555, 1, 1, 1, 106, 0], indexed: false }, divisor: 100, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 2, 1, 555, 1, 1, 1, 107, 0], indexed: false }, divisor: 100, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 2, 1, 555, 1, 1, 1, 119, 0], indexed: false }, divisor: 100, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 2, 1, 555, 1, 1, 1, 120, 0], indexed: false }, divisor: 100, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 2, 1, 555, 1, 1, 1, 132, 0], indexed: false }, divisor: 100, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 2, 1, 555, 1, 1, 1, 133, 0], indexed: false }, divisor: 100, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 2, 1, 555, 1, 1, 1, 145, 0], indexed: false }, divisor: 100, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 2, 1, 555, 1, 1, 1, 146, 0], indexed: false }, divisor: 100, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 2, 1, 555, 1, 1, 1, 158, 0], indexed: false }, divisor: 100, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 2, 1, 555, 1, 1, 1, 159, 0], indexed: false }, divisor: 100, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 2, 1, 555, 1, 1, 1, 171, 0], indexed: false }, divisor: 100, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 2, 1, 555, 1, 1, 1, 172, 0], indexed: false }, divisor: 100, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 2, 1, 555, 1, 1, 1, 184, 0], indexed: false }, divisor: 100, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 2, 1, 555, 1, 1, 1, 185, 0], indexed: false }, divisor: 100, multiplier: 1 },
            ],
        },
    ),
    (
        "supermicro-bmc",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 21317, 1, 14, 2, 1, 9], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 21317, 1, 14, 2, 1, 10], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 21317, 1, 3, 1, 2], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "swos",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 14988, 1, 1, 19, 1, 1, 6], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 14988, 1, 1, 3, 5], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 14988, 1, 1, 3, 6], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 14988, 1, 1, 3, 7], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 14988, 1, 1, 3, 10], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 14988, 1, 1, 3, 11], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 14988, 1, 1, 3, 100, 1, 3], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "tachyon",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 57344, 1, 3, 6], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::Percent { percent: OidColumn { base: &[1, 3, 6, 1, 4, 1, 57344, 1, 3, 7, 0], indexed: false } },
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 57344, 1, 3, 3], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 57344, 1, 2, 2, 1, 11], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 57344, 1, 2, 2, 1, 10], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "tailyn",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 5833, 2017, 3, 15, 1], indexed: true }, precision: 1 },
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 5833, 2017, 4, 12, 2, 1, 9], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 5833, 2017, 4, 11, 1], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "tait-infra93",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 3570, 2, 2, 2, 1, 2, 2, 6, 1], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "tait-tnadmin",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 3570, 3, 15, 2, 1, 1, 1, 6], indexed: true }, precision: 1 },
            ],
            mem: &[
            ],
            temp: &[
            ],
        },
    ),
    (
        "teldat",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2007, 4, 1, 2, 1, 2, 1, 1, 1], indexed: true }, precision: 1 },
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2007, 4, 1, 2, 1, 2, 1, 1, 2], indexed: true }, precision: 1 },
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2007, 4, 1, 2, 1, 2, 1, 1, 3], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::TotalFree { total: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2007, 4, 1, 2, 1, 1, 29, 1], indexed: false }, free: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2007, 4, 1, 2, 1, 1, 30, 1], indexed: false }, precision: 1 },
                MemRule::TotalFree { total: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2007, 4, 1, 2, 1, 1, 27, 2], indexed: false }, free: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2007, 4, 1, 2, 1, 1, 28, 2], indexed: false }, precision: 1 },
            ],
            temp: &[
            ],
        },
    ),
    (
        "televes-fs",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 23180, 2, 1, 1, 2, 2, 4, 8], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "terra-receivers",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 30631, 1, 8, 1, 1, 3], indexed: true }, precision: 1 },
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 30631, 1, 18, 1, 326, 3], indexed: true }, precision: 1 },
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 30631, 1, 17, 1, 586, 3], indexed: true }, precision: 1 },
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 30631, 1, 8, 1, 1, 4], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 30631, 1, 18, 1, 326, 4], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 30631, 1, 17, 1, 586, 4], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 30631, 1, 17, 1, 586, 5], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "timos",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 6527, 3, 1, 2, 1, 1, 1], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::Pool { used: OidColumn { base: &[1, 3, 6, 1, 4, 1, 6527, 3, 1, 2, 1, 1, 11, 0], indexed: false }, free: OidColumn { base: &[1, 3, 6, 1, 4, 1, 6527, 3, 1, 2, 1, 1, 10, 0], indexed: false }, precision: 1000 },
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 6527, 3, 1, 2, 2, 1, 8, 1, 18], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 6527, 3, 1, 2, 2, 4, 31, 1, 1], indexed: true }, divisor: 256, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 6527, 6, 1, 2, 2, 7, 1, 6, 1, 12], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 6527, 3, 1, 2, 2, 4, 66, 1, 2], indexed: true }, divisor: 256, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 6527, 3, 1, 2, 2, 3, 74, 1, 24], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 6527, 3, 1, 2, 2, 3, 74, 1, 25], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 6527, 3, 1, 2, 2, 3, 74, 1, 35], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 6527, 3, 1, 2, 2, 3, 74, 1, 36], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 6527, 3, 1, 2, 2, 3, 74, 1, 17], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 6527, 3, 1, 2, 2, 3, 74, 1, 18], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "tpdin",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 45621, 2, 2, 13], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 45621, 2, 2, 14], indexed: true }, divisor: 10, multiplier: 1 },
            ],
        },
    ),
    (
        "tpdin3",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 45621, 3, 3, 23], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 45621, 3, 3, 24], indexed: true }, divisor: 10, multiplier: 1 },
            ],
        },
    ),
    (
        "truenas-scale",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 50536, 3, 1, 3], indexed: true }, divisor: 1000, multiplier: 1 },
            ],
        },
    ),
    (
        "ucopia",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 31218, 3, 3], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 31218, 3, 4], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "uhp",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 8000, 22, 8, 2], indexed: true }, precision: 1 },
            ],
            mem: &[
            ],
            temp: &[
            ],
        },
    ),
    (
        "unifi",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
                MemRule::TotalFree { total: OidColumn { base: &[1, 3, 6, 1, 4, 1, 10002, 1, 1, 1, 1, 1, 0], indexed: false }, free: OidColumn { base: &[1, 3, 6, 1, 4, 1, 10002, 1, 1, 1, 1, 2, 0], indexed: false }, precision: 1024 },
            ],
            temp: &[
            ],
        },
    ),
    (
        "uniping",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 25728, 8800, 1, 1, 2], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 25728, 8400, 1, 1, 4], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "valere",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 13858, 2, 3, 5], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 13858, 7, 1, 1, 1, 3], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "vertiv-dcs",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 13400, 3, 3, 3031, 1, 1, 56], indexed: true }, divisor: 100, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 13400, 3, 3, 3031, 1, 1, 57], indexed: true }, divisor: 100, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 13400, 3, 3, 3031, 1, 1, 58], indexed: true }, divisor: 100, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 13400, 3, 3, 3031, 1, 1, 59], indexed: true }, divisor: 100, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 13400, 3, 4, 491, 1, 1, 67], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 13400, 3, 5, 541, 1, 1, 2], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 13400, 3, 5, 541, 1, 1, 3], indexed: true }, divisor: 10, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 13400, 3, 5, 541, 1, 1, 19], indexed: true }, divisor: 10, multiplier: 1 },
            ],
        },
    ),
    (
        "vertiv-ita2",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 13400, 2, 54, 2, 5, 8], indexed: true }, divisor: 1000, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 13400, 2, 54, 2, 5, 9], indexed: true }, divisor: 1000, multiplier: 1 },
            ],
        },
    ),
    (
        "vertiv-pdu",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 21239, 5, 2, 9, 1, 5], indexed: true }, divisor: 10, multiplier: 1 },
            ],
        },
    ),
    (
        "vigintos",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 27993, 5, 9, 1, 3], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "viprinux",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 35424, 1, 2, 1], indexed: true }, precision: 1 },
            ],
            mem: &[
            ],
            temp: &[
            ],
        },
    ),
    (
        "viptela",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 41916, 11, 1, 16], indexed: true }, precision: -1 },
            ],
            mem: &[
                MemRule::Pool { used: OidColumn { base: &[1, 3, 6, 1, 4, 1, 41916, 11, 1, 18, 0], indexed: false }, free: OidColumn { base: &[1, 3, 6, 1, 4, 1, 41916, 11, 1, 19, 0], indexed: false }, precision: 1024 },
            ],
            temp: &[
            ],
        },
    ),
    (
        "volius",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 34652, 2, 10, 6], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "vos",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 42359, 2, 2, 1, 1, 1, 1, 2], indexed: true }, precision: 1 },
            ],
            mem: &[
            ],
            temp: &[
            ],
        },
    ),
    (
        "voss",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2272, 1, 85, 10, 1, 1, 3], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::Pool { used: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2272, 1, 85, 10, 1, 1, 6, 0], indexed: false }, free: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2272, 1, 85, 10, 1, 1, 7, 0], indexed: false }, precision: 1024 },
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2272, 1, 101, 1, 1, 2, 1, 3], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "vrp",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2011, 5, 25, 31, 1, 1, 1, 1, 5], indexed: true }, precision: 1 },
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2011, 5, 25, 31, 1, 1, 3, 1, 5], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2011, 5, 25, 31, 1, 1, 1, 1, 11], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2011, 5, 25, 31, 1, 1, 8, 1, 6], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "vsolution",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 37950, 1, 1, 5, 10, 12, 3], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::Percent { percent: OidColumn { base: &[1, 3, 6, 1, 4, 1, 37950, 1, 1, 5, 10, 12, 4, 0], indexed: false } },
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 37950, 1, 1, 5, 10, 13, 1, 1, 2], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 37950, 1, 1, 5, 10, 12, 5, 9], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 37950, 1, 1, 5, 12, 2, 1, 8, 1, 3], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "vutlan",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 39052, 1, 3, 1, 9], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "webpower-smart2",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2468, 1, 2, 1, 1, 2, 7], indexed: true }, divisor: 10, multiplier: 1 },
            ],
        },
    ),
    (
        "wisi-tangram",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 7465, 20, 2, 9, 1, 3, 1, 2, 1, 3], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 7465, 20, 2, 9, 1, 3, 1, 6, 1, 8], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "witchos",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 48690, 1, 8], indexed: true }, precision: 1 },
            ],
            mem: &[
            ],
            temp: &[
            ],
        },
    ),
    (
        "wti-console",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2634, 1, 200, 10, 1, 3], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "wti-power",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2634, 3, 200, 10, 1, 3], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "wut",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 5040, 1, 2, 18, 1, 4, 1, 1], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 5040, 1, 2, 8, 1, 4, 1, 1], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 5040, 1, 2, 6, 1, 4, 1, 1], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 5040, 1, 2, 9, 1, 4, 1, 1], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 5040, 1, 2, 42, 1, 4, 1, 1], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "xclarity",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 19046, 11, 1, 1, 1, 2, 1, 3], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "xirrus_aos",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 21013, 1, 2, 28, 21, 1, 3], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "xos",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 1916, 1, 32, 1, 2], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::TotalFree { total: OidColumn { base: &[1, 3, 6, 1, 4, 1, 1916, 1, 32, 2, 2, 1, 2, 0], indexed: false }, free: OidColumn { base: &[1, 3, 6, 1, 4, 1, 1916, 1, 32, 2, 2, 1, 3, 0], indexed: false }, precision: 1024 },
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 1916, 1, 1, 1, 8], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "yunshan",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2011, 5, 25, 31, 1, 1, 1, 1, 5], indexed: true }, precision: 1 },
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2011, 5, 25, 31, 1, 1, 3, 1, 5], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2011, 5, 25, 31, 1, 1, 1, 1, 11], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 2011, 5, 25, 31, 1, 1, 8, 1, 6], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "zenitel-vsdevice",
        DiscoveryProfile {
            cpu: &[
            ],
            mem: &[
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 26122, 3, 2, 2, 1, 4], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "zxa10",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 3902, 1082, 10, 1, 2, 4, 1, 9], indexed: true }, precision: 1 },
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 3902, 3, 6002, 2, 1, 1, 7], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::PercentTotal { percent: OidColumn { base: &[1, 3, 6, 1, 4, 1, 3902, 1082, 10, 1, 2, 4, 1, 11, 0], indexed: false }, total: OidColumn { base: &[1, 3, 6, 1, 4, 1, 3902, 1082, 10, 1, 2, 4, 1, 19, 0], indexed: false }, precision: 1 },
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 3902, 1082, 10, 10, 2, 1, 6, 1, 2], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 3902, 3, 6002, 2, 5, 1, 9], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 3902, 3, 6002, 2, 4, 1, 3], indexed: true }, divisor: 1, multiplier: 1 },
            ],
        },
    ),
    (
        "zxr10",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 3902, 3, 3, 1, 1, 12], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::PercentTotal { percent: OidColumn { base: &[1, 3, 6, 1, 4, 1, 3902, 3, 3, 1, 1, 4, 0], indexed: false }, total: OidColumn { base: &[1, 3, 6, 1, 4, 1, 3902, 3, 3, 1, 1, 3, 0], indexed: false }, precision: 1 },
            ],
            temp: &[
            ],
        },
    ),
    (
        "zynos",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 890, 1, 5, 8, 20, 12, 7], indexed: true }, precision: 1 },
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 890, 1, 5, 8, 55, 12, 7], indexed: true }, precision: 1 },
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 890, 1, 5, 8, 47, 11, 7], indexed: true }, precision: 1 },
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 890, 1, 15, 3, 2, 7], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::Percent { percent: OidColumn { base: &[1, 3, 6, 1, 4, 1, 890, 1, 15, 3, 2, 5, 0], indexed: false } },
            ],
            temp: &[
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 890, 1, 15, 3, 26, 1, 2, 1, 3], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 890, 1, 5, 8, 47, 8, 2, 1, 2], indexed: true }, divisor: 1, multiplier: 1 },
                TempRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 890, 1, 5, 8, 47, 117, 2, 1, 7], indexed: true }, divisor: 100, multiplier: 1 },
            ],
        },
    ),
    (
        "zywall",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 890, 1, 6, 22, 1, 1], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::Percent { percent: OidColumn { base: &[1, 3, 6, 1, 4, 1, 890, 1, 6, 22, 1, 2, 0], indexed: false } },
                MemRule::Percent { percent: OidColumn { base: &[1, 3, 6, 1, 4, 1, 890, 1, 15, 3, 2, 6, 1], indexed: false } },
            ],
            temp: &[
            ],
        },
    ),
    (
        "zyxelnwa",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 890, 1, 6, 22, 1, 1], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::Percent { percent: OidColumn { base: &[1, 3, 6, 1, 4, 1, 890, 1, 6, 22, 1, 2, 0], indexed: false } },
            ],
            temp: &[
            ],
        },
    ),
    (
        "zyxelusg",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 890, 1, 15, 3, 2, 4], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::Percent { percent: OidColumn { base: &[1, 3, 6, 1, 4, 1, 890, 1, 15, 3, 2, 5, 0], indexed: false } },
                MemRule::Percent { percent: OidColumn { base: &[1, 3, 6, 1, 4, 1, 890, 1, 15, 3, 2, 6, 1], indexed: false } },
            ],
            temp: &[
            ],
        },
    ),
    (
        "zyxelwlc",
        DiscoveryProfile {
            cpu: &[
                CpuRule { column: OidColumn { base: &[1, 3, 6, 1, 4, 1, 890, 1, 15, 3, 2, 4], indexed: true }, precision: 1 },
            ],
            mem: &[
                MemRule::Percent { percent: OidColumn { base: &[1, 3, 6, 1, 4, 1, 890, 1, 15, 3, 2, 5, 0], indexed: false } },
                MemRule::Percent { percent: OidColumn { base: &[1, 3, 6, 1, 4, 1, 890, 1, 15, 3, 2, 6, 1], indexed: false } },
            ],
            temp: &[
            ],
        },
    ),
];
