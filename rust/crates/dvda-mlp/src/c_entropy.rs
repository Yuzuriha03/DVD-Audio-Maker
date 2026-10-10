use crate::Bits;
use crate::parameters::MlpCodingParams;

fn floor_div(value: i64, divisor: i64) -> i64 {
    if value >= 0 {
        value / divisor
    } else {
        -((-value + divisor - 1) / divisor)
    }
}

pub(crate) fn mlp_entropy_code(x: i32, p: &MlpCodingParams, qss: u32) -> Result<(u32, u32), ()> {
    if !(0..=3).contains(&p.mode)
        || qss > 15
        || p.total_width < qss as i32
        || p.total_width > 31
        || !(-16384..=16383).contains(&p.offset)
        || !(-8388608..=8388607).contains(&x)
    {
        return Err(());
    }
    let b = p.total_width as u32 - qss;
    if b > 24 {
        return Err(());
    }
    let unit = 1u64 << b;
    let (code, width);
    if p.mode == 0 {
        let u = i64::from(x) - i64::from(p.offset) + (unit / 2) as i64;
        if u < 0 || u as u64 >= unit {
            return Err(());
        }
        code = u as u64;
        width = b;
    } else {
        let central = 1u64 << (b + 3 - p.mode as u32);
        let u = i64::from(x) - i64::from(p.offset) + (central / 2) as i64;
        if u < 0 {
            let quotient = floor_div(u, unit as i64);
            if quotient < -7 {
                return Err(());
            }
            width = b + (-quotient) as u32 + 2;
            code = unit | (u as u64 & (unit - 1));
        } else if u as u64 >= central {
            let quotient = floor_div(u - central as i64, unit as i64);
            if quotient > 6 {
                return Err(());
            }
            width = b + quotient as u32 + 3;
            code = unit | (1u64 << (width - 2)) | (u as u64 & (unit - 1));
        } else {
            width = b + 4 - p.mode as u32;
            code = u as u64 + central;
        }
    }
    if width > 31 || width > b + 9 || code >= 1u64 << width {
        return Err(());
    }
    Ok((code as u32, width))
}

pub(crate) fn mlp_entropy_put(
    w: &mut Bits,
    x: i32,
    p: &MlpCodingParams,
    qss: u32,
) -> Result<(), ()> {
    let (value, width) = mlp_entropy_code(x, p, qss)?;
    w.put(value, width);
    Ok(())
}
