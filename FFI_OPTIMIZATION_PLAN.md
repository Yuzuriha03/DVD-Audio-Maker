# 🚀 Rust FFI 性能优化方案 - C 库调用优化

## 项目中的 C 库

### 1. FFmpeg (媒体处理)
- **库**: avformat, avcodec, avutil, swresample, swscale
- **用途**: 音频解码、格式转换、重采样
- **位置**: `rust/crates/dvda-bridges/src/ffmpeg.rs`

### 2. ImageMagick (图像处理)
- **库**: MagickWand, MagickCore
- **用途**: 菜单图像处理、格式转换
- **位置**: `rust/crates/dvda-bridges/src/image.rs`

---

## 🎯 FFI 性能优化策略

### 优化 1: 减少跨边界调用

**问题**: 每次 FFI 调用都有固定开销（5-50ns）

**解决方案**: 批量处理

#### 优化前（频繁调用）:
```rust
// 每个样本都跨 FFI
for sample in samples {
    unsafe { av_audio_convert(sample) }; // 1000次 FFI 调用
}
```

#### 优化后（批量处理）:
```rust
// 一次传递整个缓冲区
let buffer = samples.as_ptr();
unsafe {
    av_audio_convert_batch(buffer, samples.len()); // 1次 FFI 调用
}
```

**预期提升**: 20-50x

---

### 优化 2: 零拷贝数据传递

**问题**: Rust ↔ C 之间的数据拷贝开销

**解决方案**: 直接共享内存

#### 当前实现:
```rust
// dvda-bridges/src/ffmpeg.rs
pub unsafe fn text(p: *const c_char) -> String {
    if p.is_null() {
        String::new()
    } else {
        unsafe { std::ffi::CStr::from_ptr(p).to_string_lossy().into_owned() }
        //       ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
        //       这里会拷贝数据
    }
}
```

#### 优化后（零拷贝）:
```rust
// 使用借用而不是拷贝
pub unsafe fn text_borrowed<'a>(p: *const c_char) -> &'a str {
    if p.is_null() {
        ""
    } else {
        unsafe {
            std::ffi::CStr::from_ptr(p)
                .to_str()
                .unwrap_or("")
        }
    }
}

// 配合使用
let name = unsafe { text_borrowed(stream.name_ptr()) };
// 直接使用，无拷贝
println!("Stream: {}", name);
```

**预期提升**: 2-5x（大字符串场景）

---

### 优化 3: 内联 FFI 包装函数

**当前代码**:
```rust
// dvda-bridges/src/ffmpeg.rs:191
pub unsafe fn streams(f: *mut Format) -> Vec<*mut Stream> {
    unsafe { 
        std::slice::from_raw_parts(
            (*f).streams(), 
            (*f).nb_streams() as usize
        ).to_vec() 
    }
}
```

**优化后**:
```rust
#[inline(always)]
pub unsafe fn streams(f: *mut Format) -> Vec<*mut Stream> {
    unsafe { 
        std::slice::from_raw_parts(
            (*f).streams(), 
            (*f).nb_streams() as usize
        ).to_vec() 
    }
}

// 更激进：完全内联展开
#[inline(always)]
pub unsafe fn streams_slice<'a>(f: *mut Format) -> &'a [*mut Stream] {
    unsafe { 
        std::slice::from_raw_parts(
            (*f).streams(), 
            (*f).nb_streams() as usize
        )
    }
}
```

**预期提升**: 5-10%

---

### 优化 4: 预分配和缓冲池

**问题**: 频繁分配/释放内存

**解决方案**: 对象池模式

```rust
use std::sync::Mutex;

// FFmpeg 帧缓冲池
pub struct FramePool {
    available: Mutex<Vec<*mut Frame>>,
    capacity: usize,
}

impl FramePool {
    pub fn new(capacity: usize) -> Self {
        let mut available = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            available.push(unsafe { av_frame_alloc() });
        }
        
        Self {
            available: Mutex::new(available),
            capacity,
        }
    }
    
    pub fn acquire(&self) -> Option<*mut Frame> {
        self.available.lock().unwrap().pop()
    }
    
    pub fn release(&self, frame: *mut Frame) {
        unsafe { av_frame_unref(frame) };
        self.available.lock().unwrap().push(frame);
    }
}

// 使用
static FRAME_POOL: Lazy<FramePool> = Lazy::new(|| FramePool::new(16));

fn decode_audio() {
    let frame = FRAME_POOL.acquire().expect("Pool exhausted");
    // ... 使用 frame
    FRAME_POOL.release(frame);
}
```

**预期提升**: 30-50%（高频分配场景）

---

### 优化 5: 并行 FFI 调用

**问题**: FFI 调用是串行的

**解决方案**: 多线程并行处理

```rust
use rayon::prelude::*;

// 并行解码多个音频流
pub fn decode_streams_parallel(
    streams: &[*mut Stream],
    output: &mut [Vec<f32>]
) -> Result<()> {
    streams
        .par_iter()
        .zip(output.par_iter_mut())
        .try_for_each(|(stream, output_buffer)| {
            unsafe {
                // 每个线程独立调用 FFmpeg
                decode_stream(*stream, output_buffer)
            }
        })?;
    
    Ok(())
}
```

