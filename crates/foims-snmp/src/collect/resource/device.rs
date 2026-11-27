//! 设备信息执行器：model/serial/version 角色列（按序 GET，首个非空生效）。

use super::{CandidateFailure, ExecArgs, ExecOutput, get_one, varbind_text};
use crate::vendor::model;
use model::{CandidateKind, Column};

pub(super) async fn exec<R: crate::core::SnmpReader>(
    args: ExecArgs<'_, '_, R>,
) -> ExecOutput<super::DeviceReading> {
    let CandidateKind::Device(fields) = &args.candidate.kind else {
        return Err(CandidateFailure::Unsupported);
    };
    let model = read_first_text(args.reader, &fields.model).await;
    let serial = read_first_text(args.reader, &fields.serial).await;
    let version = read_first_text(args.reader, &fields.version).await;
    if model.is_none() && serial.is_none() && version.is_none() {
        return Err(CandidateFailure::Empty);
    }
    Ok((
        vec![super::DeviceReading {
            model,
            serial,
            version,
        }],
        None,
    ))
}

/// 角色列按序 GET，首个非空文本生效。
async fn read_first_text<R: crate::core::SnmpReader>(
    reader: &R,
    cols: &[Column],
) -> Option<String> {
    for col in cols {
        if let Some(value) = get_one(reader, &col.oid).await {
            let text = varbind_text(&async_snmp::VarBind::new(col.oid.clone(), value));
            if text.as_ref().is_some_and(|t| !t.is_empty()) {
                return text;
            }
        }
    }
    None
}
