use sqlx::Row;
use tracing::{info, warn};
use uuid::Uuid;

pub async fn create_tables(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query("CREATE EXTENSION IF NOT EXISTS \"uuid-ossp\"")
        .execute(pool)
        .await?;

    create_schema_migrations_table(pool).await?;
    create_users_table(pool).await?;
    create_network_tables(pool).await?;
    create_room_tables(pool).await?;
    create_switch_tables(pool).await?;
    create_cabinet_tables(pool).await?;
    create_workstation_tables(pool).await?;
    create_ips_table(pool).await?;
    create_log_tables(pool).await?;
    create_token_tables(pool).await?;
    create_notification_tables(pool).await?;
    create_system_tables(pool).await?;

    create_indexes(pool).await?;
    create_views(pool).await?;
    create_triggers(pool).await?;

    Ok(())
}

async fn create_schema_migrations_table(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS schema_migrations (
            version VARCHAR(50) PRIMARY KEY,
            applied_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            description TEXT
        )",
    )
    .execute(pool)
    .await
    .map(|_| ())
}

async fn create_users_table(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS users (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            username VARCHAR(50) UNIQUE NOT NULL,
            password_hash VARCHAR(255) NOT NULL,
            email VARCHAR(100) UNIQUE NOT NULL,
            role VARCHAR(20) NOT NULL,
            status BOOLEAN NOT NULL DEFAULT TRUE,
            reset_token VARCHAR(255),
            reset_token_expiry TIMESTAMP WITH TIME ZONE,
            two_factor_secret VARCHAR(255),
            two_factor_enabled BOOLEAN NOT NULL DEFAULT FALSE,
            two_factor_verified BOOLEAN NOT NULL DEFAULT FALSE,
            two_factor_email_code VARCHAR(10),
            two_factor_email_code_expiry TIMESTAMP WITH TIME ZONE,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
        )",
    )
    .execute(pool)
    .await
    .map(|_| ())
}

async fn create_network_tables(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS network_regions (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            name VARCHAR(20) NOT NULL UNIQUE,
            description TEXT,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS network_cidrs (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            name VARCHAR(50) NOT NULL,
            network_region_id UUID NOT NULL REFERENCES network_regions(id),
            ipv4_cidr CIDR,
            ipv6_cidr CIDR,
            ipv4_gateway INET,
            ipv6_gateway INET,
            ipv4_dns INET[],
            ipv6_dns INET[],
            description TEXT,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
        )",
    )
    .execute(pool)
    .await
    .map(|_| ())
}

async fn create_room_tables(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS rooms (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            name VARCHAR(50) NOT NULL,
            room_type VARCHAR(20) NOT NULL DEFAULT 'OFFICE',
            description TEXT,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            CONSTRAINT chk_room_type CHECK (room_type IN ('OFFICE', 'DATA_CENTER'))
        )",
    )
    .execute(pool)
    .await?;

    if let Err(e) = sqlx::query("ALTER TABLE rooms ADD CONSTRAINT uq_rooms_name UNIQUE (name)")
        .execute(pool)
        .await
    {
        warn!("rooms.name唯一约束可能已存在: {}", e);
    }

    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS room_networks (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            room_id UUID NOT NULL REFERENCES rooms(id) ON DELETE CASCADE,
            network_id UUID REFERENCES network_cidrs(id),
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            UNIQUE(room_id, network_id)
        )",
    )
    .execute(pool)
    .await?;

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

    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS cabinet_layouts (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            cabinet_id UUID NOT NULL REFERENCES cabinets(id) ON DELETE CASCADE,
            x INTEGER NOT NULL DEFAULT 0,
            y INTEGER NOT NULL DEFAULT 0,
            width INTEGER NOT NULL DEFAULT 160,
            height INTEGER NOT NULL DEFAULT 160,
            rotation INTEGER NOT NULL DEFAULT 0,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            UNIQUE(cabinet_id)
        )",
    )
    .execute(pool)
    .await
    .map(|_| ())
}

async fn create_cabinet_tables(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS cabinets (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            name VARCHAR(50) NOT NULL,
            room_id UUID REFERENCES rooms(id) ON DELETE SET NULL,
            capacity INTEGER NOT NULL DEFAULT 42,
            description TEXT,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
        )",
    )
    .execute(pool)
    .await?;

    if let Err(e) = sqlx::query(
        "ALTER TABLE cabinets ADD CONSTRAINT uq_cabinets_room_name UNIQUE (room_id, name)",
    )
    .execute(pool)
    .await
    {
        warn!("cabinets复合唯一约束可能已存在: {}", e);
    }

    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS positions (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            name VARCHAR(50) NOT NULL,
            cabinet_id UUID REFERENCES cabinets(id) ON DELETE CASCADE,
            start_u INTEGER NOT NULL DEFAULT 1,
            end_u INTEGER NOT NULL DEFAULT 1,
            device_type VARCHAR(20) DEFAULT 'cabinet_position',
            device_id UUID,
            description TEXT,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            CONSTRAINT chk_position_device_type CHECK (device_type IN ('cabinet_position', 'switch'))
        )",
    )
    .execute(pool)
    .await
    .map(|_| ())
}

async fn create_workstation_tables(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS workstations (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            name VARCHAR(50) NOT NULL,
            room_id UUID NOT NULL REFERENCES rooms(id),
            manager VARCHAR(50),
            description TEXT,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
        )",
    )
    .execute(pool)
    .await
    .map(|_| ())
}

