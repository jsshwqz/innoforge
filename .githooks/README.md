# Git 钩子 / Git Hooks

这些钩子在 git commit 前自动运行，是防止忘记跑检查的本地关卡。

## 设置（克隆后只需一次）

```bash
sh .githooks/setup-githooks.sh
```

或 Windows：

```bat
.githooks\setup-githooks.cmd
```

或直接：`git config core.hooksPath .githooks`

## 原理

钩子用 `core.hooksPath` 指向本目录，因此随仓库提交、跨机器共享。

## 注意

- 钩子可被 `git commit --no-verify` 跳过——它是方便层。
- 真正的强制关卡是远端 **CI**（.github/workflows/ci.yml），任何本地跳过都拦不住远端合并。
- 钩子只是入口；核心防线是 check_html_functions.mjs 本身（静态扫描 + 基线回归）。