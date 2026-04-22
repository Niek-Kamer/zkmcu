#![no_std]

extern crate alloc;

use bls12_381::{pairing, G1Affine, G1Projective, G2Affine, Scalar};
use group::Group;

// Purpose: force every major BLS12-381 primitive into the compilation graph
// so the target backend actually lowers them. If this type-checks + codegens
// clean on thumbv8m.main-none-eabihf and riscv32imac-unknown-none-elf, the
// crate is no_std-clean for our purposes and the project is mostly wiring.
pub fn touch_all() -> bool {
    let g1 = G1Affine::generator();
    let g2 = G2Affine::generator();
    let s = Scalar::one();
    let g1p = G1Projective::from(g1) * s;
    let _g1_back = G1Affine::from(g1p);
    let gt = pairing(&g1, &g2);
    bool::from(!gt.is_identity())
}
