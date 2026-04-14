use sqlx::Row;
use tracing::warn;
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
    create_ip_managers_table(pool).await?;
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
        r#"CREATE TABLE IF NOT EXISTS schema_migrations (
            version VARCHAR(50) PRIMARY KEY,
            applied_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            description TEXT
        )"#,
    )
    .execute(pool)
    .await
    .map(|_| ())
}

async fn create_users_table(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS users (
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
        )"#,
    )
    .execute(pool)
    .await
    .map(|_| ())
}

async fn create_network_tables(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS network_regions (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            name VARCHAR(20) NOT NULL UNIQUE,
            description TEXT,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
        )"#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS network_cidrs (
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
        )"#,
    )
    .execute(pool)
    .await
    .map(|_| ())
}

async fn create_room_tables(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS rooms (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            name VARCHAR(50) NOT NULL,
            room_type VARCHAR(20) NOT NULL DEFAULT 'OFFICE',
            description TEXT,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            CONSTRAINT chk_room_type CHECK (room_type IN ('OFFICE', 'DATA_CENTER'))
        )"#,
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
        r#"CREATE TABLE IF NOT EXISTS room_networks (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            room_id UUID NOT NULL REFERENCES rooms(id) ON DELETE CASCADE,
            network_id UUID NOT NULL REFERENCES network_cidrs(id),
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            UNIQUE(room_id, network_id)
        )"#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS svg_layouts (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            layout_type VARCHAR(20) NOT NULL,
            room_id UUID REFERENCES rooms(id) ON DELETE CASCADE,
            network_region_id UUID REFERENCES network_regions(id) ON DELETE CASCADE,
            element_id UUID NOT NULL,
            element_type VARCHAR(20) NOT NULL,
            x INTEGER NOT NULL DEFAULT 0,
            y INTEGER NOT NULL DEFAULT 0,
            width INTEGER NOT NULL DEFAULT 160,
            height INTEGER NOT NULL DEFAULT 160,
            rotation INTEGER NOT NULL DEFAULT 0,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            UNIQUE NULLS NOT DISTINCT(layout_type, room_id, network_region_id, element_id)
        )"#,
    )
    .execute(pool)
    .await
    .map(|_| ())
}

async fn create_cabinet_tables(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS cabinets (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            name VARCHAR(50) NOT NULL,
            room_id UUID REFERENCES rooms(id) ON DELETE SET NULL,
            capacity INTEGER NOT NULL DEFAULT 42,
            network_id UUID REFERENCES network_cidrs(id),
            description TEXT,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
        )"#,
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
        r#"CREATE TABLE IF NOT EXISTS positions (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            name VARCHAR(50) NOT NULL,
            cabinet_id UUID NOT NULL REFERENCES cabinets(id) ON DELETE CASCADE,
            start_u INTEGER NOT NULL DEFAULT 1,
            end_u INTEGER NOT NULL DEFAULT 1,
            network_id UUID REFERENCES network_cidrs(id),
            description TEXT,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
        )"#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS position_ports (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            position_id UUID NOT NULL REFERENCES positions(id) ON DELETE CASCADE,
            switch_port_id UUID NOT NULL REFERENCES switch_ports(id) ON DELETE CASCADE,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            UNIQUE(position_id, switch_port_id)
        )"#,
    )
    .execute(pool)
    .await
    .map(|_| ())
}

async fn create_workstation_tables(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS workstations (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            name VARCHAR(50) NOT NULL,
            room_id UUID NOT NULL REFERENCES rooms(id),
            manager VARCHAR(50),
            description TEXT,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
        )"#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS workstation_ports (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            workstation_id UUID NOT NULL REFERENCES workstations(id) ON DELETE CASCADE,
            switch_port_id UUID NOT NULL REFERENCES switch_ports(id) ON DELETE CASCADE,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            UNIQUE(workstation_id, switch_port_id)
        )"#,
    )
    .execute(pool)
    .await
    .map(|_| ())
}

