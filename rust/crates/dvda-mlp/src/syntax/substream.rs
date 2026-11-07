use crate::bitstream::{Bits, ParameterBlock};
use crate::syntax::parameters::{MlpParameters, mlp_parameters_put};
use crate::syntax::restart::{MlpRestart, mlp_restart_put};

pub(crate) struct MlpSubstream<'a> {
    pub main: &'a MlpParameters,
    pub previous: &'a MlpParameters,
    pub initial: Option<&'a MlpParameters>,
    pub restart: Option<&'a MlpRestart>,
    pub residual: &'a [i32],
    pub bypass: &'a [u8],
    pub count: usize,
    pub stride: usize,
    pub bypass_bits: u32,
    pub primary: bool,
    pub single_restart: bool,
    pub end_markers: bool,
}

fn samples(
    w: &mut Bits,
    s: &MlpSubstream<'_>,
    p: &MlpParameters,
    first: usize,
    count: usize,
) -> Result<(), ()> {
    let channels = p.minimum_channel as usize..=p.maximum_channel as usize;
    let frames = s.residual[first * s.stride..(first + count) * s.stride].chunks_exact(s.stride);
    for (offset, frame) in frames.enumerate() {
        if s.bypass_bits != 0 {
            w.put(
                u32::from(s.bypass[first + offset]) >> (8 - s.bypass_bits),
                s.bypass_bits,
            );
        }
        for ((&residual, coding), &qss) in frame[channels.clone()]
            .iter()
            .zip(&p.coding[channels.clone()])
            .zip(&p.qss[channels.clone()])
        {
            crate::syntax::residual::mlp_entropy_put(w, residual, coding, qss)?;
        }
    }
    Ok(())
}

pub(crate) fn mlp_substream_put(writer: &mut Bits, s: &MlpSubstream<'_>) -> Result<(), ()> {
    if !writer.words.is_empty()
        || writer.has_counted_words()
        || writer.width != 0
        || writer.pending != 0
        || s.count == 0
        || s.count > 160
        || s.stride == 0
        || s.stride > 16
        || s.main.minimum_channel > s.main.maximum_channel
        || s.main.maximum_channel as usize >= s.stride
        || s.bypass_bits > 8
        || s.residual.len() < s.count * s.stride
        || (s.bypass_bits != 0 && s.bypass.len() < s.count)
    {
        return Err(());
    }
    if s.single_restart
        && (s.restart.is_none()
            || s.count < 8
            || s.main.blocksize as usize != s.count
            || s.main.flags & 2 == 0)
    {
        return Err(());
    }
    if let Some(r) = s.restart {
        if r.minimum_channel != s.main.minimum_channel
            || r.maximum_channel != s.main.maximum_channel
        {
            return Err(());
        }
        if !s.single_restart
            && (s.count < 16
                || s.initial.is_none_or(|p| {
                    p.minimum_channel != s.main.minimum_channel
                        || p.maximum_channel != s.main.maximum_channel
                }))
        {
            return Err(());
        }
    }
    // Cover typical access units without changing the buffer's growth semantics.
    let mut local = Bits::with_capacity(512);
    if let Some(r) = s.restart {
        mlp_restart_put(&mut local, r, s.primary)?;
    }
    // The restart CRC needs actual header bits; subsequent fields only need their lengths.
    local.copy_mode_from(writer);
    let (first, baseline, block) = if s.restart.is_some() {
        if s.single_restart {
            mlp_parameters_put(&mut local, s.main, s.previous, ParameterBlock::Continuation)?;
            samples(&mut local, s, s.main, 0, s.count)?;
            local.put(1, 1);
            local.finish_into(writer, s.end_markers);
            return Ok(());
        }
        let initial = s.initial.ok_or(())?;
        mlp_parameters_put(
            &mut local,
            initial,
            s.previous,
            ParameterBlock::RestartInitial,
        )?;
        samples(&mut local, s, initial, 0, 8)?;
        local.put(2, 3);
        (8, initial, ParameterBlock::RestartMain)
    } else {
        local.put(2, 2);
        (0, s.previous, ParameterBlock::Continuation)
    };
    mlp_parameters_put(&mut local, s.main, baseline, block)?;
    samples(&mut local, s, s.main, first, s.count - first)?;
    local.put(1, 1);
    local.finish_into(writer, s.end_markers);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::parameters::MlpCodingParams;

    #[test]
    #[ignore = "deterministic throughput benchmark"]
    fn au_substream_buffer_throughput() {
        use std::hint::black_box;
        use std::time::Instant;

        let mut parameters = MlpParameters {
            blocksize: 160,
            flags: 2,
            maximum_channel: 1,
            ..Default::default()
        };
        parameters.coding[0] = MlpCodingParams {
            mode: 1,
            total_width: 24,
            offset: 0,
        };
        parameters.coding[1] = MlpCodingParams {
            mode: 2,
            total_width: 20,
            offset: 0,
        };
        let mut state = 0x4d59_5df4u32;
        let residual: Vec<i32> = (0..320)
            .map(|_| {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                i32::try_from((state >> 16) & 0x1fff).unwrap() - 4096
            })
            .collect();
        let substream = MlpSubstream {
            main: &parameters,
            previous: &parameters,
            initial: None,
            restart: None,
            residual: &residual,
            bypass: &[],
            count: 160,
            stride: 2,
            bypass_bits: 0,
            primary: false,
            single_restart: false,
            end_markers: true,
        };
        let mut words = 0u64;
        let mut capacity = 0u64;
        let start = Instant::now();
        for _ in 0..10_000 {
            let mut writer = Bits::default();
            mlp_substream_put(black_box(&mut writer), black_box(&substream)).unwrap();
            words += u64::try_from(writer.word_len()).unwrap();
            capacity += u64::try_from(writer.words.capacity()).unwrap();
        }
        println!(
            "[METRIC] au_substream_ns={} words={words} capacity={capacity}",
            start.elapsed().as_nanos()
        );
        assert_ne!(words, 0);
    }
}
