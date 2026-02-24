#!/bin/bash
# 推送到 GitHub

echo "========================================"
echo "  Patent Hub - GitHub 推送工具"
echo "========================================"
echo ""

echo "[提示] 请先在 GitHub 创建仓库："
echo "  1. 访问 https://github.com/new"
echo "  2. 仓库名：patent-hub"
echo "  3. 类型：Public"
echo "  4. 不要勾选 'Initialize with README'"
echo ""

read -p "请输入你的 GitHub 用户名: " username
if [ -z "$username" ]; then
    echo "[错误] 用户名不能为空"
    exit 1
fi

echo ""
echo "[步骤 1/3] 添加远程仓库..."
git remote remove origin 2>/dev/null
git remote add origin https://github.com/$username/patent-hub.git
if [ $? -ne 0 ]; then
    echo "[错误] 添加远程仓库失败"
    exit 1
fi
echo "✓ 远程仓库已添加"

echo ""
echo "[步骤 2/3] 重命名分支为 main..."
git branch -M main
echo "✓ 分支已重命名"

echo ""
echo "[步骤 3/3] 推送到 GitHub..."
echo "[提示] 如果提示输入密码，请使用 Personal Access Token"
echo "       获取方式：GitHub Settings > Developer settings > Personal access tokens"
echo ""
git push -u origin main
if [ $? -ne 0 ]; then
    echo ""
    echo "[错误] 推送失败！"
    echo ""
    echo "可能的原因："
    echo "  1. 认证失败 - 需要使用 Personal Access Token"
    echo "  2. 仓库不存在 - 请先在 GitHub 创建仓库"
    echo "  3. 网络问题 - 检查网络连接"
    echo ""
    exit 1
fi

echo ""
echo "========================================"
echo "  ✓ 推送成功！"
echo "========================================"
echo ""
echo "仓库地址：https://github.com/$username/patent-hub"
echo ""
echo "下一步："
echo "  1. 访问仓库页面"
echo "  2. 添加 Topics: rust, patent, search, ai"
echo "  3. 发布第一个 Release (v0.1.0)"
echo "  4. 详见 GITHUB_SETUP.md"
echo ""
