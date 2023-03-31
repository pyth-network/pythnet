//! A MerkleTree based Accumulator.

use {
    crate::{
        accumulators::Accumulator,
        hashers::{
            keccak256::Keccak256,
            Hasher,
        },
        PriceId,
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
const LEAF_PREFIX: &[u8] = &[0];
const NODE_PREFIX: &[u8] = &[1];

macro_rules! hash_leaf {
    {$x:ty, $d:ident} => {
        <$x as Hasher>::hashv(&[LEAF_PREFIX, $d])
    }
}

macro_rules! hash_node {
    {$x:ty, $l:ident, $r:ident} => {
        <$x as Hasher>::hashv(&[NODE_PREFIX, $l.as_ref(), $r.as_ref()])
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
    #[serde(skip)]
    pub nodes:      Vec<H::Hash>,
    pub leaf_count: usize,
}

impl<'a, H: Hasher + 'a> Accumulator<'a> for MerkleAccumulator<H> {
    type Proof = MerklePath<H>;

    fn from_set(items: impl Iterator<Item = &'a [u8]>) -> Option<Self> {
        let items: Vec<H::Hash> = items.map(|i| hash_leaf!(H, i)).collect();
        Some(Self::new(&items))
    }

    fn prove(&'a self, item: &[u8]) -> Option<Self::Proof> {
        let item = hash_leaf!(H, item);
        let index = self.nodes.iter().position(|i| i == &item)?;
        self.find_path(index)
    }

    fn check(&'a self, proof: Self::Proof, item: &[u8]) -> bool {
        let item = hash_leaf!(H, item);
        proof.validate(item)
    }
}

// This code is adapted from the solana-merkle-tree crate to use a generic hasher.
impl<H: Hasher> MerkleAccumulator<H> {
    fn calculate_vec_capacity(leaf_count: usize) -> usize {
        if leaf_count > 0 {
            fast_math::log2_raw(leaf_count as f32) as usize + 2 * leaf_count + 1
        } else {
            0
        }
    }

    pub fn new(items: &[H::Hash]) -> Self {
        let mut mt = Self {
            nodes:      Vec::with_capacity(Self::calculate_vec_capacity(items.len())),
            leaf_count: items.len(),
        };

        // Create leaf nodes and add them to the nodes vec of MerkleTree
        for item in items {
            mt.nodes.push(*item);
        }

        let mut prev_level_len = items.len();
        let mut prev_level_start = 0;

        // Compute intermediate nodes for the rest of the tree.
        while prev_level_len > 1 {
            let mut level_nodes = vec![];

            // Iterate over the previous level nodes two at a time
            for chunk in
                mt.nodes[prev_level_start..prev_level_start + prev_level_len].chunks_exact(2)
            {
                let lsib: &H::Hash = &chunk[0];
                let rsib: &H::Hash = &chunk[(chunk.len() == 2) as usize];
                level_nodes.push(hash_node!(H, lsib, rsib));
            }

            mt.nodes.extend_from_slice(&level_nodes);
            prev_level_start += prev_level_len;
            prev_level_len = level_nodes.len();
        }

        mt
    }

    pub fn get_root(&self) -> Option<&H::Hash> {
        self.nodes.iter().last()
    }

    #[inline]
    fn next_level_length(level_length: usize) -> usize {
        match level_length {
            1 => 0,
            _ => (level_length + 1) / 2,
        }
    }

    pub fn find_path(&self, index: usize) -> Option<MerklePath<H>> {
        if index >= self.leaf_count {
            return None;
        }

        let mut level_length = self.leaf_count;
        let mut level_start = 0;
        let mut path = MerklePath::<H>::default();
        let mut node_index = index;
        let mut lsib = None;
        let mut rsib = None;

        while level_length > 0 {
            let level = &self.nodes[level_start..(level_start + level_length)];
            let target = level[node_index];

            if lsib.is_some() || rsib.is_some() {
                path.push(MerkleNode::new(target, lsib, rsib));
            }

            if node_index % 2 == 0 {
                lsib = None;
                rsib = level.get(node_index + 1).copied().or(Some(target));
            } else {
                lsib = Some(level[node_index - 1]);
                rsib = None;
            }

            node_index /= 2;
            level_start += level_length;
            level_length = Self::next_level_length(level_length);
        }

        Some(path)
    }
}

#[derive(Clone, Default, Debug, PartialEq, Eq, Serialize)]
pub struct MerklePath<H: Hasher>(Vec<MerkleNode<H>>);

impl<H: Hasher> MerklePath<H> {
    pub fn push(&mut self, entry: MerkleNode<H>) {
        self.0.push(entry)
    }

    pub fn validate(&self, candidate: H::Hash) -> bool {
        let result = self.0.iter().try_fold(candidate, |candidate, pe| {
            let lsib = &pe.1.unwrap_or(candidate);
            let rsib = &pe.2.unwrap_or(candidate);
            let hash = hash_node!(H, lsib, rsib);

            if hash == pe.0 {
                Some(hash)
            } else {
                None
            }
        });
        matches!(result, Some(_))
    }
}

#[derive(Clone, Default, Debug, PartialEq, Eq, Serialize)]
pub struct MerkleNode<H: Hasher>(H::Hash, Option<H::Hash>, Option<H::Hash>);

impl<H: Hasher> MerkleNode<H> {
    pub fn new(
        target: H::Hash,
        left_sibling: Option<H::Hash>,
        right_sibling: Option<H::Hash>,
    ) -> Self {
        assert!(left_sibling.is_none() ^ right_sibling.is_none());
        Self(target, left_sibling, right_sibling)
    }
}

#[derive(Serialize, PartialEq, Eq, Default)]
pub struct PriceProofs<H: Hasher>(Vec<(PriceId, MerklePath<H>)>);

impl<H: Hasher> PriceProofs<H> {
    pub fn new(price_proofs: &[(PriceId, MerklePath<H>)]) -> Self {
        let mut price_proofs = price_proofs.to_vec();
        price_proofs.sort_by(|(a, _), (b, _)| a.cmp(b));
        Self(price_proofs)
    }
}

#[cfg(test)]
mod test {
    use {
        super::*,
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
        println!(
            "proof: {:#?}",
            proof.0.iter().map(|x| format!("{x:?}")).collect::<Vec<_>>()
        );
        println!("accumulator root: {:?}", accumulator.get_root().unwrap());
        println!(
            r"
                Sizes:
                    MerkleAccumulator::Proof    {:?}
                    Keccak256Hasher::Hash       {:?}
                    MerkleNode                  {:?}
                    MerklePath                  {:?}

            ",
            size_of::<<MerkleAccumulator as Accumulator>::Proof>(),
            size_of::<<Keccak256 as Hasher>::Hash>(),
            size_of::<MerkleNode<Keccak256>>(),
            size_of::<MerklePath<Keccak256>>()
        );
        assert!(!accumulator.check(proof, &item_d));
    }
}
