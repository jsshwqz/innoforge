#!/bin/bash
# =============================================================================
# Patent Hub - HarmonyOS HAP 构建脚本
# Patent Hub - HarmonyOS HAP Build Script
#
# 前置条件 / Prerequisites:
#   1. 安装 DevEco Studio 5.0+ (NEXT)
#      Install DevEco Studio 5.0+ (NEXT)
#      https://developer.huawei.com/consumer/cn/deveco-studio/
#
#   2. 安装 HarmonyOS SDK (API 12+)
#      Install HarmonyOS SDK (API 12+)
#      可通过 DevEco Studio > Settings > SDK 管理器安装
#      Install via DevEco Studio > Settings > SDK Manager
#
#   3. 安装 ohpm (OpenHarmony Package Manager)
#      Install ohpm (OpenHarmony Package Manager)
#      通常随 DevEco Studio 一起安装
#      Usually installed with DevEco Studio
#
#   4. 配置环境变量 / Configure environment variables:
#      export DEVECO_SDK_HOME=/path/to/sdk   (HarmonyOS SDK 路径)
#      export NODE_HOME=/path/to/node        (Node.js 路径，DevEco 内置)
#
# 用法 / Usage:
#   ./build.sh [debug|release]
#
# 输出 / Output:
#   entry/build/default/outputs/default/entry-default-signed.hap
# =============================================================================

set -e

BUILD_MODE="${1:-debug}"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

echo "============================================"
echo " Patent Hub - HarmonyOS HAP 构建"
echo " Patent Hub - HarmonyOS HAP Build"
echo " 构建模式 / Build mode: ${BUILD_MODE}"
echo "============================================"

# ── 检查环境 / Check environment ──────────────────────────────────────────────

check_command() {
  if ! command -v "$1" &> /dev/null; then
    echo "错误 / Error: $1 未找到 / not found"
    echo "请安装 DevEco Studio 并配置环境变量"
    echo "Please install DevEco Studio and configure environment variables"
    exit 1
  fi
}

check_command hvigorw 2>/dev/null || {
  # 尝试使用项目内的 hvigorw / Try project-local hvigorw
  if [ -f "${SCRIPT_DIR}/hvigorw" ]; then
    HVIGOR="${SCRIPT_DIR}/hvigorw"
  elif [ -f "${SCRIPT_DIR}/hvigorw.js" ]; then
    HVIGOR="node ${SCRIPT_DIR}/hvigorw.js"
  else
    echo "警告 / Warning: hvigorw 未找到，尝试使用 hvigor CLI"
    echo "Warning: hvigorw not found, trying hvigor CLI"
    check_command hvigor
    HVIGOR="hvigor"
  fi
}
HVIGOR="${HVIGOR:-hvigorw}"

cd "${SCRIPT_DIR}"

# ── 安装依赖 / Install dependencies ──────────────────────────────────────────

echo ""
echo ">> 安装 ohpm 依赖 / Installing ohpm dependencies..."
if command -v ohpm &> /dev/null; then
  ohpm install
else
  echo "警告: ohpm 未找到，跳过依赖安装"
  echo "Warning: ohpm not found, skipping dependency installation"
fi

# ── 构建 HAP / Build HAP ─────────────────────────────────────────────────────

echo ""
echo ">> 构建 HAP 包 / Building HAP package..."
if [ "${BUILD_MODE}" = "release" ]; then
  ${HVIGOR} assembleHap --mode module \
    -p product=default \
    -p buildMode=release \
    --no-daemon
else
  ${HVIGOR} assembleHap --mode module \
    -p product=default \
    -p buildMode=debug \
    --no-daemon
fi

# ── 检查输出 / Check output ──────────────────────────────────────────────────

HAP_FILE=$(find entry/build -name "*.hap" -type f 2>/dev/null | head -1)

if [ -n "${HAP_FILE}" ]; then
  HAP_SIZE=$(stat -f%z "${HAP_FILE}" 2>/dev/null || stat -c%s "${HAP_FILE}" 2>/dev/null)
  echo ""
  echo "============================================"
  echo " 构建成功 / Build successful!"
  echo " HAP: ${HAP_FILE}"
  echo " 大小 / Size: ${HAP_SIZE} bytes"
  echo "============================================"

  # 复制到项目根目录方便上传 / Copy to project root for upload
  cp "${HAP_FILE}" "${SCRIPT_DIR}/../patent-hub-harmonyos.hap" 2>/dev/null || true
else
  echo ""
  echo "错误: 未找到 HAP 输出文件 / Error: HAP output file not found"
  echo "请检查构建日志 / Please check build logs"
  exit 1
fi
