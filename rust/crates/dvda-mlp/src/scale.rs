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
    for b in 0..blocks {
        for i in 0..channels {
            let v = summary[b * 6 + i];
            if v > 0xffffff {
                return -1;
            }
            combined[i] |= v;
            all |= v;
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
        if high[i] < minimum {
            minimum = high[i];
        }
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
            if *item < common {
                *item = common;
            }
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
        work.shift[i] = if dimension[i] < low[i] {
            dimension[i]
        } else {
            low[i]
        };
        work.scale_count[i] = dimension[i] - work.shift[i];
        if work.maximum_shift < work.shift[i] {
            work.maximum_shift = work.shift[i];
        }
        for b in 0..blocks {
            if work.scale_count[i] == 0 && summary[b * 6 + i] != 0 {
                let q = trailing(summary[b * 6 + i]) - work.shift[i];
                work.qss[b][i] = if q > 15 { 15 } else { q };
            }
        }
    }
    *plan = work;
    0
}
