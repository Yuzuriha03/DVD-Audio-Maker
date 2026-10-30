#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MlpRateState {
    pub arrival: u16,
    pub decode: u16,
}

pub(crate) fn mlp_rate_init() -> MlpRateState {
    MlpRateState {
        arrival: 0x8000,
        decode: 0,
    }
}

pub(crate) fn mlp_rate_update(
    state: &mut MlpRateState,
    samples: u32,
    words: u32,
    rate_field: u32,
    flags: &mut u32,
    arrival: &mut u16,
) -> Result<bool, ()> {
    let rate = rate_field & 0x7fff;
    if samples == 0 || samples > 160 || words == 0 || words > 0xfff || rate == 0 {
        return Err(());
    }
    let mut start = i32::from(state.arrival);
    let lower = i32::from(state.decode) - samples as i32 * 90;
    if start > i32::from(state.decode) + 0x4000 {
        start -= 0x10000;
    }
    if start < lower {
        start = lower;
    }
    let duration = (words * 256).div_ceil(rate) as i32;
    let mut end = start + duration;
    let failure = end >= i32::from(state.decode);
    if failure {
        *flags |= 0x100;
        end = i32::from(state.decode);
    }
    if words > 0x300 {
        *flags |= 0x2000;
    }
    *arrival = start as u16;
    state.arrival = end as u16;
    state.decode = state.decode.wrapping_add(samples as u16);
    Ok(failure)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_input_preserves_state_and_outputs() {
        let mut state = mlp_rate_init();
        let before = state;
        let mut flags = 7;
        let mut arrival = 123;
        assert_eq!(
            mlp_rate_update(&mut state, 0, 1, 1, &mut flags, &mut arrival),
            Err(())
        );
        assert_eq!(state, before);
        assert_eq!((flags, arrival), (7, 123));
    }

    #[test]
    fn rate_failure_still_commits_timing_and_flags() {
        let mut state = MlpRateState {
            arrival: 99,
            decode: 100,
        };
        let mut flags = 1;
        let mut arrival = 0;
        assert_eq!(
            mlp_rate_update(&mut state, 40, 0x301, 256, &mut flags, &mut arrival),
            Ok(true)
        );
        assert_eq!(
            state,
            MlpRateState {
                arrival: 100,
                decode: 140
            }
        );
        assert_eq!((flags, arrival), (0x2101, 99));
    }
}
