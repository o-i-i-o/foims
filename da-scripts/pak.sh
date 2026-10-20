#!/bin/sh
set -e
echo 删除旧程序
sudo rm -rf /opt/foims/
sudo rm -f /usr/bin/foims

echo 创建工作目录
sudo mkdir -p /opt/foims/    

echo 列出 /opt目录下的所有文件
sudo ls -l /opt/

echo 编译后端
cargo build --release

echo 构建前端
(cd frontend && npm run build)

echo 安装到生产目录
sudo cp -f target/release/foims /usr/bin/foims
sudo mkdir -p /opt/foims/web
sudo cp -rf frontend/dist /opt/foims/web/static
sudo cp -f config.toml /opt/foims/

echo 重启服务
sudo systemctl restart foims
sudo systemctl restart nginx
sudo systemctl status foims
sudo systemctl status nginx

exit 0
