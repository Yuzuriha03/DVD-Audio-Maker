//! Small FFI helpers with checked buffer lengths and portable CPU dispatch.
/// Convert an `FFmpeg` status into a Rust result.
///
/// # Errors
/// Returns the original negative `FFmpeg` status as an error.
#[inline]
pub fn check_result(code: i32) -> Result<(), i32> {
    if code >= 0 { Ok(()) } else { Err(code) }
}

/// Assert a return code previously checked by the caller.
#[inline]
pub fn check_result_unchecked(code: i32) {
    debug_assert!(code >= 0, "FFmpeg error: {code}");
}

#[inline]
#[must_use]
pub fn should_batch_process(count: usize) -> bool {
    count >= 4
}

/// Convert signed PCM samples to floats.
///
/// AVX2 is a performance dispatch only: it is selected at runtime with
/// `is_x86_feature_detected!` when available, and the scalar loop below is the
/// canonical implementation on every other target. The dispatch is semantically
/// equivalent: `conversion_matches_scalar_for_vectors_and_tail` asserts that both
/// paths produce bit-identical `f32` results for vectors and the tail, so this
/// `target_arch` gate never changes behavior or output.
///
/// The runtime check keeps one executable working on CPUs without that
/// instruction set.
///
/// # Panics
/// Panics when input and output slices have different lengths.
pub fn convert_audio_i16_to_f32_simd(input: &[i16], output: &mut [f32]) {
    assert_eq!(
        input.len(),
        output.len(),
        "PCM buffers must have equal lengths"
    );
    #[cfg(target_arch = "x86_64")]
    if std::arch::is_x86_feature_detected!("avx2") {
        // SAFETY: CPU support was checked and both slices have equal lengths.
        unsafe {
            convert_avx2(input, output);
        }
        return;
    }
    for (input, output) in input.iter().zip(output) {
        *output = f32::from(*input) / 32768.0;
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
/// # Safety
/// The CPU must support AVX2. The caller checks this with `is_x86_feature_detected!`.
/// `input` and `output` must have the same length, as `convert_audio_i16_to_f32_simd`
/// asserts. The eight-lane loop reads and writes only within both slices.
unsafe fn convert_avx2(input: &[i16], output: &mut [f32]) {
    use std::arch::x86_64::{
        _mm_loadu_si128, _mm256_cvtepi16_epi32, _mm256_cvtepi32_ps, _mm256_mul_ps, _mm256_set1_ps,
        _mm256_storeu_ps,
    };
    let scale = _mm256_set1_ps(1.0 / 32768.0);
    let prefix = input.len() / 8 * 8;
    for index in (0..prefix).step_by(8) {
        // SAFETY: each iteration reads eight i16 and writes eight f32 within
        // equal-length slices. Unaligned loads/stores impose no extra alignment.
        unsafe {
            let raw = _mm_loadu_si128(input.as_ptr().add(index).cast());
            let value = _mm256_cvtepi32_ps(_mm256_cvtepi16_epi32(raw));
            _mm256_storeu_ps(output.as_mut_ptr().add(index), _mm256_mul_ps(value, scale));
        }
    }
    for index in prefix..input.len() {
        output[index] = f32::from(input[index]) / 32768.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn conversion_matches_scalar_for_vectors_and_tail() {
        let input: Vec<_> = (i16::MIN..=i16::MAX).step_by(13).collect();
        let mut output = vec![0.0; input.len()];
        convert_audio_i16_to_f32_simd(&input, &mut output);
        assert!(
            input
                .iter()
                .zip(&output)
                .all(|(i, o)| o.to_bits() == (f32::from(*i) / 32768.0).to_bits())
        );
        convert_audio_i16_to_f32_simd(&[], &mut []);
    }
    #[test]
    #[should_panic(expected = "PCM buffers must have equal lengths")]
    fn rejects_mismatched_buffers_before_simd() {
        convert_audio_i16_to_f32_simd(&[1; 8], &mut [0.0; 1]);
    }
    #[test]
    fn return_codes_and_batch_threshold() {
        assert_eq!(check_result(0), Ok(()));
        assert_eq!(check_result(-12), Err(-12));
        assert!(!should_batch_process(3));
        assert!(should_batch_process(4));
    }
}
