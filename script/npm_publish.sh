#!/usr/bin/env bash

# ntd npm 发布脚本
# 用法: ./npm_publish.sh v0.1.2
#
# 需要先执行交叉编译: make cross-build

set -e

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

info()  { echo -e "${BLUE}[INFO]${NC} $1"; }
success(){ echo -e "${GREEN}[SUCCESS]${NC} $1"; }
warning(){ echo -e "${YELLOW}[WARNING]${NC} $1"; }
error() { echo -e "${RED}[ERROR]${NC} $1"; }

# 检查参数
if [ $# -ne 1 ]; then
    error "请指定版本号"
    echo "用法: $0 v0.1.2"
    echo "示例:"
    echo "  $0 v0.1.2         # 发布正式版本"
    echo "  $0 v0.2.0-beta.1  # 发布 beta 版本"
    exit 1
fi

VERSION_TAG=$1

# 验证版本号格式（必须以 v 开头）
if [[ ! $VERSION_TAG =~ ^v[0-9]+\.[0-9]+\.[0-9]+ ]]; then
    error "版本号格式错误: $VERSION_TAG"
    error "版本号必须以 'v' 开头，如 v0.1.2, v0.2.0-beta.1"
    exit 1
fi

# 提取版本号（去掉 v 前缀）
VERSION=${VERSION_TAG#v}

info "========================================"
info "开始发布 ntd"
info "版本号: $VERSION_TAG (npm: $VERSION)"
info "========================================"
echo ""

# 切换到项目根目录
cd "$(dirname "$0")/.."
PROJECT_ROOT=$(pwd)

# 步骤 1: 检查交叉编译产物，不存在则自动构建
info "步骤 1/7: 检查交叉编译产物..."
CROSS_DIR="$PROJECT_ROOT/backend/target/cross"

PLATFORMS=(
    "ntd-linux-x64:linux-x64:linux:x64:ntd-x86_64-unknown-linux-gnu"
    "ntd-linux-arm64:linux-arm64:linux:arm64:ntd-aarch64-unknown-linux-gnu"
    "ntd-darwin-arm64:darwin-arm64:darwin:arm64:ntd-aarch64-apple-darwin"
    "ntd-windows-x64:windows-x64:win32:x64:ntd-x86_64-pc-windows-gnu.exe"
)

# 检查是否需要构建
need_build=false
if [ ! -d "$CROSS_DIR" ]; then
    need_build=true
else
    for entry in "${PLATFORMS[@]}"; do
        IFS=':' read -r pkg_name platform binary <<< "$entry"
        if [ ! -f "$CROSS_DIR/$binary" ]; then
            need_build=true
            break
        fi
    done
fi

if [ "$need_build" = true ]; then
    warning "交叉编译目录不存在或产物不全，自动执行构建..."
    info "  执行: make build"
    make build || { error "make build 失败"; exit 1; }
    info "  执行: make cross-build"
    make cross-build || { error "make cross-build 失败"; exit 1; }
    success "构建完成"
else
    info "交叉编译产物已存在，跳过构建"
fi
echo ""

# 步骤 2: 检查工作区是否干净
info "步骤 2/7: 检查工作区状态..."
if [ -n "$(git status --porcelain)" ]; then
    error "工作区有未提交的更改，请先提交或清理"
    git status --short
    exit 1
fi
success "工作区干净"
echo ""

# 步骤 3: 创建 Git Tag
info "步骤 3/7: 创建 Git Tag: $VERSION_TAG..."
if git rev-parse "$VERSION_TAG" >/dev/null 2>&1; then
    warning "标签 $VERSION_TAG 已存在，删除并重新创建"
    git tag -d "$VERSION_TAG"
fi
git tag "$VERSION_TAG"
success "标签 $VERSION_TAG 创建成功"
echo ""

# 步骤 4: 同步版本号到各 platform package.json
info "步骤 4/7: 同步版本号到 package.json..."
for entry in "${PLATFORMS[@]}"; do
    IFS=':' read -r pkg_name platform <<< "$entry"
    pkg_json="$PROJECT_ROOT/packages/$pkg_name/package.json"
    if [ -f "$pkg_json" ]; then
        node -e "const fs=require('fs'); const p=JSON.parse(fs.readFileSync('$pkg_json','utf8')); p.version='$VERSION'; fs.writeFileSync('$pkg_json', JSON.stringify(p, null, 2)+'\n');"
        success "  $pkg_name: $VERSION"
    fi
done

# 同步主包
node -e "const fs=require('fs'); const p=JSON.parse(fs.readFileSync('$PROJECT_ROOT/packages/ntd/package.json','utf8')); p.version='$VERSION'; fs.writeFileSync('$PROJECT_ROOT/packages/ntd/package.json', JSON.stringify(p, null, 2)+'\n');"
success "  ntd (wrapper): $VERSION"
echo ""

# 步骤 5: 提交版本更新
info "步骤 5/7: 提交版本更新..."
git add packages/
git commit -m "chore: bump version to $VERSION"
success "版本更新已提交"
echo ""

# 步骤 6: 推送到远程仓库
info "步骤 6/7: 推送到远程仓库..."
git push origin HEAD
git push origin "$VERSION_TAG"
success "推送完成"
echo ""

# 步骤 7: 发布到 npm
info "步骤 7/7: 发布到 npm..."

publish_pkg() {
    local pkg_path=$1
    local pkg_name=$(node -e "console.log(require('$pkg_path/package.json').name)")
    info "发布 $pkg_name..."
    cd "$pkg_path"
    npm publish --access public
    cd "$PROJECT_ROOT"
    success "$pkg_name 发布成功"
}

# 按顺序发布平台包，最后发布主包（主包依赖平台包）
for entry in "${PLATFORMS[@]}"; do
    IFS=':' read -r pkg_name platform <<< "$entry"
    publish_pkg "$PROJECT_ROOT/packages/$pkg_name"
done

# 发布主包（wrapper）
publish_pkg "$PROJECT_ROOT/packages/ntd"

echo ""
success "========================================"
success "发布成功!"
success "版本: $VERSION_TAG"
success "========================================"
echo ""
info "验证发布:"
echo "  npm view @weibaohui/ntd"
echo ""
info "安装测试:"
echo "  npm install -g @weibaohui/ntd"
echo "  ntd --help"