async fn create_switch_tables(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS switches (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            name VARCHAR(100) NOT NULL,
            position_id UUID REFERENCES positions(id) ON DELETE SET NULL,
            model VARCHAR(100),
            vendor VARCHAR(50),
            location VARCHAR(100),
            snmp_version VARCHAR(3) DEFAULT 'v2c',
            snmp_community VARCHAR(64),
            snmp_username VARCHAR(22),
            snmp_auth_protocol VARCHAR(10),
            snmp_auth_password VARCHAR(100),
            snmp_priv_protocol VARCHAR(10),
            snmp_priv_password VARCHAR(100),
            snmp_port INTEGER DEFAULT 161,
            description TEXT,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS switch_ports (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            switch_id UUID NOT NULL REFERENCES switches(id) ON DELETE CASCADE,
            port_number VARCHAR(30) NOT NULL,
            port_name VARCHAR(50),
            port_type VARCHAR(20) DEFAULT 'access',
            vlan_id INTEGER,
            status VARCHAR(20) DEFAULT 'up',
            speed VARCHAR(20),
            description TEXT,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            UNIQUE(switch_id, port_number)
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS switch_macs (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            switch_id UUID NOT NULL REFERENCES switches(id) ON DELETE CASCADE,
            ip_address INET NOT NULL,
            mac_address VARCHAR(20) NOT NULL,
            interface VARCHAR(50),
            vlan_id INTEGER,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            UNIQUE(switch_id, ip_address)
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS switch_lldps (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            switch_id UUID NOT NULL REFERENCES switches(id) ON DELETE CASCADE,
            local_port VARCHAR(50) NOT NULL,
            neighbor_chassis_id VARCHAR(100),
            neighbor_port_id VARCHAR(100),
            neighbor_port_desc VARCHAR(255),
            neighbor_sys_name VARCHAR(255),
            neighbor_sys_desc TEXT,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            UNIQUE(switch_id, local_port)
        )",
    )
    .execute(pool)
    .await?;

    Ok(())
}

async fn create_ips_table(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS ips (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            workstation_id UUID REFERENCES workstations(id) ON DELETE SET NULL,
            position_id UUID REFERENCES positions(id) ON DELETE SET NULL,
            switch_port_id UUID REFERENCES switch_ports(id) ON DELETE SET NULL,
            device_type VARCHAR(20) NOT NULL,
            ip_address INET NOT NULL,
            ip_version SMALLINT NOT NULL DEFAULT 4,
            mac_address VARCHAR(20),
            hostname VARCHAR(100),
            status VARCHAR(20) NOT NULL DEFAULT 'active',
            last_seen TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            last_mac VARCHAR(20),
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            CONSTRAINT chk_device_type CHECK (device_type IN ('workstation', 'cabinet_position')),
            CONSTRAINT chk_device_consistency CHECK (
                (device_type = 'workstation' AND workstation_id IS NOT NULL AND position_id IS NULL) OR
                (device_type = 'cabinet_position' AND position_id IS NOT NULL AND workstation_id IS NULL)
            )
        )",
    )
    .execute(pool)
    .await?;
    Ok(())
}

async fn create_log_tables(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS operation_logs (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            user_id UUID NOT NULL REFERENCES users(id),
            action VARCHAR(100) NOT NULL,
            resource_type VARCHAR(50) NOT NULL,
            resource_id UUID NOT NULL,
            details JSONB NOT NULL DEFAULT '{}',
            result BOOLEAN NOT NULL,
            ip_address VARCHAR(50) NOT NULL,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS task_logs (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            task_name VARCHAR(100) NOT NULL,
            status VARCHAR(20) NOT NULL,
            details JSONB NOT NULL DEFAULT '{}',
            start_time TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            end_time TIMESTAMP WITH TIME ZONE,
            duration INTEGER
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS login_logs (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            username VARCHAR(50) NOT NULL,
            ip_address VARCHAR(50) NOT NULL,
            user_agent VARCHAR(255),
            success BOOLEAN NOT NULL,
            error_message VARCHAR(255),
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
        )",
    )
    .execute(pool)
    .await?;

    Ok(())
}

async fn create_token_tables(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS revoked_tokens (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            token_hash VARCHAR(255) NOT NULL,
            user_id UUID REFERENCES users(id),
            revoked_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            expiry TIMESTAMP WITH TIME ZONE NOT NULL
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS token_usage (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            token_hash VARCHAR(255) NOT NULL,
            user_id UUID REFERENCES users(id),
            ip_address VARCHAR(50) NOT NULL,
            user_agent VARCHAR(255),
            request_path VARCHAR(255) NOT NULL,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
        )",
    )
    .execute(pool)
    .await
    .map(|_| ())
}

async fn create_notification_tables(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS notifications (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            user_id UUID REFERENCES users(id),
            title VARCHAR(100) NOT NULL,
            content TEXT NOT NULL,
            notification_type VARCHAR(20) NOT NULL,
            read BOOLEAN NOT NULL DEFAULT FALSE,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
        )",
    )
    .execute(pool)
    .await
    .map(|_| ())
}

async fn create_system_tables(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS system_configs (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            config_type VARCHAR(50) NOT NULL,
            key VARCHAR(100) NOT NULL,
            value TEXT,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            UNIQUE(config_type, key)
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS scheduled_tasks (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            name VARCHAR(100) NOT NULL UNIQUE,
            task_type VARCHAR(50) NOT NULL,
            cron_expression VARCHAR(100) NOT NULL,
            enabled BOOLEAN NOT NULL DEFAULT TRUE,
            config JSONB DEFAULT '{}',
            last_run_at TIMESTAMP WITH TIME ZONE,
            next_run_at TIMESTAMP WITH TIME ZONE,
            last_result TEXT,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
        )",
    )
    .execute(pool)
    .await?;

    Ok(())
}

async fn create_indexes(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    let indexes: &[&str] = &[
        "CREATE INDEX IF NOT EXISTS idx_users_username ON users(username)",
        "CREATE INDEX IF NOT EXISTS idx_users_email ON users(email)",
        "CREATE INDEX IF NOT EXISTS idx_ips_workstation_id ON ips(workstation_id)",
        "CREATE INDEX IF NOT EXISTS idx_ips_position_id ON ips(position_id)",
        "CREATE INDEX IF NOT EXISTS idx_ips_switch_port_id ON ips(switch_port_id)",
        "CREATE INDEX IF NOT EXISTS idx_ips_ip_address ON ips(ip_address)",
        "CREATE INDEX IF NOT EXISTS idx_ips_mac_address ON ips(mac_address)",
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_ips_ip_unique ON ips(ip_address)",
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_ips_mac_unique ON ips(mac_address) WHERE mac_address IS NOT NULL AND mac_address != ''",
        "CREATE INDEX IF NOT EXISTS idx_ip_last_mac ON ips(last_mac)",
        "CREATE INDEX IF NOT EXISTS idx_operation_logs_user_id ON operation_logs(user_id)",
        "CREATE INDEX IF NOT EXISTS idx_operation_logs_created_at ON operation_logs(created_at)",
        "CREATE INDEX IF NOT EXISTS idx_login_logs_username ON login_logs(username)",
        "CREATE INDEX IF NOT EXISTS idx_login_logs_created_at ON login_logs(created_at)",
        "CREATE INDEX IF NOT EXISTS idx_revoked_tokens_token_hash ON revoked_tokens(token_hash)",
        "CREATE INDEX IF NOT EXISTS idx_revoked_tokens_user_id ON revoked_tokens(user_id)",
        "CREATE INDEX IF NOT EXISTS idx_revoked_tokens_expiry ON revoked_tokens(expiry)",
        "CREATE INDEX IF NOT EXISTS idx_token_usage_token_hash ON token_usage(token_hash)",
        "CREATE INDEX IF NOT EXISTS idx_token_usage_user_id ON token_usage(user_id)",
        "CREATE INDEX IF NOT EXISTS idx_token_usage_ip_address ON token_usage(ip_address)",
        "CREATE INDEX IF NOT EXISTS idx_token_usage_created_at ON token_usage(created_at)",
        "CREATE INDEX IF NOT EXISTS idx_notifications_user_id ON notifications(user_id)",
        "CREATE INDEX IF NOT EXISTS idx_notifications_created_at ON notifications(created_at)",
        "CREATE INDEX IF NOT EXISTS idx_switches_parent_switch_id ON switches(parent_switch_id)",
        "CREATE INDEX IF NOT EXISTS idx_switches_position_id ON switches(position_id)",
        "CREATE INDEX IF NOT EXISTS idx_switch_ports_switch_id ON switch_ports(switch_id)",
        "CREATE INDEX IF NOT EXISTS idx_switch_macs_switch_id ON switch_macs(switch_id)",
        "CREATE INDEX IF NOT EXISTS idx_switch_macs_ip_address ON switch_macs(ip_address)",
        "CREATE INDEX IF NOT EXISTS idx_switch_macs_mac_address ON switch_macs(mac_address)",
        "CREATE INDEX IF NOT EXISTS idx_switch_lldps_switch_id ON switch_lldps(switch_id)",
        "CREATE INDEX IF NOT EXISTS idx_cabinets_room_id ON cabinets(room_id)",
        "CREATE INDEX IF NOT EXISTS idx_positions_cabinet_id ON positions(cabinet_id)",
        "CREATE INDEX IF NOT EXISTS idx_positions_device_type ON positions(device_type)",
        "CREATE INDEX IF NOT EXISTS idx_positions_device_id ON positions(device_id)",
        "CREATE INDEX IF NOT EXISTS idx_workstations_room_id ON workstations(room_id)",
        "CREATE INDEX IF NOT EXISTS idx_room_networks_room_id ON room_networks(room_id)",
        "CREATE INDEX IF NOT EXISTS idx_room_networks_network_id ON room_networks(network_id)",
        "CREATE INDEX IF NOT EXISTS idx_workstation_layouts_room_id ON workstation_layouts(room_id)",
        "CREATE INDEX IF NOT EXISTS idx_workstation_layouts_element_type ON workstation_layouts(element_type)",
        "CREATE INDEX IF NOT EXISTS idx_cabinet_layouts_cabinet_id ON cabinet_layouts(cabinet_id)",
        "CREATE INDEX IF NOT EXISTS idx_scheduled_tasks_name ON scheduled_tasks(name)",
        "CREATE INDEX IF NOT EXISTS idx_scheduled_tasks_enabled ON scheduled_tasks(enabled)",
    ];

    for idx in indexes {
        if let Err(e) = sqlx::query(idx).execute(pool).await {
            warn!("索引创建失败（可能已存在）: {}", e);
        }
    }

    run_migrations(pool).await?;

    Ok(())
}

pub async fn run_migrations_only(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    create_schema_migrations_table(pool).await?;
    run_migrations(pool).await?;
    Ok(())
}

async fn run_migrations(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    migrate_switch_position_fields(pool).await?;
    migrate_switch_macs_ip_type(pool).await?;
    migrate_drop_wrong_ip_unique_index(pool).await?;
    migrate_drop_switch_cabinet_fields(pool).await?;
    migrate_log_cleanup_tasks(pool).await?;
    migrate_log_table_partitions(pool).await?;
    migrate_switch_position_link(pool).await?;
    migrate_switch_position_constraint(pool).await?;
    migrate_position_device_type(pool).await?;
    migrate_drop_switch_cabinet_columns(pool).await?;
    migrate_ips_network_indirect(pool).await?;
    migrate_ips_drop_network_id(pool).await?;
    migrate_ips_device_type_cleanup(pool).await?;
    migrate_ips_drop_network_id_v2(pool).await?;
    migrate_ip_with_details_network_match(pool).await?;
    migrate_workstation_layouts_structure(pool).await?;
    migrate_switch_parent_columns_removal(pool).await?;
    migrate_switch_view_add_room(pool).await?;
    Ok(())
}

async fn migrate_switch_position_fields(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    let result = sqlx::query(
        "SELECT COUNT(*) as count FROM schema_migrations WHERE version = 'switch_position_fields'",
    )
    .fetch_one(pool)
    .await?;

    let count: i64 = result.try_get("count").unwrap_or(0);

    if count == 0 {
        sqlx::query(
            r"INSERT INTO schema_migrations (version, description) VALUES ('switch_position_fields', 'switches表通过position_id关联positions获取位置信息，不再使用cabinet_id/start_u/end_u字段')"
        )
        .execute(pool)
        .await?;
    }

    Ok(())
}

async fn migrate_switch_view_add_room(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    let result = sqlx::query(
        "SELECT COUNT(*) as count FROM schema_migrations WHERE version = 'switch_view_add_room'",
    )
    .fetch_one(pool)
    .await?;

    let count: i64 = result.try_get("count").unwrap_or(0);

    if count == 0 {
        if let Err(e) = sqlx::query(
            r"CREATE OR REPLACE VIEW switches_with_details AS
            SELECT 
                s.id, s.name, s.model, s.vendor,
                s.location, s.snmp_version, 
                s.snmp_community,
                s.snmp_username, s.snmp_auth_protocol, 
                s.snmp_auth_password,
                s.snmp_priv_protocol, 
                s.snmp_priv_password,
                s.snmp_port,
                s.position_id,
                p.cabinet_id, c.name as cabinet_name,
                r.id as room_id, r.name as room_name,
                p.start_u, p.end_u,
                rn.network_id as position_network_id,
                n.network_region_id,
                s.description,
                'switch' as device_type,
                host(im.ip_address) as ip_address,
                im.mac_address,
                s.created_at, s.updated_at
            FROM switches s
            LEFT JOIN positions p ON s.position_id = p.id
            LEFT JOIN cabinets c ON p.cabinet_id = c.id
            LEFT JOIN rooms r ON c.room_id = r.id
            LEFT JOIN room_networks rn ON r.id = rn.room_id
            LEFT JOIN network_cidrs n ON rn.network_id = n.id
            LEFT JOIN LATERAL (
                SELECT ip_address, mac_address
                FROM ips 
                WHERE position_id = p.id
                LIMIT 1
            ) im ON true",
        )
        .execute(pool)
        .await
        {
            warn!("更新 switches_with_details 视图添加room字段失败: {}", e);
        }

        sqlx::query(
            r"INSERT INTO schema_migrations (version, description) VALUES ('switch_view_add_room', 'switches_with_details视图添加room_id和room_name字段')"
        )
        .execute(pool)
        .await?;
    }

    Ok(())
}

async fn migrate_switch_macs_ip_type(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    let result = sqlx::query(
        "SELECT data_type FROM information_schema.columns WHERE table_name = 'switch_macs' AND column_name = 'ip_address'",
    )
    .fetch_optional(pool)
    .await?;

    if let Some(row) = result {
        let data_type: String = row.try_get("data_type").unwrap_or_default();
        if data_type == "character varying"
            && let Err(e) = sqlx::query(
                "ALTER TABLE switch_macs ALTER COLUMN ip_address TYPE INET USING ip_address::INET",
            )
            .execute(pool)
            .await
        {
            warn!("switch_macs.ip_address类型迁移失败: {}", e);
        }
    }

    Ok(())
}

async fn migrate_drop_wrong_ip_unique_index(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    if let Err(e) = sqlx::query("DROP INDEX IF EXISTS idx_ips_ip_unique")
        .execute(pool)
        .await
    {
        warn!("删除错误的IP唯一索引失败: {}", e);
    }

    Ok(())
}

async fn migrate_drop_switch_cabinet_fields(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    let result = sqlx::query(
        "SELECT COUNT(*) as count FROM schema_migrations WHERE version = 'drop_switch_cabinet_fields'",
    )
    .fetch_one(pool)
    .await?;

    let count: i64 = result.try_get("count").unwrap_or(0);

    if count == 0 {
        if let Err(e) = sqlx::query(
            r"CREATE OR REPLACE VIEW switches_with_details AS
            SELECT 
                s.id, s.name, s.position_id, s.model, s.vendor,
                s.location, s.snmp_version, 
                s.snmp_community,
                s.snmp_username, s.snmp_auth_protocol, 
                s.snmp_auth_password,
                s.snmp_priv_protocol, 
                s.snmp_priv_password,
                s.snmp_port,
                s.parent_switch_id, ps.name as parent_switch_name,
                s.parent_port_id, pp.port_number as parent_port_number,
                p.cabinet_id, c.name as cabinet_name,
                p.start_u, p.end_u,
                s.description,
                'switch' as device_type,
                host(im.ip_address) as ip_address,
                im.mac_address,
                s.created_at, s.updated_at
            FROM switches s
            LEFT JOIN switches ps ON s.parent_switch_id = ps.id
            LEFT JOIN switch_ports pp ON s.parent_port_id = pp.id
            LEFT JOIN positions p ON s.position_id = p.id
            LEFT JOIN cabinets c ON p.cabinet_id = c.id
            LEFT JOIN LATERAL (
                SELECT ip_address, mac_address
                FROM ips 
                WHERE position_id = p.id
                LIMIT 1
            ) im ON true",
        )
        .execute(pool)
        .await
        {
            warn!("更新switches_with_details视图失败: {}", e);
        }

        sqlx::query(
            r"INSERT INTO schema_migrations (version, description) VALUES ('drop_switch_cabinet_fields', 'switches表cabinet_id/start_u/end_u通过ips+positions关联获取')"
        )
        .execute(pool)
        .await?;
    }

    Ok(())
}

async fn migrate_log_cleanup_tasks(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    let result = sqlx::query(
        "SELECT COUNT(*) as count FROM schema_migrations WHERE version = 'log_cleanup_tasks'",
    )
    .fetch_one(pool)
    .await?;

    let count: i64 = result.try_get("count").unwrap_or(0);

    if count == 0 {
        let tasks = [
            (
                "cleanup_operation_logs",
                "log_cleanup",
                "0 3 * * *",
                r#"{"retention_days": 30, "table": "operation_logs"}"#,
            ),
            (
                "cleanup_token_usage",
                "log_cleanup",
                "0 3 * * *",
                r#"{"retention_days": 90, "table": "token_usage"}"#,
            ),
            (
                "cleanup_revoked_tokens",
                "log_cleanup",
                "0 4 * * *",
                r#"{"retention_days": 0, "table": "revoked_tokens"}"#,
            ),
            (
                "cleanup_login_logs",
                "log_cleanup",
                "0 4 * * *",
                r#"{"retention_days": 60, "table": "login_logs"}"#,
            ),
        ];

        for (name, task_type, cron, config) in &tasks {
            if let Err(e) = sqlx::query(
                r"INSERT INTO scheduled_tasks (id, name, task_type, cron_expression, enabled, config, created_at, updated_at)
                   VALUES (uuid_generate_v4(), $1, $2, $3, TRUE, $4::JSONB, NOW(), NOW())
                   ON CONFLICT (name) DO NOTHING"
            )
            .bind(name)
            .bind(task_type)
            .bind(cron)
            .bind(config)
            .execute(pool)
            .await
            {
                warn!("创建日志清理任务 {} 失败: {}", name, e);
            }
        }

        sqlx::query(
            r"INSERT INTO schema_migrations (version, description) VALUES ('log_cleanup_tasks', '添加日志表定期清理任务')"
        )
        .execute(pool)
        .await?;
    }

    Ok(())
}

async fn migrate_log_table_partitions(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    let result = sqlx::query(
        "SELECT COUNT(*) as count FROM schema_migrations WHERE version = 'log_table_partitions'",
    )
    .fetch_one(pool)
    .await?;

    let count: i64 = result.try_get("count").unwrap_or(0);

    if count == 0 {
        let partition_tables = [
            ("operation_logs", "created_at", "month"),
            ("login_logs", "created_at", "month"),
            ("token_usage", "created_at", "month"),
        ];

        for (table_name, partition_col, _interval) in &partition_tables {
            let is_partitioned: bool =
                sqlx::query_scalar("SELECT relispartition FROM pg_class WHERE relname = $1")
                    .bind(*table_name)
                    .fetch_optional(pool)
                    .await
                    .unwrap_or(None)
                    .unwrap_or(false);

            if !is_partitioned
                && let Err(e) = sqlx::query(&format!(
                    "ALTER TABLE {table_name} PARTITION BY RANGE ({partition_col})"
                ))
                .execute(pool)
                .await
            {
                warn!(
                    "表 {} 分区设置失败（可能不支持或已有数据）: {}",
                    table_name, e
                );
            }
        }

        sqlx::query(
            r"INSERT INTO schema_migrations (version, description) VALUES ('log_table_partitions', '日志表按时间分区（RANGE by month）')"
        )
        .execute(pool)
        .await?;
    }

    Ok(())
}

async fn migrate_switch_position_link(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    let result = sqlx::query(
        "SELECT COUNT(*) as count FROM schema_migrations WHERE version = 'switch_position_link'",
    )
    .fetch_one(pool)
    .await?;

    let count: i64 = result.try_get("count").unwrap_or(0);

    if count == 0 {
        let rows = sqlx::query(
            r"SELECT s.id, s.name, s.cabinet_id, s.start_u, s.end_u, s.description
               FROM switches s
               WHERE s.cabinet_id IS NOT NULL
               AND NOT EXISTS (
                   SELECT 1 FROM ips im WHERE im.position_id IN (SELECT p.id FROM positions p WHERE p.device_id = s.id)
               )"
        )
        .fetch_all(pool)
        .await
        .unwrap_or_default();

        for row in &rows {
            let switch_id: Uuid = row.get("id");
            let name: String = row.get("name");
            let cabinet_id: Option<Uuid> = row.get("cabinet_id");
            let start_u: Option<i32> = row.get("start_u");
            let end_u: Option<i32> = row.get("end_u");
            let description: Option<String> = row.get("description");

            let pos_id: Uuid = if let Ok(Some(existing_id)) = sqlx::query_scalar::<_, Uuid>(
                "SELECT id FROM positions WHERE name = $1 AND cabinet_id = $2",
            )
            .bind(&name)
            .bind(cabinet_id)
            .fetch_optional(pool)
            .await
            {
                existing_id
            } else {
                let new_id = Uuid::new_v4();
                if let Err(e) = sqlx::query(
                    "INSERT INTO positions (id, name, cabinet_id, start_u, end_u, description, device_type, device_id, created_at, updated_at) VALUES ($1, $2, $3, $4, $5, $6, 'switch', $7, NOW(), NOW())"
                )
                .bind(new_id)
                .bind(&name)
                .bind(cabinet_id)
                .bind(start_u.unwrap_or(1i32))
                .bind(end_u.unwrap_or(1i32))
                .bind(&description)
                .bind(switch_id)
                .execute(pool)
                .await
                {
                    warn!("为交换机 {} 创建关联机位失败: {}", switch_id, e);
                    continue;
                }
                new_id
            };

            if let Err(e) = sqlx::query(
                "UPDATE ips SET position_id = $1 WHERE position_id IS NULL AND position_id IN (SELECT p.id FROM positions p WHERE p.device_id = $2)"
            )
            .bind(pos_id)
            .bind(switch_id)
            .execute(pool)
            .await
            {
                warn!("关联交换机 {} 的IP到机位失败: {}", switch_id, e);
            }
        }

        sqlx::query(
            r"INSERT INTO schema_migrations (version, description) VALUES ('switch_position_link', '为有cabinet_id但无position关联的交换机创建position记录并关联ips')"
        )
        .execute(pool)
        .await?;
    }

    Ok(())
}

async fn migrate_switch_position_constraint(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    let result = sqlx::query(
        "SELECT COUNT(*) as count FROM schema_migrations WHERE version = 'switch_position_constraint'",
    )
    .fetch_one(pool)
    .await?;

    let count: i64 = result.try_get("count").unwrap_or(0);

    if count == 0 {
        sqlx::query("ALTER TABLE ips DROP CONSTRAINT IF EXISTS chk_device_consistency")
            .execute(pool)
            .await?;

        sqlx::query(
            r"ALTER TABLE ips ADD CONSTRAINT chk_device_consistency CHECK (
                (device_type = 'workstation' AND workstation_id IS NOT NULL AND position_id IS NULL) OR
                (device_type = 'cabinet_position' AND position_id IS NOT NULL AND workstation_id IS NULL)
            )"
        )
        .execute(pool)
        .await?;

        let switches_without_position = sqlx::query(
            r"SELECT s.id, s.name, s.cabinet_id, s.start_u, s.end_u, s.description
               FROM switches s
               WHERE s.cabinet_id IS NOT NULL
               AND NOT EXISTS (
                   SELECT 1 FROM positions p WHERE p.device_id = s.id AND p.device_type = 'switch'
               )",
        )
        .fetch_all(pool)
        .await
        .unwrap_or_default();

        for row in &switches_without_position {
            let switch_id: Uuid = row.get("id");
            let name: String = row.get("name");
            let cabinet_id: Option<Uuid> = row.get("cabinet_id");
            let start_u: Option<i32> = row.get("start_u");
            let end_u: Option<i32> = row.get("end_u");
            let description: Option<String> = row.get("description");

            let pos_id: Uuid = if let Ok(Some(existing_id)) = sqlx::query_scalar::<_, Uuid>(
                "SELECT id FROM positions WHERE name = $1 AND cabinet_id = $2",
            )
            .bind(&name)
            .bind(cabinet_id)
            .fetch_optional(pool)
            .await
            {
                existing_id
            } else {
                let new_id = Uuid::new_v4();
                if let Err(e) = sqlx::query(
                    "INSERT INTO positions (id, name, cabinet_id, start_u, end_u, description, device_type, device_id, created_at, updated_at) VALUES ($1, $2, $3, $4, $5, $6, 'switch', $7, NOW(), NOW())"
                )
                .bind(new_id)
                .bind(&name)
                .bind(cabinet_id)
                .bind(start_u.unwrap_or(1i32))
                .bind(end_u.unwrap_or(1i32))
                .bind(&description)
                .bind(switch_id)
                .execute(pool)
                .await
                {
                    warn!("为交换机 {} 创建关联机位失败: {}", switch_id, e);
                    continue;
                }
                new_id
            };

            if let Err(e) = sqlx::query(
                "UPDATE ips SET position_id = $1 WHERE position_id IN (SELECT p.id FROM positions p WHERE p.device_id = $2)"
            )
            .bind(pos_id)
            .bind(switch_id)
            .execute(pool)
            .await
            {
                warn!("关联交换机 {} 的IP到机位失败: {}", switch_id, e);
            }
        }

        sqlx::query(
            r"INSERT INTO schema_migrations (version, description) VALUES ('switch_position_constraint', '更新chk_device_consistency约束允许switch类型有position_id，并为现有交换机创建position关联')"
        )
        .execute(pool)
        .await?;
    }

    Ok(())
}

