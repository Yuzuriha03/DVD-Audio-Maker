use core::arch::asm;

#[derive(Clone, Copy)]
pub(crate) struct Extended([u8; 10]);
impl Extended {
    // C compares the extended decimal constant, not its binary64 rounding.
    pub(crate) fn tenth() -> Self {
        Self([0xcd, 0xcc, 0xcc, 0xcc, 0xcc, 0xcc, 0xcc, 0xcc, 0xfb, 0x3f])
    }
    pub(crate) fn from_f64(value: f64) -> Self {
        let mut result = Self([0; 10]);
        unsafe {
            asm!("fld qword ptr [{input}]", "fstp tbyte ptr [{output}]",input=in(reg) &value,output=in(reg) result.0.as_mut_ptr(),options(nostack));
        }
        result
    }
    pub(crate) fn to_f64(self) -> f64 {
        let mut result = 0.0f64;
        unsafe {
            asm!("fld tbyte ptr [{input}]", "fstp qword ptr [{output}]",input=in(reg) self.0.as_ptr(),output=in(reg) &mut result,options(nostack));
        }
        result
    }
    pub(crate) fn add(self, other: Self) -> Self {
        let mut result = Self([0; 10]);
        unsafe {
            asm!("fld tbyte ptr [{left}]","fld tbyte ptr [{right}]","faddp st(1), st(0)","fstp tbyte ptr [{output}]",left=in(reg) self.0.as_ptr(),right=in(reg) other.0.as_ptr(),output=in(reg) result.0.as_mut_ptr(),options(nostack));
        }
        result
    }
    pub(crate) fn mul(self, other: Self) -> Self {
        let mut result = Self([0; 10]);
        unsafe {
            asm!("fld tbyte ptr [{left}]","fld tbyte ptr [{right}]","fmulp st(1), st(0)","fstp tbyte ptr [{output}]",left=in(reg) self.0.as_ptr(),right=in(reg) other.0.as_ptr(),output=in(reg) result.0.as_mut_ptr(),options(nostack));
        }
        result
    }
    pub(crate) fn div(self, other: Self) -> Self {
        let mut result = Self([0; 10]);
        unsafe {
            asm!("fld tbyte ptr [{left}]","fld tbyte ptr [{right}]","fdivp st(1), st(0)","fstp tbyte ptr [{output}]",left=in(reg) self.0.as_ptr(),right=in(reg) other.0.as_ptr(),output=in(reg) result.0.as_mut_ptr(),options(nostack));
        }
        result
    }
    pub(crate) fn neg(mut self) -> Self {
        self.0[9] ^= 128;
        self
    }
    pub(crate) fn log(self) -> Self {
        let mut result = Self([0; 10]);
        unsafe {
            asm!("fldln2", "fld tbyte ptr [{input}]", "fyl2x", "fstp tbyte ptr [{output}]", input=in(reg) self.0.as_ptr(), output=in(reg) result.0.as_mut_ptr(), options(nostack));
        }
        result
    }
    pub(crate) fn sub(self, other: Self) -> Self {
        self.add(other.neg())
    }
    pub(crate) fn finite(self) -> bool {
        (u16::from_le_bytes([self.0[8], self.0[9]]) & 0x7fff) != 0x7fff
    }
    pub(crate) fn less(self, other: Self) -> bool {
        let status: u16;
        unsafe {
            asm!("fld tbyte ptr [{right}]","fld tbyte ptr [{left}]","fcompp","fnstsw ax",left=in(reg) self.0.as_ptr(),right=in(reg) other.0.as_ptr(),out("ax") status,options(nostack));
        }
        status & 0x4500 == 0x0100
    }
}

pub(crate) struct HostFpScope(u16, u32);
impl HostFpScope {
    pub(crate) fn with_caller<T>(&self, callback: impl FnOnce() -> T) -> T {
        let codec = Self::codec();
        unsafe {
            asm!("fldcw word ptr [{cw}]", "ldmxcsr dword ptr [{mxcsr}]", cw=in(reg) &self.0, mxcsr=in(reg) &self.1, options(nostack));
        }
        let result = callback();
        drop(codec);
        result
    }
    pub(crate) fn codec() -> Self {
        let mut cw = 0u16;
        let mut mxcsr = 0u32;
        unsafe {
            asm!("fnstcw word ptr [{cw}]", "stmxcsr dword ptr [{mxcsr}]", cw=in(reg) &mut cw, mxcsr=in(reg) &mut mxcsr, options(nostack));
            let codec_cw = 0x027fu16;
            let codec_mxcsr = 0x1f80u32;
            asm!("fldcw word ptr [{cw}]", "ldmxcsr dword ptr [{mxcsr}]", cw=in(reg) &codec_cw, mxcsr=in(reg) &codec_mxcsr, options(nostack));
        }
        Self(cw, mxcsr)
    }
}
impl Drop for HostFpScope {
    fn drop(&mut self) {
        unsafe {
            asm!("fldcw word ptr [{cw}]", "ldmxcsr dword ptr [{mxcsr}]", cw=in(reg) &self.0, mxcsr=in(reg) &self.1, options(nostack));
        }
    }
}

#[test]
fn arithmetic_and_comparison() {
    let a = Extended::from_f64(7.0);
    let b = Extended::from_f64(2.0);
    assert_eq!(a.div(b).to_f64(), 3.5);
    assert_eq!(a.mul(b).add(b.neg()).to_f64(), 12.0);
    assert!(b.less(a));
    assert!(!a.less(b));
    assert!(!a.less(a));
}
