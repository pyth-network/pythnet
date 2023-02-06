//! A type to hold data for the [`Accumulator` sysvar][sv].
//!
//! TODO: replace this with an actual link if needed
//! [sv]: https://docs.pythnetwork.org/developing/runtime-facilities/sysvars#accumulator
//!
//! The sysvar ID is declared in [`sysvar::accumulator`].
//!
//! [`sysvar::accumulator`]: crate::sysvar::accumulator


/*** Dummy Fields for now just to test updating the sysvar ***/
/// The unit of time given to a leader for encoding a block.
///
/// It is some some number of _ticks_ long.
pub type Slot = u64;

//TODO: #[derive(AbiExample)]?
#[repr(C)]
#[derive(Serialize, Deserialize, Debug, CloneZeroed, Default, PartialEq, Eq)]
pub struct Accumulator {
	pub slot: Slot,
}


