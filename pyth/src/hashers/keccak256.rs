use {
    crate::hashers::Hasher,
    serde::Serialize,
    sha3::{
        Digest,
        Keccak256 as Keccak256Digest,
    },
};

#[derive(Clone, Default, Debug, Serialize)]
pub struct Keccak256 {}

impl Hasher for Keccak256 {
    type Hash = [u8; 32];

    fn hashv(data: &[impl AsRef<[u8]>]) -> [u8; 32] {
        let mut hasher = Keccak256Digest::new();
        data.iter().for_each(|d| hasher.update(d));
        hasher.finalize().into()
    }
}
