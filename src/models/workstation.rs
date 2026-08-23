//! 由 models.rs 按资源域拆分而来，字段与校验规则未变。

use super::ip::IpManager;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use validator::Validate;

// ==================== 工位模型 ====================

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct Workstation {
    pub id: Uuid,
    pub name: String,
    pub room_id: Uuid,
    pub room_name: Option<String>,
    pub manager: Option<String>,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone, sqlx::FromRow)]
pub struct WorkstationWithDetails {
    pub id: Uuid,
    pub name: String,
    pub room_id: Uuid,
    pub room_name: Option<String>,
    pub manager: Option<String>,
    /// IP 明细不从 SQL 映射（列表不携带、详情单独查询后手动填充）
    #[sqlx(skip)]
    pub ips: Vec<IpManager>,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct WorkstationCreate {
    #[validate(length(
        min = 1,
        max = 50,
        message = "server.workstation.validation.name_length"
    ))]
    pub name: String,
    pub room_id: Uuid,
    #[validate(length(max = 50, message = "server.workstation.validation.manager_length"))]
    pub manager: Option<String>,
    #[validate(length(max = 255, message = "server.common.validation.description_length"))]
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct WorkstationUpdate {
    #[validate(length(
        min = 1,
        max = 50,
        message = "server.workstation.validation.name_length"
    ))]
    pub name: Option<String>,
    pub room_id: Option<Uuid>,
    pub manager: Option<String>,
    #[validate(length(max = 255, message = "server.common.validation.description_length"))]
    pub description: Option<String>,
}

