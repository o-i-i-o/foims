use sqlx::Row;

pub async fn create_tables(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query("CREATE EXTENSION IF NOT EXISTS \"uuid-ossp\"")
        .execute(pool)
        .await?;

    sqlx::query("CREATE EXTENSION IF NOT EXISTS \"pgcrypto\"")
        .execute(pool)
        .await?;

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
    create_crypto_functions(pool).await?;
    create_triggers(pool).await?;

    Ok(())
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

    let _ = sqlx::query(
        "ALTER TABLE switches ADD CONSTRAINT fk_parent_port_id FOREIGN KEY (parent_port_id) REFERENCES switch_ports(id) ON DELETE SET NULL"
    ).execute(pool).await;

    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS switch_macs (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            switch_id UUID NOT NULL REFERENCES switches(id) ON DELETE CASCADE,
            ip_address VARCHAR(45) NOT NULL,
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
                (device_type = 'unknown')
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
            network_id UUID NOT NULL REFERENCES network_cidrs(id),
            change_type VARCHAR(20) NOT NULL DEFAULT 'update',
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
        )"#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_mac_history_mac_address ON mac_history(mac_address)"
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_mac_history_ip_address ON mac_history(ip_address)"
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_mac_history_ip_manager_id ON mac_history(ip_manager_id)"
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_mac_history_created_at ON mac_history(created_at)"
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

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_scheduled_tasks_name ON scheduled_tasks(name)")
        .execute(pool)
        .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_scheduled_tasks_enabled ON scheduled_tasks(enabled)")
        .execute(pool)
        .await?;

    Ok(())
}

async fn create_indexes(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    let indexes = [
        "CREATE INDEX IF NOT EXISTS idx_users_username ON users(username)",
        "CREATE INDEX IF NOT EXISTS idx_users_email ON users(email)",
        "CREATE INDEX IF NOT EXISTS idx_ip_managers_workstation_id ON ip_managers(workstation_id)",
        "CREATE INDEX IF NOT EXISTS idx_ip_managers_switch_id ON ip_managers(switch_id) WHERE device_type = 'switch'",
        "CREATE INDEX IF NOT EXISTS idx_ip_managers_ip_address ON ip_managers(ip_address)",
        "CREATE INDEX IF NOT EXISTS idx_ip_managers_mac_address ON ip_managers(mac_address)",
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
        "CREATE INDEX IF NOT EXISTS idx_switch_ports_switch_id ON switch_ports(switch_id)",
        "CREATE INDEX IF NOT EXISTS idx_switch_macs_switch_id ON switch_macs(switch_id)",
        "CREATE INDEX IF NOT EXISTS idx_switch_macs_ip_address ON switch_macs(ip_address)",
        "CREATE INDEX IF NOT EXISTS idx_switch_macs_mac_address ON switch_macs(mac_address)",
        "CREATE INDEX IF NOT EXISTS idx_switch_lldps_switch_id ON switch_lldps(switch_id)",
        "CREATE INDEX IF NOT EXISTS idx_ip_managers_switch_id ON ip_managers(switch_id)",
        "CREATE INDEX IF NOT EXISTS idx_ip_managers_switch_device ON ip_managers(switch_id, device_type)",
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_ip_managers_ip_unique ON ip_managers(ip_address)",
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_ip_managers_mac_unique ON ip_managers(mac_address) WHERE mac_address IS NOT NULL AND mac_address != ''",
    ];

    for idx in &indexes {
        sqlx::query(idx).execute(pool).await?;
    }

    migrate_ip_managers_constraint(pool).await?;
    migrate_switch_position_fields(pool).await?;
    migrate_mac_history(pool).await?;

    Ok(())
}

async fn migrate_ip_managers_constraint(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    let result = sqlx::query(
        "SELECT COUNT(*) as count FROM pg_constraint WHERE conname = 'chk_device_consistency'"
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
                (device_type = 'unknown')
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
         WHERE table_name = 'switches' AND column_name = 'cabinet_id'"
    )
    .fetch_one(pool)
    .await?;

    let count: i64 = result.try_get("count").unwrap_or(0);
    
    if count == 0 {
        sqlx::query(
            "ALTER TABLE switches 
             ADD COLUMN cabinet_id UUID REFERENCES cabinets(id) ON DELETE SET NULL,
             ADD COLUMN start_u INTEGER,
             ADD COLUMN end_u INTEGER"
        )
        .execute(pool)
        .await?;

        sqlx::query(
            r#"UPDATE switches s 
               SET cabinet_id = p.cabinet_id, 
                   start_u = p.start_u, 
                   end_u = p.end_u
               FROM positions p
               JOIN ip_managers im ON im.position_id = p.id 
               WHERE im.switch_id = s.id
               AND s.cabinet_id IS NULL"#
        )
        .execute(pool)
        .await?;
    }

    Ok(())
}

