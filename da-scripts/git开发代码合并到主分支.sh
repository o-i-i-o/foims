#!/bin/sh
set -e # 任意命令失败，脚本直接退出

#开发分支
DEV_BRANCH=Dev-vue3
#主分支
MASTER_BRANCH=master



echo "===== 开始更新 master 分支 ====="
echo "1. 拉取远程最新代码..."
git fetch origin

echo "2. 检查：master 是否存在 Dev 以外的提交"
# Dev..master：在master有，但Dev没有的提交。有输出代表master领先Dev，不能快进
COMMITS=$(git log --oneline $DEV_BRANCH..$MASTER_BRANCH)
if [ -n "$COMMITS" ]; then
  echo "❌ 检测到 $MASTER_BRANCH 存在 $DEV_BRANCH 没有的提交，无法快进合并！"
  echo "$COMMITS"
  exit 1
fi

# 可选：校验本地master是否和远程origin/master同步
# git log --oneline origin/master..master
# if [ -n "$(git log --oneline origin/master..master)" ]; then
#   echo "本地master比远程新，终止"
#   exit 1
# fi

echo "✅ 校验通过，切换到 master"
git checkout $MASTER_BRANCH
# 把本地master更新到远程origin/master最新（重要！）
git merge --ff-only origin/$MASTER_BRANCH

echo "合并 Dev 到 master（仅允许快进）"
git merge $DEV_BRANCH --ff-only

echo "推送 master 到远程 origin"
git push origin $MASTER_BRANCH

echo "切回 Dev 分支"
git checkout $DEV_BRANCH

echo "===== 完成！$MASTER_BRANCH 已同步 $DEV_BRANCH ====="
