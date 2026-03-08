#!/bin/bash

set -e

echo "=== IPMA PostgreSQL 初始化脚本 ==="
echo ""

PG_USER="${PG_USER:-ipma}"
PG_PASSWORD="${PG_PASSWORD:-}"
PG_DATABASE="${PG_DATABASE:-ipma}"
PG_HOST="${PG_HOST:-localhost}"
PG_PORT="${PG_PORT:-5432}"

if [ -z "$PG_PASSWORD" ]; then
    echo "错误: 请设置环境变量 PG_PASSWORD"
    echo "用法: PG_PASSWORD=your_password $0"
    echo ""
    echo "可选环境变量:"
    echo "  PG_USER      - 数据库用户名 (默认: ipma)"
    echo "  PG_PASSWORD  - 数据库密码 (必需)"
    echo "  PG_DATABASE  - 数据库名称 (默认: ipma)"
    echo "  PG_HOST      - 数据库主机 (默认: localhost)"
    echo "  PG_PORT      - 数据库端口 (默认: 5432)"
    exit 1
fi

echo "配置信息:"
echo "  用户名: $PG_USER"
echo "  数据库: $PG_DATABASE"
echo "  主机: $PG_HOST:$PG_PORT"
echo ""

if ! command -v psql &> /dev/null; then
    echo "错误: psql 命令未找到，请先安装 PostgreSQL"
    exit 1
fi

if ! systemctl is-active --quiet postgresql 2>/dev/null; then
    echo "启动 PostgreSQL 服务..."
    systemctl start postgresql
    systemctl enable postgresql
fi

echo "创建数据库用户和数据库..."

if [ "$PG_HOST" = "localhost" ] || [ "$PG_HOST" = "127.0.0.1" ]; then
    sudo -u postgres psql -v ON_ERROR_STOP=1 << EOSQL
-- 创建用户（带创建数据库权限）
DO \$\$
BEGIN
    IF NOT EXISTS (SELECT FROM pg_catalog.pg_roles WHERE rolname = '$PG_USER') THEN
        CREATE USER $PG_USER WITH PASSWORD '$PG_PASSWORD' CREATEDB;
        RAISE NOTICE '用户 $PG_USER 创建成功（带 CREATEDB 权限）';
    ELSE
        ALTER USER $PG_USER WITH PASSWORD '$PG_PASSWORD' CREATEDB;
        RAISE NOTICE '用户 $PG_USER 已存在，密码和权限已更新';
    END IF;
END
\$\$;

-- 创建数据库（如果不存在）
SELECT 'CREATE DATABASE $PG_DATABASE OWNER $PG_USER'
WHERE NOT EXISTS (SELECT FROM pg_database WHERE datname = '$PG_DATABASE')\gexec

-- 授权
GRANT ALL PRIVILEGES ON DATABASE $PG_DATABASE TO $PG_USER;

-- 连接到新数据库并授权 schema
\c $PG_DATABASE

GRANT ALL ON SCHEMA public TO $PG_USER;
GRANT ALL PRIVILEGES ON ALL TABLES IN SCHEMA public TO $PG_USER;
GRANT ALL PRIVILEGES ON ALL SEQUENCES IN SCHEMA public TO $PG_USER;
ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT ALL ON TABLES TO $PG_USER;
ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT ALL ON SEQUENCES TO $PG_USER;
EOSQL

    echo ""
    echo "PostgreSQL 初始化完成!"
else
    echo "远程数据库模式，请确保数据库已创建并有访问权限"
fi

CONFIG_FILE="/etc/ipma/config.toml"

if [ -f "$CONFIG_FILE" ]; then
    echo ""
    echo "更新配置文件: $CONFIG_FILE"
    
    sed -i "s/^username = .*/username = \"$PG_USER\"/" "$CONFIG_FILE"
    sed -i "s/^password = .*/password = \"$PG_PASSWORD\"/" "$CONFIG_FILE"
    sed -i "s/^database = .*/database = \"$PG_DATABASE\"/" "$CONFIG_FILE"
    sed -i "s/^host = .*/host = \"$PG_HOST\"/" "$CONFIG_FILE"
    sed -i "s/^port = .*/port = $PG_PORT/" "$CONFIG_FILE"
    
    echo "配置文件已更新"
fi

echo ""
echo "=== 初始化完成 ==="
echo ""
echo "测试连接:"
echo "  PGPASSWORD=$PG_PASSWORD psql -U $PG_USER -h $PG_HOST -d $PG_DATABASE"
echo ""
echo "启动服务:"
echo "  systemctl restart ipma"
echo ""
echo "查看日志:"
echo "  journalctl -u ipma -f"