async fn migrate_position_device_type(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    let result = sqlx::query(
        "SELECT COUNT(*) as count FROM schema_migrations WHERE version = 'position_device_type'",
    )
    .fetch_one(pool)
    .await?;

    let count: i64 = result.try_get("count").unwrap_or(0);

    if count == 0 {
        if let Err(e) = sqlx::query(
            "ALTER TABLE positions ADD COLUMN IF NOT EXISTS device_type VARCHAR(20) DEFAULT 'cabinet_position'"
        )
        .execute(pool)
        .await
        {
            warn!("positions添加device_type列失败: {}", e);
        }

        if let Err(e) = sqlx::query("ALTER TABLE positions ADD COLUMN IF NOT EXISTS device_id UUID")
            .execute(pool)
            .await
        {
            warn!("positions添加device_id列失败: {}", e);
        }

        if let Err(e) = sqlx::query(
            r"ALTER TABLE positions ADD CONSTRAINT chk_position_device_type 
               CHECK (device_type IN ('cabinet_position', 'switch'))",
        )
        .execute(pool)
        .await
        {
            warn!("positions添加device_type约束失败: {}", e);
        }

        if let Err(e) = sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_positions_device_type ON positions(device_type)",
        )
        .execute(pool)
        .await
        {
            warn!("创建positions.device_type索引失败: {}", e);
        }

        if let Err(e) = sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_positions_device_id ON positions(device_id)",
        )
        .execute(pool)
        .await
        {
            warn!("创建positions.device_id索引失败: {}", e);
        }

        let switches = sqlx::query(
            r"SELECT s.id as switch_id, s.name, s.cabinet_id, s.start_u, s.end_u, s.description
               FROM switches s
               WHERE s.cabinet_id IS NOT NULL",
        )
        .fetch_all(pool)
        .await
        .unwrap_or_default();

        for row in &switches {
            let switch_id: Uuid = row.get("switch_id");
            let name: String = row.get("name");
            let cabinet_id: Option<Uuid> = row.get("cabinet_id");
            let start_u: Option<i32> = row.get("start_u");
            let end_u: Option<i32> = row.get("end_u");
            let description: Option<String> = row.get("description");

            let existing_pos: Option<(Uuid, String)> = sqlx::query_as(
                "SELECT id, device_type FROM positions WHERE name = $1 AND cabinet_id = $2",
            )
            .bind(&name)
            .bind(cabinet_id)
            .fetch_optional(pool)
            .await
            .unwrap_or(None);

            if let Some((pos_id, device_type)) = existing_pos {
                if device_type != "switch"
                    && let Err(e) = sqlx::query(
                        "UPDATE positions SET device_type = 'switch', device_id = $1 WHERE id = $2",
                    )
                    .bind(switch_id)
                    .bind(pos_id)
                    .execute(pool)
                    .await
                {
                    warn!("更新机位 {} 为交换机类型失败: {}", pos_id, e);
                }
            } else {
                let pos_id = Uuid::new_v4();
                if let Err(e) = sqlx::query(
                    r"INSERT INTO positions 
                       (id, name, cabinet_id, start_u, end_u, description, device_type, device_id, created_at, updated_at) 
                       VALUES ($1, $2, $3, $4, $5, $6, 'switch', $7, NOW(), NOW())"
                )
                .bind(pos_id)
                .bind(&name)
                .bind(cabinet_id)
                .bind(start_u.unwrap_or(1i32))
                .bind(end_u.unwrap_or(1i32))
                .bind(&description)
                .bind(switch_id)
                .execute(pool)
                .await
                {
                    warn!("为交换机 {} 创建机位记录失败: {}", switch_id, e);
                }
            }
        }

        sqlx::query(
            r"INSERT INTO schema_migrations (version, description) 
               VALUES ('position_device_type', '为positions表添加device_type和device_id字段，并将交换机机位迁移到positions表')"
        )
        .execute(pool)
        .await?;
    }

    Ok(())
}

