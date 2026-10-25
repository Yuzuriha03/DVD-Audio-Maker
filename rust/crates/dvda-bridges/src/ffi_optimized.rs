//! Small FFI helpers with checked buffer lengths and portable CPU dispatch.
use std::ffi::c_char;

/// Borrow a UTF-8 C string for the lifetime of the caller's live C object.
///
/// # Safety
/// A non-null pointer must address a terminated, readable C string which stays
/// live and immutable for the entire returned borrow.
#[inline]
pub unsafe fn text_borrowed<'a>(p: *const c_char) -> &'a str {
    if p.is_null() {
        ""
    } else {
        unsafe { std::ffi::CStr::from_ptr(p) }
            .to_str()
            .unwrap_or("")
    }
}

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
pub fn should_batch_process(count: usize) -> bool {
    count >= 4
}

/// Convert signed PCM samples to floats. AVX2 is selected at runtime so the
/// same executable also works on computers without that instruction set.
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
    use std::arch::x86_64::*;
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
                .all(|(i, o)| *o == f32::from(*i) / 32768.0)
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
