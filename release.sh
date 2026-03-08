#!/bin/sh
set -e
timestamp=$(date +%Y%m%d_%H%M)
dir=../ipma_release/
rm -rf $dir
mkdir $dir

cp Cargo.toml  $dir/
cp config.toml $dir/
cp -r  src     $dir/
cp -r  web     $dir/
cp README.md   $dir/
cp LICENSE     $dir/
cp NOTICE      $dir/
cp git_init.sh $dir/
rm -rf $dir/web/.trae

echo  。。。。。。
echo ！！！结束！！！
