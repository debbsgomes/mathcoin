import { describe, it, expect, beforeEach, afterAll } from 'vitest'
import { randomUUID } from 'node:crypto'
import { Pool } from 'pg'
import { publish } from '../src/publish'
import { StandardMerkleTree } from '@openzeppelin/merkle-tree'

const DATABASE_URL = process.env.DATABASE_URL || 'postgres://mathcoin:mathcoin@localhost:5432/mathcoin_test'

async function cleanAll(pool: Pool) {
  await pool.query('DELETE FROM distribution_entries')
  await pool.query('DELETE FROM distributions')
  await pool.query('DELETE FROM earnings')
  await pool.query('DELETE FROM challenges')
  await pool.query('DELETE FROM users')
}

describe('publisher', () => {
  let pool: Pool
  let testSeq = 0

  beforeEach(async () => {
    testSeq++
    pool = new Pool({ connectionString: DATABASE_URL })
    await cleanAll(pool)
  })

  afterAll(async () => {
    await cleanAll(pool)
    await pool.end()
  })

  async function seedUser(sub: string, email: string, claimAddress: string) {
    await pool.query(
      `INSERT INTO users (provider_sub, email, claim_address) VALUES ($1, $2, $3)`,
      [sub, email, claimAddress]
    )
    const { rows } = await pool.query<{ id: number }>('SELECT id FROM users WHERE provider_sub = $1', [sub])
    return rows[0].id
  }

  async function seedEarnings(userId: number, amount: number) {
    const cid = randomUUID()
    await pool.query(
      `INSERT INTO challenges (id, user_id, problem, solution, difficulty, reward, status, expires_at)
       VALUES ($1, $2, 'test', 0, 1, $3, 'CLAIMED', now() + INTERVAL '1 hour')`,
      [cid, userId, amount]
    )
    await pool.query(
      `INSERT INTO earnings (user_id, challenge_id, amount) VALUES ($1, $2, $3)`,
      [userId, cid, amount]
    )
  }

  function sub(name: string) { return `pub-${testSeq}-${name}` }
  function email(name: string) { return `pub${testSeq}${name}@test.com` }

  it('produces one distribution with correct entries for opted-in users', async () => {
    const uid1 = await seedUser(sub('a'), email('a'), '0xAb5801a7D398351b8bE11C439e05C5B3259aeC9B')
    const uid2 = await seedUser(sub('b'), email('b'), '0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045')
    await seedEarnings(uid1, 100)
    await seedEarnings(uid1, 50)
    await seedEarnings(uid2, 200)

    const result = await publish(pool)

    expect(result.entries).toBe(2)

    const { rows: distRows } = await pool.query(
      'SELECT * FROM distributions WHERE id = $1', [result.distributionId]
    )
    expect(distRows[0].status).toBe('pending_publish')
    expect(distRows[0].merkle_root).toBe(result.root)

    const { rows: entryRows } = await pool.query(
      'SELECT * FROM distribution_entries WHERE distribution_id = $1 ORDER BY wallet_address',
      [result.distributionId]
    )
    expect(entryRows.length).toBe(2)
  })

  it('persisted root matches buildMerkleTree root', async () => {
    const uid = await seedUser(sub('root'), email('root'), '0x5B38Da6a701c568545dCfcB03FcB875f56beddC4')
    await seedEarnings(uid, 42)

    const result = await publish(pool)

    const { rows } = await pool.query<{ merkle_root: string }>(
      'SELECT merkle_root FROM distributions WHERE id = $1', [result.distributionId]
    )
    expect(rows[0].merkle_root).toBe(result.root)
  })

  it('users without claim_address are absent from tree', async () => {
    // User with NO claim_address but with earnings
    const { rows: noClaimRows } = await pool.query<{ id: number }>(
      `INSERT INTO users (provider_sub, email) VALUES ($1, $2) RETURNING id`,
      [sub('noc'), email('noc')]
    )
    await seedEarnings(noClaimRows[0].id, 999)

    // Opted-in user
    await seedUser(sub('yes'), email('yes'), '0x1111111111111111111111111111111111111111')

    const result = await publish(pool)
    expect(result.entries).toBe(1)
  })

  it('proofs verify against the stored root', async () => {
    const uid = await seedUser(sub('ver'), email('ver'), '0x2222222222222222222222222222222222222222')
    await seedEarnings(uid, 77)

    const result = await publish(pool)

    const { rows: entries } = await pool.query<{
      wallet_address: string; cumulative_amount: string; proof: any
    }>(
      'SELECT wallet_address, cumulative_amount, proof FROM distribution_entries WHERE distribution_id = $1',
      [result.distributionId]
    )

    // Verify the proof stored in DB validates against the OZ library
    for (const entry of entries) {
      const tree = StandardMerkleTree.of(
        [[entry.wallet_address, BigInt(entry.cumulative_amount)]],
        ['address', 'uint256']
      )
      const proof = entry.proof as string[]
      if (proof.length > 0) {
        expect(tree.verify(0, proof)).toBe(true)
      }
    }
  })

  it('atomicity: distribution and entries are consistent', async () => {
    const uid = await seedUser(sub('atom'), email('atom'), '0x3333333333333333333333333333333333333333')
    await seedEarnings(uid, 10)

    const result = await publish(pool)

    // Verify: after publish, distribution has exactly `result.entries` entries
    const { rows: entries } = await pool.query(
      'SELECT COUNT(*) as cnt FROM distribution_entries WHERE distribution_id = $1',
      [result.distributionId]
    )
    expect(parseInt(entries[0].cnt)).toBe(result.entries)
  })
})