// ==================== 批量同步模型 ====================

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct WorkstationSyncItem {
    pub id: Option<Uuid>,
    #[validate(length(
        min = 1,
        max = 50,
        message = "server.workstation.validation.name_length"
    ))]
    pub name: String,
    #[validate(length(max = 50, message = "server.workstation.validation.manager_length"))]
    pub manager: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct CabinetSyncItem {
    pub id: Option<Uuid>,
    #[validate(length(min = 1, max = 50, message = "server.cabinet.validation.name_length"))]
    pub name: String,
    #[validate(range(
        min = 1,
        max = 48,
        message = "server.cabinet.validation.capacity_range"
    ))]
    pub capacity: i32,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct NetOutletSyncItem {
    pub id: Option<Uuid>,
    #[validate(length(
        min = 1,
        max = 100,
        message = "server.net_outlet.validation.name_length"
    ))]
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct RoomNetOutletsSync {
    pub net_outlets: Vec<NetOutletSyncItem>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NetOutletBrief {
    pub id: Uuid,
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PatchPanelBrief {
    pub id: Uuid,
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct PositionSyncItem {
    pub id: Option<Uuid>,
    #[validate(length(min = 1, max = 50, message = "server.position.validation.name_length"))]
    pub name: String,
    #[validate(range(
        min = 1,
        max = 48,
        message = "server.position.validation.start_u_range"
    ))]
    pub start_u: i32,
    #[validate(range(min = 1, max = 48, message = "server.position.validation.end_u_range"))]
    pub end_u: i32,
    #[validate(length(max = 255, message = "server.common.validation.description_length"))]
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct RoomChildrenSync {
    pub workstations: Option<Vec<WorkstationSyncItem>>,
    pub cabinets: Option<Vec<CabinetSyncItem>>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct CabinetPositionsSync {
    pub positions: Vec<PositionSyncItem>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct PatchPanelSyncItem {
    pub id: Option<Uuid>,
    #[validate(length(
        min = 1,
        max = 100,
        message = "server.patch_panel.validation.name_length"
    ))]
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct CabinetPatchPanelsSync {
    pub patch_panels: Vec<PatchPanelSyncItem>,
}

// ==================== 单元测试 ====================

#[cfg(test)]
mod tests {
    use super::*;
    use validator::Validate;

    // ---------- 工位 ----------

    #[test]
    fn test_workstation_create_valid() -> Result<(), serde_json::Error> {
        let req: WorkstationCreate = serde_json::from_value(serde_json::json!({
            "name": "研发-01",
            "room_id": Uuid::new_v4(),
            "manager": "张三",
            "description": "靠窗工位"
        }))?;
        assert!(req.validate().is_ok());
        Ok(())
    }

    #[test]
    fn test_workstation_create_invalid() -> Result<(), serde_json::Error> {
        // 空名称 / 超长负责人 / 超长描述同时拒绝
        let req: WorkstationCreate = serde_json::from_value(serde_json::json!({
            "name": "",
            "room_id": Uuid::new_v4(),
            "manager": "M".repeat(51),
            "description": "D".repeat(256)
        }))?;
        let Err(errors) = req.validate() else {
            panic!("非法工位应被拒绝");
        };
        assert!(errors.errors().contains_key("name"));
        assert!(errors.errors().contains_key("manager"));
        assert!(errors.errors().contains_key("description"));
        Ok(())
    }

    #[test]
    fn test_workstation_update_valid_and_invalid() -> Result<(), serde_json::Error> {
        let ok: WorkstationUpdate = serde_json::from_value(serde_json::json!({
            "name": "研发-02",
            "manager": null
        }))?;
        assert!(ok.validate().is_ok());
        assert_eq!(ok.manager, None);

        let bad: WorkstationUpdate = serde_json::from_value(serde_json::json!({ "name": "" }))?;
        let Err(errors) = bad.validate() else {
            panic!("空工位名应被拒绝");
        };
        assert!(errors.errors().contains_key("name"));
        Ok(())
    }

    // ---------- 批量同步项 ----------

    #[test]
    fn test_workstation_sync_item_validation() -> Result<(), serde_json::Error> {
        let ok: WorkstationSyncItem = serde_json::from_value(serde_json::json!({
            "id": Uuid::new_v4(),
            "name": "研发-03",
            "manager": "李四"
        }))?;
        assert!(ok.validate().is_ok());

        let bad: WorkstationSyncItem = serde_json::from_value(serde_json::json!({ "name": "" }))?;
        assert!(bad.validate().is_err());
        Ok(())
    }

    #[test]
    fn test_cabinet_sync_item_validation() -> Result<(), serde_json::Error> {
        let ok: CabinetSyncItem = serde_json::from_value(serde_json::json!({
            "name": "B 机柜",
            "capacity": 24
        }))?;
        assert!(ok.validate().is_ok());

        // 容量越界拒绝
        let bad: CabinetSyncItem =
            serde_json::from_value(serde_json::json!({ "name": "B 机柜", "capacity": 50 }))?;
        let Err(errors) = bad.validate() else {
            panic!("越界容量应被拒绝");
        };
        assert!(errors.errors().contains_key("capacity"));
        Ok(())
    }

    #[test]
    fn test_net_outlet_sync_item_validation() -> Result<(), serde_json::Error> {
        let ok: NetOutletSyncItem = serde_json::from_value(serde_json::json!({
            "name": "D201"
        }))?;
        assert!(ok.validate().is_ok());

        let bad: NetOutletSyncItem =
            serde_json::from_value(serde_json::json!({ "name": "N".repeat(101) }))?;
        let Err(errors) = bad.validate() else {
            panic!("超长信息点名应被拒绝");
        };
        assert!(errors.errors().contains_key("name"));
        Ok(())
    }

    #[test]
    fn test_position_sync_item_validation() -> Result<(), serde_json::Error> {
        let ok: PositionSyncItem = serde_json::from_value(serde_json::json!({
            "id": Uuid::new_v4(),
            "name": "U1-U2",
            "start_u": 1,
            "end_u": 2,
            "description": null
        }))?;
        assert!(ok.validate().is_ok());

        // 起始 U 越界拒绝
        let bad: PositionSyncItem = serde_json::from_value(serde_json::json!({
            "name": "机位",
            "start_u": 0,
            "end_u": 2
        }))?;
        let Err(errors) = bad.validate() else {
            panic!("越界 start_u 应被拒绝");
        };
        assert!(errors.errors().contains_key("start_u"));
        Ok(())
    }

    #[test]
    fn test_patch_panel_sync_item_validation() -> Result<(), serde_json::Error> {
        let ok: PatchPanelSyncItem = serde_json::from_value(serde_json::json!({
            "name": "配线架 1"
        }))?;
        assert!(ok.validate().is_ok());

        let bad: PatchPanelSyncItem = serde_json::from_value(serde_json::json!({ "name": "" }))?;
        assert!(bad.validate().is_err());
        Ok(())
    }

    // ---------- 批量同步载体 ----------

    #[test]
    fn test_room_net_outlets_sync() -> Result<(), serde_json::Error> {
        let req: RoomNetOutletsSync = serde_json::from_value(serde_json::json!({
            "net_outlets": [
                { "id": Uuid::new_v4(), "name": "D201" },
                { "name": "D202" }
            ]
        }))?;
        assert!(req.validate().is_ok());
        assert_eq!(req.net_outlets.len(), 2);
        Ok(())
    }

    #[test]
    fn test_room_children_sync_and_cabinet_positions_sync() -> Result<(), serde_json::Error> {
        // 子项全部合法 → 通过
        let ok: RoomChildrenSync = serde_json::from_value(serde_json::json!({
            "workstations": [{ "name": "研发-01" }],
            "cabinets": [{ "name": "A 柜", "capacity": 42 }]
        }))?;
        assert!(ok.validate().is_ok());

        // 容器自身字段（Vec 容量等）无校验规则，空对象也可通过
        let empty: RoomChildrenSync = serde_json::from_value(serde_json::json!({}))?;
        assert!(empty.validate().is_ok());
        assert!(empty.workstations.is_none());

        let positions: CabinetPositionsSync = serde_json::from_value(serde_json::json!({
            "positions": [{ "name": "U1", "start_u": 1, "end_u": 1 }]
        }))?;
        assert!(positions.validate().is_ok());

        let panels: CabinetPatchPanelsSync = serde_json::from_value(serde_json::json!({
            "patch_panels": [{ "name": "配线架 1" }]
        }))?;
        assert!(panels.validate().is_ok());
        Ok(())
    }

    // ---------- 实体序列化往返 ----------

    #[test]
    fn test_workstation_entity_serde_roundtrip() -> Result<(), serde_json::Error> {
        let ws = Workstation {
            id: Uuid::new_v4(),
            name: "研发-01".to_string(),
            room_id: Uuid::new_v4(),
            room_name: Some("301".to_string()),
            manager: Some("张三".to_string()),
            description: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let first = serde_json::to_value(&ws)?;
        let back: Workstation = serde_json::from_value(first.clone())?;
        let second = serde_json::to_value(&back)?;
        assert_eq!(first, second);
        Ok(())
    }

    #[test]
    fn test_workstation_with_details_serde_roundtrip() -> Result<(), serde_json::Error> {
        let details = WorkstationWithDetails {
            id: Uuid::new_v4(),
            name: "研发-01".to_string(),
            room_id: Uuid::new_v4(),
            room_name: None,
            manager: None,
            // ips 为 sqlx(skip) 字段，serde 往返仍保留
            ips: Vec::new(),
            description: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let first = serde_json::to_value(&details)?;
        let back: WorkstationWithDetails = serde_json::from_value(first.clone())?;
        let second = serde_json::to_value(&back)?;
        assert_eq!(first, second);
        Ok(())
    }
}
