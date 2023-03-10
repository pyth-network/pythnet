use crate::hashers::Hasher;
use crate::{Hash, RawPubkey};
use borsh::{BorshDeserialize, BorshSerialize};
use serde::Serialize;
use std::collections::HashSet;

pub mod merkle;
mod mul;

/*

TODO: need to serialize a root as well as the entire accumulator

merkle_tree:
    `accumulator.accumulator.get_root()`(root hash) goes into VAA
    proof_pda_v1: {
        Vec<(Key, MerkleTreeProof)>
        where
            TODO: key for handling projections
            Key = PriceId(Pubkey)
            MerkleTreeProof = Vec<MerkleNode<Hasher>>
    }
        this wastes a lot of space due to duplication of nodes
    proof_pda_v2: {
        accumulator: MerkleTree,
        proofs: Vec<(PriceId, MerkleTreeProof_v2)>
        where MerkleTreeProof_v2 = usize

mul:
    `accumulator.accumulator` goes into VAA
    proof_pda: {
        Vec<(Key, MulProof)>
        where MulProof = accumulator.accumulator/bytes as u128
    }

 */

trait Accumulator<'a>: Sized {
    type Proof: 'a;
    fn from_set(items: impl Iterator<Item = &'a &'a [u8]>) -> Option<Self>;
    fn prove(&'a self, item: &[u8]) -> Option<Self::Proof>;
    fn verify(&'a self, proof: Self::Proof, item: &[u8]) -> bool;
}
