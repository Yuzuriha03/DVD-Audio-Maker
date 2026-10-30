//! Native binary64 arithmetic; historical x87 output parity is not required.
#[derive(Clone, Copy)]
pub(crate) struct Extended(f64);
impl Extended {
    pub(crate) fn tenth() -> Self {
        Self(0.1)
    }
    pub(crate) fn from_f64(value: f64) -> Self {
        Self(value)
    }
    pub(crate) fn to_f64(self) -> f64 {
        self.0
    }
    pub(crate) fn add(self, other: Self) -> Self {
        Self(self.0 + other.0)
    }
    pub(crate) fn mul(self, other: Self) -> Self {
        Self(self.0 * other.0)
    }
    pub(crate) fn div(self, other: Self) -> Self {
        Self(self.0 / other.0)
    }
    pub(crate) fn neg(self) -> Self {
        Self(-self.0)
    }
    pub(crate) fn log(self) -> Self {
        Self(self.0.ln())
    }
    pub(crate) fn sub(self, other: Self) -> Self {
        self.add(other.neg())
    }
    pub(crate) fn finite(self) -> bool {
        self.0.is_finite()
    }
    pub(crate) fn less(self, other: Self) -> bool {
        self.0 < other.0
    }
}
#[test]
fn binary64_boundaries() {
    let e = Extended::from_f64;
    assert_eq!(e(0.0).neg().to_f64().to_bits(), (-0.0f64).to_bits());
    assert!(!e(f64::NAN).less(e(1.0)));
    assert!(!e(1.0).less(e(f64::NAN)));
    assert!(!e(f64::INFINITY).finite());
    assert!(!e(f64::NAN).finite());
    assert!(e(f64::MIN_POSITIVE).finite());
    assert_eq!(e(1.0).log().to_f64(), 0.0);
    assert_eq!(e(0.0).log().to_f64(), f64::NEG_INFINITY);
    assert!(e(-1.0).log().to_f64().is_nan());
    assert_eq!(Extended::tenth().to_f64(), 0.1);
    assert_eq!(e(1e16).add(e(1.0)).sub(e(1e16)).to_f64(), 0.0);
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
