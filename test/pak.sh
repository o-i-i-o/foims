#!/bin/sh
set -e
echo 删除旧程序
rm -rf /opt/foims/
rm -f /usr/bin/foims

echo 创建工作目录
mkdir -p /opt/foims/

ls /opt/

echo 编译后端
cargo build --release

echo 安装到生产目录
cp -f target/release/foims /usr/bin/foims
cp -rf web/ /opt/foims/
cp -f config.toml /opt/foims/

echo 重启服务
systemctl restart foims

systemctl status foims

exit 0
