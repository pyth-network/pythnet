//! A type to hold data for the [`Accumulator` sysvar][sv].
//!
//! TODO: replace this with an actual link if needed
//! [sv]: https://docs.pythnetwork.org/developing/runtime-facilities/sysvars#accumulator
//!
//! The sysvar ID is declared in [`sysvar::accumulator`].
//!
//! [`sysvar::accumulator`]: crate::sysvar::accumulator

use crate::account_info::AccountInfo;
use crate::program_error::ProgramError;
use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Serialize, Serializer};
use solana_program::clock::UnixTimestamp;
use std::cell::RefMut;
use std::io::ErrorKind::InvalidData;
use std::io::{Error, Read, Write};
use std::ops::DerefMut;
use std::{fmt, mem};
use {
    crate::{
        hash::{hashv, Hash},
        pubkey::Pubkey,
    },
    bytemuck::{try_from_bytes, try_from_bytes_mut, Pod, Zeroable},
    // solana_merkle_tree::MerkleTree,
    std::{iter::FromIterator, mem::size_of, ops::Deref},
};

use hex::FromHexError;
use schemars::JsonSchema;

// TODO:
//  1. decide what will be pulled out into a "pythnet" crate and what needs to remain in here
//      a. be careful of cyclic dependencies
//      b. git submodules?

/*** Dummy Field(s) for now just to test updating the sysvar ***/
pub type Slot = u64;

#[repr(C)]
#[derive(
    Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize, Default, Debug,
)]
pub struct Accumulator {
    pub merkle_tree: MerkleTree,
}

// TODO: this needs to store all relevant information that will go into the
// proof - everything but the unused fields
// #[repr(transparent)]
// pub struct AccumulatorPrice(u32);
// TODO: check if this is correct repr
// might need to use #[repr(align(x))]
#[repr(C)]
pub struct AccumulatorPrice {
    pub price_type: u32,
}

impl Accumulator {
    pub fn new(price_accounts: &Vec<&PriceAccount>) -> (Self, Vec<Proof>) {
        let accumulator = Self {
            merkle_tree: MerkleTree::new(
                price_accounts
                    .iter()
                    .map(|p_a| AccumulatorPrice {
                        price_type: p_a.price_type,
                    })
                    .collect::<Vec<AccumulatorPrice>>()
                    .as_slice(),
            ),
            // merkle_tree2: MerkleTree::new(
            //     price_accounts
            //         .iter()
            //         .map(|p_a| AccumulatorPrice(p_a.price_type))
            //         .collect::<Vec<AccumulatorPrice>>()
            //         .as_slice(),
            // )
            // .nodes,
        };
        //TODO: plz handle failures & errors
        let proofs: Vec<Proof> = price_accounts
            .iter()
            .enumerate()
            .map(|(idx, _)| accumulator.merkle_tree.find_path(idx).unwrap())
            .collect();
        (accumulator, proofs)
    }
    // pub fn add(&mut self, price_accounts: Vec<PriceAccount>) {
    //     MerkleTree::new(price_accounts.map(|p_a| {
    //         AccumulatorPrice {
    //             price_type: p_a.price_type,
    //         }
    //         .as_ref()
    //     }))
    // }
}

// impl AsRef<[u8]> for AccumulatorPrice {
//     fn as_ref(&self) -> &[u8] {
//         // implement
//         // unsafe { core::slice::from_raw_parts_mut(self as *mut Self as *mut u8, 4 as usize) }
//         // unsafe {  &*(self as *Self as *const [u8]) }
//         unsafe { *(&self as *const _ as *const &[u8]) }
//     }
// }
//
// struct AccumulatorPrice2 {
//     price_type: u32,
// }
/**
bless chatGPT

This implementation uses std::mem::transmute to cast
a reference to the AccumulatorPrice struct to a
byte array of the same size, and then creates a slice from the resulting pointer.
The size of the slice is the size of the struct in bytes,
which is the sum of the sizes of the u32 and u64 fields.

Note that this implementation still uses unsafe code
to create a raw pointer to the AccumulatorPrice struct,
so you need to ensure that the pointer is valid and that
the memory it points to is properly aligned and initialized.

struct AccumulatorPrice {
    price_type: u32,
    price: u64,
}

impl AsRef<[u8]> for AccumulatorPrice {
    fn as_ref(&self) -> &[u8] {
        unsafe {
            std::slice::from_raw_parts(
                std::mem::transmute::<&AccumulatorPrice, &[u8; std::mem::size_of::<AccumulatorPrice>()]>(self).as_ptr(),
                std::mem::size_of::<AccumulatorPrice>(),
            )
        }
    }
}
*/