async fn create_switch_tables(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS switches (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            name VARCHAR(100) NOT NULL,
            network_region_id UUID NOT NULL REFERENCES network_regions(id),
            network_id UUID NOT NULL REFERENCES network_cidrs(id),
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
            parent_switch_id UUID REFERENCES switches(id) ON DELETE SET NULL,
            parent_port_id UUID,
            description TEXT,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
        )"#,
    )
    .execute(pool)
    .await?;

    if let Err(e) = sqlx::query(
        "ALTER TABLE switches ADD CONSTRAINT fk_parent_port_id FOREIGN KEY (parent_port_id) REFERENCES switch_ports(id) ON DELETE SET NULL"
    ).execute(pool).await {
        warn!("switches.fk_parent_port_id约束可能已存在: {}", e);
    }

    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS switch_ports (
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
        )"#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS switch_macs (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            switch_id UUID NOT NULL REFERENCES switches(id) ON DELETE CASCADE,
            ip_address INET NOT NULL,
            mac_address VARCHAR(20) NOT NULL,
            interface VARCHAR(50),
            vlan_id INTEGER,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            UNIQUE(switch_id, ip_address)
        )"#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS switch_lldps (
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
        )"#,
    )
    .execute(pool)
    .await?;

    Ok(())
}

async fn create_ip_managers_table(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(r#"CREATE TABLE IF NOT EXISTS ip_managers (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            workstation_id UUID REFERENCES workstations(id) ON DELETE SET NULL,
            position_id UUID REFERENCES positions(id) ON DELETE SET NULL,
            switch_id UUID REFERENCES switches(id) ON DELETE SET NULL,
            switch_port_id UUID REFERENCES switch_ports(id) ON DELETE SET NULL,
            device_type VARCHAR(20) NOT NULL,
            network_id UUID NOT NULL REFERENCES network_cidrs(id),
            ip_address INET NOT NULL,
            ip_version SMALLINT NOT NULL DEFAULT 4,
            mac_address VARCHAR(20),
            hostname VARCHAR(100),
            status VARCHAR(20) NOT NULL DEFAULT 'active',
            last_seen TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            CONSTRAINT chk_device_type CHECK (device_type IN ('workstation', 'cabinet_position', 'switch', 'unknown')),
            CONSTRAINT chk_device_consistency CHECK (
                (device_type = 'switch' AND switch_id IS NOT NULL AND workstation_id IS NULL) OR
                (device_type = 'workstation' AND workstation_id IS NOT NULL AND position_id IS NULL AND switch_id IS NULL) OR
                (device_type = 'cabinet_position' AND position_id IS NOT NULL AND workstation_id IS NULL AND switch_id IS NULL) OR
                (device_type = 'unknown' AND workstation_id IS NULL AND position_id IS NULL AND switch_id IS NULL AND switch_port_id IS NULL)
            )
        )"#)
        .execute(pool).await?;
    Ok(())
}

async fn create_log_tables(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS operation_logs (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            user_id UUID NOT NULL REFERENCES users(id),
            action VARCHAR(100) NOT NULL,
            resource_type VARCHAR(50) NOT NULL,
            resource_id UUID NOT NULL,
            details JSONB NOT NULL DEFAULT '{}',
            result BOOLEAN NOT NULL,
            ip_address VARCHAR(50) NOT NULL,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
        )"#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS task_logs (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            task_name VARCHAR(100) NOT NULL,
            status VARCHAR(20) NOT NULL,
            details JSONB NOT NULL DEFAULT '{}',
            start_time TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            end_time TIMESTAMP WITH TIME ZONE,
            duration INTEGER
        )"#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS login_logs (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            username VARCHAR(50) NOT NULL,
            ip_address VARCHAR(50) NOT NULL,
            user_agent VARCHAR(255),
            success BOOLEAN NOT NULL,
            error_message VARCHAR(255),
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
        )"#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS mac_history (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            mac_address VARCHAR(20) NOT NULL,
            ip_address INET NOT NULL,
            ip_manager_id UUID NOT NULL REFERENCES ip_managers(id) ON DELETE CASCADE,
            device_type VARCHAR(20) NOT NULL,
            workstation_id UUID REFERENCES workstations(id) ON DELETE SET NULL,
            position_id UUID REFERENCES positions(id) ON DELETE SET NULL,
            switch_id UUID REFERENCES switches(id) ON DELETE SET NULL,
            switch_port_id UUID REFERENCES switch_ports(id) ON DELETE SET NULL,
            network_id UUID NOT NULL REFERENCES network_cidrs(id),
            change_type VARCHAR(20) NOT NULL DEFAULT 'update',
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
        )"#,
    )
    .execute(pool)
    .await?;

    Ok(())
}

