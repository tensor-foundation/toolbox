#![allow(unused_imports)]
use solana_program::pubkey::Pubkey;
use tensor_macros::pubkey;

#[test]
fn compile_tests() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/compile_fail/*.rs");
}
