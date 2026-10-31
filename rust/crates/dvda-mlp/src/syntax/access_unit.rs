use super::substream::{MlpSubstream, mlp_substream_put};
use crate::bitstream::Bits;
use crate::pipeline::encode::IntervalOutput;

pub(crate) fn serialize_planned(
    writer: &mut Bits,
    pending: &IntervalOutput,
    au: usize,
    channels: usize,
) -> Result<(), ()> {
    let p = pending.au.get(au).ok_or(())?;
    mlp_substream_put(
        writer,
        &MlpSubstream {
            restart: (au == 0).then_some(&pending.restart),
            initial: Some(&p.initial),
            main: &p.main,
            previous: &p.previous,
            residual: &p.residual,
            bypass: &p.bypass,
            count: p.count,
            stride: channels,
            bypass_bits: p.bypass_bits,
            primary: true,
            single_restart: p.single_restart,
            end_markers: p.end_markers,
        },
    )
}

pub(crate) fn interval_fits(pending: &mut IntervalOutput, channels: usize) -> Result<bool, ()> {
    pending.restart.maximum_lsbs = pending.maximum_lsbs;
    for au in 0..pending.au.len() {
        let mut writer = Bits::counting();
        serialize_planned(&mut writer, pending, au, channels)?;
        if writer.word_len() + if au == 0 { 17 } else { 3 } > 0x300 {
            return Ok(false);
        }
    }
    Ok(true)
}