async fn migrate_drop_switch_cabinet_columns(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    let result = sqlx::query(
        "SELECT COUNT(*) as count FROM schema_migrations WHERE version = 'drop_switch_cabinet_columns'",
    )
    .fetch_one(pool)
    .await?;

    let count: i64 = result.try_get("count").unwrap_or(0);

    if count == 0 {
        let has_cabinet_id: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM information_schema.columns WHERE table_name = 'switches' AND column_name = 'cabinet_id')"
        )
        .fetch_one(pool)
        .await
        .unwrap_or(false);

        if has_cabinet_id
            && let Err(e) = sqlx::query(
                "ALTER TABLE switches DROP COLUMN IF EXISTS cabinet_id, DROP COLUMN IF EXISTS start_u, DROP COLUMN IF EXISTS end_u"
            )
            .execute(pool)
            .await
        {
            warn!("删除switches表cabinet_id/start_u/end_u列失败: {}", e);
        }

        if let Err(e) = sqlx::query(
            r"CREATE OR REPLACE VIEW switches_with_details AS
            SELECT 
                s.id, s.name, s.position_id, s.model, s.vendor,
                s.location, s.snmp_version, 
                s.snmp_community,
                s.snmp_username, s.snmp_auth_protocol, 
                s.snmp_auth_password,
                s.snmp_priv_protocol, 
                s.snmp_priv_password,
                s.snmp_port,
                s.parent_switch_id, ps.name as parent_switch_name,
                s.parent_port_id, pp.port_number as parent_port_number,
                p.cabinet_id, c.name as cabinet_name,
                p.start_u, p.end_u,
                rn.network_id as position_network_id,
                n.network_region_id,
                s.description,
                'switch' as device_type,
                host(im.ip_address) as ip_address,
                im.mac_address,
                s.created_at, s.updated_at
            FROM switches s
            LEFT JOIN switches ps ON s.parent_switch_id = ps.id
            LEFT JOIN switch_ports pp ON s.parent_port_id = pp.id
            LEFT JOIN positions p ON s.position_id = p.id
            LEFT JOIN cabinets c ON p.cabinet_id = c.id
            LEFT JOIN rooms r ON c.room_id = r.id
            LEFT JOIN room_networks rn ON r.id = rn.room_id
            LEFT JOIN network_cidrs n ON rn.network_id = n.id
            LEFT JOIN LATERAL (
                SELECT ip_address, mac_address
                FROM ips 
                WHERE position_id = p.id
                LIMIT 1
            ) im ON true",
        )
        .execute(pool)
        .await
        {
            warn!("更新switches_with_details视图失败: {}", e);
        }

        sqlx::query(
            r"INSERT INTO schema_migrations (version, description) VALUES ('drop_switch_cabinet_columns', '从switches表删除cabinet_id/start_u/end_u列，通过positions表关联获取')"
        )
        .execute(pool)
        .await?;
    }

    Ok(())
}

