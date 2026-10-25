#!/bin/sh
set -e

NGINXCONF=/etc/nginx/nginx.conf
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

echo 更新Nginx配置文件
sudo rm -f /etc/nginx/conf.d/foims-dev.conf
sudo rm -f /etc/nginx/conf.d/foims.conf
sudo cp -f deploy/nginx/foims-dev.conf /etc/nginx/conf.d/foims-dev.conf

echo 拷贝配置文件到 /etc/foims/config.toml
sudo mkdir -p /etc/foims
sudo cp -f config.toml /etc/foims/config.toml

echo 修改配置文件中sock用户归属和nginx运行用户一致
# 1) 从 nginx.conf 提取 user 的值（去掉分号、空白）
NGINX_USER=$(sudo sed -n 's/^[[:space:]]*user[[:space:]]\+\([^;]*\);.*/\1/p' /etc/nginx/nginx.conf | head -n1 | xargs)

# 2) 用该值替换 config.toml 中 uds_group 的双引号内内容
sudo sed -i -E "s/^([[:space:]]*uds_group[[:space:]]*=[[:space:]]*\")[^\"]*(\".*)$/\1${NGINX_USER}\2/" /etc/foims/config.toml


echo 更新foims服务文件
sudo rm -f /etc/systemd/system/foims.service
sudo cp -f deploy/services/foims.service /etc/systemd/system/foims.service
sudo systemctl daemon-reload


echo 重启服务
sudo systemctl restart foims
sudo systemctl status foims

sudo systemctl restart nginx
sudo systemctl status nginx

exit