async fn create_token_tables(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS revoked_tokens (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            token_hash VARCHAR(255) NOT NULL,
            user_id UUID REFERENCES users(id),
            revoked_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            expiry TIMESTAMP WITH TIME ZONE NOT NULL
        )"#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS token_usage (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            token_hash VARCHAR(255) NOT NULL,
            user_id UUID REFERENCES users(id),
            ip_address VARCHAR(50) NOT NULL,
            user_agent VARCHAR(255),
            request_path VARCHAR(255) NOT NULL,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
        )"#,
    )
    .execute(pool)
    .await
    .map(|_| ())
}

async fn create_notification_tables(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS notifications (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            user_id UUID REFERENCES users(id),
            title VARCHAR(100) NOT NULL,
            content TEXT NOT NULL,
            notification_type VARCHAR(20) NOT NULL,
            read BOOLEAN NOT NULL DEFAULT FALSE,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
        )"#,
    )
    .execute(pool)
    .await
    .map(|_| ())
}

async fn create_system_tables(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS system_configs (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            config_type VARCHAR(50) NOT NULL,
            key VARCHAR(100) NOT NULL,
            value TEXT,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            UNIQUE(config_type, key)
        )"#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS scheduled_tasks (
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
        )"#,
    )
    .execute(pool)
    .await?;

    Ok(())
}

