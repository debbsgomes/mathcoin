#!/usr/bin/env node
import { Pool } from 'pg'
import { publish } from './publish'

const databaseUrl = process.env.DATABASE_URL
if (!databaseUrl) {
  console.error('DATABASE_URL environment variable is required')
  process.exit(1)
}

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