async fn migrate_ips_network_indirect(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    let result = sqlx::query(
        "SELECT COUNT(*) as count FROM schema_migrations WHERE version = 'ips_network_indirect'",
    )
    .fetch_one(pool)
    .await?;

    let count: i64 = result.try_get("count").unwrap_or(0);

    if count == 0 {
        if let Err(e) = sqlx::query("DROP INDEX IF EXISTS idx_ips_ip_network_unique")
            .execute(pool)
            .await
        {
            warn!("删除 idx_ips_ip_network_unique 索引失败: {}", e);
        }

        if let Err(e) =
            sqlx::query("CREATE UNIQUE INDEX IF NOT EXISTS idx_ips_ip_unique ON ips(ip_address)")
                .execute(pool)
                .await
        {
            warn!("创建 idx_ips_ip_unique 索引失败: {}", e);
        }

        sqlx::query(
            r"INSERT INTO schema_migrations (version, description) VALUES ('ips_network_indirect', 'ips表network_id已删除，通过room_networks间接关联')"
        )
        .execute(pool)
        .await?;
    }

    Ok(())
}

async fn migrate_ips_drop_network_id(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    let result = sqlx::query(
        "SELECT COUNT(*) as count FROM schema_migrations WHERE version = 'ips_drop_network_id'",
    )
    .fetch_one(pool)
    .await?;

    let count: i64 = result.try_get("count").unwrap_or(0);

    if count == 0 {
        if let Err(e) = sqlx::query("DROP VIEW IF EXISTS switches_with_details CASCADE")
            .execute(pool)
            .await
        {
            warn!("删除 switches_with_details 视图失败: {}", e);
        }

        if let Err(e) = sqlx::query("DROP VIEW IF EXISTS ip_with_details CASCADE")
            .execute(pool)
            .await
        {
            warn!("删除 ip_with_details 视图失败: {}", e);
        }

        if let Err(e) =
            sqlx::query("ALTER TABLE ips DROP CONSTRAINT IF EXISTS ip_managers_network_id_fkey")
                .execute(pool)
                .await
        {
            warn!("删除 ips.network_id 外键约束失败: {}", e);
        }

        if let Err(e) = sqlx::query("ALTER TABLE ips DROP CONSTRAINT IF EXISTS ips_network_id_fkey")
            .execute(pool)
            .await
        {
            warn!("删除 ips_network_id_fkey 外键约束失败: {}", e);
        }

        if let Err(e) = sqlx::query("ALTER TABLE ips DROP COLUMN IF EXISTS network_id")
            .execute(pool)
            .await
        {
            warn!("删除 ips.network_id 列失败: {}", e);
        }

        if let Err(e) = sqlx::query("DROP INDEX IF EXISTS idx_ips_network_id")
            .execute(pool)
            .await
        {
            warn!("删除 idx_ips_network_id 索引失败: {}", e);
        }

        if let Err(e) = sqlx::query("DROP INDEX IF EXISTS idx_ip_managers_network_id")
            .execute(pool)
            .await
        {
            warn!("删除 idx_ip_managers_network_id 索引失败: {}", e);
        }

        if let Err(e) = sqlx::query(
            r"
            CREATE OR REPLACE VIEW ip_with_details AS
            SELECT 
                imm.id,
                imm.workstation_id,
                imm.position_id,
                imm.switch_port_id,
                imm.device_type,
                CASE
                    WHEN cp.device_type = 'switch' AND cp.device_id IS NOT NULL THEN s.name::text
                    WHEN w.id IS NOT NULL THEN w.name::text
                    WHEN cp.id IS NOT NULL THEN cp.name::text
                    ELSE 'unknown device'
                END AS device_name,
                rn.network_id AS network_id,
                CASE
                    WHEN w.id IS NOT NULL THEN w.name::text
                    ELSE NULL
                END AS workstation_name,
                CASE
                    WHEN cp.id IS NOT NULL THEN cp.name::text
                    ELSE NULL
                END AS cabinet_position_name,
                CASE
                    WHEN cp.device_type = 'switch' AND cp.device_id IS NOT NULL THEN s.name::text
                    ELSE NULL
                END AS switch_name,
                sp.port_number::text AS switch_port_number,
                CASE
                    WHEN r.id IS NOT NULL THEN r.name::text
                    ELSE NULL
                END AS room_name,
                CASE
                    WHEN c.id IS NOT NULL THEN c.name::text
                    ELSE NULL
                END AS cabinet_name,
                COALESCE(rn.name, 'unknown')::text AS network_name,
                COALESCE(rn.region_name, 'unknown')::text AS network_region,
                host(imm.ip_address) as ip_address,
                imm.ip_version,
                imm.mac_address,
                imm.last_mac,
                imm.hostname,
                imm.status,
                imm.last_seen,
                imm.created_at,
                imm.updated_at
            FROM ips imm
            LEFT JOIN workstations w ON imm.workstation_id = w.id
            LEFT JOIN rooms r ON w.room_id = r.id
            LEFT JOIN positions cp ON imm.position_id = cp.id
            LEFT JOIN cabinets c ON cp.cabinet_id = c.id
            LEFT JOIN rooms r2 ON c.room_id = r2.id
            LEFT JOIN switches s ON cp.device_type = 'switch' AND cp.device_id = s.id
            LEFT JOIN switch_ports sp ON imm.switch_port_id = sp.id
            LEFT JOIN LATERAL (
                SELECT rn_l.network_id, nc.name, nr.name as region_name, nc.network_region_id
                FROM room_networks rn_l
                JOIN network_cidrs nc ON rn_l.network_id = nc.id
                JOIN network_regions nr ON nc.network_region_id = nr.id
                WHERE rn_l.room_id = COALESCE(r.id, r2.id)
                AND (
                    (nc.ipv4_cidr IS NOT NULL AND imm.ip_address <<= nc.ipv4_cidr::inet)
                    OR (nc.ipv6_cidr IS NOT NULL AND imm.ip_address <<= nc.ipv6_cidr::inet)
                )
                LIMIT 1
            ) rn ON true
            ",
        )
        .execute(pool)
        .await
        {
            warn!("更新 ip_with_details 视图失败: {}", e);
        }

        sqlx::query(
            r"INSERT INTO schema_migrations (version, description) VALUES ('ips_drop_network_id', '删除ips表network_id列，完全通过room_networks间接关联')"
        )
        .execute(pool)
        .await?;
    }

    Ok(())
}