async fn create_indexes(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    let indexes: &[&str] = &[
        "CREATE INDEX IF NOT EXISTS idx_users_username ON users(username)",
        "CREATE INDEX IF NOT EXISTS idx_users_email ON users(email)",
        "CREATE INDEX IF NOT EXISTS idx_ip_managers_workstation_id ON ip_managers(workstation_id)",
        "CREATE INDEX IF NOT EXISTS idx_ip_managers_switch_device ON ip_managers(switch_id, device_type)",
        "CREATE INDEX IF NOT EXISTS idx_ip_managers_position_id ON ip_managers(position_id)",
        "CREATE INDEX IF NOT EXISTS idx_ip_managers_network_id ON ip_managers(network_id)",
        "CREATE INDEX IF NOT EXISTS idx_ip_managers_switch_port_id ON ip_managers(switch_port_id)",
        "CREATE INDEX IF NOT EXISTS idx_ip_managers_ip_address ON ip_managers(ip_address)",
        "CREATE INDEX IF NOT EXISTS idx_ip_managers_mac_address ON ip_managers(mac_address)",
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_ip_managers_ip_network_unique ON ip_managers(ip_address, network_id)",
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_ip_managers_mac_unique ON ip_managers(mac_address) WHERE mac_address IS NOT NULL AND mac_address != ''",
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
        "CREATE INDEX IF NOT EXISTS idx_switches_network_region_id ON switches(network_region_id)",
        "CREATE INDEX IF NOT EXISTS idx_switches_network_id ON switches(network_id)",
        "CREATE INDEX IF NOT EXISTS idx_switch_ports_switch_id ON switch_ports(switch_id)",
        "CREATE INDEX IF NOT EXISTS idx_switch_macs_switch_id ON switch_macs(switch_id)",
        "CREATE INDEX IF NOT EXISTS idx_switch_macs_ip_address ON switch_macs(ip_address)",
        "CREATE INDEX IF NOT EXISTS idx_switch_macs_mac_address ON switch_macs(mac_address)",
        "CREATE INDEX IF NOT EXISTS idx_switch_lldps_switch_id ON switch_lldps(switch_id)",
        "CREATE INDEX IF NOT EXISTS idx_cabinets_room_id ON cabinets(room_id)",
        "CREATE INDEX IF NOT EXISTS idx_positions_cabinet_id ON positions(cabinet_id)",
        "CREATE INDEX IF NOT EXISTS idx_workstations_room_id ON workstations(room_id)",
        "CREATE INDEX IF NOT EXISTS idx_room_networks_room_id ON room_networks(room_id)",
        "CREATE INDEX IF NOT EXISTS idx_room_networks_network_id ON room_networks(network_id)",
        "CREATE INDEX IF NOT EXISTS idx_position_ports_switch_port_id ON position_ports(switch_port_id)",
        "CREATE INDEX IF NOT EXISTS idx_workstation_ports_switch_port_id ON workstation_ports(switch_port_id)",
        "CREATE INDEX IF NOT EXISTS idx_mac_history_mac_address ON mac_history(mac_address)",
        "CREATE INDEX IF NOT EXISTS idx_mac_history_ip_address ON mac_history(ip_address)",
        "CREATE INDEX IF NOT EXISTS idx_mac_history_ip_manager_id ON mac_history(ip_manager_id)",
        "CREATE INDEX IF NOT EXISTS idx_mac_history_created_at ON mac_history(created_at)",
        "CREATE INDEX IF NOT EXISTS idx_mac_history_network_id ON mac_history(network_id)",
        "CREATE INDEX IF NOT EXISTS idx_svg_layouts_element_id ON svg_layouts(element_id)",
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
    migrate_ip_managers_constraint(pool).await?;
    migrate_switch_position_fields(pool).await?;
    migrate_mac_history(pool).await?;
    migrate_switch_macs_ip_type(pool).await?;
    migrate_drop_wrong_ip_unique_index(pool).await?;
    migrate_drop_switch_cabinet_fields(pool).await?;
    migrate_mac_history_switch_port_id(pool).await?;
    migrate_log_cleanup_tasks(pool).await?;
    migrate_log_table_partitions(pool).await?;
    migrate_switch_position_link(pool).await?;
    migrate_switch_position_constraint(pool).await?;
    Ok(())
}

async fn migrate_ip_managers_constraint(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    let result = sqlx::query(
        "SELECT COUNT(*) as count FROM pg_constraint WHERE conname = 'chk_device_consistency'",
    )
    .fetch_one(pool)
    .await?;

    let count: i64 = result.try_get("count").unwrap_or(0);

    if count > 0 {
        sqlx::query("ALTER TABLE ip_managers DROP CONSTRAINT IF EXISTS chk_device_consistency")
            .execute(pool)
            .await?;

        sqlx::query("ALTER TABLE ip_managers DROP CONSTRAINT IF EXISTS chk_device_type")
            .execute(pool)
            .await?;

        sqlx::query(
            r#"ALTER TABLE ip_managers ADD CONSTRAINT chk_device_type CHECK (device_type IN ('workstation', 'cabinet_position', 'switch', 'unknown'))"#
        )
        .execute(pool)
        .await?;

        sqlx::query(
            r#"ALTER TABLE ip_managers ADD CONSTRAINT chk_device_consistency CHECK (
                (device_type = 'switch' AND switch_id IS NOT NULL AND workstation_id IS NULL) OR
                (device_type = 'workstation' AND workstation_id IS NOT NULL AND position_id IS NULL AND switch_id IS NULL) OR
                (device_type = 'cabinet_position' AND position_id IS NOT NULL AND workstation_id IS NULL AND switch_id IS NULL) OR
                (device_type = 'unknown' AND workstation_id IS NULL AND position_id IS NULL AND switch_id IS NULL AND switch_port_id IS NULL)
            )"#
        )
        .execute(pool)
        .await?;
    }

    Ok(())
}

async fn migrate_switch_position_fields(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    let result = sqlx::query(
        "SELECT COUNT(*) as count FROM information_schema.columns 
         WHERE table_name = 'switches' AND column_name = 'cabinet_id'",
    )
    .fetch_one(pool)
    .await?;

    let count: i64 = result.try_get("count").unwrap_or(0);

    if count == 0 {
        if let Err(e) = sqlx::query(
            "ALTER TABLE switches 
             ADD COLUMN cabinet_id UUID REFERENCES cabinets(id) ON DELETE SET NULL,
             ADD COLUMN start_u INTEGER,
             ADD COLUMN end_u INTEGER",
        )
        .execute(pool)
        .await
        {
            warn!("switches添加cabinet_id列失败: {}", e);
        }

        if let Err(e) = sqlx::query(
            r#"UPDATE switches s 
               SET cabinet_id = p.cabinet_id, 
                   start_u = p.start_u, 
                   end_u = p.end_u
               FROM positions p
               JOIN ip_managers im ON im.position_id = p.id 
               WHERE im.switch_id = s.id
               AND s.cabinet_id IS NULL"#,
        )
        .execute(pool)
        .await
        {
            warn!("迁移交换机位置数据失败: {}", e);
        }
    }

    Ok(())
}

