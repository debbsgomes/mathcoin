import { describe, it, expect } from 'vitest'
import { buildMerkleTree } from '../src/buildMerkleTree'
import { StandardMerkleTree } from '@openzeppelin/merkle-tree'
import * as fs from 'fs'
import * as path from 'path'

describe('buildMerkleTree', () => {
  it('builds tree for single leaf', () => {
    const entries = [{ address: '0xAb5801a7D398351b8bE11C439e05C5B3259aeC9B', cumulativeAmount: 100n }]
    const result = buildMerkleTree(entries)
    expect(result.root).toBeTruthy()
    expect(result.root).toMatch(/^0x[0-9a-f]{64}$/)
    expect(result.proofs.size).toBe(1)
  })

  it('builds tree for two leaves and generates valid proofs', () => {
    const entries = [
      { address: '0xAb5801a7D398351b8bE11C439e05C5B3259aeC9B', cumulativeAmount: 100n },
      { address: '0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045', cumulativeAmount: 250n },
    ]
    const result = buildMerkleTree(entries)

    for (const entry of entries) {
      const proof = result.proofs.get(entry.address)
      expect(proof).toBeDefined()
      expect(proof!.length).toBe(1) // 2 leaves → 1 sibling per proof
    }
  })

  it('builds tree for odd number of leaves (3)', () => {
    const entries = [
      { address: '0xAb5801a7D398351b8bE11C439e05C5B3259aeC9B', cumulativeAmount: 100n },
      { address: '0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045', cumulativeAmount: 200n },
      { address: '0x5B38Da6a701c568545dCfcB03FcB875f56beddC4', cumulativeAmount: 300n },
    ]
    const result = buildMerkleTree(entries)

    // Verify each proof validates against OZ's own verification
    const tree = StandardMerkleTree.of(
      entries.map(e => [e.address, e.cumulativeAmount] as [string, bigint]),
      ['address', 'uint256']
    )

    for (const [i, entry] of entries.entries()) {
      const proof = result.proofs.get(entry.address)!
      expect(proof).toBeDefined()
      // Verify using OZ's own verify method
      expect(tree.verify(i, proof)).toBe(true)
    }
  })

  it('same data produces same root (deterministic)', () => {
    const entries = [
      { address: '0xAb5801a7D398351b8bE11C439e05C5B3259aeC9B', cumulativeAmount: 42n },
    ]
    const r1 = buildMerkleTree(entries)
    const r2 = buildMerkleTree(entries)
    expect(r1.root).toBe(r2.root)
    expect(r1.proofs.get(entries[0].address)).toEqual(r2.proofs.get(entries[0].address))
  })
})

// ---- Cross-stack parity test ----

describe('Cross-stack parity (TS → Solidity)', () => {
  it('exports root + proof to fixture that Solidity test can verify', () => {
    const entries: { address: string; cumulativeAmount: bigint }[] = [
      { address: '0x1111111111111111111111111111111111111111', cumulativeAmount: 100n },
      { address: '0x2222222222222222222222222222222222222222', cumulativeAmount: 250n },
      { address: '0x3333333333333333333333333333333333333333', cumulativeAmount: 500n },
    ]

    const result = buildMerkleTree(entries)
    const targetAddr = '0x2222222222222222222222222222222222222222'
    const proof = result.proofs.get(targetAddr)!

    // Verify proof is valid using OZ's own verification
    const tree = StandardMerkleTree.of(
      entries.map(e => [e.address, e.cumulativeAmount] as [string, bigint]),
      ['address', 'uint256']
    )
    const targetIdx = entries.findIndex(e => e.address === targetAddr)
    expect(tree.verify(targetIdx, proof)).toBe(true)

    // Build leaf for the Solidity side to verify against
    // leaf = keccak256(bytes.concat(keccak256(abi.encode(address, uint256))))
    // We compute it here and put it in the fixture
    const { keccak256, encodeAbiParameters, concat } = require('viem')
    const leaf = keccak256(
      concat([keccak256(encodeAbiParameters(
        [{ type: 'address' }, { type: 'uint256' }],
        [targetAddr, 250n]
      ))])
    )

    const fixture = {
      root: result.root,
      account: targetAddr,
      cumulativeAmount: '250',
      leaf,
      proof,
    }

    const fixturePath = path.join(__dirname, '..', '..', 'contracts', 'test', 'fixtures', 'parity_proof.json')
    fs.mkdirSync(path.dirname(fixturePath), { recursive: true })
    fs.writeFileSync(fixturePath, JSON.stringify(fixture, null, 2))

    expect(fixture.root).toBeTruthy()
    expect(fixture.proof.length).toBeGreaterThan(0)
  })
})
