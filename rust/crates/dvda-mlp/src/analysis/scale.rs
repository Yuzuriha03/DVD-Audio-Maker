pub(crate) const MLP_SCALE_BLOCKS: usize = 32;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[repr(C)]
pub(crate) struct MlpScalePlan {
    pub scale_count: [u32; 6],
    pub shift: [u32; 6],
    pub qss: [[u32; 6]; MLP_SCALE_BLOCKS],
    pub maximum_shift: u32,
    pub maximum_bits: u32,
}

fn trailing(mut value: u32) -> u32 {
    let mut n = 0;
    while value & 1 == 0 {
        value >>= 1;
        n += 1;
    }
    n
}

fn headroom(mut value: u32) -> u32 {
    let mut n = 0;
    while value & 0xc00000 == 0 {
        value <<= 1;
        n += 1;
    }
    n
}

pub(crate) fn mlp_scale_analyze(
    summary: &[u32],
    blocks: usize,
    channels: usize,
    candidates: usize,
    required_headroom: &[u32; 6],
    plan: &mut MlpScalePlan,
) -> i32 {
    let mut work = MlpScalePlan::default();
    let mut combined = [0u32; 6];
    let mut all = 0u32;
    let mut low = [0u32; 6];
    let mut high = [0u32; 6];
    let mut dimension = [0u32; 6];
    let mut minimum = 24;
    let mut budget = 6i32;
    if blocks == 0
        || blocks > MLP_SCALE_BLOCKS
        || channels == 0
        || channels > 6
        || candidates > channels
        || summary.len() < blocks * 6
    {
        return -1;
    }
    for &required in &required_headroom[..candidates] {
        if required > 31 {
            return -1;
        }
    }
    let (summary, _) = summary.as_chunks::<6>();
    for block in summary.iter().take(blocks) {
        for (combined, &value) in combined[..channels].iter_mut().zip(block) {
            if value > 0xffffff {
                return -1;
            }
            *combined |= value;
            all |= value;
        }
    }
    let mut common = if all != 0 { trailing(all) } else { 0 };
    for i in 0..channels {
        low[i] = if combined[i] != 0 {
            trailing(combined[i])
        } else {
            common
        };
        high[i] = if combined[i] != 0 {
            headroom(combined[i])
        } else {
            24 - common
        };
        minimum = minimum.min(high[i]);
    }
    work.maximum_bits = 25;
    let mut v = all;
    while v & 0x800000 == 0 && work.maximum_bits != 0 {
        v <<= 1;
        work.maximum_bits -= 1;
    }
    for i in 0..candidates {
        dimension[i] = required_headroom[i].saturating_sub(minimum);
        let consumed = dimension[i] as i32 - low[i] as i32;
        budget -= if consumed > 0 { consumed } else { 1 };
    }
    if all & 1 == 0 {
        common = 0;
        while all & 0xf00000 != 0 && all & 1 == 0 {
            common += 1;
            all >>= 1;
        }
        for item in &mut dimension[..channels] {
            *item = (*item).max(common);
        }
    } else {
        for i in 0..channels {
            let desired = u32::from((low[i] != 0 && high[i] < 3) || (budget > 0 && high[i] == 0));
            if dimension[i] < desired {
                dimension[i] = desired;
                if low[i] < desired {
                    budget -= 1;
                }
            }
        }
    }
    for i in 0..channels {
        work.shift[i] = dimension[i].min(low[i]);
        work.scale_count[i] = dimension[i] - work.shift[i];
        work.maximum_shift = work.maximum_shift.max(work.shift[i]);
        for (qss, block) in work.qss[..blocks].iter_mut().zip(summary) {
            if work.scale_count[i] == 0 && block[i] != 0 {
                let q = trailing(block[i]) - work.shift[i];
                qss[i] = q.min(15);
            }
        }
    }
    *plan = work;
    0
}