async fn migrate_mac_history(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    let result = sqlx::query(
        "SELECT COUNT(*) as count FROM information_schema.tables WHERE table_name = 'mac_history'",
    )
    .fetch_one(pool)
    .await?;

    let count: i64 = result.try_get("count").unwrap_or(0);

    if count == 0 {
        sqlx::query(
            r#"CREATE TABLE mac_history (
                id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
                mac_address VARCHAR(20) NOT NULL,
                ip_address INET NOT NULL,
                ip_manager_id UUID NOT NULL REFERENCES ip_managers(id) ON DELETE CASCADE,
                device_type VARCHAR(20) NOT NULL,
                workstation_id UUID REFERENCES workstations(id) ON DELETE SET NULL,
                position_id UUID REFERENCES positions(id) ON DELETE SET NULL,
                switch_id UUID REFERENCES switches(id) ON DELETE SET NULL,
                switch_port_id UUID REFERENCES switch_ports(id) ON DELETE SET NULL,
                network_id UUID NOT NULL REFERENCES network_cidrs(id),
                change_type VARCHAR(20) NOT NULL DEFAULT 'update',
                created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
            )"#,
        )
        .execute(pool)
        .await?;
    }

    let trigger_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM pg_trigger WHERE tgname = 'trg_log_mac_address_change')",
    )
    .fetch_one(pool)
    .await
    .unwrap_or(false);

    if !trigger_exists {
        sqlx::query(r#"
            CREATE OR REPLACE FUNCTION log_mac_address_change() RETURNS TRIGGER AS $$
            BEGIN
                IF TG_OP = 'INSERT' THEN
                    IF NEW.mac_address IS NOT NULL AND NEW.mac_address != '' THEN
                        INSERT INTO mac_history (
                            mac_address, ip_address, ip_manager_id, device_type,
                            workstation_id, position_id, switch_id, switch_port_id, network_id, change_type
                        ) VALUES (
                            NEW.mac_address, NEW.ip_address, NEW.id, NEW.device_type,
                            NEW.workstation_id, NEW.position_id, NEW.switch_id, NEW.switch_port_id, NEW.network_id, 'create'
                        );
                    END IF;
                ELSIF TG_OP = 'UPDATE' THEN
                    IF OLD.mac_address IS DISTINCT FROM NEW.mac_address THEN
                        IF NEW.mac_address IS NOT NULL AND NEW.mac_address != '' THEN
                            INSERT INTO mac_history (
                                mac_address, ip_address, ip_manager_id, device_type,
                                workstation_id, position_id, switch_id, switch_port_id, network_id, change_type
                            ) VALUES (
                                NEW.mac_address, NEW.ip_address, NEW.id, NEW.device_type,
                                NEW.workstation_id, NEW.position_id, NEW.switch_id, NEW.switch_port_id, NEW.network_id, 'update'
                            );
                        END IF;
                    END IF;
                END IF;
                RETURN NEW;
            END;
            $$ LANGUAGE plpgsql;
        "#)
        .execute(pool)
        .await?;

        sqlx::query(
            "CREATE TRIGGER trg_log_mac_address_change AFTER INSERT OR UPDATE OF mac_address ON ip_managers FOR EACH ROW EXECUTE FUNCTION log_mac_address_change()"
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
    if let Err(e) = sqlx::query("DROP INDEX IF EXISTS idx_ip_managers_ip_unique")
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
            r#"CREATE OR REPLACE VIEW switches_with_details AS
            SELECT 
                s.id, s.name, s.network_region_id, s.network_id, s.model, s.vendor,
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
            LEFT JOIN LATERAL (
                SELECT ip_address, mac_address, position_id
                FROM ip_managers 
                WHERE switch_id = s.id AND device_type = 'switch' 
                LIMIT 1
            ) im ON true
            LEFT JOIN positions p ON im.position_id = p.id
            LEFT JOIN cabinets c ON p.cabinet_id = c.id"#,
        )
        .execute(pool)
        .await
        {
            warn!("更新switches_with_details视图失败: {}", e);
        }

        sqlx::query(
            r#"INSERT INTO schema_migrations (version, description) VALUES ('drop_switch_cabinet_fields', 'switches表cabinet_id/start_u/end_u通过ip_managers+positions关联获取')"#
        )
        .execute(pool)
        .await?;
    }

    Ok(())
}

