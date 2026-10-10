use std::{
    cell::RefCell,
    ffi::c_void,
    panic::{AssertUnwindSafe, catch_unwind},
};
thread_local! { static RESOURCES: RefCell<Vec<(usize, i32)>> = const { RefCell::new(Vec::new()) }; }
#[unsafe(no_mangle)]
pub extern "C" fn menu_rust_own(object: *mut c_void, kind: i32) -> i32 {
    catch_unwind(AssertUnwindSafe(|| {
        RESOURCES.with(|r| {
            let mut resources = r.borrow_mut();
            if resources.try_reserve(1).is_err() {
                return 1;
            }
            resources.push((object as usize, kind));
            0
        })
    }))
    .unwrap_or(1)
}
#[unsafe(no_mangle)]
pub extern "C" fn menu_rust_disown(object: *mut c_void, kind: i32) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        RESOURCES.with(|r| {
            let mut resources = r.borrow_mut();
            if let Some(i) = resources
                .iter()
                .rposition(|entry| *entry == (object as usize, kind))
            {
                resources.remove(i);
            }
        })
    }));
}
/// # Safety
/// Both output pointers must be writable. Objects are returned in reverse ownership order.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn menu_rust_take(object: *mut *mut c_void, kind: *mut i32) -> i32 {
    catch_unwind(AssertUnwindSafe(|| {
        RESOURCES.with(|r| {
            let Some((p, k)) = r.borrow_mut().pop() else {
                return 0;
            };
            unsafe {
                *object = p as *mut c_void;
                *kind = k;
            }
            1
        })
    }))
    .unwrap_or(0)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cleanup_is_lifo_and_disown_is_exact() {
        let one = std::ptr::dangling_mut::<c_void>();
        let two = 2usize as *mut c_void;
        assert_eq!(menu_rust_own(one, 1), 0);
        assert_eq!(menu_rust_own(two, 2), 0);
        assert_eq!(menu_rust_own(one, 4), 0);
        menu_rust_disown(one, 1);
        let mut object = std::ptr::null_mut();
        let mut kind = 0;
        unsafe {
            assert_eq!(menu_rust_take(&mut object, &mut kind), 1);
            assert_eq!((object, kind), (one, 4));
            assert_eq!(menu_rust_take(&mut object, &mut kind), 1);
            assert_eq!((object, kind), (two, 2));
            assert_eq!(menu_rust_take(&mut object, &mut kind), 0);
        }
    }
}
