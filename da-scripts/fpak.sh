#!/bin/sh
set -e

cd /media/oi-io/data/foims

echo 构建前端
(cd frontend && npm run build)

echo 安装到生产目录

echo 删除旧版静态资源
sudo rm -rf /usr/share/foims/web

sudo mkdir -p /usr/share/foims/web
sudo cp -rf frontend/dist/. /usr/share/foims/web/

echo 配置静态资源文件目录权限
sudo find /usr/share/foims/web -type d -exec chmod 755 {} +
sudo find /usr/share/foims/web -type f -exec chmod 644 {} +


echo 重启服务

sudo systemctl restart nginx
sudo systemctl status nginx

exit
