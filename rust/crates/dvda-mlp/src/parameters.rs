use crate::Bits;
use crate::matrix::MlpMatrixPrimitive;
use crate::search::{MlpWireFilter, MlpWireState};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(C)]
pub(crate) struct MlpCodingParams {
    pub mode: i32,
    pub total_width: i32,
    pub offset: i32,
}

#[derive(Clone, Debug, Default)]
#[repr(C)]
pub(crate) struct MlpParameters {
    pub minimum_channel: u32,
    pub maximum_channel: u32,
    pub flags: u32,
    pub blocksize: u32,
    pub matrix_changed: u32,
    pub matrix_count: u32,
    pub matrix: [MlpMatrixPrimitive; 15],
    pub output_shift: [i32; 16],
    pub qss: [u32; 16],
    pub a: [MlpWireFilter; 16],
    pub b: [MlpWireFilter; 16],
    pub state: [MlpWireState; 16],
    pub coding: [MlpCodingParams; 16],
}

fn trailing(value: u32) -> u32 {
    value.trailing_zeros()
}

fn filter(w: &mut Bits, f: &MlpWireFilter, q: u32) -> Result<(), ()> {
    if f.order > 8 || f.precision > 15 || !(8..=15).contains(&q) {
        return Err(());
    }
    w.put(f.order, 4);
    if f.order == 0 {
        return Ok(());
    }
    let mut cf = [0i32; 8];
    let mut combined = 0u32;
    let mut l = 16u32;
    for (i, c) in cf.iter_mut().enumerate().take(f.order as usize) {
        let scaled = (f.coefficient[i] * 65536.0).trunc();
        if !scaled.is_finite() || scaled < i32::MIN as f64 || scaled > i32::MAX as f64 {
            return Err(());
        }
        *c = scaled as i32;
        combined |= *c as u32;
        while l < 31 && ((*c as i64) < -(1i64 << l) || (*c as i64) >= 1i64 << l) {
            l += 1;
        }
    }
    let mut t = 16 - f.precision;
    if combined != 0 {
        while t < 32 && combined & (1u32 << t) == 0 {
            t += 1;
        }
    }
    t = t.min(23 - q);
    let bits = l + 1 - t;
    let k = q.wrapping_add(t).wrapping_sub(16);
    if bits == 0 || bits > 16 || k > 7 {
        return Err(());
    }
    w.put(q, 4);
    w.put(bits, 5);
    w.put(k, 3);
    for &c in &cf[..f.order as usize] {
        w.put((c as u32 >> t) & ((1u32 << bits) - 1), bits);
    }
    Ok(())
}

fn state(w: &mut Bits, s: &MlpWireState) -> Result<(), ()> {
    if s.count > 8 {
        return Err(());
    }
    let mut e = 0u32;
    let mut combined = 0u32;
    for &v in &s.value[..s.count as usize] {
        combined |= v as u32;
        while e < 31 && ((v as i64) < -(1i64 << e) || (v as i64) >= 1i64 << e) {
            e += 1;
        }
    }
    let p = if combined != 0 {
        trailing(combined).min(15)
    } else {
        0
    };
    let bits = (e + 1).wrapping_sub(p);
    if bits == 0 || bits > 15 {
        return Err(());
    }
    w.put(bits, 4);
    w.put(p, 4);
    for &v in &s.value[..s.count as usize] {
        w.put((v as u32 >> p) & ((1u32 << bits) - 1), bits);
    }
    Ok(())
}

