#!/bin/sh
set -e
echo 删除旧程序
rm -rf /opt/ipma/
rm -f /usr/bin/ipma

echo 创建工作目录
mkdir -p /opt/ipma/

ls /opt/

echo 编译后端
cargo build --release

echo 编译前端
cd /root/ipma/web && npm run build 2>&1 && cd -

echo 安装前端构建产物到 static 目录
cp -f /root/ipma/web/dist/static/index.html /root/ipma/web/static/index.html
cp -f /root/ipma/web/dist/static/main.html /root/ipma/web/static/main.html
mkdir -p /root/ipma/web/static/js/assets
cp -f /root/ipma/web/dist/js/assets/*.js /root/ipma/web/static/js/assets/
mkdir -p /root/ipma/web/static/css
cp -f /root/ipma/web/dist/css/*.css /root/ipma/web/static/css/ 2>/dev/null || true

echo 安装到生产目录
cp -f target/release/ipma /usr/bin/ipma
cp -rf web/ /opt/ipma/
cp -f config.toml /opt/ipma/

echo 重启服务
systemctl restart ipma

systemctl status ipma

exit 0
