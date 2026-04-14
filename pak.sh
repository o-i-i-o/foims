#!/bin/sh
set -e
echo 删除旧程序
rm -rf /opt/ipma/
rm -f /usr/bin/ipma

echo 创建工作目录
mkdir  -p /opt/ipma/

ls /opt/

echo 编译
cargo build --release

echo 安装
cp -f  target/release/ipma /usr/bin/ipma
cp -rf web/ /opt/ipma/
cp -f  config.toml /opt/ipma/

echo 重启服务
systemctl restart ipma

systemctl status ipma

exit 0