async fn migrate_mac_history_switch_port_id(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    let result = sqlx::query(
        "SELECT COUNT(*) as count FROM information_schema.columns WHERE table_name = 'mac_history' AND column_name = 'switch_port_id'",
    )
    .fetch_one(pool)
    .await?;

    let count: i64 = result.try_get("count").unwrap_or(0);

    if count == 0
        && let Err(e) = sqlx::query(
            "ALTER TABLE mac_history ADD COLUMN switch_port_id UUID REFERENCES switch_ports(id) ON DELETE SET NULL"
        )
        .execute(pool)
        .await
    {
        warn!("mac_history添加switch_port_id列失败: {}", e);
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
            (
                "cleanup_mac_history",
                "log_cleanup",
                "0 5 * * *",
                r#"{"retention_days": 90, "table": "mac_history"}"#,
            ),
        ];

        for (name, task_type, cron, config) in &tasks {
            if let Err(e) = sqlx::query(
                r#"INSERT INTO scheduled_tasks (id, name, task_type, cron_expression, enabled, config, created_at, updated_at)
                   VALUES (uuid_generate_v4(), $1, $2, $3, TRUE, $4::JSONB, NOW(), NOW())
                   ON CONFLICT (name) DO NOTHING"#
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
            r#"INSERT INTO schema_migrations (version, description) VALUES ('log_cleanup_tasks', '添加日志表定期清理任务')"#
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
            ("mac_history", "created_at", "month"),
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
                    "ALTER TABLE {} PARTITION BY RANGE ({})",
                    table_name, partition_col
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
            r#"INSERT INTO schema_migrations (version, description) VALUES ('log_table_partitions', '日志表按时间分区（RANGE by month）')"#
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
            r#"SELECT s.id, s.name, s.cabinet_id, s.start_u, s.end_u, s.description
               FROM switches s
               WHERE s.cabinet_id IS NOT NULL
               AND NOT EXISTS (
                   SELECT 1 FROM ip_managers im WHERE im.switch_id = s.id AND im.position_id IS NOT NULL
               )"#
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

            let pos_id: Uuid = match sqlx::query_scalar::<_, Uuid>(
                "SELECT id FROM positions WHERE name = $1 AND cabinet_id = $2",
            )
            .bind(&name)
            .bind(cabinet_id)
            .fetch_optional(pool)
            .await
            {
                Ok(Some(existing_id)) => existing_id,
                _ => {
                    let new_id = Uuid::new_v4();
                    if let Err(e) = sqlx::query(
                        "INSERT INTO positions (id, name, cabinet_id, start_u, end_u, description, created_at, updated_at) VALUES ($1, $2, $3, $4, $5, $6, NOW(), NOW())"
                    )
                    .bind(new_id)
                    .bind(&name)
                    .bind(cabinet_id)
                    .bind(start_u.unwrap_or(1i32))
                    .bind(end_u.unwrap_or(1i32))
                    .bind(&description)
                    .execute(pool)
                    .await
                    {
                        warn!("为交换机 {} 创建关联机位失败: {}", switch_id, e);
                        continue;
                    }
                    new_id
                }
            };

            if let Err(e) = sqlx::query(
                "UPDATE ip_managers SET position_id = $1 WHERE switch_id = $2 AND position_id IS NULL"
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
            r#"INSERT INTO schema_migrations (version, description) VALUES ('switch_position_link', '为有cabinet_id但无position关联的交换机创建position记录并关联ip_managers')"#
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
        sqlx::query("ALTER TABLE ip_managers DROP CONSTRAINT IF EXISTS chk_device_consistency")
            .execute(pool)
            .await?;

        sqlx::query(
            r#"ALTER TABLE ip_managers ADD CONSTRAINT chk_device_consistency CHECK (
                (device_type = 'switch' AND switch_id IS NOT NULL AND workstation_id IS NULL) OR
                (device_type = 'workstation' AND workstation_id IS NOT NULL AND position_id IS NULL AND switch_id IS NULL) OR
                (device_type = 'cabinet_position' AND position_id IS NOT NULL AND workstation_id IS NULL AND switch_id IS NULL) OR
                (device_type = 'unknown' AND workstation_id IS NULL AND position_id IS NULL AND switch_id IS NULL AND switch_port_id IS NULL)
            )"#
        )
        .execute(pool)
        .await?;

        let switches_without_position = sqlx::query(
            r#"SELECT s.id, s.name, s.cabinet_id, s.start_u, s.end_u, s.description
               FROM switches s
               WHERE s.cabinet_id IS NOT NULL
               AND NOT EXISTS (
                   SELECT 1 FROM ip_managers im WHERE im.switch_id = s.id AND im.position_id IS NOT NULL
               )"#
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

            let pos_id: Uuid = match sqlx::query_scalar::<_, Uuid>(
                "SELECT id FROM positions WHERE name = $1 AND cabinet_id = $2",
            )
            .bind(&name)
            .bind(cabinet_id)
            .fetch_optional(pool)
            .await
            {
                Ok(Some(existing_id)) => existing_id,
                _ => {
                    let new_id = Uuid::new_v4();
                    if let Err(e) = sqlx::query(
                        "INSERT INTO positions (id, name, cabinet_id, start_u, end_u, description, created_at, updated_at) VALUES ($1, $2, $3, $4, $5, $6, NOW(), NOW())"
                    )
                    .bind(new_id)
                    .bind(&name)
                    .bind(cabinet_id)
                    .bind(start_u.unwrap_or(1i32))
                    .bind(end_u.unwrap_or(1i32))
                    .bind(&description)
                    .execute(pool)
                    .await
                    {
                        warn!("为交换机 {} 创建关联机位失败: {}", switch_id, e);
                        continue;
                    }
                    new_id
                }
            };

            if let Err(e) = sqlx::query(
                "UPDATE ip_managers SET position_id = $1 WHERE switch_id = $2 AND position_id IS NULL"
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
            r#"INSERT INTO schema_migrations (version, description) VALUES ('switch_position_constraint', '更新chk_device_consistency约束允许switch类型有position_id，并为现有交换机创建position关联')"#
        )
        .execute(pool)
        .await?;
    }

    Ok(())
}

async fn create_views(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query("DROP VIEW IF EXISTS ip_managers_with_details CASCADE")
        .execute(pool)
        .await?;

    sqlx::query(
        r#"
        CREATE VIEW ip_managers_with_details AS
        SELECT 
            imm.id,
            imm.workstation_id,
            imm.position_id,
            imm.switch_id,
            imm.switch_port_id,
            imm.device_type,
            CASE
                WHEN imm.device_type = 'switch' AND s.id IS NOT NULL THEN s.name::text
                WHEN w.id IS NOT NULL THEN w.name::text
                WHEN cp.id IS NOT NULL THEN cp.name::text
                ELSE '未知设备'
            END AS device_name,
            imm.network_id,
            CASE
                WHEN w.id IS NOT NULL THEN w.name::text
                ELSE NULL
            END AS workstation_name,
            CASE
                WHEN cp.id IS NOT NULL THEN cp.name::text
                ELSE NULL
            END AS cabinet_position_name,
            CASE
                WHEN s.id IS NOT NULL THEN s.name::text
                ELSE NULL
            END AS switch_name,
            sp.port_number::text AS switch_port_number,
            COALESCE(n.name, '未知')::text AS network_name,
            COALESCE(nt.name, '未知')::text AS network_region,
            host(imm.ip_address) as ip_address,
            imm.ip_version,
            imm.mac_address,
            imm.hostname,
            imm.status,
            imm.last_seen,
            imm.created_at,
            imm.updated_at
        FROM ip_managers imm
        LEFT JOIN workstations w ON imm.workstation_id = w.id
        LEFT JOIN positions cp ON imm.position_id = cp.id
        LEFT JOIN switches s ON imm.switch_id = s.id
        LEFT JOIN switch_ports sp ON imm.switch_port_id = sp.id
        LEFT JOIN network_cidrs n ON imm.network_id = n.id
        LEFT JOIN network_regions nt ON n.network_region_id = nt.id
    "#,
    )
    .execute(pool)
    .await?;

    sqlx::query("DROP VIEW IF EXISTS mac_comparison CASCADE")
        .execute(pool)
        .await?;

    sqlx::query(
        r#"
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
        LEFT JOIN ip_managers im ON sm.ip_address = im.ip_address AND im.device_type != 'switch'
    "#,
    )
    .execute(pool)
    .await?;

    Ok(())
}

async fn create_triggers(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        CREATE OR REPLACE FUNCTION update_updated_at_column()
        RETURNS TRIGGER AS $$
        BEGIN
            NEW.updated_at = NOW();
            RETURN NEW;
        END;
        $$ LANGUAGE plpgsql;
    "#,
    )
    .execute(pool)
    .await?;

    let tables_with_updated_at = [
        "users",
        "network_regions",
        "network_cidrs",
        "rooms",
        "room_networks",
        "svg_layouts",
        "cabinets",
        "positions",
        "position_ports",
        "workstations",
        "workstation_ports",
        "switches",
        "switch_ports",
        "switch_macs",
        "switch_lldps",
        "ip_managers",
        "system_configs",
        "scheduled_tasks",
    ];

    for table in &tables_with_updated_at {
        let trigger_name = format!("trg_{}_updated_at", table);
        if let Err(e) = sqlx::query(&format!(
            "CREATE TRIGGER {} BEFORE UPDATE ON {} FOR EACH ROW EXECUTE FUNCTION update_updated_at_column()",
            trigger_name, table
        ))
        .execute(pool)
        .await
        {
            warn!("updated_at触发器创建失败（可能已存在） {}: {}", table, e);
        }
    }

    sqlx::query(
        r#"
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
    "#,
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
        r#"
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
    "#,
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
        r#"
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
    "#,
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

    sqlx::query(r#"
        CREATE OR REPLACE FUNCTION log_mac_address_change() RETURNS TRIGGER AS $$
        BEGIN
            IF TG_OP = 'INSERT' THEN
                IF NEW.mac_address IS NOT NULL AND NEW.mac_address != '' THEN
                    INSERT INTO mac_history (
                        mac_address, ip_address, ip_manager_id, device_type,
                        workstation_id, position_id, switch_id, switch_port_id, network_id, change_type
                    ) VALUES (
                        NEW.mac_address, NEW.ip_address, NEW.id, NEW.device_type,
                        NEW.workstation_id, NEW.position_id, NEW.switch_id, NEW.switch_port_id, NEW.network_id, 'create'
                    );
                END IF;
            ELSIF TG_OP = 'UPDATE' THEN
                IF OLD.mac_address IS DISTINCT FROM NEW.mac_address THEN
                    IF NEW.mac_address IS NOT NULL AND NEW.mac_address != '' THEN
                        INSERT INTO mac_history (
                            mac_address, ip_address, ip_manager_id, device_type,
                            workstation_id, position_id, switch_id, switch_port_id, network_id, change_type
                        ) VALUES (
                            NEW.mac_address, NEW.ip_address, NEW.id, NEW.device_type,
                            NEW.workstation_id, NEW.position_id, NEW.switch_id, NEW.switch_port_id, NEW.network_id, 'update'
                        );
                    END IF;
                END IF;
            END IF;
            RETURN NEW;
        END;
        $$ LANGUAGE plpgsql;
    "#)
    .execute(pool)
    .await?;

    if let Err(e) = sqlx::query("DROP TRIGGER IF EXISTS trg_log_mac_address_change ON ip_managers")
        .execute(pool)
        .await
    {
        warn!("删除旧触发器失败: {}", e);
    }

    sqlx::query(
        "CREATE TRIGGER trg_log_mac_address_change AFTER INSERT OR UPDATE OF mac_address ON ip_managers FOR EACH ROW EXECUTE FUNCTION log_mac_address_change()"
    )
    .execute(pool)
    .await?;

    Ok(())
}
