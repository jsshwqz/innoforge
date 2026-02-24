#!/bin/bash
# 使用 curl 创建 GitHub Release

OWNER="jsshwqz"
REPO="patent-hub"
TAG="v0.1.0"
NAME="Patent Hub v0.1.0 - 专利检索分析系统"

# Release 描述
BODY=$(cat <<'EOF'
## Patent Hub v0.1.0 - 首个公开发布版本

### 功能特性

#### 核心功能
- ✓ **在线专利搜索** - 集成 SerpAPI，支持全球专利数据库检索
- ✓ **本地数据库** - SQLite 存储，快速访问历史数据
- ✓ **AI 智能分析** - OpenAI 兼容接口，支持智谱 GLM 等模型
- ✓ **专利对比** - 多专利并行对比分析
- ✓ **相似推荐** - 智能推荐相关专利
- ✓ **文件上传** - 支持专利文件上传对比
- ✓ **搜索历史** - 完整的搜索历史记录管理
- ✓ **统计图表** - 数据可视化展示
- ✓ **Excel 导出** - 一键导出分析结果

#### 跨平台支持
- ✓ Windows 10/11 (x86_64)
- ✓ Linux (x86_64)
- ✓ macOS (x86_64)
- ✓ 移动设备访问 (Android/iOS/HarmonyOS)

### 快速开始

1. 下载 `patent-hub-v0.1.0-windows-x86_64.zip`
2. 解压到任意目录
3. 配置 `.env` 文件（复制 `.env.example`）
4. 运行 `start.bat` 或 `patent-hub.exe`
5. 浏览器访问 http://localhost:3000

### API 密钥获取
- **SerpAPI**: https://serpapi.com/
- **AI API**: https://open.bigmodel.cn/

### 技术支持
- **Issues**: https://github.com/jsshwqz/patent-hub/issues
- **文档**: https://github.com/jsshwqz/patent-hub/tree/main/docs

### 许可证
MIT License
EOF
)

echo "Creating GitHub Release..."
echo "Repository: $OWNER/$REPO"
echo "Tag: $TAG"
echo ""

# 检查 GITHUB_TOKEN
if [ -z "$GITHUB_TOKEN" ]; then
    echo "Error: GITHUB_TOKEN environment variable not set!"
    echo ""
    echo "Please set your GitHub Personal Access Token:"
    echo "  export GITHUB_TOKEN='your_token_here'"
    echo ""
    echo "Or create the release manually at:"
    echo "  https://github.com/$OWNER/$REPO/releases/new"
    exit 1
fi

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

# 创建 Release
RESPONSE=$(curl -s -X POST \
    -H "Authorization: token $GITHUB_TOKEN" \
    -H "Accept: application/vnd.github.v3+json" \
    -d "$JSON_PAYLOAD" \
    "https://api.github.com/repos/$OWNER/$REPO/releases")

# 检查是否成功
if echo "$RESPONSE" | grep -q '"id"'; then
    echo "✓ Release created successfully!"
    
    RELEASE_ID=$(echo "$RESPONSE" | grep '"id"' | head -1 | sed 's/.*: \([0-9]*\).*/\1/')
    UPLOAD_URL=$(echo "$RESPONSE" | grep '"upload_url"' | sed 's/.*: "\(.*\){.*/\1/')
    HTML_URL=$(echo "$RESPONSE" | grep '"html_url"' | head -1 | sed 's/.*: "\(.*\)".*/\1/')
    
    echo "Release ID: $RELEASE_ID"
    echo ""
    
    # 上传 ZIP 文件
    ZIP_FILE="patent-hub-v0.1.0-windows-x86_64.zip"
    if [ -f "$ZIP_FILE" ]; then
        echo "Uploading $ZIP_FILE..."
        
        curl -s -X POST \
            -H "Authorization: token $GITHUB_TOKEN" \
            -H "Content-Type: application/zip" \
            --data-binary @"$ZIP_FILE" \
            "${UPLOAD_URL}?name=$ZIP_FILE" > /dev/null
        
        echo "✓ File uploaded successfully!"
    else
        echo "Warning: $ZIP_FILE not found!"
    fi
    
    echo ""
    echo "View release at: $HTML_URL"
else
    echo "Error creating release:"
    echo "$RESPONSE"
    echo ""
    echo "Please create the release manually at:"
    echo "  https://github.com/$OWNER/$REPO/releases/new"
fi
