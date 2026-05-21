use sqlx::Error;
use sqlx::PgPool;
use sqlx::Row;
use tracing::{info, warn};

pub async fn run(pool: &PgPool) -> Result<(), Error> {
    migrate_workstation_layouts_structure(pool).await?;
    migrate_element_layouts_structure(pool).await?;
    migrate_cabinet_layouts_add_room_id(pool).await?;
    migrate_workstation_layouts_cleanup_fk(pool).await?;
    Ok(())
}

async fn migrate_workstation_layouts_structure(pool: &PgPool) -> Result<(), Error> {
    let result = sqlx::query(
        "SELECT COUNT(*) as count FROM schema_migrations WHERE version = 'workstation_layouts_structure'",
    )
    .fetch_one(pool)
    .await?;

    let count: i64 = result.try_get("count").unwrap_or(0);

    if count == 0 {
        let table_exists = sqlx::query(
            "SELECT COUNT(*) as count FROM information_schema.tables 
             WHERE table_schema = 'public' AND table_name = 'workstation_layouts'",
        )
        .fetch_one(pool)
        .await?;

        let table_count: i64 = table_exists.try_get("count").unwrap_or(0);
        let needs_recreate = if table_count > 0 {
            let column_exists = sqlx::query(
                "SELECT COUNT(*) as count FROM information_schema.columns 
                 WHERE table_name = 'workstation_layouts' AND column_name = 'workstation_id'",
            )
            .fetch_one(pool)
            .await?;

            let col_count: i64 = column_exists.try_get("count").unwrap_or(0);

            if col_count > 0 {
                if let Err(e) = sqlx::query(
                    "CREATE TABLE IF NOT EXISTS workstation_layouts_backup AS SELECT * FROM workstation_layouts",
                )
                .execute(pool)
                .await
                {
                    warn!("创建 workstation_layouts 备份表失败: {}", e);
                }

                if let Err(e) = sqlx::query("DROP TABLE IF EXISTS workstation_layouts CASCADE")
                    .execute(pool)
                    .await
                {
                    warn!("删除旧 workstation_layouts 表失败: {}", e);
                }

                info!("workstation_layouts 旧表已删除，将重新创建新表结构");
                true
            } else {
                false
            }
        } else {
            info!("workstation_layouts 表不存在，将创建新表结构");
            true
        };

        if needs_recreate {
            sqlx::query(
                r"CREATE TABLE IF NOT EXISTS workstation_layouts (
                    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
                    room_id UUID NOT NULL REFERENCES rooms(id) ON DELETE CASCADE,
                    element_id UUID NOT NULL,
                    element_type VARCHAR(20) NOT NULL DEFAULT 'workstation',
                    x INTEGER NOT NULL DEFAULT 0,
                    y INTEGER NOT NULL DEFAULT 0,
                    width INTEGER NOT NULL DEFAULT 160,
                    height INTEGER NOT NULL DEFAULT 160,
                    rotation INTEGER NOT NULL DEFAULT 0,
                    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
                    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
                    UNIQUE(room_id, element_id)
                )",
            )
            .execute(pool)
            .await?;

            if let Err(e) = sqlx::query(
                "CREATE INDEX IF NOT EXISTS idx_workstation_layouts_room_id ON workstation_layouts(room_id)",
            )
            .execute(pool)
            .await
            {
                warn!("创建 idx_workstation_layouts_room_id 索引失败: {}", e);
            }

            if let Err(e) = sqlx::query(
                "CREATE INDEX IF NOT EXISTS idx_workstation_layouts_element_type ON workstation_layouts(element_type)",
            )
            .execute(pool)
            .await
            {
                warn!("创建 idx_workstation_layouts_element_type 索引失败: {}", e);
            }
        }

        sqlx::query(
            r"INSERT INTO schema_migrations (version, description) 
             VALUES ('workstation_layouts_structure', '重构workstation_layouts表结构')",
        )
        .execute(pool)
        .await?;
    }

    Ok(())
}

