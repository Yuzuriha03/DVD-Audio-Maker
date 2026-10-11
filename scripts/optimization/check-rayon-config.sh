#!/bin/bash
# Rayon 线程配置检查工具

echo "================================================================"
echo "  Rayon 并行配置检查"
echo "================================================================"
echo ""

echo "系统信息："
echo "  CPU 逻辑核心数: $(nproc 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null || echo '未知')"
echo ""

echo "Rayon 默认行为："
echo "  ❌ 不是: CPU 核心数 x 2"
echo "  ✅ 而是: 等于 CPU 逻辑核心数"
echo ""

echo "示例："
echo "  4 核无超线程 CPU → Rayon 使用 4 线程"
echo "  4 核有超线程 CPU → Rayon 使用 8 线程"
echo "  8 核无超线程 CPU → Rayon 使用 8 线程"
echo "  8 核有超线程 CPU → Rayon 使用 16 线程"
echo ""

echo "您的系统："
NUM_CORES=$(nproc 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null || echo '8')
echo "  CPU 核心数: $NUM_CORES"
echo "  Rayon 将使用: $NUM_CORES 线程"
echo ""

echo "================================================================"
echo "如何自定义线程数"
echo "================================================================"
echo ""
echo "方法 1: 环境变量"
echo "  export RAYON_NUM_THREADS=4"
echo ""
echo "方法 2: 代码配置"
echo "  ThreadPoolBuilder::new().num_threads(4).build_global()"
echo ""
echo "推荐配置:"
echo "  - 默认 (推荐): 使用全部逻辑核心"
echo "  - 高性能: 保持默认"
echo "  - 省电: 设置为物理核心数的 50-75%"
echo ""

echo "================================================================"
echo "性能预期 (基于 Amdahl 定律)"
echo "================================================================"
echo ""

# 计算不同核心数的加速比
for cores in 1 2 4 8; do
    if [ $cores -le $NUM_CORES ]; then
        # 使用 bc 或 awk 计算
        speedup=$(awk "BEGIN {p=0.85; o=0.05; s=1/((1-p)+p/$cores)*(1-o); printf \"%.2f\", s}")
        percent=$(awk "BEGIN {printf \"%.0f\", ($speedup-1)*100}")
        echo "  $cores 线程: ${speedup}x 加速 (${percent}% 更快)"
    fi
done

echo ""
echo "================================================================"
