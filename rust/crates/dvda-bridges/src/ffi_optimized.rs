// FFI Performance Optimization Utilities
// Provides optimized wrappers and utilities for C library calls

use std::ffi::c_char;

/// Zero-copy string access from C
///
/// Instead of copying the string, borrows it directly
#[inline(always)]
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

/// Fast error checking for FFmpeg return codes
#[inline(always)]
pub unsafe fn check_result(code: i32) -> Result<(), i32> {
    if code >= 0 {
        Ok(())
    } else {
        Err(code)
    }
}

/// Unchecked version for hot paths where success is expected
#[inline(always)]
pub unsafe fn check_result_unchecked(code: i32) {
    debug_assert!(code >= 0, "FFmpeg error: {}", code);
}

/// Batch processing hint - marks functions that should process multiple items
#[inline(always)]
pub fn should_batch_process(count: usize) -> bool {
    count >= 4 // Batch if 4+ items
}

/// SIMD-accelerated i16 to f32 conversion for audio samples
/// Converts FFmpeg's native format to Rust floats
#[cfg(target_arch = "x86_64")]
#[inline(always)]
pub unsafe fn convert_audio_i16_to_f32_simd(input: &[i16], output: &mut [f32]) {
    debug_assert_eq!(input.len(), output.len());

    #[cfg(target_feature = "avx2")]
    {
        use std::arch::x86_64::*;

        let chunks = input.len() / 8;
        let scale = 1.0 / 32768.0;

        for i in 0..chunks {
            let idx = i * 8;

            // Load 8 i16 values
            let vi16 = _mm_loadu_si128(input.as_ptr().add(idx) as *const __m128i);

            // Convert to i32 (lower 4 and upper 4)
            let vi32_lo = _mm_cvtepi16_epi32(vi16);
            let vi32_hi = _mm_cvtepi16_epi32(_mm_srli_si128(vi16, 8));

            // Convert to f32
            let vf32_lo = _mm_cvtepi32_ps(vi32_lo);
            let vf32_hi = _mm_cvtepi32_ps(vi32_hi);

            // Scale to [-1.0, 1.0]
            let vscale = _mm_set1_ps(scale);
            let result_lo = _mm_mul_ps(vf32_lo, vscale);
            let result_hi = _mm_mul_ps(vf32_hi, vscale);

            // Store results
            _mm_storeu_ps(output.as_mut_ptr().add(idx), result_lo);
            _mm_storeu_ps(output.as_mut_ptr().add(idx + 4), result_hi);
        }

        // Handle remaining elements
        let remainder_start = chunks * 8;
        for i in remainder_start..input.len() {
            output[i] = input[i] as f32 * scale;
        }
    }

    #[cfg(not(target_feature = "avx2"))]
    {
        // Fallback scalar implementation
        for i in 0..input.len() {
            output[i] = input[i] as f32 / 32768.0;
        }
    }
}

/// Fallback for non-x86 architectures
#[cfg(not(target_arch = "x86_64"))]
#[inline(always)]
pub unsafe fn convert_audio_i16_to_f32_simd(input: &[i16], output: &mut [f32]) {
    for i in 0..input.len() {
        output[i] = input[i] as f32 / 32768.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_conversion() {
        let input: Vec<i16> = vec![0, 16384, 32767, -16384, -32768];
        let mut output = vec![0.0f32; input.len()];

        unsafe {
            convert_audio_i16_to_f32_simd(&input, &mut output);
        }

        assert!((output[0] - 0.0).abs() < 0.001);
        assert!((output[1] - 0.5).abs() < 0.001);
        assert!((output[2] - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_should_batch() {
        assert!(!should_batch_process(3));
        assert!(should_batch_process(4));
        assert!(should_batch_process(100));
    }
}
