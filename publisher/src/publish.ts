import { Pool } from 'pg'
import { buildMerkleTree, MerkleEntry } from './buildMerkleTree'

/**
 * Publish a new Merkle distribution: snapshot opted-in users' cumulative earnings,
 * build the Merkle tree, and persist the root + per-address proofs in a single transaction.
 *
 * NO chain calls. NO private key. Pure data pipeline.
 */
export async function publish(pool: Pool): Promise<{ distributionId: number; root: string; entries: number }> {
  // Snapshot cumulative earnings per opted-in address
  const { rows } = await pool.query<{ claim_address: string; total: string }>(
    `SELECT u.claim_address, COALESCE(SUM(e.amount), 0)::BIGINT AS total
     FROM users u
     LEFT JOIN earnings e ON e.user_id = u.id
     WHERE u.claim_address IS NOT NULL
     GROUP BY u.claim_address`
  )

  if (rows.length === 0) {
    throw new Error('No opted-in users to snapshot')
  }

  const entries: MerkleEntry[] = rows.map((r) => ({
    address: r.claim_address,
    cumulativeAmount: BigInt(r.total),
  }))

  const { root, proofs } = buildMerkleTree(entries)

  const totalAmount = entries.reduce((sum, e) => sum + e.cumulativeAmount, 0n)

  // Write distribution + entries in ONE transaction
  const client = await pool.connect()
  try {
    await client.query('BEGIN')

    const distResult = await client.query<{ id: number }>(
      `INSERT INTO distributions (merkle_root, total_amount, status)
       VALUES ($1, $2, 'pending_publish')
       RETURNING id`,
      [root, totalAmount.toString()]
    )
    const distributionId = distResult.rows[0].id

    for (const entry of entries) {
      const proof = proofs.get(entry.address)!
      await client.query(
        `INSERT INTO distribution_entries (distribution_id, wallet_address, cumulative_amount, proof)
         VALUES ($1, $2, $3, $4)`,
        [distributionId, entry.address, entry.cumulativeAmount.toString(), JSON.stringify(proof)]
      )
    }

    await client.query('COMMIT')

    return { distributionId, root, entries: entries.length }
  } catch (err) {
    await client.query('ROLLBACK')
    throw err
  } finally {
    client.release()
  }
}

// Run as standalone script
if (require.main === module) {
  const databaseUrl = process.env.DATABASE_URL || 'postgres://mathcoin:mathcoin@localhost:5432/mathcoin_dev'
  const pool = new Pool({ connectionString: databaseUrl })

  publish(pool)
    .then((result) => {
      console.log(`Published distribution #${result.distributionId}`)
      console.log(`  root: ${result.root}`)
      console.log(`  entries: ${result.entries}`)
      process.exit(0)
    })
    .catch((err) => {
      console.error('Publish failed:', err)
      process.exit(1)
    })
    .finally(() => pool.end())
}
