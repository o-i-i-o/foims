#!/bin/sh
set -e
echo 删除旧程序
sudo rm -f /usr/bin/foims
# 旧版静态资源目录（/opt/foims）已废弃：统一到 /usr/share/foims/web
sudo rm -rf /opt/foims

echo 编译后端
cargo build --release

echo 构建前端
(cd frontend && npm run build)

echo 安装到生产目录
sudo cp -f target/release/foims /usr/bin/foims
# 前端静态资源：Linux 标准 share 布局，nginx $web_dir 直接指向该目录
# （入口页 index/init_index/main.html 与 assets/、icons/、favicon.ico 同级）
sudo rm -rf /usr/share/foims/web
sudo mkdir -p /usr/share/foims/web
sudo cp -rf frontend/dist/. /usr/share/foims/web/
# 权限与安装包一致：目录 755、文件 644（nginx worker 只读访问）
sudo find /usr/share/foims/web -type d -exec chmod 755 {} +
sudo find /usr/share/foims/web -type f -exec chmod 644 {} +
# 配置唯一落点 /etc/foims/config.toml
sudo mkdir -p /etc/foims
sudo cp -f config.toml /etc/foims/config.toml
# 数据库备份目录（与安装包 postinst 的属主/权限一致）
sudo install -d -o foims -g foims -m 750 /usr/share/foims/backups

echo 重启服务
sudo systemctl restart foims
sudo systemctl restart nginx
sudo systemctl status foims
sudo systemctl status nginx

exit 0
