// TODO: Go back to a reference based implementation ala Solana's original.

use {
    crate::pyth::PriceAccount,
    crate::{AccumulatorPrice, Hash, PriceId},
    borsh::{BorshDeserialize, BorshSerialize},
    serde::{Deserialize, Serialize},
};

// We need to discern between leaf and intermediate nodes to prevent trivial second
// pre-image attacks.
// https://flawed.net.nz/2018/02/21/attacking-merkle-trees-with-a-second-preimage-attack
const LEAF_PREFIX: &[u8] = &[0];
const INTERMEDIATE_PREFIX: &[u8] = &[1];

// Implement a function that takes a list of byte slices, and hashes them all using sha3 Keccak.
fn hashv(data: &[&[u8]]) -> Hash {
    use sha3::{Digest, Keccak256};
    let mut hasher = Keccak256::new();
    for d in data {
        hasher.update(d);
    }
    hasher.finalize().into()
}

macro_rules! hash_leaf {
    {$d:ident} => {
        hashv(&[LEAF_PREFIX, $d])
    }
}

macro_rules! hash_intermediate {
    {$l:ident, $r:ident} => {
        hashv(&[INTERMEDIATE_PREFIX, $l.as_ref(), $r.as_ref()])
    }
}

/// An implementation of a Sha3/Keccak256 based Merkle Tree based on the implementation provided by
/// solana-merkle-tree. This modifies the structure slightly to be serialization friendly, and to
/// make verification cheaper on EVM based networks.
#[derive(
    Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize, Default,
)]
pub struct Accumulator {
    pub leaf_count: usize,
    pub nodes: Vec<Hash>,
}

impl Accumulator {
    #[inline]
    fn next_level_len(level_len: usize) -> usize {
        if level_len == 1 {
            0
        } else {
            (level_len + 1) / 2
        }
    }

    fn calculate_vec_capacity(leaf_count: usize) -> usize {
        // the most nodes consuming case is when n-1 is full balanced binary tree
        // then n will cause the previous tree add a left only path to the root
        // this cause the total nodes number increased by tree height, we use this
        // condition as the max nodes consuming case.
        // n is current leaf nodes number
        // assuming n-1 is a full balanced binary tree, n-1 tree nodes number will be
        // 2(n-1) - 1, n tree height is closed to log2(n) + 1
        // so the max nodes number is 2(n-1) - 1 + log2(n) + 1, finally we can use
        // 2n + log2(n+1) as a safe capacity value.
        // test results:
        // 8192 leaf nodes(full balanced):
        // computed cap is 16398, actually using is 16383
        // 8193 leaf nodes:(full balanced plus 1 leaf):
        // computed cap is 16400, actually using is 16398
        // about performance: current used fast_math log2 code is constant algo time
        if leaf_count > 0 {
            fast_math::log2_raw(leaf_count as f32) as usize + 2 * leaf_count + 1
        } else {
            0
        }
    }

    pub fn from_slices<T: AsRef<[u8]>>(items: &[T]) -> Self {
        let cap = Accumulator::calculate_vec_capacity(items.len());
        let mut mt = Accumulator {
            leaf_count: items.len(),
            nodes: Vec::with_capacity(cap),
        };

        for item in items {
            let item = item.as_ref();
            let hash = hash_leaf!(item);
            mt.nodes.push(hash);
        }

        let mut level_len = Accumulator::next_level_len(items.len());
        let mut level_start = items.len();
        let mut prev_level_len = items.len();
        let mut prev_level_start = 0;
        while level_len > 0 {
            for i in 0..level_len {
                let prev_level_idx = 2 * i;
                let lsib = &mt.nodes[prev_level_start + prev_level_idx];
                let rsib = if prev_level_idx + 1 < prev_level_len {
                    &mt.nodes[prev_level_start + prev_level_idx + 1]
                } else {
                    // Duplicate last entry if the level length is odd
                    &mt.nodes[prev_level_start + prev_level_idx]
                };

                let hash = hash_intermediate!(lsib, rsib);
                mt.nodes.push(hash);
            }
            prev_level_start = level_start;
            prev_level_len = level_len;
            level_start += level_len;
            level_len = Accumulator::next_level_len(level_len);
        }

        mt
    }

    pub fn get_root(&self) -> Option<&Hash> {
        self.nodes.iter().last()
    }

    pub fn find_path(&self, index: usize) -> Option<MerklePath> {
        if index >= self.leaf_count {
            return None;
        }

        let mut level_len = self.leaf_count;
        let mut level_start = 0;
        let mut path = MerklePath::default();
        let mut node_index = index;
        let mut lsib = None;
        let mut rsib = None;
        while level_len > 0 {
            let level = &self.nodes[level_start..(level_start + level_len)];

            let target = level[node_index];
            if lsib.is_some() || rsib.is_some() {
                path.push(MerkleNode::new(target, lsib, rsib));
            }
            if node_index % 2 == 0 {
                lsib = None;
                rsib = if node_index + 1 < level.len() {
                    Some(level[node_index + 1])
                } else {
                    Some(level[node_index])
                };
            } else {
                lsib = Some(level[node_index - 1]);
                rsib = None;
            }
            node_index /= 2;

            level_start += level_len;
            level_len = Accumulator::next_level_len(level_len);
        }
        Some(path)
    }

    pub fn new<'r, I>(price_accounts: I) -> (Self, Vec<(PriceId, MerklePath)>)
    where
        I: Iterator<Item = (PriceId, &'r PriceAccount)> + Clone,
    {
        let merkle = Self::from_slices(
            price_accounts
                .clone()
                .map(|(_, p_a): (_, &'r PriceAccount)| AccumulatorPrice {
                    price_type: p_a.price_type,
                })
                .collect::<Vec<AccumulatorPrice>>()
                .as_slice(),
        );

        //TODO: plz handle failures & errors
        let proofs: Vec<(PriceId, MerklePath)> = price_accounts
            .enumerate()
            .map(|(idx, (id, _))| (id, merkle.find_path(idx).unwrap()))
            .collect();

        (merkle, proofs)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MerklePath(Vec<MerkleNode>);

impl MerklePath {
    pub fn push(&mut self, entry: MerkleNode) {
        self.0.push(entry)
    }

    pub fn verify(&self, candidate: Hash) -> bool {
        let result = self.0.iter().try_fold(candidate, |candidate, pe| {
            let lsib = pe.1.unwrap_or(candidate);
            let rsib = pe.2.unwrap_or(candidate);
            let hash = hash_intermediate!(lsib, rsib);

            if hash == pe.0 {
                Some(hash)
            } else {
                None
            }
        });
        matches!(result, Some(_))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MerkleNode(Hash, Option<Hash>, Option<Hash>);

impl<'a> MerkleNode {
    pub fn new(target: Hash, left_sibling: Option<Hash>, right_sibling: Option<Hash>) -> Self {
        assert!(left_sibling.is_none() ^ right_sibling.is_none());
        Self(target, left_sibling, right_sibling)
    }
}
