//! A MerkleTree based Accumulator.

use {
    crate::{
        accumulators::Accumulator,
        hashers::{
            keccak256::Keccak256,
            Hasher,
        },
    },
    borsh::{
        BorshDeserialize,
        BorshSerialize,
    },
    serde::{
        Deserialize,
        Serialize,
    },
};

// We need to discern between leaf and intermediate nodes to prevent trivial second pre-image
// attacks. If we did not do this it would be possible for an attacker to intentionally create
// non-leaf nodes that have the same hash as a leaf node, and then use that to prove the existence
// of a leaf node that does not exist.
//
// See:
//
// - https://flawed.net.nz/2018/02/21/attacking-merkle-trees-with-a-second-preimage-attack
// - https://en.wikipedia.org/wiki/Merkle_tree#Second_preimage_attack
//
// NOTE: We use a NULL prefix for leaf nodes to distinguish them from the empty message (""), while
// there is no path that allows empty messages this is a safety measure to prevent future
// vulnerabilities being introduced.
const LEAF_PREFIX: &[u8] = &[0];
const NODE_PREFIX: &[u8] = &[1];
const NULL_PREFIX: &[u8] = &[2];

fn hash_leaf<H: Hasher>(leaf: &[u8]) -> H::Hash {
    H::hashv(&[LEAF_PREFIX, leaf])
}

fn hash_node<H: Hasher>(l: &H::Hash, r: &H::Hash) -> H::Hash {
    H::hashv(&[
        NODE_PREFIX,
        (if l <= r { l } else { r }).as_ref(),
        (if l <= r { r } else { l }).as_ref(),
    ])
}

fn hash_null<H: Hasher>() -> H::Hash {
    H::hashv(&[NULL_PREFIX])
}

#[derive(Clone, Default, Debug, PartialEq, Eq, Serialize)]
pub struct MerklePath<H: Hasher>(Vec<H::Hash>);

impl<H: Hasher> MerklePath<H> {
    pub fn new(path: Vec<H::Hash>) -> Self {
        Self(path)
    }
}

/// A MerkleAccumulator maintains a Merkle Tree.
///
/// The implementation is based on Solana's Merkle Tree implementation. This structure also stores
/// the items that are in the tree due to the need to look-up the index of an item in the tree in
/// order to create a proof.
#[derive(
    Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize, Default,
)]
pub struct MerkleAccumulator<H: Hasher = Keccak256> {
    pub root:  H::Hash,
    #[serde(skip)]
    pub nodes: Vec<H::Hash>,
}

// Layout:
//
// ```
// 4 bytes:  magic number
// 1 byte:   update type
// 4 byte:   storage id
// 32 bytes: root hash
// ```
//
// TODO: This code does not belong to MerkleAccumulator, we should be using the wire data types in
// calling code to wrap this value.
impl<'a, H: Hasher + 'a> MerkleAccumulator<H> {
    pub fn serialize(&self, storage: u32) -> Vec<u8> {
        let mut serialized = vec![];
        serialized.extend_from_slice(0x41555756u32.to_be_bytes().as_ref());
        serialized.extend_from_slice(0u8.to_be_bytes().as_ref());
        serialized.extend_from_slice(storage.to_be_bytes().as_ref());
        serialized.extend_from_slice(self.root.as_ref());
        serialized
    }
}

impl<'a, H: Hasher + 'a> Accumulator<'a> for MerkleAccumulator<H> {
    type Proof = MerklePath<H>;

    fn from_set(items: impl Iterator<Item = &'a [u8]>) -> Option<Self> {
        let items: Vec<&[u8]> = items.collect();
        Self::new(&items)
    }

    fn prove(&'a self, item: &[u8]) -> Option<Self::Proof> {
        let item = hash_leaf::<H>(item);
        let index = self.nodes.iter().position(|i| i == &item)?;
        Some(self.find_path(index))
    }

    fn check(&'a self, proof: Self::Proof, item: &[u8]) -> bool {
        let mut current = hash_leaf::<H>(item);
        for hash in proof.0 {
            current = hash_node::<H>(&current, &hash);
        }
        current == self.root
    }
}

// This code is adapted from the solana-merkle-tree crate to use a generic hasher.
impl<H: Hasher> MerkleAccumulator<H> {
    pub fn new(items: &[&[u8]]) -> Option<Self> {
        if items.is_empty() {
            return None;
        }

        let depth = items.len().next_power_of_two().trailing_zeros();
        let mut tree: Vec<H::Hash> = vec![Default::default(); 1 << (depth + 1)];

        // Filling the leaf hashes
        for i in 0..(1 << depth) {
            if i < items.len() {
                tree[(1 << depth) + i] = hash_leaf::<H>(items[i].as_ref());
            } else {
                tree[(1 << depth) + i] = hash_null::<H>();
            }
        }

        // Filling the node hashes from bottom to top
        for k in (1..=depth).rev() {
            let level = k - 1;
            let level_num_nodes = 1 << level;
            for i in 0..level_num_nodes {
                let id = (1 << level) + i;
                tree[id] = hash_node::<H>(&tree[id * 2], &tree[id * 2 + 1]);
            }
        }

        Some(Self {
            root:  tree[1],
            nodes: tree,
        })
    }

