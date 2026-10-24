#!/bin/sh
set -e

WORKDIR=$(pwd)
cd /media/oi-io/data/foims

echo 编译后端
cargo build --release

echo 构建前端
(cd frontend && npm run build)

echo 安装到生产目录

echo 删除旧版静态资源
sudo rm -rf /usr/share/foims/web

echo 删除旧版程序
sudo rm -f /usr/bin/foims

sudo cp -f target/release/foims /usr/bin/foims

sudo mkdir -p /usr/share/foims/web
sudo cp -rf frontend/dist/. /usr/share/foims/web/

echo 配置静态资源文件目录权限
sudo find /usr/share/foims/web -type d -exec chmod 755 {} +
sudo find /usr/share/foims/web -type f -exec chmod 644 {} +

echo 拷贝配置文件到 /etc/foims/config.toml
sudo mkdir -p /etc/foims
sudo cp -f config.toml /etc/foims/config.toml

echo 更新Nginx配置文件
sudo rm -f /etc/nginx/conf.d/foims.conf
sudo cp -f deploy/nginx/foims.conf /etc/nginx/conf.d/foims.conf

echo 更新foims服务文件
sudo rm -f /etc/systemd/system/foims.service
sudo cp -f deploy/services/foims.service /etc/systemd/system/foims.service
sudo systemctl daemon-reload


echo 重启服务
sudo systemctl restart foims
sudo systemctl restart nginx
sudo systemctl status foims
sudo systemctl status nginx

exit