impl AsRef<[u8]> for AccumulatorPrice {
    fn as_ref(&self) -> &[u8] {
        unsafe {
            std::slice::from_raw_parts(
                &self.price_type as *const u32 as *const u8,
                std::mem::size_of::<u32>(),
            )
        }
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
pub type PriceHash = Proof;
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
//
// impl FromIterator<(PriceId, PriceHash<'_>)> for PriceProofs<'_> {
//     fn from_iter<I: for<'a> IntoIterator<Item = (PriceId, PriceHash<'a>)>>(iter: 'a I) -> Self {
//         Self(iter.into_iter().collect())
//     }
//     // fn from_iter<I: IntoIterator<Item = (PriceId, PriceHash)>>(iter: I) -> Self {
//     //     Self(iter.into_iter().collect())
//     // }
// }

// impl Deref for PriceProofs<'_> {
//     type Target<'a> = Vec<PriceProof<'a>>;
//     fn deref(&self) -> &Self::Target {
//         &self.0
//     }
// }

#[repr(C)]
#[derive(Copy, Clone, Zeroable, Pod)]
pub struct AccountHeader {
    pub magic_number: u32,
    pub version: u32,
    pub account_type: u32,
    pub size: u32,
}

pub const PC_MAP_TABLE_SIZE: u32 = 640;
pub const PC_MAGIC: u32 = 2712847316;
pub const PC_VERSION: u32 = 2;

#[repr(C)]
#[derive(Copy, Clone)]
pub struct MappingAccount {
    pub header: AccountHeader,
    pub number_of_products: u32,
    pub unused_: u32,
    pub next_mapping_account: Pubkey,
    pub products_list: [Pubkey; PC_MAP_TABLE_SIZE as usize],
}

pub const PC_ACCTYPE_MAPPING: u32 = 1;
pub const PC_MAP_TABLE_T_PROD_OFFSET: size_t = 56;

impl PythAccount for MappingAccount {
    const ACCOUNT_TYPE: u32 = PC_ACCTYPE_MAPPING;
    /// Equal to the offset of `prod_` in `MappingAccount`, see the trait comment for more detail
    const INITIAL_SIZE: u32 = PC_MAP_TABLE_T_PROD_OFFSET as u32;
}

// Unsafe impl because product_list is of size 640 and there's no derived trait for this size
unsafe impl Pod for MappingAccount {}

unsafe impl Zeroable for MappingAccount {}

#[repr(C)]
// #[derive(Copy, Clone, Pod, Zeroable)]
#[cfg_attr(not(test), derive(Copy, Clone, Pod, Zeroable))]
#[cfg_attr(test, derive(Copy, Clone, Pod, Zeroable, Default))]
pub struct PriceAccount {
    pub header: AccountHeader,
    /// Type of the price account
    pub price_type: u32,
    /// Exponent for the published prices
    pub exponent: i32,
    /// Current number of authorized publishers
    pub num_: u32,
    /// Number of valid quotes for the last aggregation
    pub num_qt_: u32,
    /// Last slot with a succesful aggregation (status : TRADING)
    pub last_slot_: u64,
    /// Second to last slot where aggregation was attempted
    pub valid_slot_: u64,
    /// Ema for price
    pub twap_: PriceEma,
    /// Ema for confidence
    pub twac_: PriceEma,
    /// Last time aggregation was attempted
    pub timestamp_: i64,
    /// Minimum valid publisher quotes for a succesful aggregation
    pub min_pub_: u8,
    pub unused_1_: i8,
    pub unused_2_: i16,
    pub unused_3_: i32,
    /// Corresponding product account
    pub product_account: Pubkey,
    /// Next price account in the list
    pub next_price_account: Pubkey,
    /// Second to last slot where aggregation was succesful (i.e. status : TRADING)
    pub prev_slot_: u64,
    /// Aggregate price at prev_slot_
    pub prev_price_: i64,
    /// Confidence interval at prev_slot_
    pub prev_conf_: u64,
    /// Timestamp of prev_slot_
    pub prev_timestamp_: i64,
    /// Last attempted aggregate results
    pub agg_: PriceInfo,
    /// Publishers' price components
    pub comp_: [PriceComponent; PC_COMP_SIZE as usize],
}

pub const PC_COMP_SIZE: u32 = 32;

#[repr(C)]
// #[derive(Copy, Clone, Pod, Zeroable)]
#[cfg_attr(not(test), derive(Copy, Clone, Pod, Zeroable))]
#[cfg_attr(test, derive(Copy, Clone, Pod, Zeroable, Default))]
pub struct PriceComponent {
    pub pub_: Pubkey,
    pub agg_: PriceInfo,
    pub latest_: PriceInfo,
}

#[repr(C)]
// #[derive(Debug, Copy, Clone, Pod, Zeroable)]
#[cfg_attr(not(test), derive(Copy, Clone, Pod, Zeroable))]
#[cfg_attr(test, derive(Copy, Clone, Pod, Zeroable, Default))]
pub struct PriceInfo {
    pub price_: i64,
    pub conf_: u64,
    pub status_: u32,
    pub corp_act_status_: u32,
    pub pub_slot_: u64,
}

#[repr(C)]
// #[derive(Debug, Copy, Clone, Pod, Zeroable)]
#[cfg_attr(not(test), derive(Copy, Clone, Pod, Zeroable))]
#[cfg_attr(test, derive(Copy, Clone, Pod, Zeroable, Default))]
pub struct PriceEma {
    pub val_: i64,
    pub numer_: i64,
    pub denom_: i64,
}

pub const PC_ACCTYPE_PRICE: u32 = 3;
pub type size_t = ::std::os::raw::c_ulong;
pub const PC_PRICE_T_COMP_OFFSET: size_t = 240;

impl PythAccount for PriceAccount {
    const ACCOUNT_TYPE: u32 = PC_ACCTYPE_PRICE;
    /// Equal to the offset of `comp_` in `PriceAccount`, see the trait comment for more detail
    const INITIAL_SIZE: u32 = PC_PRICE_T_COMP_OFFSET as u32;
}

/// The PythAccount trait's purpose is to attach constants to the 3 types of accounts that Pyth has
/// (mapping, price, product). This allows less duplicated code, because now we can create generic
/// functions to perform common checks on the accounts and to load and initialize the accounts.
pub trait PythAccount: Pod {
    /// `ACCOUNT_TYPE` is just the account discriminator, it is different for mapping, product and
    /// price
    const ACCOUNT_TYPE: u32;

    /// `INITIAL_SIZE` is the value that the field `size_` will take when the account is first
    /// initialized this one is slightly tricky because for mapping (resp. price) `size_` won't
    /// include the unpopulated entries of `prod_` (resp. `comp_`). At the beginning there are 0
    /// products (resp. 0 components) therefore `INITIAL_SIZE` will be equal to the offset of
    /// `prod_` (resp. `comp_`)  Similarly the product account `INITIAL_SIZE` won't include any
    /// key values.
    const INITIAL_SIZE: u32;

    /// `minimum_size()` is the minimum size that the solana account holding the struct needs to
    /// have. `INITIAL_SIZE` <= `minimum_size()`
    const MINIMUM_SIZE: usize = size_of::<Self>();

    // /// Given an `AccountInfo`, verify it is sufficiently large and has the correct discriminator.
    // fn initialize<'a>(
    //     account: &'a AccountInfo,
    //     version: u32,
    // ) -> Result<RefMut<'a, Self>, ProgramError> {
    //     pyth_assert(
    //         account.data_len() >= Self::MINIMUM_SIZE,
    //         OracleError::AccountTooSmall.into(),
    //     )?;
    //
    //     check_valid_fresh_account(account)?;
    //     clear_account(account)?;
    //
    //     {
    //         let mut account_header = load_account_as_mut::<AccountHeader>(account)?;
    //         account_header.magic_number = PC_MAGIC;
    //         account_header.version = version;
    //         account_header.account_type = Self::ACCOUNT_TYPE;
    //         account_header.size = Self::INITIAL_SIZE;
    //     }
    //     load_account_as_mut::<Self>(account)
    // }
    //
    // /// Creates PDA accounts only when needed, and initializes it as one of the Pyth accounts.
    // /// This PDA initialization assumes that the account has 0 lamports.
    // /// TO DO: Fix this once we can resize the program.
    // fn initialize_pda<'a>(
    //     account: &AccountInfo<'a>,
    //     funding_account: &AccountInfo<'a>,
    //     system_program: &AccountInfo<'a>,
    //     program_id: &Pubkey,
    //     seeds: &[&[u8]],
    //     version: u32,
    // ) -> Result<(), ProgramError> {
    //     let target_rent = get_rent()?.minimum_balance(Self::MINIMUM_SIZE);
    //
    //     if account.data_len() == 0 {
    //         create(
    //             funding_account,
    //             account,
    //             system_program,
    //             program_id,
    //             Self::MINIMUM_SIZE,
    //             target_rent,
    //             seeds,
    //         )?;
    //         Self::initialize(account, version)?;
    //     }
    //
    //     Ok(())
    // }
}

/// Interpret the bytes in `data` as a value of type `T`
/// This will fail if :
/// - `data` is too short
/// - `data` is not aligned for T
pub fn load<T: Pod>(data: &[u8]) -> &T {
    try_from_bytes(data.get(0..size_of::<T>()).unwrap()).unwrap()
}

// pub fn load_checked<'a, T: PythAccount>(
//     account: &'a AccountInfo,
//     version: u32,
// ) -> Result<RefMut<'a, T>, ProgramError> {
//     pyth_assert(
//         account.data_len() >= T::MINIMUM_SIZE,
//         OracleError::AccountTooSmall.into(),
//     )?;
//
//     {
//         let account_header = load_account_as::<AccountHeader>(account)?;
//         pyth_assert(
//             account_header.magic_number == PC_MAGIC
//                 && account_header.version == version
//                 && account_header.account_type == T::ACCOUNT_TYPE,
//             OracleError::InvalidAccountHeader.into(),
//         )?;
//     }
//
//     load_account_as_mut::<T>(account)
// }

// We need to discern between leaf and intermediate nodes to prevent trivial second
// pre-image attacks.
// https://flawed.net.nz/2018/02/21/attacking-merkle-trees-with-a-second-preimage-attack

// We need to discern between leaf and intermediate nodes to prevent trivial second
// pre-image attacks.
// https://flawed.net.nz/2018/02/21/attacking-merkle-trees-with-a-second-preimage-attack
const LEAF_PREFIX: &[u8] = &[0];
const INTERMEDIATE_PREFIX: &[u8] = &[1];

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

#[derive(
    Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize, Default,
)]
pub struct MerkleTree {
    pub leaf_count: usize,
    pub nodes: Vec<Hash>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProofEntry(Hash, Option<Hash>, Option<Hash>);

impl<'a> ProofEntry {
    pub fn new(target: Hash, left_sibling: Option<Hash>, right_sibling: Option<Hash>) -> Self {
        assert!(left_sibling.is_none() ^ right_sibling.is_none());
        Self(target, left_sibling, right_sibling)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Proof(Vec<ProofEntry>);

impl Proof {
    pub fn push(&mut self, entry: ProofEntry) {
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

impl MerkleTree {
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

    pub fn new<T: AsRef<[u8]>>(items: &[T]) -> Self {
        let cap = MerkleTree::calculate_vec_capacity(items.len());
        let mut mt = MerkleTree {
            leaf_count: items.len(),
            nodes: Vec::with_capacity(cap),
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

    pub fn get_root(&self) -> Option<&Hash> {
        self.nodes.iter().last()
    }

    pub fn find_path(&self, index: usize) -> Option<Proof> {
        if index >= self.leaf_count {
            return None;
        }

        let mut level_len = self.leaf_count;
        let mut level_start = 0;
        let mut path = Proof::default();
        let mut node_index = index;
        let mut lsib = None;
        let mut rsib = None;
        while level_len > 0 {
            let level = &self.nodes[level_start..(level_start + level_len)];

            let target = level[node_index];
            if lsib.is_some() || rsib.is_some() {
                path.push(ProofEntry::new(target, lsib, rsib));
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
}

#[derive(Clone, Default, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccumulatorAttestation {
    pub accumulator: Accumulator,
    #[serde(serialize_with = "use_to_string")]
    pub ring_buffer_idx: u64,
    #[serde(serialize_with = "use_to_string")]
    pub height: u64,
    pub timestamp: UnixTimestamp,
}

pub type ErrBox = Box<dyn std::error::Error>;

/// Precedes every message implementing the p2w serialization format
pub const PACC2W_MAGIC: &[u8] = b"acc";

/// Format version used and understood by this codebase
pub const P2W_FORMAT_VER_MAJOR: u16 = 3;

/// Starting with v3, format introduces a minor version to mark
/// forward-compatible iterations.
/// IMPORTANT: Remember to reset this to 0 whenever major version is
/// bumped.
/// Changelog:
/// * v3.1 - last_attested_publish_time field added
pub const P2W_FORMAT_VER_MINOR: u16 = 1;

/// Starting with v3, format introduces append-only
/// forward-compatibility to the header. This is the current number of
/// bytes after the hdr_size field. After the specified bytes, inner
/// payload-specific fields begin.
pub const P2W_FORMAT_HDR_SIZE: u16 = 1;

pub const PUBKEY_LEN: usize = 32;

#[repr(u8)]
pub enum PayloadId {
    PriceAttestation = 1,      // Not in use
    PriceBatchAttestation = 2, // Not in use
    AccumulationAttestation = 3,
}

// from pyth-crosschain/wormhole_attester/sdk/rust/src/lib.rs
impl AccumulatorAttestation {
    /**
    let acc_vaa_payload = accumulatorAttestation.serialize().map_err(|e| {
        trace!(&e.to_string());
        ProgramError::InvalidAccountData
    })?;
    MessageData {
        //..
        payload: acc_vaa_payload,
    }
    */
    pub fn serialize(&self) -> Result<Vec<u8>, ErrBox> {
        // magic
        let mut buf = PACC2W_MAGIC.to_vec();

        // major_version
        buf.extend_from_slice(&P2W_FORMAT_VER_MAJOR.to_be_bytes()[..]);

        // minor_version
        buf.extend_from_slice(&P2W_FORMAT_VER_MINOR.to_be_bytes()[..]);

        // hdr_size
        buf.extend_from_slice(&P2W_FORMAT_HDR_SIZE.to_be_bytes()[..]);

        // // payload_id
        buf.push(PayloadId::AccumulationAttestation as u8);

        // Header is over. NOTE: If you need to append to the header,
        // make sure that the number of bytes after hdr_size is
        // reflected in the P2W_FORMAT_HDR_SIZE constant.

        // n_attestations
        // buf.extend_from_slice(&(self.price_attestations.len() as u16).to_be_bytes()[..]);

        // TODO: is u16 enough?
        //
        // buf.extend_from_slice(&(self.accumulator.merkle_tree.leaf_count as u16).to_be_bytes()[..]);

        let AccumulatorAttestation {
            accumulator,
            ring_buffer_idx,
            height,
            timestamp,
        } = self;
        // let mut accumulator_buf = Vec::with_capacity(accumulator.merkle_tree.leaf_count);
        //TODO: decide on pyth-accumulator-over-wormhole serialization format.
        let mut serialized_acc = bincode::serialize(&accumulator).unwrap();

        buf.extend_from_slice(&(serialized_acc.len() as u16).to_be_bytes()[..]);
        buf.append(&mut serialized_acc);
        buf.extend_from_slice(&ring_buffer_idx.to_be_bytes()[..]);
        buf.extend_from_slice(&height.to_be_bytes()[..]);
        buf.extend_from_slice(&timestamp.to_be_bytes()[..]);

        Ok(buf)
    }

    //TODO: update this for accumulator attest
    pub fn deserialize(mut bytes: impl Read) -> Result<Self, ErrBox> {
        let mut magic_vec = vec![0u8; PACC2W_MAGIC.len()];
        bytes.read_exact(magic_vec.as_mut_slice())?;

        if magic_vec.as_slice() != PACC2W_MAGIC {
            return Err(
                format!("Invalid magic {magic_vec:02X?}, expected {PACC2W_MAGIC:02X?}",).into(),
            );
        }

        let mut major_version_vec = vec![0u8; mem::size_of_val(&P2W_FORMAT_VER_MAJOR)];
        bytes.read_exact(major_version_vec.as_mut_slice())?;
        let major_version = u16::from_be_bytes(major_version_vec.as_slice().try_into()?);

        // Major must match exactly
        if major_version != P2W_FORMAT_VER_MAJOR {
            return Err(format!(
                "Unsupported format major_version {major_version}, expected {P2W_FORMAT_VER_MAJOR}"
            )
            .into());
        }

        let mut minor_version_vec = vec![0u8; mem::size_of_val(&P2W_FORMAT_VER_MINOR)];
        bytes.read_exact(minor_version_vec.as_mut_slice())?;
        let minor_version = u16::from_be_bytes(minor_version_vec.as_slice().try_into()?);

        // Only older minors are not okay for this codebase
        if minor_version < P2W_FORMAT_VER_MINOR {
            return Err(format!(
                "Unsupported format minor_version {minor_version}, expected {P2W_FORMAT_VER_MINOR} or more"
            )
            .into());
        }

        // Read header size value
        let mut hdr_size_vec = vec![0u8; mem::size_of_val(&P2W_FORMAT_HDR_SIZE)];
        bytes.read_exact(hdr_size_vec.as_mut_slice())?;
        let hdr_size = u16::from_be_bytes(hdr_size_vec.as_slice().try_into()?);

        // Consume the declared number of remaining header
        // bytes. Remaining header fields must be read from hdr_buf
        let mut hdr_buf = vec![0u8; hdr_size as usize];
        bytes.read_exact(hdr_buf.as_mut_slice())?;

        let mut payload_id_vec = vec![0u8; mem::size_of::<PayloadId>()];
        hdr_buf
            .as_slice()
            .read_exact(payload_id_vec.as_mut_slice())?;

        if payload_id_vec[0] != PayloadId::AccumulationAttestation as u8 {
            return Err(format!(
                "Invalid Payload ID {}, expected {}",
                payload_id_vec[0],
                PayloadId::AccumulationAttestation as u8,
            )
            .into());
        }

        // Header consumed, continue with remaining fields
        let mut accum_len_vec = vec![0u8; mem::size_of::<u16>()];
        bytes.read_exact(accum_len_vec.as_mut_slice())?;
        let accum_len = u16::from_be_bytes(accum_len_vec.as_slice().try_into()?);

        // let accum_vec = Vec::with_capacity(accum_len_vec as usize);
        let mut accum_vec = vec![0u8; accum_len as usize];
        bytes.read_exact(accum_vec.as_mut_slice())?;
        let accumulator = match bincode::deserialize(accum_vec.as_slice()) {
            Ok(acc) => acc,
            Err(e) => return Err(format!("AccumulatorDeserialization failed: {}", e).into()),
        };

        let mut ring_buff_idx_vec = vec![0u8; mem::size_of::<u64>()];
        bytes.read_exact(ring_buff_idx_vec.as_mut_slice());
        let ring_buffer_idx = u64::from_be_bytes(ring_buff_idx_vec.as_slice().try_into()?);

        let mut height_vec = vec![0u8; mem::size_of::<u64>()];
        bytes.read_exact(height_vec.as_mut_slice());
        let height = u64::from_be_bytes(height_vec.as_slice().try_into()?);

        let mut timestamp_vec = vec![0u8; mem::size_of::<UnixTimestamp>()];
        bytes.read_exact(timestamp_vec.as_mut_slice())?;
        let timestamp = UnixTimestamp::from_be_bytes(timestamp_vec.as_slice().try_into()?);

        Ok(Self {
            accumulator,
            ring_buffer_idx,
            height,
            timestamp,
        })
    }
}

pub fn use_to_string<T, S>(val: &T, s: S) -> Result<S::Ok, S::Error>
where
    T: ToString,
    S: Serializer,
{
    s.serialize_str(&val.to_string())
}

pub fn pubkey_to_hex<S>(val: &Identifier, s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    s.serialize_str(&hex::encode(val.to_bytes()))
}

#[derive(
    Copy,
    Clone,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    BorshSerialize,
    BorshDeserialize,
    serde::Serialize,
    serde::Deserialize,
    JsonSchema,
)]
#[repr(C)]
pub struct Identifier(
    #[serde(with = "hex")]
    #[schemars(with = "String")]
    [u8; 32],
);

impl Identifier {
    pub fn new(bytes: [u8; 32]) -> Identifier {
        Identifier(bytes)
    }

    pub fn to_bytes(&self) -> [u8; 32] {
        self.0
    }

    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    pub fn from_hex<T: AsRef<[u8]>>(s: T) -> Result<Identifier, FromHexError> {
        let mut bytes = [0u8; 32];
        hex::decode_to_slice(s, &mut bytes)?;
        Ok(Identifier::new(bytes))
    }
}

impl fmt::Debug for Identifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{}", self.to_hex())
    }
}

impl fmt::Display for Identifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{}", self.to_hex())
    }
}

impl AsRef<[u8]> for Identifier {
    fn as_ref(&self) -> &[u8] {
        &self.0[..]
    }
}

#[repr(transparent)]
#[derive(Default)]
pub struct PostedMessageUnreliableData {
    pub message: MessageData,
}

#[derive(Debug, Default, BorshSerialize, BorshDeserialize, Clone, Serialize, Deserialize)]
pub struct MessageData {
    /// Header of the posted VAA
    pub vaa_version: u8,

    /// Level of consistency requested by the emitter
    pub consistency_level: u8,

    /// Time the vaa was submitted
    pub vaa_time: u32,

    /// Account where signatures are stored
    pub vaa_signature_account: Pubkey,

    /// Time the posted message was created
    pub submission_time: u32,

    /// Unique nonce for this message
    pub nonce: u32,

    /// Sequence number of this message
    pub sequence: u64,

    /// Emitter of the message
    pub emitter_chain: u16,

    /// Emitter of the message
    pub emitter_address: [u8; 32],

    /// Message payload
    pub payload: Vec<u8>,
}

impl BorshSerialize for PostedMessageUnreliableData {
    fn serialize<W: Write>(&self, writer: &mut W) -> std::io::Result<()> {
        writer.write_all(b"msu")?;
        BorshSerialize::serialize(&self.message, writer)
    }
}

impl BorshDeserialize for PostedMessageUnreliableData {
    fn deserialize(buf: &mut &[u8]) -> std::io::Result<Self> {
        if buf.len() < 3 {
            return Err(Error::new(InvalidData, "Not enough bytes"));
        }

        let expected = b"msu";
        let magic: &[u8] = &buf[0..3];
        if magic != expected {
            return Err(Error::new(
                InvalidData,
                format!(
                    "Magic mismatch. Expected {:?} but got {:?}",
                    expected, magic
                ),
            ));
        };
        *buf = &buf[3..];
        Ok(PostedMessageUnreliableData {
            message: <MessageData as BorshDeserialize>::deserialize(buf)?,
        })
    }
}

impl Deref for PostedMessageUnreliableData {
    type Target = MessageData;

    fn deref(&self) -> &Self::Target {
        &self.message
    }
}

impl DerefMut for PostedMessageUnreliableData {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.message
    }
}

impl Clone for PostedMessageUnreliableData {
    fn clone(&self) -> Self {
        PostedMessageUnreliableData {
            message: self.message.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accumulator::PayloadId::AccumulationAttestation;

    impl Default for AccountHeader {
        fn default() -> Self {
            Self {
                magic_number: PC_MAGIC,
                version: 0,
                account_type: PC_ACCTYPE_PRICE,
                size: 0,
            }
        }
    }

    // TODO: - keep current cfg_attr or implement it all here?
    // impl Default for PriceInfo {
    //     fn default() -> Self {
    //         Self {
    //            todo!()
    //         }
    //     }
    // }
    // impl Default for PriceEma {
    //     fn default() -> Self {
    //         Self {
    //            todo!()
    //         }
    //     }
    // }
    // impl Default for PriceComponent {
    //     fn default() -> Self {
    //         Self {
    //            todo!()
    //          }
    //     }
    // }
    //
    // impl Default for PriceAccount {
    //     fn default() -> Self {
    //         Self {
    //             ..Default::default()
    //         }
    //     }
    // }

    impl AccountHeader {
        fn new(account_type: u32) -> Self {
            Self {
                account_type,
                ..AccountHeader::default()
            }
        }
    }

    // only using the price_type field for hashing for merkle tree.
    fn generate_price_account(price_type: u32) -> (Pubkey, PriceAccount) {
        (
            Pubkey::new_unique(),
            PriceAccount {
                price_type,
                header: AccountHeader::new(PC_ACCTYPE_PRICE),
                ..PriceAccount::default()
            },
        )
    }

    #[test]
    fn test_pa_default() {
        println!("testing pa");
        let acct_header = AccountHeader::default();
        println!("acct_header.acct_type: {}", acct_header.account_type);
        let pa = PriceAccount::default();
        println!("price_account.price_type: {}", pa.price_type);
    }

    #[test]
    fn test_new_accumulator() {
        let price_accts_and_keys: Vec<(Pubkey, PriceAccount)> =
            (0..2).map(|i| generate_price_account(i)).collect();
        let price_accts: Vec<&PriceAccount> =
            price_accts_and_keys.iter().map(|(_, pa)| pa).collect();
        let (acc, proofs) = Accumulator::new(&price_accts);
        println!("acc: {acc:#?}\nproofs:{proofs:#?}")
    }

    #[test]
    fn test_accumulator_attest_serde() -> Result<(), ErrBox> {
        let price_accts_and_keys: Vec<(Pubkey, PriceAccount)> =
            (0..2).map(|i| generate_price_account(i)).collect();
        let price_accts: Vec<&PriceAccount> =
            price_accts_and_keys.iter().map(|(_, pa)| pa).collect();
        let (accumulator, proofs) = Accumulator::new(&price_accts);

        // arbitrary values
        let ring_buffer_idx = 17;
        let height = 28;
        let timestamp = 294;

        let accumulator_attest = AccumulatorAttestation {
            accumulator,
            ring_buffer_idx,
            height,
            timestamp,
        };

        println!("accumulator attest hex struct:  {accumulator_attest:#02X?}");

        let serialized = accumulator_attest.serialize()?;
        println!("accumulator attest hex bytes: {serialized:02X?}");

        let deserialized = AccumulatorAttestation::deserialize(serialized.as_slice())?;

        println!("deserialized accumulator attest hex struct:  {deserialized:#02X?}");
        assert_eq!(accumulator_attest, deserialized);
        Ok(())
    }
}