    fn find_path(&self, mut index: usize) -> MerklePath<H> {
        let mut path = Vec::new();
        while index > 1 {
            path.push(self.nodes[index ^ 1].clone());
            index /= 2;
        }
        MerklePath::new(path)
    }
}

#[cfg(test)]
mod test {
    use {
        super::*,
        proptest::prelude::*,
        rand::RngCore,
        std::{
            collections::HashSet,
            mem::size_of,
        },
    };

    #[derive(Default, Clone, Debug, borsh::BorshSerialize)]
    struct PriceAccount {
        pub id:         u64,
        pub price:      u64,
        pub price_expo: u64,
        pub ema:        u64,
        pub ema_expo:   u64,
    }

    #[derive(Default, Debug, borsh::BorshSerialize)]
    struct PriceOnly {
        pub price_expo: u64,
        pub price:      u64,

        pub id: u64,
    }

    impl From<PriceAccount> for PriceOnly {
        fn from(other: PriceAccount) -> Self {
            Self {
                id:         other.id,
                price:      other.price,
                price_expo: other.price_expo,
            }
        }
    }

    #[derive(Debug)]
    struct MerkleAccumulatorDataWrapper {
        pub accumulator: MerkleAccumulator,
        pub data:        HashSet<Vec<u8>>,
    }

    impl Arbitrary for MerkleAccumulatorDataWrapper {
        type Parameters = usize;

        fn arbitrary_with(size: Self::Parameters) -> Self::Strategy {
            let size = size.saturating_add(1);
            prop::collection::vec(
                prop::collection::vec(any::<u8>(), 1..=10),
                size..=size.saturating_add(100),
            )
            .prop_map(|v| {
                let data: HashSet<Vec<u8>> = v.into_iter().collect();
                let accumulator =
                    MerkleAccumulator::<Keccak256>::from_set(data.iter().map(|i| i.as_ref()))
                        .unwrap();
                MerkleAccumulatorDataWrapper { accumulator, data }
            })
            .boxed()
        }

        type Strategy = BoxedStrategy<Self>;
    }

    #[test]
    fn test_merkle() {
        let mut set: HashSet<&[u8]> = HashSet::new();

        // Create some random elements (converted to bytes). All accumulators store arbitrary bytes so
        // that we can target any account (or subset of accounts).
        let price_account_a = PriceAccount {
            id:         1,
            price:      100,
            price_expo: 2,
            ema:        50,
            ema_expo:   1,
        };
        let item_a = borsh::BorshSerialize::try_to_vec(&price_account_a).unwrap();

        let mut price_only_b = PriceOnly::from(price_account_a);
        price_only_b.price = 200;
        let item_b = BorshSerialize::try_to_vec(&price_only_b).unwrap();
        let item_c = 2usize.to_be_bytes();
        let item_d = 88usize.to_be_bytes();

        // Insert the bytes into the Accumulate type.
        set.insert(&item_a);
        set.insert(&item_b);
        set.insert(&item_c);

        let accumulator = MerkleAccumulator::<Keccak256>::from_set(set.into_iter()).unwrap();
        let proof = accumulator.prove(&item_a).unwrap();

        assert!(accumulator.check(proof, &item_a));
        let proof = accumulator.prove(&item_a).unwrap();
        assert_eq!(size_of::<<Keccak256 as Hasher>::Hash>(), 32);

        assert!(!accumulator.check(proof, &item_d));
    }

    // We're using proptest to generate arbitrary Merkle trees as part of our fuzzing strategy.
    // This will help us identify any edge cases or unexpected behavior in the implementation.
    proptest! {
        #[test]
        fn test_merkle_tree(v in any::<MerkleAccumulatorDataWrapper>()) {
            let accumulator = v.accumulator;
            let data = v.data;
            for d in data.iter() {
                let proof = accumulator.prove(d).unwrap();
                let mut invalid_proof = proof.clone();
                invalid_proof.0.push(hash_leaf::<Keccak256>(&[0]));
                assert!(accumulator.check(proof, d));
                assert!(!accumulator.check(invalid_proof, d));

            }

            let false_data = data.clone()
                .into_iter()
                .map(|d| {
                    let mut d  = d.to_vec();
                    d.extend_from_slice(&[0]);
                    d
                })
                .filter(|d| !data.contains(d))
                .collect::<HashSet<Vec<u8>>>();


            for f_d in false_data.iter() {
                let proof = accumulator.prove(f_d);
                assert!(proof.is_none());
            }
        }
    }
}