async fn migrate_mac_history(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    let result = sqlx::query(
        "SELECT COUNT(*) as count FROM information_schema.tables WHERE table_name = 'mac_history'"
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
                network_id UUID NOT NULL REFERENCES network_cidrs(id),
                change_type VARCHAR(20) NOT NULL DEFAULT 'update',
                created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
            )"#
        )
        .execute(pool)
        .await?;

        sqlx::query("CREATE INDEX idx_mac_history_mac_address ON mac_history(mac_address)")
            .execute(pool)
            .await?;
        
        sqlx::query("CREATE INDEX idx_mac_history_ip_address ON mac_history(ip_address)")
            .execute(pool)
            .await?;
        
        sqlx::query("CREATE INDEX idx_mac_history_ip_manager_id ON mac_history(ip_manager_id)")
            .execute(pool)
            .await?;
        
        sqlx::query("CREATE INDEX idx_mac_history_created_at ON mac_history(created_at)")
            .execute(pool)
            .await?;
    }

    let trigger_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM pg_trigger WHERE tgname = 'trg_log_mac_address_change')"
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
                            workstation_id, position_id, switch_id, network_id, change_type
                        ) VALUES (
                            NEW.mac_address, NEW.ip_address, NEW.id, NEW.device_type,
                            NEW.workstation_id, NEW.position_id, NEW.switch_id, NEW.network_id, 'create'
                        );
                    END IF;
                ELSIF TG_OP = 'UPDATE' THEN
                    IF OLD.mac_address IS DISTINCT FROM NEW.mac_address THEN
                        IF NEW.mac_address IS NOT NULL AND NEW.mac_address != '' THEN
                            INSERT INTO mac_history (
                                mac_address, ip_address, ip_manager_id, device_type,
                                workstation_id, position_id, switch_id, network_id, change_type
                            ) VALUES (
                                NEW.mac_address, NEW.ip_address, NEW.id, NEW.device_type,
                                NEW.workstation_id, NEW.position_id, NEW.switch_id, NEW.network_id, 'update'
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

async fn create_views(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query("DROP VIEW IF EXISTS ip_managers_with_details CASCADE")
        .execute(pool)
        .await?;

    sqlx::query(r#"
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
        LEFT JOIN rooms r ON w.room_id = r.id
        LEFT JOIN positions cp ON imm.position_id = cp.id
        LEFT JOIN cabinets c ON cp.cabinet_id = c.id
        LEFT JOIN switches s ON imm.switch_id = s.id
        LEFT JOIN switch_ports sp ON imm.switch_port_id = sp.id
        LEFT JOIN network_cidrs n ON imm.network_id = n.id
        LEFT JOIN network_regions nt ON n.network_region_id = nt.id
    "#)
    .execute(pool)
    .await?;

    Ok(())
}

async fn create_crypto_functions(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(r#"
        CREATE TABLE IF NOT EXISTS encryption_keys (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            key_name VARCHAR(50) UNIQUE NOT NULL,
            encryption_key TEXT NOT NULL,
            created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
        )
    "#)
    .execute(pool)
    .await?;

    let key_path = "/etc/ipma/encryption.key";
    let key_base64 = if std::path::Path::new(key_path).exists() {
        if let Ok(key) = std::fs::read(key_path) {
            if key.len() == 32 {
                use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
                BASE64.encode(&key)
            } else {
                tracing::warn!("加密密钥长度不正确，将使用默认密钥");
                "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=".to_string()
            }
        } else {
            tracing::warn!("无法读取加密密钥文件，将使用默认密钥");
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=".to_string()
        }
    } else {
        tracing::warn!("加密密钥文件不存在，将使用默认密钥");
        "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=".to_string()
    };

    sqlx::query(r#"
        INSERT INTO encryption_keys (key_name, encryption_key) 
        VALUES ('system_configs_key', $1)
        ON CONFLICT (key_name) DO UPDATE SET encryption_key = EXCLUDED.encryption_key, updated_at = NOW()
    "#)
    .bind(&key_base64)
    .execute(pool)
    .await?;

    sqlx::query(r#"
        CREATE OR REPLACE FUNCTION encrypt_password(p_password TEXT)
        RETURNS TEXT AS $$
        DECLARE
            v_key BYTEA;
            v_padded BYTEA;
            v_encrypted BYTEA;
            v_padding_len INTEGER;
        BEGIN
            IF p_password IS NULL OR p_password = '' THEN
                RETURN p_password;
            END IF;
            
            SELECT decode(encryption_key, 'base64') INTO v_key 
            FROM encryption_keys 
            WHERE key_name = 'system_configs_key';
            
            IF v_key IS NULL THEN
                RAISE EXCEPTION 'Encryption key not found';
            END IF;
            
            v_padding_len := 16 - (length(p_password::BYTEA) % 16);
            v_padded := p_password::BYTEA || repeat(chr(v_padding_len), v_padding_len)::BYTEA;
            
            v_encrypted := encrypt(v_padded, v_key, 'aes-ecb/pad:none');
            
            RETURN encode(v_encrypted, 'base64');
        END;
        $$ LANGUAGE plpgsql STRICT IMMUTABLE;
    "#)
    .execute(pool)
    .await?;

    sqlx::query(r#"
        CREATE OR REPLACE FUNCTION decrypt_password(p_encrypted TEXT)
        RETURNS TEXT AS $$
        DECLARE
            v_key BYTEA;
            v_ciphertext BYTEA;
            v_decrypted BYTEA;
            v_padding_val INTEGER;
        BEGIN
            IF p_encrypted IS NULL OR p_encrypted = '' THEN
                RETURN p_encrypted;
            END IF;
            
            SELECT decode(encryption_key, 'base64') INTO v_key 
            FROM encryption_keys 
            WHERE key_name = 'system_configs_key';
            
            IF v_key IS NULL THEN
                RAISE EXCEPTION 'Encryption key not found';
            END IF;
            
            v_ciphertext := decode(p_encrypted, 'base64');
            
            v_decrypted := decrypt(v_ciphertext, v_key, 'aes-ecb/pad:none');
            
            v_padding_val := get_byte(v_decrypted, length(v_decrypted) - 1);
            
            IF v_padding_val > 0 AND v_padding_val <= 16 THEN
                v_decrypted := substring(v_decrypted, 1, length(v_decrypted) - v_padding_val);
            END IF;
            
            RETURN convert_from(v_decrypted, 'UTF8');
        END;
        $$ LANGUAGE plpgsql STRICT IMMUTABLE;
    "#)
    .execute(pool)
    .await?;

    sqlx::query(r#"
        DROP VIEW IF EXISTS switches_with_details;
        CREATE VIEW switches_with_details AS
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
            s.cabinet_id, c.name as cabinet_name,
            s.start_u, s.end_u,
            s.description,
            'switch' as device_type,
            host(im.ip_address) as ip_address,
            im.mac_address,
            s.created_at, s.updated_at
        FROM switches s
        LEFT JOIN switches ps ON s.parent_switch_id = ps.id
        LEFT JOIN switch_ports pp ON s.parent_port_id = pp.id
        LEFT JOIN cabinets c ON s.cabinet_id = c.id
        LEFT JOIN LATERAL (
            SELECT ip_address, mac_address 
            FROM ip_managers 
            WHERE switch_id = s.id AND device_type = 'switch' 
            LIMIT 1
        ) im ON true
    "#)
    .execute(pool)
    .await?;

    Ok(())
}

async fn create_triggers(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(r#"
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
    "#)
    .execute(pool)
    .await?;

    let _ = sqlx::query(
        "DROP TRIGGER IF EXISTS trg_check_position_overlap ON positions"
    ).execute(pool).await;

    sqlx::query(
        "CREATE TRIGGER trg_check_position_overlap BEFORE INSERT OR UPDATE ON positions FOR EACH ROW EXECUTE FUNCTION check_position_overlap()"
    )
    .execute(pool)
    .await?;

    sqlx::query(r#"
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
    "#)
    .execute(pool)
    .await?;

    let _ = sqlx::query(
        "DROP TRIGGER IF EXISTS trg_check_switch_circular_dependency ON switches"
    ).execute(pool).await;

    sqlx::query(
        "CREATE TRIGGER trg_check_switch_circular_dependency BEFORE INSERT OR UPDATE OF parent_switch_id ON switches FOR EACH ROW EXECUTE FUNCTION check_switch_circular_dependency()"
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
                        workstation_id, position_id, switch_id, network_id, change_type
                    ) VALUES (
                        NEW.mac_address, NEW.ip_address, NEW.id, NEW.device_type,
                        NEW.workstation_id, NEW.position_id, NEW.switch_id, NEW.network_id, 'create'
                    );
                END IF;
            ELSIF TG_OP = 'UPDATE' THEN
                IF OLD.mac_address IS DISTINCT FROM NEW.mac_address THEN
                    IF NEW.mac_address IS NOT NULL AND NEW.mac_address != '' THEN
                        INSERT INTO mac_history (
                            mac_address, ip_address, ip_manager_id, device_type,
                            workstation_id, position_id, switch_id, network_id, change_type
                        ) VALUES (
                            NEW.mac_address, NEW.ip_address, NEW.id, NEW.device_type,
                            NEW.workstation_id, NEW.position_id, NEW.switch_id, NEW.network_id, 'update'
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

    let _ = sqlx::query(
        "DROP TRIGGER IF EXISTS trg_log_mac_address_change ON ip_managers"
    ).execute(pool).await;

    sqlx::query(
        "CREATE TRIGGER trg_log_mac_address_change AFTER INSERT OR UPDATE OF mac_address ON ip_managers FOR EACH ROW EXECUTE FUNCTION log_mac_address_change()"
    )
    .execute(pool)
    .await?;

    Ok(())
}
