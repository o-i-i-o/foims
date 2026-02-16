#!/bin/sh
set -e
timestamp=$(date +%Y%m%d_%H%M)
dir=../ipma_${timestamp}/
mkdir $dir
ls ..
echo ！！！开始备份！！！

cp Cargo.toml  $dir/
cp config.toml $dir/
cp -r  src     $dir/
cp -r  web     $dir/
cp -r  test    $dir/
cp backup.sh   $dir/
cp git_init.sh $dir/
cp pak.sh      $dir/

echo  。。。。。。
echo ！！！备份结束！！！
