use anchor_lang::solana_program::keccak::hashv;

pub fn validate_proof(root: &[u8; 32], leaf: &[u8; 32], proof: &[[u8; 32]]) -> bool {
    let mut path = *leaf;
    proof.iter().for_each(|sibling| {
        path = if path <= *sibling {
            hashv(&[&path, sibling]).0
        } else {
            hashv(&[sibling, &path]).0
        };
    });

    path == *root
}
