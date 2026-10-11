#!/bin/bash
# 清理临时文件和测试产物

echo "================================================================"
echo "  清理临时文件"
echo "================================================================"
echo ""

echo "[1/3] 清理测试结果目录..."
if [ -d "test_results" ]; then
    rm -rf test_results
    echo "  ✓ 删除 test_results/"
fi

if [ -d "test_samples" ]; then
    # 只删除空目录
    if [ -z "$(ls -A test_samples)" ]; then
        rmdir test_samples
        echo "  ✓ 删除空目录 test_samples/"
    else
        echo "  ⚠ test_samples/ 包含文件，保留"
    fi
fi

echo ""
echo "[2/3] 清理临时脚本..."
TEMP_SCRIPTS=(
    "cleanup-docs.sh"
    "check_threads.rs"
)

for file in "${TEMP_SCRIPTS[@]}"; do
    if [ -f "$file" ]; then
        rm "$file"
        echo "  ✓ 删除 $file"
    fi
done

echo ""
echo "[3/3] 清理其他临时文件..."

# 清理编译产生的临时文件
if [ -f "rust/crates/dvda-core/examples/check_rayon_threads.rs" ]; then
    rm "rust/crates/dvda-core/examples/check_rayon_threads.rs"
    echo "  ✓ 删除临时示例文件"
fi

# 清理可能的备份文件
find . -name "*.bak" -o -name "*~" -o -name "*.tmp" 2>/dev/null | while read file; do
    rm "$file"
    echo "  ✓ 删除 $file"
done

# 清理 Git 未追踪的优化相关临时文件
TEMP_DOCS=(
    "RAYON_THREADS_EXPLAINED.md"
    "RAYON_CONFIG_GUIDE.md"
)

for doc in "${TEMP_DOCS[@]}"; do
    # 检查是否已移动到 docs/optimization
    if [ -f "$doc" ] && [ -f "docs/optimization/$doc" ]; then
        rm "$doc"
        echo "  ✓ 删除重复文件 $doc"
    fi
done

echo ""
echo "================================================================"
echo "清理完成"
echo "================================================================"
echo ""
echo "保留的文件:"
echo "  • 核心文档 (根目录 3 个)"
echo "  • 技术文档 (docs/optimization/)"
echo "  • 测试脚本 (scripts/optimization/)"
echo "  • 源代码优化"
echo ""
echo "已删除:"
echo "  • 临时测试文件"
echo "  • 重复文档"
echo "  • 临时脚本"
echo "  • 备份文件"
echo ""