fn matrix(w: &mut Bits, c: &MlpParameters) -> Result<(), ()> {
    let mut index = c.matrix_count as usize;
    if index > 15 {
        return Err(());
    }
    w.put(index as u32, 4);
    while index != 0 {
        index -= 1;
        let m = &c.matrix[index];
        if m.target > 15 {
            return Err(());
        }
        let mut combined = 0u32;
        for &v in &m.coefficient[..c.maximum_channel as usize + 3] {
            if !(-32768..=32767).contains(&v) {
                return Err(());
            }
            combined |= v as u32;
        }
        let t = trailing(combined).min(14);
        w.put(m.target, 4);
        w.put(14 - t, 4);
        w.put(u32::from(m.bypass != 0), 1);
        for &v in &m.coefficient[..c.maximum_channel as usize + 3] {
            let value = v as u16 as u32;
            w.put(u32::from(value != 0), 1);
            if value != 0 {
                w.put(value >> t, 16 - t);
            }
        }
    }
    Ok(())
}

pub(crate) fn mlp_parameters_put(
    writer: &mut Bits,
    c: &MlpParameters,
    old: &MlpParameters,
    restart: bool,
    initial: bool,
) -> Result<(), ()> {
    if writer.width > 15 || c.minimum_channel > c.maximum_channel || c.maximum_channel > 15 {
        return Err(());
    }
    let mut local = Bits {
        words: Vec::new(),
        pending: writer.pending,
        width: writer.width,
    };
    let eligible = initial || !restart;
    local.put(0, 1);
    let changed = !initial && ((c.flags | old.flags) & 2 != 0);
    local.put(u32::from(changed), 1);
    if changed {
        let skip = if restart { 8 } else { 0 };
        if c.blocksize < skip || c.blocksize - skip > 511 {
            return Err(());
        }
        local.put(c.blocksize - skip, 9);
    }
    let changed = eligible && c.matrix_changed != 0;
    local.put(u32::from(changed), 1);
    if changed {
        matrix(&mut local, c)?;
    }
    let end = c.maximum_channel as usize + 1;
    let mut changed = false;
    for i in 0..end {
        if !(-8..=7).contains(&c.output_shift[i]) || c.qss[i] > 15 {
            return Err(());
        }
        if c.output_shift[i] != old.output_shift[i] {
            changed = eligible;
        }
    }
    local.put(u32::from(changed), 1);
    if changed {
        for &v in &c.output_shift[..end] {
            local.put(v as u32 & 15, 4);
        }
    }
    let changed = eligible && (0..end).any(|i| c.qss[i] != old.qss[i]);
    local.put(u32::from(changed), 1);
    if changed {
        for &v in &c.qss[..end] {
            local.put(v, 4);
        }
    }
    for i in c.minimum_channel as usize..end {
        let p = &c.coding[i];
        let previous = &old.coding[i];
        let new_a = !initial && c.a[i].changed != 0;
        let new_b = !initial && c.b[i].changed != 0;
        let new_offset = p.offset != previous.offset;
        if !(0..=3).contains(&p.mode)
            || !(0..=31).contains(&p.total_width)
            || !(-16384..=16383).contains(&p.offset)
        {
            return Err(());
        }
        let changed = new_a
            || new_b
            || new_offset
            || p.mode != previous.mode
            || p.total_width != previous.total_width;
        local.put(u32::from(changed), 1);
        if !changed {
            continue;
        }
        let q = 8.max(c.a[i].precision).max(c.b[i].precision);
        local.put(u32::from(new_a), 1);
        if new_a {
            filter(&mut local, &c.a[i], q)?;
            if c.a[i].order != 0 {
                local.put(0, 1);
            }
        }
        local.put(u32::from(new_b), 1);
        if new_b {
            filter(&mut local, &c.b[i], q)?;
            if c.b[i].order != 0 {
                local.put(u32::from(c.state[i].changed != 0), 1);
                if c.state[i].changed != 0 {
                    state(&mut local, &c.state[i])?;
                }
            }
        }
        local.put(u32::from(new_offset), 1);
        if new_offset {
            local.put(p.offset as u32 & 32767, 15);
        }
        local.put(p.mode as u32, 2);
        local.put(p.total_width as u32, 5);
    }
    writer.words.extend(local.words);
    writer.pending = local.pending;
    writer.width = local.width;
    Ok(())
}
