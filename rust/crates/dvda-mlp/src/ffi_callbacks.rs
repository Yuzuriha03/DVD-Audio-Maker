//! Validated, synchronous access to foreign callbacks.
use super::{Read, Write};
use std::{ffi::c_void, marker::PhantomData};

pub(crate) struct Reader<'a> {
    callback: Read,
    context: *mut c_void,
    channels: usize,
    lifetime: PhantomData<&'a mut c_void>,
}

impl<'a> Reader<'a> {
    /// # Safety
    /// The callback/context must satisfy `Read` for `'a`, with exactly `channels`
    /// samples per frame. Calls sharing a context must be sequential, and must not
    /// invalidate other live readers/writers. No borrowed buffer may be retained.
    pub(super) unsafe fn new(callback: Read, context: *mut c_void, channels: usize) -> Self {
        assert!((1..=6).contains(&channels));
        Self {
            callback,
            context,
            channels,
            lifetime: PhantomData,
        }
    }

    pub(crate) fn read(&mut self, buffer: &mut [i32], capacity: usize) -> Result<usize, ()> {
        if capacity == 0 || capacity > buffer.len() / self.channels {
            return Err(());
        }
        let mut received = 0;
        // SAFETY: construction establishes callback validity; the checked slice
        // holds the requested frames and the count is exclusive stack storage.
        let status =
            unsafe { (self.callback)(self.context, buffer.as_mut_ptr(), capacity, &mut received) };
        if status != 0 || received == 0 || received > capacity {
            Err(())
        } else {
            Ok(received)
        }
    }
}

pub(crate) struct Writer<'a> {
    callback: Write,
    context: *mut c_void,
    lifetime: PhantomData<&'a mut c_void>,
}

impl<'a> Writer<'a> {
    /// # Safety
    /// The callback/context must satisfy `Write` for `'a`. Calls sharing a context
    /// must be sequential and must not invalidate other live readers/writers.
    pub(super) unsafe fn new(callback: Write, context: *mut c_void) -> Self {
        Self {
            callback,
            context,
            lifetime: PhantomData,
        }
    }

    pub(crate) fn write(&mut self, bytes: &[u8]) -> Result<(), ()> {
        // SAFETY: construction establishes callback validity; bytes remain live
        // and immutably borrowed until this synchronous callback returns.
        if unsafe { (self.callback)(self.context, bytes.as_ptr(), bytes.len()) } == 0 {
            Ok(())
        } else {
            Err(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct State {
        calls: usize,
        count: usize,
        status: i32,
    }

    unsafe extern "C" fn read(p: *mut c_void, _: *mut i32, _: usize, count: *mut usize) -> i32 {
        // SAFETY: tests supply exclusive State/count storage for this call.
        let state = unsafe { &mut *p.cast::<State>() };
        state.calls += 1;
        unsafe { *count = state.count };
        state.status
    }

    unsafe extern "C" fn write(p: *mut c_void, _: *const u8, _: usize) -> i32 {
        // SAFETY: tests supply exclusive State storage for this call.
        let state = unsafe { &mut *p.cast::<State>() };
        state.calls += 1;
        state.status
    }

    #[test]
    fn reader_checks_capacity_before_call_and_count_after_call() {
        let mut state = State {
            calls: 0,
            count: 1,
            status: 0,
        };
        let mut buffer = [0; 6];
        // SAFETY: state outlives each adapter and is only accessed between calls.
        let mut reader = unsafe { Reader::new(read, (&raw mut state).cast(), 3) };
        assert_eq!(reader.read(&mut buffer, 0), Err(()));
        assert_eq!(reader.read(&mut buffer, 3), Err(()));
        assert_eq!(reader.read(&mut buffer, usize::MAX), Err(()));
        assert_eq!(state.calls, 0);
        assert_eq!(reader.read(&mut buffer, 2), Ok(1));
        for count in [0, 3, usize::MAX] {
            state.count = count;
            std::hint::black_box(&state);
            assert_eq!(reader.read(&mut buffer, 2), Err(()));
        }
        state.count = 2;
        state.status = -1;
        std::hint::black_box(&state);
        assert_eq!(reader.read(&mut buffer, 2), Err(()));
    }

    #[test]
    fn writer_accepts_empty_slices_and_propagates_failure() {
        let mut state = State {
            calls: 0,
            count: 0,
            status: 0,
        };
        // SAFETY: state is live and only accessed between synchronous calls.
        let mut writer = unsafe { Writer::new(write, (&raw mut state).cast()) };
        assert_eq!(writer.write(&[]), Ok(()));
        state.status = -1;
        std::hint::black_box(&state);
        assert_eq!(writer.write(&[1, 2]), Err(()));
        assert_eq!(state.calls, 2);
    }
}
