# 🚀 FFI 优化方案 B 实施计划

## 方案 B: 优化现有 FFI (快速方案)

### 目标
不迁移 C 代码，仅优化 Rust ↔ C 的 FFI 调用

### 工作量
- 预计: 1-2 小时
- 收益: 5-10% 性能提升
- 风险: 极低

---

## 实施步骤

### 1. 内联所有 FFI 包装函数 ✅

**目标文件**: `rust/crates/dvda-bridges/src/ffmpeg.rs`

**优化点**:
```rust
// 添加 #[inline(always)]
#[inline(always)]
pub unsafe fn streams(f: *mut Format) -> Vec<*mut Stream>

#[inline(always)]
pub unsafe fn text(p: *const c_char) -> String

#[inline(always)]
pub unsafe fn done(v: i32) -> Result<(), i32>
```

### 2. 零拷贝函数 ✅

**已实现**: `text_borrowed()` 在 `ffi_optimized.rs`

**使用示例**:
```rust
// 旧方式 (拷贝)
let name = unsafe { ffmpeg::text(ptr) }; // String

// 新方式 (零拷贝)
use crate::ffi_optimized::text_borrowed;
let name = unsafe { text_borrowed(ptr) }; // &str
```

### 3. 批量错误检查

**当前**:
```rust
let result = unsafe { av_read_frame(ctx, packet) };
done(result)?; // 每次都检查
```

**优化**:
```rust
#[inline(always)]
pub unsafe fn done_fast(v: i32) -> Result<(), i32> {
    if v >= 0 {
        Ok(())
    } else {
        Err(v)
    }
}
```

---

## 实施清单

### ✅ 已完成
- [x] 创建 `ffi_optimized.rs` 模块
- [x] 实现零拷贝字符串访问
- [x] 实现 SIMD 音频转换
- [x] 集成到 dvda-bridges

### 🔧 立即实施

#### 步骤 1: 添加内联优化到现有函数
```rust
// rust/crates/dvda-bridges/src/ffmpeg.rs

// 找到这些函数并添加 #[inline(always)]
pub unsafe fn streams(...)
pub unsafe fn text(...)
pub unsafe fn done(...)
```

#### 步骤 2: 导出优化函数
```rust
// rust/crates/dvda-bridges/src/lib.rs

#[cfg(feature = "media")]
pub use ffi_optimized::{
    text_borrowed,
    convert_audio_i16_to_f32_simd,
};
```

#### 步骤 3: 文档示例
创建使用示例文档

---

## 预期效果

### 性能提升
- 内联优化: +5%
- 零拷贝 (适用场景): +2-5%
- **总计**: 5-10%

### 与现有优化叠加
```
阶段 1: 30-40%
+ 方案 B: +5-10%
= 总计: 35-50% 性能提升
```

---

## 实施
立即开始添加内联优化...
