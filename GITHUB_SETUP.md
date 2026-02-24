# GitHub 推送指南

## 第一步：在 GitHub 创建仓库

1. 访问 https://github.com/new
2. 仓库名称：`patent-hub`
3. 描述：`🔍 开源专利检索系统 | Open Source Patent Search System`
4. 选择：Public（公开）
5. **不要**勾选 "Initialize this repository with a README"
6. 点击 "Create repository"

## 第二步：推送代码

在 patent-hub 目录执行：

```bash
# 添加远程仓库（替换 YOUR_USERNAME 为你的 GitHub 用户名）
git remote add origin https://github.com/YOUR_USERNAME/patent-hub.git

# 推送到 GitHub
git branch -M main
git push -u origin main
```

## 第三步：配置仓库

### 添加 Topics

在 GitHub 仓库页面，点击 "Add topics"，添加：
- `rust`
- `patent`
- `search`
- `ai`
- `axum`
- `sqlite`
- `open-source`
- `patent-search`
- `intellectual-property`

### 设置 About

- Website: 你的部署地址（如果有）
- Description: `🔍 开源专利检索与分析系统 - 支持在线搜索、AI分析、专利对比 | Open Source Patent Search & Analysis System`

### 启用 GitHub Pages（可选）

如果要发布文档：
1. Settings > Pages
2. Source: Deploy from a branch
3. Branch: main, /docs

## 第四步：添加徽章

在 README.md 顶部已经包含了徽章：
- License
- Rust version
- Platform support

## 第五步：发布第一个 Release

1. 在 GitHub 仓库页面，点击 "Releases"
2. 点击 "Create a new release"
3. Tag: `v0.1.0`
4. Title: `v0.1.0 - Initial Release`
5. 描述：
   ```markdown
   ## 🎉 首次发布 / Initial Release
   
   ### 功能特性 / Features
   - ✅ 在线专利搜索（SerpAPI + Google Patents）
   - ✅ 本地数据库存储
   - ✅ AI 智能分析
   - ✅ 专利对比
   - ✅ 相似专利推荐
   - ✅ 文件上传对比
   - ✅ 搜索历史
   - ✅ 统计分析图表
   - ✅ Excel 导出
   - ✅ 跨平台支持（Windows/macOS/Linux）
   - ✅ Docker 支持
   - ✅ 移动设备访问
   
   ### 安装 / Installation
   
   详见 [安装指南](docs/INSTALL.md)
   
   ### 快速开始 / Quick Start
   
   详见 [快速开始](docs/QUICK_START.md)
   ```
6. 点击 "Publish release"

## 第六步：社区推广

### 1. 提交到 Awesome Lists

- [awesome-rust](https://github.com/rust-unofficial/awesome-rust)
- [awesome-patent](https://github.com/topics/patent)

### 2. 发布到社区

- Reddit: r/rust, r/opensource
- Hacker News
- 掘金、CSDN（中文社区）
- V2EX

### 3. 写博客介绍

分享开发经验和技术细节

## 常见问题

### Q: 推送失败，提示认证错误？

A: 使用 Personal Access Token：
1. GitHub Settings > Developer settings > Personal access tokens
2. Generate new token (classic)
3. 勾选 `repo` 权限
4. 使用 token 作为密码推送

### Q: 如何更新代码？

```bash
git add .
git commit -m "feat: 添加新功能"
git push
```

### Q: 如何处理冲突？

```bash
git pull --rebase
# 解决冲突
git add .
git rebase --continue
git push
```

## 下一步

- [ ] 完善文档
- [ ] 添加更多测试
- [ ] 收集用户反馈
- [ ] 迭代新功能
- [ ] 建立社区

---

**准备好了吗？开始推送吧！** 🚀