async fn migrate_ips_device_type_cleanup(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    let result = sqlx::query(
        "SELECT COUNT(*) as count FROM schema_migrations WHERE version = 'ips_device_type_cleanup'",
    )
    .fetch_one(pool)
    .await?;

    let count: i64 = result.try_get("count").unwrap_or(0);

    if count == 0 {
        if let Err(e) = sqlx::query(
            "UPDATE ips SET device_type = 'cabinet_position' WHERE device_type = 'switch'",
        )
        .execute(pool)
        .await
        {
            warn!(
                "更新 ips.device_type = 'switch' 为 'cabinet_position' 失败: {}",
                e
            );
        }

        if let Err(e) = sqlx::query("DELETE FROM ips WHERE device_type = 'unknown'")
            .execute(pool)
            .await
        {
            warn!("删除 ips.device_type = 'unknown' 的记录失败: {}", e);
        }

        sqlx::query("ALTER TABLE ips DROP CONSTRAINT IF EXISTS chk_device_type")
            .execute(pool)
            .await?;

        sqlx::query("ALTER TABLE ips DROP CONSTRAINT IF EXISTS chk_device_consistency")
            .execute(pool)
            .await?;

        if let Err(e) = sqlx::query(
            r"ALTER TABLE ips ADD CONSTRAINT chk_device_type CHECK (device_type IN ('workstation', 'cabinet_position'))"
        )
        .execute(pool)
        .await
        {
            warn!("添加 ips.device_type 约束失败: {}", e);
        }

        if let Err(e) = sqlx::query(
            r"ALTER TABLE ips ADD CONSTRAINT chk_device_consistency CHECK (
                (device_type = 'workstation' AND workstation_id IS NOT NULL AND position_id IS NULL) OR
                (device_type = 'cabinet_position' AND position_id IS NOT NULL AND workstation_id IS NULL)
            )"
        )
        .execute(pool)
        .await
        {
            warn!("添加 ips.device_consistency 约束失败: {}", e);
        }

        sqlx::query(
            r"INSERT INTO schema_migrations (version, description) VALUES ('ips_device_type_cleanup', '清理ips表device_type字段，将switch改为cabinet_position，删除unknown类型')"
        )
        .execute(pool)
        .await?;
    }

    Ok(())
}

async fn migrate_ips_drop_network_id_v2(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    let result = sqlx::query(
        "SELECT COUNT(*) as count FROM schema_migrations WHERE version = 'ips_drop_network_id_v2'",
    )
    .fetch_one(pool)
    .await?;

    let count: i64 = result.try_get("count").unwrap_or(0);

    if count == 0 {
        if let Err(e) = sqlx::query("DROP VIEW IF EXISTS switches_with_details CASCADE")
            .execute(pool)
            .await
        {
            warn!("删除 switches_with_details 视图失败: {}", e);
        }

        if let Err(e) = sqlx::query("DROP VIEW IF EXISTS ip_with_details CASCADE")
            .execute(pool)
            .await
        {
            warn!("删除 ip_with_details 视图失败: {}", e);
        }

        if let Err(e) =
            sqlx::query("ALTER TABLE ips DROP CONSTRAINT IF EXISTS ip_managers_network_id_fkey")
                .execute(pool)
                .await
        {
            warn!("删除 ips.network_id 外键约束失败: {}", e);
        }

        if let Err(e) = sqlx::query("ALTER TABLE ips DROP CONSTRAINT IF EXISTS ips_network_id_fkey")
            .execute(pool)
            .await
        {
            warn!("删除 ips_network_id_fkey 外键约束失败: {}", e);
        }

        if let Err(e) = sqlx::query("ALTER TABLE ips DROP COLUMN IF EXISTS network_id")
            .execute(pool)
            .await
        {
            warn!("删除 ips.network_id 列失败: {}", e);
        }

        if let Err(e) = sqlx::query("DROP INDEX IF EXISTS idx_ips_network_id")
            .execute(pool)
            .await
        {
            warn!("删除 idx_ips_network_id 索引失败: {}", e);
        }

        if let Err(e) = sqlx::query("DROP INDEX IF EXISTS idx_ip_managers_network_id")
            .execute(pool)
            .await
        {
            warn!("删除 idx_ip_managers_network_id 索引失败: {}", e);
        }

        if let Err(e) = sqlx::query(
            r"
            CREATE VIEW ip_with_details AS
            SELECT 
                imm.id,
                imm.workstation_id,
                imm.position_id,
                imm.switch_port_id,
                imm.device_type,
                CASE
                    WHEN cp.device_type = 'switch' AND cp.device_id IS NOT NULL THEN s.name::text
                    WHEN w.id IS NOT NULL THEN w.name::text
                    WHEN cp.id IS NOT NULL THEN cp.name::text
                    ELSE 'unknown device'
                END AS device_name,
                rn.network_id AS network_id,
                CASE
                    WHEN w.id IS NOT NULL THEN w.name::text
                    ELSE NULL
                END AS workstation_name,
                CASE
                    WHEN cp.id IS NOT NULL THEN cp.name::text
                    ELSE NULL
                END AS cabinet_position_name,
                CASE
                    WHEN cp.device_type = 'switch' AND cp.device_id IS NOT NULL THEN s.name::text
                    ELSE NULL
                END AS switch_name,
                sp.port_number::text AS switch_port_number,
                CASE
                    WHEN r.id IS NOT NULL THEN r.name::text
                    ELSE NULL
                END AS room_name,
                CASE
                    WHEN c.id IS NOT NULL THEN c.name::text
                    ELSE NULL
                END AS cabinet_name,
                COALESCE(rn.name, 'unknown')::text AS network_name,
                COALESCE(rn.region_name, 'unknown')::text AS network_region,
                host(imm.ip_address) as ip_address,
                imm.ip_version,
                imm.mac_address,
                imm.last_mac,
                imm.hostname,
                imm.status,
                imm.last_seen,
                imm.created_at,
                imm.updated_at
            FROM ips imm
            LEFT JOIN workstations w ON imm.workstation_id = w.id
            LEFT JOIN rooms r ON w.room_id = r.id
            LEFT JOIN positions cp ON imm.position_id = cp.id
            LEFT JOIN cabinets c ON cp.cabinet_id = c.id
            LEFT JOIN rooms r2 ON c.room_id = r2.id
            LEFT JOIN switches s ON cp.device_type = 'switch' AND cp.device_id = s.id
            LEFT JOIN switch_ports sp ON imm.switch_port_id = sp.id
            LEFT JOIN LATERAL (
                SELECT rn_l.network_id, nc.name, nr.name as region_name, nc.network_region_id
                FROM room_networks rn_l
                JOIN network_cidrs nc ON rn_l.network_id = nc.id
                JOIN network_regions nr ON nc.network_region_id = nr.id
                WHERE rn_l.room_id = COALESCE(r.id, r2.id)
                AND (
                    (nc.ipv4_cidr IS NOT NULL AND imm.ip_address <<= nc.ipv4_cidr::inet)
                    OR (nc.ipv6_cidr IS NOT NULL AND imm.ip_address <<= nc.ipv6_cidr::inet)
                )
                LIMIT 1
            ) rn ON true
            ",
        )
        .execute(pool)
        .await
        {
            warn!("创建 ip_with_details 视图失败: {}", e);
        }

        if let Err(e) = sqlx::query(
            r"CREATE OR REPLACE VIEW switches_with_details AS
            SELECT 
                s.id, s.name, s.position_id, s.model, s.vendor,
                s.location, s.snmp_version, 
                s.snmp_community,
                s.snmp_username, s.snmp_auth_protocol, 
                s.snmp_auth_password,
                s.snmp_priv_protocol, 
                s.snmp_priv_password,
                s.snmp_port,
                s.parent_switch_id, ps.name as parent_switch_name,
                s.parent_port_id, pp.port_number as parent_port_number,
                p.cabinet_id, c.name as cabinet_name,
                p.start_u, p.end_u,
                rn.network_id as position_network_id,
                rn.network_region_id,
                s.description,
                'switch' as device_type,
                host(im.ip_address) as ip_address,
                im.mac_address,
                s.created_at, s.updated_at
            FROM switches s
            LEFT JOIN switches ps ON s.parent_switch_id = ps.id
            LEFT JOIN switch_ports pp ON s.parent_port_id = pp.id
            LEFT JOIN positions p ON s.position_id = p.id
            LEFT JOIN cabinets c ON p.cabinet_id = c.id
            LEFT JOIN rooms r ON c.room_id = r.id
            LEFT JOIN LATERAL (
                SELECT rn_l.network_id, nc.network_region_id
                FROM room_networks rn_l
                JOIN network_cidrs nc ON rn_l.network_id = nc.id
                WHERE rn_l.room_id = r.id
                LIMIT 1
            ) rn ON true
            LEFT JOIN LATERAL (
                SELECT ip_address, mac_address
                FROM ips 
                WHERE position_id = p.id
                LIMIT 1
            ) im ON true",
        )
        .execute(pool)
        .await
        {
            warn!("创建 switches_with_details 视图失败: {}", e);
        }

        sqlx::query(
            r"INSERT INTO schema_migrations (version, description) VALUES ('ips_drop_network_id_v2', '强制删除ips表network_id列和外键约束，完全通过room_networks间接关联')"
        )
        .execute(pool)
        .await?;
    }

    Ok(())
}

