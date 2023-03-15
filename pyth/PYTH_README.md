Pending Tasks:

- [] implement API for:
  - [] serializing "root" as well as an entire accumulator
  - [] Accumulator Proofs
    - [] MerkleAccumulator/MulAccumulator.items if not keeping as slice of bytes
- [] sync up if geyser plugin will handle generation of proof pdas
- [] implement parallelized fetching of price feed ids/accounts
  future optimization. not needed at currently magnitude of accounts to fetch (< 1000)
- [] error handling during accumulator/proof generation (`unwrap()`)
- [] adjust `measure!` logs in `bank.rs`
- [] add back serde-related tests
- [] ciborium/protobuf/flatbuffer for projection/compressing wire format
  - verkle tree?

## Notes

1. Accumulator & Proof Serialization

```
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
			where MerkleTreeProof_v2 = usize = index of leaf to reconstruct MerklePath(on-demand generation)
		}

	mul_accumulator:
		`accumulator.accumulator` goes into VAA (product of hash of all items)
		proof_pda: {
			Vec<(Key, MulProof)>
			where MulProof = accumulator.accumulator/bytes as u128
		}
```
