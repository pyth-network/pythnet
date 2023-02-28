use crate::Hash;
use serde::Serialize;

pub mod merkle;

pub(crate) type AccumulatorId = [u8; 32];

pub trait Accumulator {
    type Proof: Serialize;

    fn new<'r, I, V>(input: I) -> Self
    where
        I: Iterator<Item = (AccumulatorId, &'r V)> + Clone,
        V: std::hash::Hash + 'r;

    fn proof(&self) -> Self::Proof;
}
