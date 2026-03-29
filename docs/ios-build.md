# Patent Hub iOS 构建指南

本文档介绍如何使用 Tauri v2 构建 Patent Hub 的 iOS 版本。

## 目录

- [环境要求](#环境要求)
- [本地构建步骤](#本地构建步骤)
- [侧载安装（AltStore）](#侧载安装altstore)
- [正式分发（Apple Developer 证书）](#正式分发apple-developer-证书)
- [常见问题](#常见问题)

---

## 环境要求

构建 iOS 应用**必须使用 macOS 系统**，这是 Apple 的硬性限制。

| 依赖项 | 最低版本 | 说明 |
|--------|---------|------|
| macOS | 13.0 (Ventura) | 推荐 14.0+ (Sonoma) |
| Xcode | 15.0+ | 包含 iOS SDK 和模拟器 |
| Rust | 1.70+ | `rustup update stable` |
| Tauri CLI | 2.0+ | `cargo install tauri-cli@^2` |
| iOS target | aarch64-apple-ios | `rustup target add aarch64-apple-ios` |
| Node.js | 18+ | 前端构建需要（如有 package.json）|

## 本地构建步骤

### 1. 安装必要工具

```bash
# 安装/更新 Rust 工具链
rustup update stable

# 添加 iOS 编译目标
rustup target add aarch64-apple-ios

# 安装 Tauri CLI v2
cargo install tauri-cli@^2

# 确认 Xcode 命令行工具已安装
xcode-select --install
```

### 2. 初始化 iOS 项目

在项目根目录执行：

```bash
# 生成 Tauri iOS 项目文件（在 src-tauri/gen/apple/ 下）
cargo tauri ios init
```

此命令会根据 `src-tauri/tauri.conf.json` 中的配置自动生成：
- Xcode 项目文件（`.xcodeproj`）
- Swift 桥接代码
- Info.plist 等 iOS 配置

### 3. 在模拟器中测试

```bash
# 在 iOS 模拟器中运行（不需要 Apple Developer 账号）
cargo tauri ios dev
```

### 4. 构建 Release 版本

```bash
# 构建 Release 版本
cargo tauri ios build

# 构建 Debug 版本（更快，适合调试）
cargo tauri ios build --debug
```

构建产物位于 `src-tauri/gen/apple/build/` 目录下。

### 5. 打包 IPA

```bash
# 找到 .app 文件
APP_PATH=$(find src-tauri/gen/apple -name "*.app" -type d | head -1)

# 打包为 IPA
mkdir -p Payload
cp -r "$APP_PATH" Payload/
zip -r patent-hub-ios.ipa Payload/
rm -rf Payload
```

---

## 侧载安装（AltStore）

如果你没有 Apple Developer 证书（每年 $99），可以通过侧载方式安装到 iPhone/iPad。

### 方法一：AltStore（推荐）

AltStore 是目前最流行的 iOS 侧载工具，免费使用。

**准备工作：**
- 一台 Windows 或 macOS 电脑
- iPhone/iPad 通过 USB 连接电脑
- Apple ID（普通免费账号即可）

**步骤：**

1. **下载 AltServer**
   - 访问 https://altstore.io
   - 下载对应系统的 AltServer

2. **安装 AltStore 到 iPhone**
   - 运行 AltServer
   - 连接 iPhone 到电脑
   - 从 AltServer 菜单选择「Install AltStore」-> 选择你的设备
   - 输入 Apple ID 和密码（用于签名）

3. **安装 Patent Hub IPA**
   - 在 iPhone 上打开 AltStore
   - 点击「My Apps」->「+」
   - 选择 `patent-hub-ios.ipa` 文件
   - 等待安装完成

**注意事项：**
- 免费 Apple ID 签名的应用**每 7 天需要重新签名**
- AltServer 需要在电脑上保持运行才能自动续签
- 免费账号最多同时安装 3 个侧载应用

### 方法二：Sideloadly

Sideloadly 是另一个流行的侧载工具。

1. 下载 Sideloadly：https://sideloadly.io
2. 连接 iPhone 到电脑
3. 将 `patent-hub-ios.ipa` 拖入 Sideloadly 窗口
4. 输入 Apple ID
5. 点击「Start」开始安装

### 安装后信任开发者证书

首次安装后，需要在 iPhone 上信任开发者证书：

1. 打开「设置」->「通用」->「VPN 与设备管理」
2. 找到你的 Apple ID 对应的开发者证书
3. 点击「信任」

---

## 正式分发（Apple Developer 证书）

如果你需要将应用分发给更多用户或上架 App Store，需要加入 Apple Developer Program。

### 1. 注册 Apple Developer 账号

- 访问 https://developer.apple.com/programs/
- 费用：个人 $99/年，企业 $299/年
- 注册审核通常需要 1-3 个工作日

### 2. 创建证书和 Provisioning Profile

```bash
# 在 Xcode 中配置签名
# 打开 src-tauri/gen/apple 下的 Xcode 项目
open src-tauri/gen/apple/*.xcodeproj

# 在 Xcode 中：
# 1. 选择项目 -> Signing & Capabilities
# 2. 勾选 "Automatically manage signing"
# 3. 选择你的 Team（Developer 账号）
# 4. Bundle Identifier 设为 com.patenthub.app
```

### 3. 构建签名版本

```bash
# 使用 Xcode 构建签名的 Archive
# 在 Xcode 中：Product -> Archive

# 或使用命令行
xcodebuild archive \
  -project src-tauri/gen/apple/*.xcodeproj \
  -scheme "Patent Hub" \
  -configuration Release \
  -archivePath build/PatentHub.xcarchive \
  -destination "generic/platform=iOS"
```

### 4. 上架 App Store

1. 在 App Store Connect 创建应用记录
2. 使用 Xcode Organizer 上传 Archive
3. 填写应用信息、截图、描述
4. 提交审核（通常 1-3 天）

### 5. TestFlight 内测分发

TestFlight 适合在正式上架前进行内测：

1. 上传构建到 App Store Connect
2. 添加内部/外部测试员
3. 测试员通过 TestFlight 应用安装

---

## 常见问题

### Q: Windows/Linux 能构建 iOS 应用吗？

**不能。** Apple 要求所有 iOS 应用必须在 macOS 上使用 Xcode 构建。这是操作系统层面的限制，无法绕过。如果没有 Mac，可以考虑：
- 使用 GitHub Actions（本项目已配置，见 `.github/workflows/ios-build.yml`）
- 租用云 Mac 服务（如 MacStadium、AWS EC2 Mac）

### Q: `cargo tauri ios init` 报错怎么办？

确保：
1. `src-tauri/tauri.conf.json` 中有正确的 `identifier`（如 `com.patenthub.app`）
2. Xcode 命令行工具已安装：`xcode-select --install`
3. iOS SDK 可用：`xcrun --sdk iphoneos --show-sdk-path`

### Q: 构建时提示签名错误？

未签名构建时，确保设置了以下环境变量：
```bash
export CODE_SIGNING_ALLOWED=NO
```

或在 Xcode 项目设置中将 Code Signing 设为 "Don't Code Sign"。

### Q: 侧载的应用闪退？

1. 确认已信任开发者证书（设置 -> 通用 -> VPN 与设备管理）
2. 检查签名是否过期（免费账号 7 天有效期）
3. 确认设备 iOS 版本 >= 14.0

### Q: Tauri v2 iOS 与原生 Xcode 项目的区别？

本项目有两种 iOS 构建方式：
- **Tauri v2 iOS**（本文档）：使用 `cargo tauri ios build`，自动管理 WebView + Rust 集成
- **原生 Xcode 项目**（`ios-app/` 目录）：手动管理的 Xcode 项目，在 `release.yml` 中构建

Tauri v2 方式更简单，推荐用于标准场景。原生 Xcode 项目提供更多自定义能力。