async fn create_views(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query("DROP VIEW IF EXISTS ip_with_details CASCADE")
        .execute(pool)
        .await?;

    sqlx::query(
        r"
        CREATE VIEW ip_with_details AS
        SELECT 
            imm.id,
            imm.workstation_id,
            imm.position_id,
            imm.switch_port_id,
            imm.device_type,
            CASE
                WHEN cp.device_type = 'switch' AND cp.device_id IS NOT NULL THEN s.name::text
                WHEN w.id IS NOT NULL THEN w.name::text
                WHEN cp.id IS NOT NULL THEN cp.name::text
                ELSE 'unknown device'
            END AS device_name,
            rn.network_id AS network_id,
            CASE
                WHEN w.id IS NOT NULL THEN w.name::text
                ELSE NULL
            END AS workstation_name,
            CASE
                WHEN cp.id IS NOT NULL THEN cp.name::text
                ELSE NULL
            END AS cabinet_position_name,
            CASE
                WHEN cp.device_type = 'switch' AND cp.device_id IS NOT NULL THEN s.name::text
                ELSE NULL
            END AS switch_name,
            sp.port_number::text AS switch_port_number,
            CASE
                WHEN r.id IS NOT NULL THEN r.name::text
                ELSE NULL
            END AS room_name,
            CASE
                WHEN c.id IS NOT NULL THEN c.name::text
                ELSE NULL
            END AS cabinet_name,
            COALESCE(rn.name, 'unknown')::text AS network_name,
            COALESCE(rn.region_name, 'unknown')::text AS network_region,
            host(imm.ip_address) as ip_address,
            imm.ip_version,
            imm.mac_address,
            imm.last_mac,
            imm.hostname,
            imm.status,
            imm.last_seen,
            imm.created_at,
            imm.updated_at
        FROM ips imm
        LEFT JOIN workstations w ON imm.workstation_id = w.id
        LEFT JOIN rooms r ON w.room_id = r.id
        LEFT JOIN positions cp ON imm.position_id = cp.id
        LEFT JOIN cabinets c ON cp.cabinet_id = c.id
        LEFT JOIN rooms r2 ON c.room_id = r2.id
        LEFT JOIN switches s ON cp.device_type = 'switch' AND cp.device_id = s.id
        LEFT JOIN switch_ports sp ON imm.switch_port_id = sp.id
        LEFT JOIN LATERAL (
            SELECT rn.network_id, nc.name, nr.name as region_name, nc.network_region_id
            FROM room_networks rn
            JOIN network_cidrs nc ON rn.network_id = nc.id
            JOIN network_regions nr ON nc.network_region_id = nr.id
            WHERE rn.room_id = COALESCE(r.id, r2.id)
            LIMIT 1
        ) rn ON true
    ",
    )
    .execute(pool)
    .await?;

    sqlx::query("DROP VIEW IF EXISTS mac_comparison CASCADE")
        .execute(pool)
        .await?;

    sqlx::query(
        r"
        CREATE VIEW mac_comparison AS
        SELECT 
            sm.switch_id,
            s.name AS switch_name,
            host(sm.ip_address) AS ip_address,
            sm.mac_address AS snmp_mac,
            im.mac_address AS managed_mac,
            CASE
                WHEN im.id IS NULL THEN 'unmanaged'
                WHEN sm.mac_address = im.mac_address THEN 'match'
                ELSE 'mismatch'
            END AS comparison_result
        FROM switch_macs sm
        JOIN switches s ON sm.switch_id = s.id
        LEFT JOIN ips im ON sm.ip_address = im.ip_address AND im.device_type != 'switch'
    ",
    )
    .execute(pool)
    .await?;

    Ok(())
}

async fn create_triggers(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"
        CREATE OR REPLACE FUNCTION update_updated_at_column()
        RETURNS TRIGGER AS $$
        BEGIN
            NEW.updated_at = NOW();
            RETURN NEW;
        END;
        $$ LANGUAGE plpgsql;
    ",
    )
    .execute(pool)
    .await?;

    let tables_with_updated_at = [
        "users",
        "network_regions",
        "network_cidrs",
        "rooms",
        "room_networks",
        "workstation_layouts",
        "cabinets",
        "positions",
        "workstations",
        "switches",
        "switch_ports",
        "switch_macs",
        "switch_lldps",
        "ips",
        "cabinet_layouts",
        "system_configs",
        "scheduled_tasks",
    ];

    for table in &tables_with_updated_at {
        let trigger_name = format!("trg_{table}_updated_at");
        if let Err(e) = sqlx::query(&format!(
            "CREATE TRIGGER {trigger_name} BEFORE UPDATE ON {table} FOR EACH ROW EXECUTE FUNCTION update_updated_at_column()"
        ))
        .execute(pool)
        .await
        {
            warn!("updated_at触发器创建失败（可能已存在） {}: {}", table, e);
        }
    }

    sqlx::query(
        r"
        CREATE OR REPLACE FUNCTION check_position_overlap() RETURNS TRIGGER AS $$
        BEGIN
            IF EXISTS (
                SELECT 1 FROM positions 
                WHERE cabinet_id = NEW.cabinet_id 
                AND id != NEW.id
                AND (
                    (NEW.start_u BETWEEN start_u AND end_u)
                    OR (NEW.end_u BETWEEN start_u AND end_u)
                    OR (start_u BETWEEN NEW.start_u AND NEW.end_u)
                    OR (end_u BETWEEN NEW.start_u AND NEW.end_u)
                )
            ) THEN
                RAISE EXCEPTION '机位U位范围重叠';
            END IF;
            RETURN NEW;
        END;
        $$ LANGUAGE plpgsql;
    ",
    )
    .execute(pool)
    .await?;

    if let Err(e) = sqlx::query("DROP TRIGGER IF EXISTS trg_check_position_overlap ON positions")
        .execute(pool)
        .await
    {
        warn!("删除旧触发器失败: {}", e);
    }

    sqlx::query(
        "CREATE TRIGGER trg_check_position_overlap BEFORE INSERT OR UPDATE ON positions FOR EACH ROW EXECUTE FUNCTION check_position_overlap()"
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r"
        CREATE OR REPLACE FUNCTION check_switch_circular_dependency() RETURNS TRIGGER AS $$
        BEGIN
            IF NEW.parent_switch_id = NEW.id THEN
                RAISE EXCEPTION '交换机不能以自身为上级交换机';
            END IF;
            
            IF NEW.parent_switch_id IS NOT NULL THEN
                IF EXISTS (
                    WITH RECURSIVE switch_tree AS (
                        SELECT id, parent_switch_id FROM switches WHERE id = NEW.parent_switch_id
                        UNION ALL
                        SELECT s.id, s.parent_switch_id FROM switches s
                        JOIN switch_tree st ON s.id = st.parent_switch_id
                    )
                    SELECT 1 FROM switch_tree WHERE id = NEW.id
                ) THEN
                    RAISE EXCEPTION '交换机层级关系存在循环依赖';
                END IF;
            END IF;
            
            RETURN NEW;
        END;
        $$ LANGUAGE plpgsql;
    ",
    )
    .execute(pool)
    .await?;

    if let Err(e) =
        sqlx::query("DROP TRIGGER IF EXISTS trg_check_switch_circular_dependency ON switches")
            .execute(pool)
            .await
    {
        warn!("删除旧触发器失败: {}", e);
    }

    sqlx::query(
        "CREATE TRIGGER trg_check_switch_circular_dependency BEFORE INSERT OR UPDATE OF parent_switch_id ON switches FOR EACH ROW EXECUTE FUNCTION check_switch_circular_dependency()"
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r"
        CREATE OR REPLACE FUNCTION check_parent_port_consistency() RETURNS TRIGGER AS $$
        BEGIN
            IF NEW.parent_switch_id IS NOT NULL AND NEW.parent_port_id IS NOT NULL THEN
                IF NOT EXISTS (
                    SELECT 1 FROM switch_ports 
                    WHERE id = NEW.parent_port_id AND switch_id = NEW.parent_switch_id
                ) THEN
                    RAISE EXCEPTION '上级端口必须属于上级交换机';
                END IF;
            END IF;
            RETURN NEW;
        END;
        $$ LANGUAGE plpgsql;
    ",
    )
    .execute(pool)
    .await?;

    if let Err(e) =
        sqlx::query("DROP TRIGGER IF EXISTS trg_check_parent_port_consistency ON switches")
            .execute(pool)
            .await
    {
        warn!("删除旧触发器失败: {}", e);
    }

    sqlx::query(
        "CREATE TRIGGER trg_check_parent_port_consistency BEFORE INSERT OR UPDATE OF parent_switch_id, parent_port_id ON switches FOR EACH ROW EXECUTE FUNCTION check_parent_port_consistency()"
    )
    .execute(pool)
    .await?;

    Ok(())
}

