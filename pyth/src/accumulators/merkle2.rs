use crate::accumulators::merkle::{MerkleNode, MerklePath};
use crate::accumulators::Accumulator2;
use crate::PriceId;
use {
    borsh::{BorshDeserialize, BorshSerialize},
    serde::{Deserialize, Serialize, Serializer},
};
use crate::hashers::{Hashable, Hasher};

/// An implementation of a Sha3/Keccak256 based Merkle Tree based on the implementation provided by
/// solana-merkle-tree. This modifies the structure slightly to be serialization friendly, and to
/// make verification cheaper on EVM based networks.
#[derive(
    Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize, Default,
)]
pub struct MerkleTree<H: Hasher> {
    #[borsh_skip]
    #[serde(skip)]
    pub leaf_count: usize,
    #[borsh_skip]
    #[serde(skip)]
    pub nodes: Vec<H::Hash>,
    #[borsh_skip]
    #[serde(skip)]
    pub proofs: Vec<(PriceId, MerklePath)>,
    pub root: H::Hash,
}

impl<H: Hasher> Accumulator2 for MerkleTree<H> {
    type Proof = MerklePath;

    fn from_set<T, H>(element_bytes: &[T]) -> Self
    where
        T: Hashable<H>,
        H: Hasher,
    {
        todo!()
    }

    fn prove(&self, elem: &[u8]) -> Option<Self::Proof> {
        todo!()
    }

    fn contains(&self, elem: &[u8], proof: Self::Proof) -> bool {
        todo!()
    }
}
impl<H: Hasher> MerkleTree<H> {
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
        let cap = MerkleTree::calculate_vec_capacity(items.len());
        let mut mt = MerkleTree {
            leaf_count: items.len(),
            nodes: Vec::with_capacity(cap),
            proofs: Vec::new(),
            root: H::Hash::default(),
        };

        for item in items {
            let item = item.as_ref();
            let hash = hash_leaf!(item);
            mt.nodes.push(hash);
        }

        let mut level_len = MerkleTree::next_level_len(items.len());
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
            level_len = MerkleTree::next_level_len(level_len);
        }

        mt
    }

    pub fn get_root(&self) -> Option<&H::Hash> {
        self.nodes.iter().last()
    }

    pub fn find_path(&self, index: usize) -> Option<MerklePath> {
        if index >= self.leaf_count as usize {
            return None;
        }

        let mut level_len = self.leaf_count as usize;
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
            level_len = MerkleTree::next_level_len(level_len);
        }
        Some(path)
    }

    pub fn new_merkle<'r, I>(price_accounts: I) -> Self
    where
        I: Iterator<Item = (PriceId, &'r PriceAccount)> + Clone,
    {
        let mut merkle = Self::from_slices(
            price_accounts
                .clone()
                .map(|(_, p_a): (_, &'r PriceAccount)| AccumulatorPrice {
                    price_type: p_a.price_type,
                })
                .collect::<Vec<AccumulatorPrice>>()
                .as_slice(),
        );

        //TODO: plz handle failures & errors

        merkle.proofs = price_accounts
            .enumerate()
            .map(|(idx, (id, _))| (id, merkle.find_path(idx).unwrap()))
            .collect();
        merkle.root = merkle.get_root().unwrap().clone();
        merkle
    }
}
