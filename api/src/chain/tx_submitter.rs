use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::timeout;

#[derive(Debug, Clone)]
pub struct Transaction {
    pub to: String,
    pub data: Vec<u8>,
    pub value: u64,
}

#[derive(Debug, Clone)]
pub struct TxReceipt {
    pub tx_hash: String,
    pub success: bool,
    pub block_number: u64,
}

#[async_trait::async_trait]
pub trait TxProvider: Send + Sync {
    async fn get_transaction_count(&self, address: &str) -> Result<u64, String>;
    async fn send_raw_transaction(&self, tx: &Transaction) -> Result<String, String>;
    async fn get_transaction_receipt(&self, tx_hash: &str) -> Result<Option<TxReceipt>, String>;
}

pub struct TxSubmitter<P: TxProvider> {
    provider: Arc<P>,
    nonce: AtomicU64,
    submission_lock: Mutex<()>,
    confirmation_timeout: Duration,
    from_address: String,
}

impl<P: TxProvider> TxSubmitter<P> {
    pub async fn new(provider: Arc<P>, from_address: String) -> Result<Self, String> {
        let initial_nonce = provider.get_transaction_count(&from_address).await?;
        Ok(Self {
            provider,
            nonce: AtomicU64::new(initial_nonce),
            submission_lock: Mutex::new(()),
            confirmation_timeout: Duration::from_secs(120),
            from_address,
        })
    }

    pub async fn submit(&self, tx: Transaction) -> Result<TxReceipt, String> {
        let _guard = self.submission_lock.lock().await;
        let nonce = self.nonce.load(Ordering::SeqCst);

        let tx_hash = match self.provider.send_raw_transaction(&tx).await {
            Ok(hash) => {
                self.nonce.store(nonce + 1, Ordering::SeqCst);
                hash
            }
            Err(e) if e.contains("nonce") => {
                let fresh = self.provider.get_transaction_count(&self.from_address).await?;
                self.nonce.store(fresh, Ordering::SeqCst);
                let tx_hash = self.provider.send_raw_transaction(&tx).await?;
                self.nonce.store(fresh + 1, Ordering::SeqCst);
                tx_hash
            }
            Err(e) => return Err(e),
        };

        drop(_guard);

        let receipt = timeout(self.confirmation_timeout, async {
            loop {
                match self.provider.get_transaction_receipt(&tx_hash).await {
                    Ok(Some(receipt)) => return Ok(receipt),
                    Ok(None) => { tokio::time::sleep(Duration::from_secs(1)).await; continue }
                    Err(e) => return Err(e),
                }
            }
        }).await.map_err(|_| "tx confirmation timed out".to_string())??;

        Ok(receipt)
    }

    pub async fn recover_gap(&self) -> Result<(), String> {
        let chain_nonce = self.provider.get_transaction_count(&self.from_address).await?;
        let local_nonce = self.nonce.load(Ordering::SeqCst);
        if local_nonce > chain_nonce {
            self.nonce.store(chain_nonce, Ordering::SeqCst);
            tracing::warn!(local = local_nonce, chain = chain_nonce, "nonce gap reset");
        }
        Ok(())
    }

    pub fn current_nonce(&self) -> u64 { self.nonce.load(Ordering::SeqCst) }
    pub fn with_confirmation_timeout(mut self, d: Duration) -> Self { self.confirmation_timeout = d; self }
}

// ---- Mock (single Mutex, no deadlocks) ----

struct MockState {
    nonce: u64,
    submitted: Vec<(u64, Transaction)>,
    receipts: std::collections::HashMap<String, TxReceipt>,
    fail_next: Option<String>,
    receipt_delay: Option<Duration>,
}

pub struct MockTxProvider {
    state: Mutex<MockState>,
}

impl MockTxProvider {
    pub fn new(initial_nonce: u64) -> Self {
        Self { state: Mutex::new(MockState {
            nonce: initial_nonce,
            submitted: Vec::new(),
            receipts: std::collections::HashMap::new(),
            fail_next: None,
            receipt_delay: None,
        })}
    }

    pub async fn set_nonce(&self, n: u64) { self.state.lock().await.nonce = n; }
    pub async fn set_fail_next(&self, msg: &str) { self.state.lock().await.fail_next = Some(msg.into()); }
    pub async fn set_receipt_delay(&self, d: Duration) { self.state.lock().await.receipt_delay = Some(d); }
    pub async fn add_receipt(&self, key: &str, receipt: TxReceipt) {
        self.state.lock().await.receipts.insert(key.into(), receipt);
    }
    pub async fn submitted_count(&self) -> usize { self.state.lock().await.submitted.len() }
    pub async fn submitted_nonces(&self) -> Vec<u64> {
        self.state.lock().await.submitted.iter().map(|(n, _)| *n).collect()
    }
}

#[async_trait::async_trait]
impl TxProvider for MockTxProvider {
    async fn get_transaction_count(&self, _address: &str) -> Result<u64, String> {
        Ok(self.state.lock().await.nonce)
    }

    async fn send_raw_transaction(&self, tx: &Transaction) -> Result<String, String> {
        let mut s = self.state.lock().await;
        if let Some(err) = s.fail_next.take() {
            return Err(err);
        }
        let nonce = s.nonce;
        let hash = format!("0x{:064x}", nonce);
        s.submitted.push((nonce, tx.clone()));
        s.nonce = nonce + 1;
        Ok(hash)
    }

    async fn get_transaction_receipt(&self, tx_hash: &str) -> Result<Option<TxReceipt>, String> {
        let delay: Option<Duration>;
        {
            let s = self.state.lock().await;
            delay = s.receipt_delay;
        }
        if let Some(d) = delay {
            tokio::time::sleep(d).await;
        }
        let s = self.state.lock().await;
        Ok(s.receipts.get(tx_hash).cloned())
    }
}
