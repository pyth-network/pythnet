//! A type to hold data for the [`Accumulator` sysvar][sv].
//!
//! TODO: replace this with an actual link if needed
//! [sv]: https://docs.pythnetwork.org/developing/runtime-facilities/sysvars#accumulator
//!
//! The sysvar ID is declared in [`sysvar::accumulator`].
//!
//! [`sysvar::accumulator`]: crate::sysvar::accumulator

use {
    crate::{hash::Hash, pubkey::Pubkey},
    std::{iter::FromIterator, ops::Deref},
};

/*** Dummy Field(s) for now just to test updating the sysvar ***/
pub type Slot = u64;

#[repr(C)]
#[derive(Serialize, Deserialize, Debug, Default, PartialEq, Eq)]
pub struct Accumulator {
    pub slot: Slot,
}

impl Accumulator {
    pub fn add(&mut self, slot: Slot) {
        self.slot = slot;
    }
}

//TODO: refactor into separate file?
#[repr(C)]
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DummyPriceProof {
    pub price: u64,
    pub bump: u8,
    pub feed_id: Pubkey,
}

/** From `sdk/program/src/hash.rs **/

//TODO: update this to correct value/type later
pub type PriceHash = Hash;
pub type PriceId = Pubkey;
pub type PriceProof = (PriceId, PriceHash);

/** using `sdk/program/src/slot_hashes.rs` as a reference **/

#[repr(C)]
#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Default)]
pub struct PriceProofs(Vec<PriceProof>);

impl PriceProofs {
    pub fn new(price_proofs: &[PriceProof]) -> Self {
        let mut price_proofs = price_proofs.to_vec();
        price_proofs.sort_by(|(a, _), (b, _)| a.cmp(b));
        Self(price_proofs)
    }
}

impl FromIterator<(PriceId, PriceHash)> for PriceProofs {
    fn from_iter<I: IntoIterator<Item = (PriceId, PriceHash)>>(iter: I) -> Self {
        Self(iter.into_iter().collect())
    }
}

impl Deref for PriceProofs {
    type Target = Vec<PriceProof>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