async fn migrate_element_layouts_structure(pool: &PgPool) -> Result<(), Error> {
    let result = sqlx::query(
        "SELECT COUNT(*) as count FROM schema_migrations WHERE version = 'element_layouts_structure'",
    )
    .fetch_one(pool)
    .await?;

    let count: i64 = result.try_get("count").unwrap_or(0);

    if count == 0 {
        sqlx::query(
            r"CREATE TABLE IF NOT EXISTS element_layouts (
                id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
                room_id UUID NOT NULL REFERENCES rooms(id) ON DELETE CASCADE,
                element_type VARCHAR(20) NOT NULL,
                x INTEGER NOT NULL DEFAULT 0,
                y INTEGER NOT NULL DEFAULT 0,
                width INTEGER NOT NULL DEFAULT 160,
                height INTEGER NOT NULL DEFAULT 160,
                rotation INTEGER NOT NULL DEFAULT 0,
                created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
                updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
                UNIQUE(room_id, element_type)
            )",
        )
        .execute(pool)
        .await?;

        sqlx::query(
            r"INSERT INTO schema_migrations (version, description) 
             VALUES ('element_layouts_structure', '创建element_layouts表存储非工位元素布局')",
        )
        .execute(pool)
        .await?;
    }

    Ok(())
}

async fn migrate_cabinet_layouts_add_room_id(pool: &PgPool) -> Result<(), Error> {
    let result = sqlx::query(
        "SELECT COUNT(*) as count FROM schema_migrations WHERE version = 'cabinet_layouts_add_room_id'",
    )
    .fetch_one(pool)
    .await?;

    let count: i64 = result.try_get("count").unwrap_or(0);

    if count == 0 {
        if let Err(e) = sqlx::query(
            "ALTER TABLE cabinet_layouts ADD COLUMN IF NOT EXISTS room_id UUID REFERENCES rooms(id) ON DELETE CASCADE"
        )
        .execute(pool)
        .await
        {
            warn!("添加 cabinet_layouts.room_id 列失败: {}", e);
        }

        if let Err(e) = sqlx::query(
            r"UPDATE cabinet_layouts cl
            SET room_id = (SELECT room_id FROM cabinets WHERE id = cl.cabinet_id)
            WHERE cl.room_id IS NULL",
        )
        .execute(pool)
        .await
        {
            warn!("填充 cabinet_layouts.room_id 失败: {}", e);
        }

        if let Err(e) = sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_cabinet_layouts_room_id ON cabinet_layouts(room_id)",
        )
        .execute(pool)
        .await
        {
            warn!("创建 idx_cabinet_layouts_room_id 索引失败: {}", e);
        }

        if let Err(e) = sqlx::query(
            r"DO $$
            BEGIN
                IF NOT EXISTS (
                    SELECT 1 FROM pg_constraint 
                    WHERE conname = 'cabinet_layouts_room_cabinet_key'
                ) THEN
                    ALTER TABLE cabinet_layouts 
                    ADD CONSTRAINT cabinet_layouts_room_cabinet_key 
                    UNIQUE (room_id, cabinet_id);
                END IF;
            END $$",
        )
        .execute(pool)
        .await
        {
            warn!("添加 room_id+cabinet_id 唯一约束失败: {}", e);
        }

        sqlx::query(
            r"INSERT INTO schema_migrations (version, description) VALUES ('cabinet_layouts_add_room_id', '为cabinet_layouts表添加room_id列，统一布局表筛选方式')"
        )
        .execute(pool)
        .await?;
    }

    Ok(())
}

async fn migrate_workstation_layouts_cleanup_fk(pool: &PgPool) -> Result<(), Error> {
    let result = sqlx::query(
        "SELECT COUNT(*) as count FROM schema_migrations WHERE version = 'workstation_layouts_cleanup_fk'",
    )
    .fetch_one(pool)
    .await?;

    let count: i64 = result.try_get("count").unwrap_or(0);

    if count == 0 {
        if let Err(e) = sqlx::query(
            "ALTER TABLE workstation_layouts DROP CONSTRAINT IF EXISTS workstation_layouts_workstation_id_fkey"
        )
        .execute(pool)
        .await
        {
            warn!("删除 workstation_layouts 冗余外键约束失败: {}", e);
        }

        if let Err(e) = sqlx::query(
            r"DO $$
            BEGIN
                IF NOT EXISTS (
                    SELECT 1 FROM pg_constraint 
                    WHERE conname = 'workstation_layouts_workstation_id_key'
                ) THEN
                    ALTER TABLE workstation_layouts 
                    ADD CONSTRAINT workstation_layouts_workstation_id_key 
                    UNIQUE (workstation_id);
                END IF;
            END $$",
        )
        .execute(pool)
        .await
        {
            warn!("添加 workstation_id 唯一约束失败: {}", e);
        }

        sqlx::query(
            r"INSERT INTO schema_migrations (version, description) 
             VALUES ('workstation_layouts_cleanup_fk', '删除workstation_layouts表冗余的外键约束，添加workstation_id唯一约束')"
        )
        .execute(pool)
        .await?;
    }

    Ok(())
}
