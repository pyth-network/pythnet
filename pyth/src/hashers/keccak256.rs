use crate::hashers::Hasher;

#[derive(Clone, Default, Debug, serde::Serialize)]
pub struct Keccak256Hasher {}

impl Hasher for Keccak256Hasher {
    type Hash = [u8; 32];

    fn hashv(data: &[&[u8]]) -> [u8; 32] {
        use sha3::{Digest, Keccak256};
        let mut hasher = Keccak256::new();
        for d in data {
            hasher.update(d);
        }
        hasher.finalize().into()
    }
}

//MerkleTree::<Keccak256Algorithm>::new(...);
