# 1. 拉取远程最新状态，确认 master 没有额外改动
git fetch origin
git log --oneline Dev..master        # 输出为空 → master 无额外提交，可安全快进

# 2. 切换到 master 并快进合并 Dev（--ff-only 保证不产生多余合并提交）
git checkout master
git merge Dev --ff-only

# 3. 推送到远程
git push origin master

# 4. 切回开发分支继续工作
git checkout Dev