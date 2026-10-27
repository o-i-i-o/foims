//! 工位（workstations）表结构创建。
//!
//! 工位是「组织 → 房间 → 工位 → 设备」链上的责任人载体：管理人可空（新建工位
//! 时未必已指定），一旦指定则必须与房间同属一个组织（触发器
//! `trg_validate_workstation_manager_org`）。

pub async fn create(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS workstations (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            name VARCHAR(50) NOT NULL,
            room_id UUID NOT NULL REFERENCES rooms(id) ON DELETE RESTRICT,
            manager_employee_id UUID REFERENCES employees(id) ON DELETE RESTRICT,
            description TEXT,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            CONSTRAINT uq_workstations_room_name UNIQUE (room_id, name)
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS workstation_layouts (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            workstation_id UUID NOT NULL REFERENCES workstations(id) ON DELETE CASCADE,
            room_id UUID NOT NULL REFERENCES rooms(id) ON DELETE CASCADE,
            x INTEGER NOT NULL DEFAULT 0,
            y INTEGER NOT NULL DEFAULT 0,
            width INTEGER NOT NULL DEFAULT 160,
            height INTEGER NOT NULL DEFAULT 160,
            rotation INTEGER NOT NULL DEFAULT 0,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            UNIQUE(workstation_id)
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_workstation_layouts_workstation_id ON workstation_layouts(workstation_id)",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_workstation_layouts_room_id ON workstation_layouts(room_id)",
    )
    .execute(pool)
    .await?;

    Ok(())
}
