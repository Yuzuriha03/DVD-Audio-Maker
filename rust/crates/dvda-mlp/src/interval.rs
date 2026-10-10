#[cfg(test)]
pub(crate) const MLP_INTERVAL_SLOTS: usize = 1265;

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
#[repr(C)]
pub(crate) struct MlpBoundaryState {
    pub position: u32,
    pub index: u32,
    pub segments: u32,
}

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
#[repr(C)]
#[cfg(test)]
pub(crate) struct MlpIntervalSlot {
    pub samples: i32,
    pub flags: u32,
}

fn signed32(value: u32) -> i32 {
    value as i32
}

pub(crate) fn mlp_interval_boundary(
    state: &mut MlpBoundaryState,
    mut cycle: u32,
    preferred_span: u32,
    samples: u32,
) -> i32 {
    if cycle > i32::MAX as u32
        || preferred_span == 0
        || preferred_span > i32::MAX as u32
        || samples > 160
    {
        return -1;
    }
    let mut work = *state;
    if samples == 0 {
        work.position = 0;
        *state = work;
        return 1;
    }
    if cycle == 0 {
        cycle = preferred_span;
    }
    if work.segments == 0 || signed32(work.position) >= cycle as i32 {
        work.segments = cycle / preferred_span;
        if signed32(work.segments.wrapping_shl(5)) < cycle as i32 {
            work.segments = work.segments.wrapping_add(1);
        }
        work.position = 0;
        work.index = 0;
    }
    let boundary = i32::from(
        signed32(work.index.wrapping_mul(cycle))
            <= signed32(work.segments.wrapping_mul(work.position)),
    );
    work.index = work.index.wrapping_add(boundary as u32);
    work.position = work.position.wrapping_add(1);
    *state = work;
    boundary
}

#[cfg(test)]
pub(crate) fn mlp_interval_find(
    slots: &mut [MlpIntervalSlot; MLP_INTERVAL_SLOTS],
    write_cursor: usize,
    cursor: &mut usize,
) -> i32 {
    if write_cursor >= MLP_INTERVAL_SLOTS || *cursor >= MLP_INTERVAL_SLOTS {
        return -1;
    }
    let mut next = *cursor;
    let mut previous;
    loop {
        previous = next;
        next = if previous == MLP_INTERVAL_SLOTS - 1 {
            0
        } else {
            previous + 1
        };
        if next == write_cursor {
            *cursor = previous;
            return 0;
        }
        if slots[next].samples <= 0 || slots[next].flags & 2 != 0 {
            break;
        }
    }
    if slots[next].samples == 0 {
        slots[previous].flags |= 8;
    }
    *cursor = next;
    1
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn balanced_boundaries_and_empty_endpoint() {
        let mut state = MlpBoundaryState::default();
        let positions: Vec<_> = (0..20)
            .filter(|_| mlp_interval_boundary(&mut state, 20, 8, 40) == 1)
            .collect();
        assert_eq!(positions, [0, 10]);
        let before = state;
        assert_eq!(mlp_interval_boundary(&mut state, 20, 0, 40), -1);
        assert_eq!(state, before);
        assert_eq!(mlp_interval_boundary(&mut state, 20, 8, 0), 1);
        assert_eq!(state.index, before.index);
        assert_eq!(state.segments, before.segments);
        let mut slots = [MlpIntervalSlot::default(); MLP_INTERVAL_SLOTS];
        slots[1].samples = 40;
        let mut cursor = 0;
        assert_eq!(mlp_interval_find(&mut slots, 3, &mut cursor), 1);
        assert_eq!(cursor, 2);
        assert_eq!(slots[1].flags, 8);
    }
}
