use crate::hashers::Hasher;
use crate::{Hash, RawPubkey};
use borsh::{BorshDeserialize, BorshSerialize};
use serde::Serialize;
use std::collections::HashSet;

pub mod merkle;
// pub mod merkle2;
mod mul;

pub(crate) type AccumulatorId = [u8; 32];

pub trait Accumulator {
    type Proof: Serialize;

    fn new<'r, I, V>(input: I) -> Self
    where
        I: Iterator<Item = (AccumulatorId, &'r V)> + Clone,
        V: std::hash::Hash + 'r;

    fn proof(&self) -> Self::Proof;
}

trait Accumulator2<'a>: Sized {
    type Proof: 'a;
    fn prove(&'a self, item: &[u8]) -> Option<Self::Proof>;
    fn verify(&'a self, proof: Self::Proof, item: &[u8]) -> Option<bool>;
    fn from_set(items: impl Iterator<Item = &'a &'a [u8]>) -> Option<Self>;
}
