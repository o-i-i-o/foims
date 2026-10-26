//! 房间（rooms）与房间子网绑定表结构创建。
//!
//! 房间是「组织 → 房间 → 工位/机柜/设备」链上的空间归属点：
//! - `org_id` 非空：每个房间必须落在唯一组织节点下（判定房间的责任归属与筛选口径）；
//! - `manager_employee_id` 非空：房间总有责任人，机房/弱电井这类无工位的空间
//!   由房间管理人兜底承接设备通知，办公空间可选配工位管理人细化到人；
//! - 管理人必须属于房间所属组织（由触发器 `trg_validate_room_manager_org` 保证，
//!   `foims-resource::helpers::ensure_employee_in_org` 提供同口径的应用层预检）。

pub async fn create(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS rooms (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            name VARCHAR(50) NOT NULL,
            room_type VARCHAR(20) NOT NULL DEFAULT 'OFFICE',
            org_id UUID NOT NULL REFERENCES organizations(id) ON DELETE RESTRICT,
            manager_employee_id UUID NOT NULL REFERENCES employees(id) ON DELETE RESTRICT,
            -- 房间总功率上限（瓦，可空=不限制）；房内设备功耗之和不得超过该值
            total_power_watts INTEGER
                CONSTRAINT chk_rooms_total_power CHECK (total_power_watts IS NULL OR total_power_watts >= 0),
            description TEXT,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            CONSTRAINT chk_room_type CHECK (room_type IN ('OFFICE', 'LOBBY', 'RECEPTION', 'DATA_CENTER', 'TELECOM_CLOSET', 'OTHER')),
            CONSTRAINT uq_rooms_name UNIQUE (name)
        )",
    )
    .execute(pool)
    .await?;

    // 按组织筛房间（递归子树过滤）与按管理人反查（删除员工/变更管理人）
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_rooms_org_id ON rooms(org_id)")
        .execute(pool)
        .await?;
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_rooms_manager_employee_id ON rooms(manager_employee_id)",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS room_networks (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            room_id UUID NOT NULL REFERENCES rooms(id) ON DELETE CASCADE,
            subnet_id UUID NOT NULL REFERENCES network_cidrs(id),
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            UNIQUE(room_id, subnet_id)
        )",
    )
    .execute(pool)
    .await?;

    Ok(())
}
