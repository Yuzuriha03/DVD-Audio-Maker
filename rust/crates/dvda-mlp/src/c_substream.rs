use crate::Bits;
use crate::c_restart::{MlpRestart, mlp_restart_put};
use crate::parameters::{MlpParameters, mlp_parameters_put};

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
    for n in first..first + count {
        if s.bypass_bits != 0 {
            w.put(u32::from(s.bypass[n]) >> (8 - s.bypass_bits), s.bypass_bits);
        }
        for ch in p.minimum_channel..=p.maximum_channel {
            let ch = ch as usize;
            crate::c_entropy::mlp_entropy_put(
                w,
                s.residual[n * s.stride + ch],
                &p.coding[ch],
                p.qss[ch],
            )?;
        }
    }
    Ok(())
}

pub(crate) fn mlp_substream_put(writer: &mut Bits, s: &MlpSubstream<'_>) -> Result<(), ()> {
    if !writer.words.is_empty()
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
    let mut local = Bits::default();
    let mut first = 0;
    if let Some(r) = s.restart {
        mlp_restart_put(&mut local, r, s.primary)?;
        if s.single_restart {
            mlp_parameters_put(&mut local, s.main, s.previous, false, false)?;
            samples(&mut local, s, s.main, 0, s.count)?;
            local.put(1, 1);
            writer.words = local.seal(s.end_markers);
            return Ok(());
        }
        let initial = s.initial.ok_or(())?;
        mlp_parameters_put(&mut local, initial, s.previous, true, true)?;
        samples(&mut local, s, initial, 0, 8)?;
        local.put(2, 3);
        first = 8;
    } else {
        local.put(2, 2);
    }
    let baseline = if s.restart.is_some() {
        s.initial.ok_or(())?
    } else {
        s.previous
    };
    mlp_parameters_put(&mut local, s.main, baseline, s.restart.is_some(), false)?;
    samples(&mut local, s, s.main, first, s.count - first)?;
    local.put(1, 1);
    writer.words = local.seal(s.end_markers);
    Ok(())
}