**注意**: 需要确保 FFmpeg 调用是线程安全的（大多数解码操作是）

**预期提升**: 4-7x（8核系统）

---

### 优化 6: 使用 `repr(transparent)` 减少转换

**当前实现**:
```rust
// 生成的代码（build.rs）
#[repr(C,align(8))]
pub struct Format { 
    _opaque:[u8;1024] 
}
```

**优化后**:
```rust
// 对于简单类型使用 transparent
#[repr(transparent)]
pub struct StreamIndex(pub i32);

#[repr(transparent)]
pub struct Timestamp(pub i64);

// 零成本转换
impl From<i32> for StreamIndex {
    #[inline(always)]
    fn from(v: i32) -> Self {
        StreamIndex(v)
    }
}
```

**预期提升**: 编译器可以完全优化掉包装

---

### 优化 7: 避免不必要的错误检查

**当前代码**:
```rust
pub unsafe fn done(v: i32) -> Result<(), i32> {
    if v >= 0 {
        Ok(())
    } else {
        Err(v)
    }
}

// 使用
let result = unsafe { av_read_frame(ctx, packet) };
done(result)?; // 每次都检查
```

**优化后（快速路径）**:
```rust
#[inline(always)]
pub unsafe fn done_unchecked(v: i32) {
    debug_assert!(v >= 0, "FFmpeg error: {}", v);
}

// 在已知成功的路径上
if likely_success {
    unsafe { done_unchecked(result) };
} else {
    done(result)?;
}
```

**预期提升**: 5-10%

---

### 优化 8: SIMD 数据转换

**问题**: 音频格式转换（C ↔ Rust）

**解决方案**: 使用 SIMD 批量转换

```rust
#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

// 批量转换 i16 → f32（FFmpeg 输出 → Rust）
#[inline(always)]
pub unsafe fn convert_i16_to_f32_simd(input: &[i16], output: &mut [f32]) {
    debug_assert_eq!(input.len(), output.len());
    
    #[cfg(target_feature = "avx2")]
    {
        let chunks = input.len() / 8;
        for i in 0..chunks {
            let ptr_in = input.as_ptr().add(i * 8);
            let ptr_out = output.as_mut_ptr().add(i * 8);
            
            // 加载 8 个 i16
            let vi16 = _mm_loadu_si128(ptr_in as *const __m128i);
            
            // 转换为 i32
            let vi32_lo = _mm_cvtepi16_epi32(vi16);
            let vi32_hi = _mm_cvtepi16_epi32(_mm_srli_si128(vi16, 8));
            
            // 转换为 f32 并归一化
            let scale = _mm256_set1_ps(1.0 / 32768.0);
            let vf32_lo = _mm256_mul_ps(_mm256_cvtepi32_ps(vi32_lo), scale);
            let vf32_hi = _mm256_mul_ps(_mm256_cvtepi32_ps(vi32_hi), scale);
            
            // 存储
            _mm256_storeu_ps(ptr_out, vf32_lo);
            _mm256_storeu_ps(ptr_out.add(4), vf32_hi);
        }
    }
    
    // 处理剩余元素
    let remainder = input.len() % 8;
    for i in (input.len() - remainder)..input.len() {
        output[i] = input[i] as f32 / 32768.0;
    }
}
```

**预期提升**: 8-12x

---

## 📊 综合优化效果预测

| 优化项 | 场景 | 预期提升 |
|--------|------|----------|
| 批量处理 | 音频解码 | 20-50x |
| 零拷贝 | 字符串/元数据 | 2-5x |
| 内联优化 | 所有 FFI 调用 | 5-10% |
| 对象池 | 高频分配 | 30-50% |
| 并行调用 | 多流处理 | 4-7x |
| SIMD 转换 | 格式转换 | 8-12x |

**综合预期**: 在音频处理密集场景下，总体提升 **5-10x**

---

## 🛠️ 实施优先级

### P0 - 立即实施（投入产出比最高）
1. **批量处理** - 修改 `ffmpeg.rs` 中的循环调用
2. **内联优化** - 添加 `#[inline(always)]`
3. **零拷贝** - 使用借用而非拷贝

### P1 - 短期实施
4. **对象池** - 实现 FramePool
5. **并行调用** - 多流并行解码

### P2 - 长期优化
6. **SIMD 转换** - 音频格式转换
7. **repr(transparent)** - 类型包装优化

---

## 📝 具体实施文件

### 需要修改的文件
```
rust/crates/dvda-bridges/
├── src/
│   ├── ffmpeg.rs          # FFmpeg FFI 包装 - 主要优化目标
│   ├── image.rs           # ImageMagick FFI
│   └── media.rs           # 媒体处理逻辑
└── build.rs               # 构建脚本 - 可添加优化标志
```

---

## 🎯 下一步

**您想先实施哪个优化？**

1. 立即开始批量处理优化（最大收益）
2. 实施内联和零拷贝优化（快速实施）
3. 查看完整实施代码
4. 先做性能分析，找出真正的瓶颈

请告诉我您的选择！
