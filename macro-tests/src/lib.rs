#![allow(unused_imports)]

use solana_program::pubkey::Pubkey;
use tensor_macros::pubkey;
#[test]
fn test_valid_pubkey() {
    let pubkey: Pubkey = pubkey!("Evfeo6yn3ASo3FWkGRKJNfvjF4wCKbuNEkNfYQMtoSBr");
    assert_eq!(
        pubkey.to_string(),
        "Evfeo6yn3ASo3FWkGRKJNfvjF4wCKbuNEkNfYQMtoSBr"
    );
}

#[test]
fn test_known_pubkey() {
    let pubkey: Pubkey = pubkey!("11111111111111111111111111111111");
    assert_eq!(pubkey, Pubkey::new_from_array([0; 32]));
}

#[test]
fn test_multiple_pubkeys() {
    let pubkey1: Pubkey = pubkey!("Evfeo6yn3ASo3FWkGRKJNfvjF4wCKbuNEkNfYQMtoSBr");
    let pubkey2: Pubkey = pubkey!("11111111111111111111111111111111");
    assert_ne!(pubkey1, pubkey2);
}

#[test]
fn test_const_context() {
    const CONST_PUBKEY: Pubkey = pubkey!("Evfeo6yn3ASo3FWkGRKJNfvjF4wCKbuNEkNfYQMtoSBr");
    assert_eq!(
        CONST_PUBKEY.to_string(),
        "Evfeo6yn3ASo3FWkGRKJNfvjF4wCKbuNEkNfYQMtoSBr"
    );
}

// For compile-fail tests:
#[test]
fn compile_fail_tests() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/compile-fail/*.rs");
}
