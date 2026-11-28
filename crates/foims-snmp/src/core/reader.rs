//! SNMP 读操作抽象：探测/采集链路的唯一 IO 出口（docs/SNMP模块重构方案.md §5.6）。
//!
//! 生产实现为 async_snmp UDP 客户端；测试实现为内存 OID 快照，
//! 探测与资源解释全链路可离线验证。置于 core 使 collect 与 live 两个
//! 方向都能引用，避免 collect 依赖 live。

use std::future::Future;

use async_snmp::{Client, Oid, VarBind, transport::UdpHandle};

use crate::core::format_snmp_error;

/// SNMP 读操作抽象。
///
/// 方法用 RPITIT 而非 `async fn` 声明：显式 `+ Send` 保证返回的 Future
/// 可跨线程（agent-service 采集任务在多线程 runtime 上调度）。
pub trait SnmpReader {
    /// 多 OID 精确 GET：请求/响应等长有序，任一 OID 失败即整体 Err。
    fn get_many(&self, oids: &[Oid]) -> impl Future<Output = Result<Vec<VarBind>, String>> + Send;
    /// walk 子树：返回 base 子树内全部 varbind，空子树返回空集。
    fn walk(&self, base: &Oid) -> impl Future<Output = Vec<VarBind>> + Send;
}

impl SnmpReader for Client<UdpHandle> {
    async fn get_many(&self, oids: &[Oid]) -> Result<Vec<VarBind>, String> {
        Client::get_many(self, oids)
            .await
            .map(|resp| resp.varbinds)
            .map_err(|e| format_snmp_error(e).log_string())
    }

    async fn walk(&self, base: &Oid) -> Vec<VarBind> {
        crate::collect::metrics::walk_all(self, base.clone()).await
    }
}

/// 容忍缺失 OID 的批量 GET：先按批请求，失败（典型为 SNMPv1 任一 OID
/// 不存在导致整包 `noSuchName`）或响应长度不符时，退化为逐 OID GET，
/// 只保留成功且非异常的 varbind。
///
/// 识别/系统组等「多字段但可部分缺失」的场景必须使用本函数；纯批量性能
/// 路径（实时采样分块）仍直连 `get_many`，由调用方按块统计失败。
pub async fn get_many_tolerant<R: SnmpReader>(reader: &R, oids: &[Oid]) -> Vec<VarBind> {
    if let Ok(varbinds) = reader.get_many(oids).await
        && varbinds.len() == oids.len()
    {
        return varbinds;
    }

    tracing::debug!("SNMP 批量 GET 失败，回退逐 OID（{} 个）", oids.len());
    let mut out = Vec::with_capacity(oids.len());
    for oid in oids {
        if let Ok(mut varbinds) = reader.get_many(std::slice::from_ref(oid)).await
            && let Some(varbind) = varbinds.pop()
            && !varbind.value.is_exception()
        {
            out.push(varbind);
        }
    }
    out
}
