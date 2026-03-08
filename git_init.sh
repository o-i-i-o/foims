#!/bin/sh
git init
git config --global user.email boss@oi-io.cc
git config --global user.name oi-io0
git remote add gitee git@gitee.com:oi-io0/ipma.git
git branch -m master
echo target/ > .gitignore
