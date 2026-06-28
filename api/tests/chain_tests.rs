use mathcoin_api::chain::client::{ChainClient, ClaimedEvent, MockChainClient};
use mathcoin_api::chain::tx_submitter::{
    MockTxProvider, Transaction, TxSubmitter, TxReceipt,
};
use std::sync::Arc;
use std::time::Duration;

// ---- ChainClient reads ----

#[tokio::test]
async fn mock_chain_client_returns_configured_claimed() {
    let client = MockChainClient::new();
    client.set_claimed("0xAbc", 42).await;
    assert_eq!(client.get_claimed("0xAbc").await.unwrap(), 42);
    assert_eq!(client.get_claimed("0xUnknown").await.unwrap(), 0);
}

#[tokio::test]
async fn mock_chain_client_returns_configured_balance() {
    let client = MockChainClient::new();
    client.set_balance("0xAbc", 1000).await;
    assert_eq!(client.get_balance("0xAbc").await.unwrap(), 1000);
}

#[tokio::test]
async fn mock_chain_client_returns_root() {
    let client = MockChainClient::new();
    client.set_root("0xdeadbeef").await;
    assert_eq!(client.get_current_root().await.unwrap(), "0xdeadbeef");
}

#[tokio::test]
async fn mock_chain_client_filters_events_by_block() {
    let client = MockChainClient::new();
    client.add_event(ClaimedEvent { account: "0xA".into(), cumulative_amount: 10, block_number: 100, tx_hash: "0x1".into() }).await;
    client.add_event(ClaimedEvent { account: "0xB".into(), cumulative_amount: 20, block_number: 200, tx_hash: "0x2".into() }).await;
    let events = client.get_claim_events(150).await.unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].account, "0xB");
}

// ---- TxSubmitter ----

#[tokio::test]
async fn tx_submitter_initializes_nonce_from_provider() {
    let provider = Arc::new(MockTxProvider::new(5));
    let submitter = TxSubmitter::new(provider, "0xSender".into()).await.unwrap();
    assert_eq!(submitter.current_nonce(), 5);
}

#[tokio::test]
async fn concurrent_submissions_get_sequential_nonces() {
    let provider = Arc::new(MockTxProvider::new(0));
    for i in 0..10 {
        provider.add_receipt(&format!("0x{:064x}", i), TxReceipt {
            tx_hash: format!("0x{:064x}", i), success: true, block_number: i + 100,
        }).await;
    }

    let submitter = Arc::new(TxSubmitter::new(provider.clone(), "0xSender".into()).await.unwrap());
    let mut handles = Vec::new();
    for _ in 0..10 {
        let s = submitter.clone();
        handles.push(tokio::spawn(async move {
            s.submit(Transaction { to: "0xDest".into(), data: vec![], value: 0, gas_limit: None, max_fee_per_gas: None, max_priority_fee_per_gas: None }).await.unwrap()
        }));
    }
    for h in handles {
        let r = h.await.unwrap();
        assert!(r.success);
    }
    let nonces = provider.submitted_nonces().await;
    assert_eq!(nonces.len(), 10);
    let mut sorted = nonces.clone(); sorted.sort();
    for (i, n) in sorted.iter().enumerate() {
        assert_eq!(*n, i as u64, "nonce mismatch at position {i}");
    }
}

#[tokio::test]
async fn nonce_too_low_triggers_resync_and_retry() {
    let provider = Arc::new(MockTxProvider::new(0));
    provider.set_nonce(3).await;
    provider.set_fail_next("nonce too low").await;
    provider.add_receipt(&format!("0x{:064x}", 3u64), TxReceipt {
        tx_hash: format!("0x{:064x}", 3u64), success: true, block_number: 100,
    }).await;

    let submitter = TxSubmitter::new(provider.clone(), "0xSender".into()).await.unwrap()
        .with_confirmation_timeout(Duration::from_secs(2));

    let result = submitter.submit(Transaction { to: "0xDest".into(), data: vec![], value: 0, gas_limit: None, max_fee_per_gas: None, max_priority_fee_per_gas: None }).await;
    assert!(result.is_ok(), "should succeed after nonce resync: {result:?}");
}

#[tokio::test]
async fn receipt_timeout_triggers_gap_detection() {
    let provider = Arc::new(MockTxProvider::new(0));
    provider.set_receipt_delay(Duration::from_millis(500)).await;

    let submitter = TxSubmitter::new(provider.clone(), "0xSender".into()).await.unwrap()
        .with_confirmation_timeout(Duration::from_millis(100));

    let result = submitter.submit(Transaction { to: "0xDest".into(), data: vec![], value: 0, gas_limit: None, max_fee_per_gas: None, max_priority_fee_per_gas: None }).await;
    assert!(result.is_err());

    // Nonce was consumed locally but chain didn't confirm
    assert_eq!(submitter.current_nonce(), 1);
    // Reset chain nonce to simulate gap
    provider.set_nonce(0).await;
    submitter.recover_gap().await.unwrap();
    assert_eq!(submitter.current_nonce(), 0, "nonce should reset after gap recovery");
}
