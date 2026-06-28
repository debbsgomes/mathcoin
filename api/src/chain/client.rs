use std::sync::Arc;
use tokio::sync::Mutex;

/// A single Claimed event from the contract.
#[derive(Debug, Clone, PartialEq)]
pub struct ClaimedEvent {
    pub account: String,
    pub cumulative_amount: u64,
    pub block_number: u64,
    pub tx_hash: String,
}

/// Port: reads on-chain state from the MathCoin contract.
/// The production implementation uses alloy; tests inject a mock.
#[async_trait::async_trait]
pub trait ChainClient: Send + Sync {
    async fn get_claimed(&self, address: &str) -> Result<u64, String>;
    async fn get_balance(&self, address: &str) -> Result<u64, String>;
    async fn get_current_root(&self) -> Result<String, String>;
    async fn get_claim_events(&self, from_block: u64) -> Result<Vec<ClaimedEvent>, String>;
}

/// Mock implementation that returns configurable data — no live RPC.
pub struct MockChainClient {
    pub claimed: Mutex<std::collections::HashMap<String, u64>>,
    pub balance: Mutex<std::collections::HashMap<String, u64>>,
    pub root: Mutex<String>,
    pub events: Mutex<Vec<ClaimedEvent>>,
}

impl MockChainClient {
    pub fn new() -> Self {
        Self {
            claimed: Mutex::new(std::collections::HashMap::new()),
            balance: Mutex::new(std::collections::HashMap::new()),
            root: Mutex::new("0x0000000000000000000000000000000000000000000000000000000000000000".into()),
            events: Mutex::new(Vec::new()),
        }
    }

    pub async fn set_claimed(&self, address: &str, amount: u64) {
        self.claimed.lock().await.insert(address.to_string(), amount);
    }

    pub async fn set_balance(&self, address: &str, amount: u64) {
        self.balance.lock().await.insert(address.to_string(), amount);
    }

    pub async fn set_root(&self, root: &str) {
        *self.root.lock().await = root.to_string();
    }

    pub async fn add_event(&self, event: ClaimedEvent) {
        self.events.lock().await.push(event);
    }
}

#[async_trait::async_trait]
impl ChainClient for MockChainClient {
    async fn get_claimed(&self, address: &str) -> Result<u64, String> {
        Ok(self.claimed.lock().await.get(address).copied().unwrap_or(0))
    }

    async fn get_balance(&self, address: &str) -> Result<u64, String> {
        Ok(self.balance.lock().await.get(address).copied().unwrap_or(0))
    }

    async fn get_current_root(&self) -> Result<String, String> {
        Ok(self.root.lock().await.clone())
    }

    async fn get_claim_events(&self, from_block: u64) -> Result<Vec<ClaimedEvent>, String> {
        let events = self.events.lock().await.clone();
        Ok(events.into_iter().filter(|e| e.block_number >= from_block).collect())
    }
}