async fn migrate_workstation_layouts_structure(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    let result = sqlx::query(
        "SELECT COUNT(*) as count FROM schema_migrations WHERE version = 'workstation_layouts_structure'",
    )
    .fetch_one(pool)
    .await?;

    let count: i64 = result.try_get("count").unwrap_or(0);

    if count == 0 {
        // 检查表是否存在
        let table_exists = sqlx::query(
            "SELECT COUNT(*) as count FROM information_schema.tables 
             WHERE table_schema = 'public' AND table_name = 'workstation_layouts'",
        )
        .fetch_one(pool)
        .await?;

        let table_count: i64 = table_exists.try_get("count").unwrap_or(0);
        let needs_recreate = if table_count > 0 {
            // 检查是否是旧表结构（存在 workstation_id 字段）
            let column_exists = sqlx::query(
                "SELECT COUNT(*) as count FROM information_schema.columns 
                 WHERE table_name = 'workstation_layouts' AND column_name = 'workstation_id'",
            )
            .fetch_one(pool)
            .await?;

            let col_count: i64 = column_exists.try_get("count").unwrap_or(0);

            if col_count > 0 {
                // 备份旧数据
                if let Err(e) = sqlx::query(
                    "CREATE TABLE IF NOT EXISTS workstation_layouts_backup AS SELECT * FROM workstation_layouts",
                )
                .execute(pool)
                .await
                {
                    warn!("创建 workstation_layouts 备份表失败: {}", e);
                }

                // 删除旧表
                if let Err(e) = sqlx::query("DROP TABLE IF EXISTS workstation_layouts CASCADE")
                    .execute(pool)
                    .await
                {
                    warn!("删除旧 workstation_layouts 表失败: {}", e);
                }

                info!("workstation_layouts 旧表已删除，将重新创建新表结构");
                true
            } else {
                // 表已存在且是新结构，无需操作
                false
            }
        } else {
            // 表不存在，需要创建
            info!("workstation_layouts 表不存在，将创建新表结构");
            true
        };

        // 如果需要重新创建表，执行创建逻辑
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

            // 创建索引
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

            info!("workstation_layouts 新表结构创建成功");
        }

        // 记录迁移版本
        sqlx::query(
            "INSERT INTO schema_migrations (version, description) VALUES ('workstation_layouts_structure', '重构 workstation_layouts 表结构，支持门元素和多种元素类型')",
        )
        .execute(pool)
        .await?;
    }

    Ok(())
}

async fn migrate_ip_with_details_network_match(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    let result = sqlx::query(
        "SELECT COUNT(*) as count FROM schema_migrations WHERE version = 'ip_with_details_network_match'",
    )
    .fetch_one(pool)
    .await?;

    let count: i64 = result.try_get("count").unwrap_or(0);

    if count == 0 {
        if let Err(e) = sqlx::query("DROP VIEW IF EXISTS ip_with_details CASCADE")
            .execute(pool)
            .await
        {
            warn!("删除 ip_with_details 视图失败: {}", e);
        }

        if let Err(e) = sqlx::query(
            r"
            CREATE VIEW ip_with_details AS
            SELECT 
                imm.id,
                imm.workstation_id,
                imm.position_id,
                imm.switch_port_id,
                imm.device_type,
                CASE
                    WHEN cp.device_type = 'switch' AND cp.device_id IS NOT NULL THEN s.name::text
                    WHEN w.id IS NOT NULL THEN w.name::text
                    WHEN cp.id IS NOT NULL THEN cp.name::text
                    ELSE 'unknown device'
                END AS device_name,
                rn.network_id AS network_id,
                CASE
                    WHEN w.id IS NOT NULL THEN w.name::text
                    ELSE NULL
                END AS workstation_name,
                CASE
                    WHEN cp.id IS NOT NULL THEN cp.name::text
                    ELSE NULL
                END AS cabinet_position_name,
                CASE
                    WHEN cp.device_type = 'switch' AND cp.device_id IS NOT NULL THEN s.name::text
                    ELSE NULL
                END AS switch_name,
                sp.port_number::text AS switch_port_number,
                CASE
                    WHEN r.id IS NOT NULL THEN r.name::text
                    ELSE NULL
                END AS room_name,
                CASE
                    WHEN c.id IS NOT NULL THEN c.name::text
                    ELSE NULL
                END AS cabinet_name,
                COALESCE(rn.name, 'unknown')::text AS network_name,
                COALESCE(rn.region_name, 'unknown')::text AS network_region,
                host(imm.ip_address) as ip_address,
                imm.ip_version,
                imm.mac_address,
                imm.last_mac,
                imm.hostname,
                imm.status,
                imm.last_seen,
                imm.created_at,
                imm.updated_at
            FROM ips imm
            LEFT JOIN workstations w ON imm.workstation_id = w.id
            LEFT JOIN rooms r ON w.room_id = r.id
            LEFT JOIN positions cp ON imm.position_id = cp.id
            LEFT JOIN cabinets c ON cp.cabinet_id = c.id
            LEFT JOIN rooms r2 ON c.room_id = r2.id
            LEFT JOIN switches s ON cp.device_type = 'switch' AND cp.device_id = s.id
            LEFT JOIN switch_ports sp ON imm.switch_port_id = sp.id
            LEFT JOIN LATERAL (
                SELECT rn_l.network_id, nc.name, nr.name as region_name, nc.network_region_id
                FROM room_networks rn_l
                JOIN network_cidrs nc ON rn_l.network_id = nc.id
                JOIN network_regions nr ON nc.network_region_id = nr.id
                WHERE rn_l.room_id = COALESCE(r.id, r2.id)
                AND (
                    (nc.ipv4_cidr IS NOT NULL AND imm.ip_address <<= nc.ipv4_cidr::inet)
                    OR (nc.ipv6_cidr IS NOT NULL AND imm.ip_address <<= nc.ipv6_cidr::inet)
                )
                LIMIT 1
            ) rn ON true
            ",
        )
        .execute(pool)
        .await
        {
            warn!("创建 ip_with_details 视图失败: {}", e);
        }

        sqlx::query(
            r"INSERT INTO schema_migrations (version, description) VALUES ('ip_with_details_network_match', '更新ip_with_details视图，根据IP地址匹配正确的网络CIDR')"
        )
        .execute(pool)
        .await?;
    }

    Ok(())
}

async fn migrate_switch_parent_columns_removal(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    let result = sqlx::query(
        "SELECT COUNT(*) as count FROM schema_migrations WHERE version = 'switch_parent_columns_removal'",
    )
    .fetch_one(pool)
    .await?;

    let count: i64 = result.try_get("count").unwrap_or(0);

    if count == 0 {
        if let Err(e) =
            sqlx::query("ALTER TABLE switches DROP CONSTRAINT IF EXISTS fk_parent_port_id")
                .execute(pool)
                .await
        {
            warn!("删除 switches.fk_parent_port_id 约束失败: {}", e);
        }

        if let Err(e) = sqlx::query(
            "ALTER TABLE switches DROP CONSTRAINT IF EXISTS switches_parent_switch_id_fkey",
        )
        .execute(pool)
        .await
        {
            warn!("删除 switches.parent_switch_id 外键约束失败: {}", e);
        }

        if let Err(e) = sqlx::query("ALTER TABLE switches DROP COLUMN IF EXISTS parent_switch_id")
            .execute(pool)
            .await
        {
            warn!("删除 switches.parent_switch_id 列失败: {}", e);
        }

        if let Err(e) = sqlx::query("ALTER TABLE switches DROP COLUMN IF EXISTS parent_port_id")
            .execute(pool)
            .await
        {
            warn!("删除 switches.parent_port_id 列失败: {}", e);
        }

        if let Err(e) = sqlx::query(
            r"CREATE OR REPLACE VIEW switches_with_details AS
            SELECT 
                s.id, s.name, s.model, s.vendor,
                s.location, s.snmp_version, 
                s.snmp_community,
                s.snmp_username, s.snmp_auth_protocol, 
                s.snmp_auth_password,
                s.snmp_priv_protocol, 
                s.snmp_priv_password,
                s.snmp_port,
                s.position_id,
                p.cabinet_id, c.name as cabinet_name,
                r.id as room_id, r.name as room_name,
                p.start_u, p.end_u,
                rn.network_id as position_network_id,
                n.network_region_id,
                s.description,
                'switch' as device_type,
                host(im.ip_address) as ip_address,
                im.mac_address,
                s.created_at, s.updated_at
            FROM switches s
            LEFT JOIN positions p ON s.position_id = p.id
            LEFT JOIN cabinets c ON p.cabinet_id = c.id
            LEFT JOIN rooms r ON c.room_id = r.id
            LEFT JOIN room_networks rn ON r.id = rn.room_id
            LEFT JOIN network_cidrs n ON rn.network_id = n.id
            LEFT JOIN LATERAL (
                SELECT ip_address, mac_address
                FROM ips 
                WHERE position_id = p.id
                LIMIT 1
            ) im ON true",
        )
        .execute(pool)
        .await
        {
            warn!("更新 switches_with_details 视图失败: {}", e);
        }

        sqlx::query(
            r"INSERT INTO schema_migrations (version, description) VALUES ('switch_parent_columns_removal', '删除 switches 表的 parent_switch_id 和 parent_port_id 列，交换机端口完全由 switch_ports 表管理')"
        )
        .execute(pool)
        .await?;
    }

    Ok(())
}
