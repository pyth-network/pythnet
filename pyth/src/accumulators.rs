use crate::hashers::Hasher;
use crate::{Hash, RawPubkey};
use borsh::{BorshDeserialize, BorshSerialize};
use serde::Serialize;
use std::collections::HashSet;

pub mod merkle;
mod mul;

trait Accumulator<'a>: Sized {
    type Proof: 'a;
    fn from_set(items: impl Iterator<Item = &'a &'a [u8]>) -> Option<Self>;
    fn prove(&'a self, item: &[u8]) -> Option<Self::Proof>;
    fn verify(&'a self, proof: Self::Proof, item: &[u8]) -> Option<bool>;
    
}
