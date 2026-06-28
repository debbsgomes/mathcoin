import { StandardMerkleTree } from '@openzeppelin/merkle-tree'

export interface MerkleEntry {
  address: string
  cumulativeAmount: bigint
}

export interface MerkleResult {
  root: string
  proofs: Map<string, string[]>
}

/**
 * Build a cumulative Merkle tree from (address, cumulativeAmount) entries.
 * Uses the OpenZeppelin StandardMerkleTree with keccak256 + double-hashed leaves,
 * guaranteeing bit-for-bit parity with on-chain MerkleProof.verify.
 *
 * This is a PURE function — no DB, no chain, no side effects.
 */
export function buildMerkleTree(entries: MerkleEntry[]): MerkleResult {
  const values = entries.map((e) => [e.address, e.cumulativeAmount] as [string, bigint])
  const tree = StandardMerkleTree.of(values, ['address', 'uint256'])

  const proofs = new Map<string, string[]>()
  for (const [i, entry] of entries.entries()) {
    proofs.set(entry.address, tree.getProof(i))
  }

  return {
    root: tree.root,
    proofs,
  }
}
