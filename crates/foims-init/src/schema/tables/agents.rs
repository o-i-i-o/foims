//! 主机监控（monitored_hosts）与指标历史（host_metrics_history）表结构创建。
//!
//! 设计 docs/agent-design.md §5.2：agent 首报凭共享注册令牌鉴权后自动注册
//! （INSERT ... ON CONFLICT (machine_id) DO UPDATE），行初始 active，无
//! pending/激活流程，亦无 per-install token（token_hash 列已移除，上报
//! 鉴权比对系统配置的 enrollment_token 哈希）；唯一性用 partial unique
//! index（仅对已回填 machine_id 行生效）。host_metrics_history 存 JSONB
//! 全量指标供详情页曲线渲染，保留期由调度任务清理。
//!
//! source 区分数据来源：'agent'（FOIMS Agent 主动上报）/'snmp'（服务端
//! SNMP 轮询设备，machine_id 合成 'snmp:{device_id}'，不经上报鉴权）/
//! 'ipmi'（服务端 IPMI 带外轮询设备，machine_id 合成 'ipmi:{device_id}'）；
//! device_id 关联 devices，设备删除级联清理。同一设备的 SNMP 行与 IPMI 行
//! 并存（BMC 管理口与 OS 不同来源，各自独立落库），故 device_id 唯一性按
//! (device_id, source) 组合约束。status/source 均有 CHECK 约束白名单，与其余
//! 枚举列风格一致。

pub async fn create(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS monitored_hosts (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            machine_id TEXT,
            source TEXT NOT NULL DEFAULT 'agent'
                CONSTRAINT chk_monitored_hosts_source CHECK (source IN ('agent', 'snmp', 'ipmi')),
            device_id UUID REFERENCES devices(id) ON DELETE CASCADE,
            label TEXT,
            status TEXT NOT NULL DEFAULT 'active'
                CONSTRAINT chk_monitored_hosts_status CHECK (status IN ('active', 'offline', 'disabled')),
            hostname TEXT,
            ip TEXT,
            os TEXT,
            kernel TEXT,
            arch TEXT,
            agent_version TEXT,
            cpu_usage NUMERIC(5,2),
            mem_usage_pct NUMERIC(5,2),
            disk_usage_pct NUMERIC(5,2),
            max_temp NUMERIC(5,1),
            uptime_secs BIGINT,
            raw_metrics JSONB,
            first_seen TIMESTAMP WITH TIME ZONE,
            last_seen TIMESTAMP WITH TIME ZONE,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
        )",
    )
    .execute(pool)
    .await?;

    // 同机重复上报判重：machine_id 首报自动注册后恒非空，仅对非空行唯一
    sqlx::query(
        r"CREATE UNIQUE INDEX IF NOT EXISTS idx_monitored_hosts_machine_id
            ON monitored_hosts (machine_id) WHERE machine_id IS NOT NULL",
    )
    .execute(pool)
    .await?;

    // 一台设备每个来源至多一条采集记录：同设备 SNMP 行与 IPMI 行并存的前提
    //（snmp_poll/ipmi_poll upsert 判重锚点，device_id 对同一 source 唯一）
    sqlx::query(
        r"CREATE UNIQUE INDEX IF NOT EXISTS idx_monitored_hosts_device_id
            ON monitored_hosts (device_id, source) WHERE device_id IS NOT NULL",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS host_metrics_history (
            id BIGSERIAL PRIMARY KEY,
            agent_id UUID NOT NULL REFERENCES monitored_hosts(id) ON DELETE CASCADE,
            collected_at TIMESTAMP WITH TIME ZONE NOT NULL,
            metrics JSONB NOT NULL
        )",
    )
    .execute(pool)
    .await?;

    // (agent_id, collected_at) 唯一：配合入库 ON CONFLICT DO NOTHING 幂等去重，
    // 防止 agent 重试/重放同一采集时刻重复写重；兼顾曲线查询范围扫描
    sqlx::query(
        r"CREATE UNIQUE INDEX IF NOT EXISTS idx_host_metrics_history_agent_time
            ON host_metrics_history (agent_id, collected_at)",
    )
    .execute(pool)
    .await?;

    // 告警 episode 表：一台主机一个指标的一次「持续超标」过程（pending/active/
    // resolved）落库一行，是告警状态的事实源（内存态为工作副本，重启据此恢复）；
    // 累计次数与通知行 ID 供静默窗口内就地刷新文案，主机删除级联清理
    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS host_alert_episodes (
            host_id UUID NOT NULL REFERENCES monitored_hosts(id) ON DELETE CASCADE,
            metric TEXT NOT NULL
                CONSTRAINT chk_host_alert_episodes_metric CHECK (metric IN ('cpu', 'mem', 'disk', 'temp')),
            status TEXT NOT NULL
                CONSTRAINT chk_host_alert_episodes_status CHECK (status IN ('pending', 'active', 'resolved')),
            started_at TIMESTAMP WITH TIME ZONE NOT NULL,
            opened_at TIMESTAMP WITH TIME ZONE,
            resolved_at TIMESTAMP WITH TIME ZONE,
            last_exceed_at TIMESTAMP WITH TIME ZONE NOT NULL,
            last_notified_at TIMESTAMP WITH TIME ZONE,
            last_value DOUBLE PRECISION,
            threshold SMALLINT NOT NULL,
            exceed_count BIGINT NOT NULL DEFAULT 0,
            notify_ids TEXT NOT NULL DEFAULT '',
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            PRIMARY KEY (host_id, metric)
        )",
    )
    .execute(pool)
    .await?;

    Ok(())
}
