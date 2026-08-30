#!/bin/bash

# 设置数据库连接信息
HOST=localhost
PORT=5432
USER=foims
DB=foims
PASSWORD=admin123

# 获取所有表名
TABLES=$(PGPASSWORD=$PASSWORD psql -h $HOST -p $PORT -U $USER -d $DB -t -c "\dt" | awk '{print $3}')

# 遍历所有表并查询结构
for TABLE in $TABLES; do
    echo "\n======================================="
    echo "表名: $TABLE"
    echo "======================================="
    PGPASSWORD=$PASSWORD psql -h $HOST -p $PORT -U $USER -d $DB -c "\d $TABLE"
    echo "\n"
done
