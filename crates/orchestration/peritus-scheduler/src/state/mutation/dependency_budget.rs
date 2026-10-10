//! Finite executable budget for total dependency propagation.

#[doc = " Consumes one loop token without risking arithmetic overflow or underflow."]
pub(super) const fn consume(remaining: &mut usize, second_half: &mut bool) {
    if *remaining == 0 {
        *second_half = false;
        return;
    }
    if *second_half {
        *remaining -= 1;
        *second_half = false;
    } else {
        *second_half = true;
    }
}
