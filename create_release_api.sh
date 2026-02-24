#!/bin/bash

# 创建 GitHub Release 的脚本
# 使用 GitHub API

REPO="jsshwqz/patent-hub"
TAG="v0.1.0"
NAME="Patent Hub v0.1.0 - 专利检索分析系统"

# 读取 release notes
BODY=$(cat RELEASE_NOTES_v0.1.0.md)

# 创建 JSON payload
JSON_PAYLOAD=$(jq -n \
  --arg tag "$TAG" \
  --arg name "$NAME" \
  --arg body "$BODY" \
  '{
    tag_name: $tag,
    name: $name,
    body: $body,
    draft: false,
    prerelease: false
  }')

# 创建 release
echo "Creating release..."
RESPONSE=$(curl -X POST \
  -H "Accept: application/vnd.github+json" \
  -H "Authorization: Bearer $GITHUB_TOKEN" \
  -H "X-GitHub-Api-Version: 2022-11-28" \
  https://api.github.com/repos/$REPO/releases \
  -d "$JSON_PAYLOAD")

# 获取 release ID 和 upload URL
RELEASE_ID=$(echo $RESPONSE | jq -r '.id')
UPLOAD_URL=$(echo $RESPONSE | jq -r '.upload_url' | sed 's/{?name,label}//')

echo "Release created!"
echo "Release ID: $RELEASE_ID"
echo "Upload URL: $UPLOAD_URL"

# 上传 Windows 包
if [ -f "patent-hub-v0.1.0-windows-x86_64.zip" ]; then
  echo ""
  echo "Uploading Windows package..."
  curl -X POST \
    -H "Accept: application/vnd.github+json" \
    -H "Authorization: Bearer $GITHUB_TOKEN" \
    -H "Content-Type: application/zip" \
    --data-binary @patent-hub-v0.1.0-windows-x86_64.zip \
    "${UPLOAD_URL}?name=patent-hub-v0.1.0-windows-x86_64.zip"
  echo ""
  echo "Upload complete!"
fi

echo ""
echo "Release URL: https://github.com/$REPO/releases/tag/$TAG"
