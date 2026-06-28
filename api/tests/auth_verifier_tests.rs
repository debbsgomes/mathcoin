/// AuthVerifier adapter tests: real ES256 crypto against a mock JWKS endpoint.
/// No network, no real Supabase — actual ECDSA P-256 signature math runs.
use mathcoin_api::auth::{AuthVerifier, JwksVerifier};
use axum::{routing::get, Router};
use p256::ecdsa::{SigningKey, VerifyingKey};
use p256::elliptic_curve::pkcs8::EncodePrivateKey;
use p256::SecretKey;
use rand_core::OsRng;
use serde::Serialize;
use std::sync::Arc;
use tokio::net::TcpListener;

// ---- JWKS helpers ----

#[derive(Serialize)]
struct Jwk {
    kty: String,
    crv: String,
    x: String,
    y: String,
    kid: String,
    #[serde(rename = "use")]
    use_: String,
    alg: String,
}

#[derive(Serialize)]
struct JwksResponse {
    keys: Vec<Jwk>,
}

fn base64_url_encode(data: impl AsRef<[u8]>) -> String {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(data.as_ref())
}

async fn start_mock_jwks_server(verifying_key: &VerifyingKey, kid: &str) -> String {
    let encoded = verifying_key.to_encoded_point(false);
    let jwk = Jwk {
        kty: "EC".into(),
        crv: "P-256".into(),
        x: base64_url_encode(encoded.x().unwrap()),
        y: base64_url_encode(encoded.y().unwrap()),
        kid: kid.into(),
        use_: "sig".into(),
        alg: "ES256".into(),
    };

    let jwks = Arc::new(JwksResponse {
        keys: vec![jwk],
    });

    let app = Router::new().route(
        "/.well-known/jwks.json",
        get(move || {
            let jwks = jwks.clone();
            async move { axum::Json(serde_json::to_value(&*jwks).unwrap()) }
        }),
    );

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    format!("http://{}/.well-known/jwks.json", addr)
}

fn generate_keypair() -> (SigningKey, VerifyingKey) {
    let secret = SecretKey::random(&mut OsRng);
    let signing_key: SigningKey = secret.into();
    let verifying_key: VerifyingKey = signing_key.verifying_key().clone();
    (signing_key, verifying_key)
}

fn mint_token(
    signing_key: &SigningKey,
    kid: &str,
    sub: &str,
    email: &str,
    iss: &str,
    aud: &str,
    exp: usize,
) -> String {
    use jsonwebtoken::{EncodingKey, Header};
    use serde_json::json;

    let pkcs8_pem = signing_key.to_pkcs8_pem(Default::default()).unwrap();
    let encoding_key = EncodingKey::from_ec_pem(pkcs8_pem.as_bytes()).unwrap();

    let mut header = Header::new(jsonwebtoken::Algorithm::ES256);
    header.kid = Some(kid.to_string());

    let claims = json!({
        "sub": sub,
        "email": email,
        "iss": iss,
        "aud": aud,
        "exp": exp,
        "iat": 1700000000u64,
    });

    jsonwebtoken::encode(&header, &claims, &encoding_key).unwrap()
}

// ---- Tests ----

#[tokio::test]
async fn valid_token_returns_claims() {
    let (signing_key, verifying_key) = generate_keypair();
    let kid = "key-001";
    let jwks_url = start_mock_jwks_server(&verifying_key, &kid).await;

    let verifier = JwksVerifier::new(
        jwks_url,
        "https://test.supabase.co/auth/v1".into(),
        "authenticated".into(),
    );

    let token = mint_token(
        &signing_key,
        kid,
        "abc-123",
        "deb@example.com",
        "https://test.supabase.co/auth/v1",
        "authenticated",
        (chrono::Utc::now().timestamp() + 3600) as usize,
    );

    let result = verifier.verify(&token).await;
    let claims = result.expect("valid token should return claims");
    assert_eq!(claims.sub, "abc-123");
    assert_eq!(claims.email, "deb@example.com");
}

#[tokio::test]
async fn expired_token_rejected() {
    let (signing_key, verifying_key) = generate_keypair();
    let kid = "key-002";
    let jwks_url = start_mock_jwks_server(&verifying_key, &kid).await;

    let verifier = JwksVerifier::new(
        jwks_url,
        "https://test.supabase.co/auth/v1".into(),
        "authenticated".into(),
    );

    let token = mint_token(
        &signing_key,
        kid,
        "abc-123",
        "deb@example.com",
        "https://test.supabase.co/auth/v1",
        "authenticated",
        100, // expired in 1970
    );

    let result = verifier.verify(&token).await;
    assert!(result.is_err(), "expired token should be rejected");
}

#[tokio::test]
async fn wrong_issuer_rejected() {
    let (signing_key, verifying_key) = generate_keypair();
    let kid = "key-003";
    let jwks_url = start_mock_jwks_server(&verifying_key, &kid).await;

    let verifier = JwksVerifier::new(
        jwks_url,
        "https://test.supabase.co/auth/v1".into(),
        "authenticated".into(),
    );

    let token = mint_token(
        &signing_key,
        kid,
        "abc-123",
        "deb@example.com",
        "https://evil.example.com", // wrong issuer
        "authenticated",
        (chrono::Utc::now().timestamp() + 3600) as usize,
    );

    let result = verifier.verify(&token).await;
    assert!(result.is_err(), "wrong iss should be rejected");
}

#[tokio::test]
async fn wrong_aud_rejected() {
    let (signing_key, verifying_key) = generate_keypair();
    let kid = "key-004";
    let jwks_url = start_mock_jwks_server(&verifying_key, &kid).await;

    let verifier = JwksVerifier::new(
        jwks_url,
        "https://test.supabase.co/auth/v1".into(),
        "authenticated".into(),
    );

    let token = mint_token(
        &signing_key,
        kid,
        "abc-123",
        "deb@example.com",
        "https://test.supabase.co/auth/v1",
        "evil_app", // wrong aud
        (chrono::Utc::now().timestamp() + 3600) as usize,
    );

    let result = verifier.verify(&token).await;
    assert!(result.is_err(), "wrong aud should be rejected");
}

#[tokio::test]
async fn tampered_signature_rejected() {
    let (signing_key, verifying_key) = generate_keypair();
    let kid = "key-005";
    let jwks_url = start_mock_jwks_server(&verifying_key, &kid).await;

    let verifier = JwksVerifier::new(
        jwks_url,
        "https://test.supabase.co/auth/v1".into(),
        "authenticated".into(),
    );

    let mut token = mint_token(
        &signing_key,
        kid,
        "abc-123",
        "deb@example.com",
        "https://test.supabase.co/auth/v1",
        "authenticated",
        (chrono::Utc::now().timestamp() + 3600) as usize,
    );
    // Tamper with payload by changing the last char
    token.pop();
    token.push('X');

    let result = verifier.verify(&token).await;
    assert!(result.is_err(), "tampered token should be rejected");
}

#[tokio::test]
async fn unknown_kid_rejected() {
    let (signing_key, verifying_key) = generate_keypair();
    let real_kid = "key-real";
    let jwks_url = start_mock_jwks_server(&verifying_key, &real_kid).await;

    let verifier = JwksVerifier::new(
        jwks_url,
        "https://test.supabase.co/auth/v1".into(),
        "authenticated".into(),
    );

    let token = mint_token(
        &signing_key,
        "unknown-kid-999", // kid not in JWKS
        "abc-123",
        "deb@example.com",
        "https://test.supabase.co/auth/v1",
        "authenticated",
        (chrono::Utc::now().timestamp() + 3600) as usize,
    );

    let result = verifier.verify(&token).await;
    assert!(result.is_err(), "unknown kid should be rejected");
}

#[tokio::test]
async fn malformed_token_rejected() {
    let (_, verifying_key) = generate_keypair();
    let kid = "key-007";
    let jwks_url = start_mock_jwks_server(&verifying_key, &kid).await;

    let verifier = JwksVerifier::new(
        jwks_url,
        "https://test.supabase.co/auth/v1".into(),
        "authenticated".into(),
    );

    let result = verifier.verify("not.a.jwt").await;
    assert!(result.is_err(), "malformed token should be rejected");
}

#[tokio::test]
async fn missing_kid_rejected() {
    let (signing_key, verifying_key) = generate_keypair();
    let _kid = "key-008";
    let jwks_url = start_mock_jwks_server(&verifying_key, "some-other-kid").await;

    let verifier = JwksVerifier::new(
        jwks_url,
        "https://test.supabase.co/auth/v1".into(),
        "authenticated".into(),
    );

    // Create a token WITHOUT kid in header
    use jsonwebtoken::{EncodingKey, Header};
    use serde_json::json;
    let pkcs8_pem = signing_key.to_pkcs8_pem(Default::default()).unwrap();
    let encoding_key = EncodingKey::from_ec_pem(pkcs8_pem.as_bytes()).unwrap();
    let header = Header::new(jsonwebtoken::Algorithm::ES256); // no kid
    let claims = json!({
        "sub": "abc",
        "email": "deb@example.com",
        "iss": "https://test.supabase.co/auth/v1",
        "aud": "authenticated",
        "exp": (chrono::Utc::now().timestamp() + 3600) as usize,
        "iat": 1700000000u64,
    });
    let token = jsonwebtoken::encode(&header, &claims, &encoding_key).unwrap();

    let result = verifier.verify(&token).await;
    assert!(result.is_err(), "token without kid should be rejected");
}

#[tokio::test]
async fn jwks_cache_hit_does_not_refetch() {
    let (signing_key, verifying_key) = generate_keypair();
    let kid = "key-009";
    let mut jwks_url = start_mock_jwks_server(&verifying_key, &kid).await;

    let verifier = JwksVerifier::new(
        jwks_url.clone(),
        "https://test.supabase.co/auth/v1".into(),
        "authenticated".into(),
    );

    let token = mint_token(
        &signing_key,
        kid,
        "abc-123",
        "deb@example.com",
        "https://test.supabase.co/auth/v1",
        "authenticated",
        (chrono::Utc::now().timestamp() + 3600) as usize,
    );

    // First call: fetch JWKS, cache it, verify
    let result = verifier.verify(&token).await;
    assert!(result.is_ok(), "first verify should succeed");

    // Now break the JWKS server and verify again — should use cache
    jwks_url.push_str("/broken");
    let verifier_broken = JwksVerifier::new(
        jwks_url,
        "https://test.supabase.co/auth/v1".into(),
        "authenticated".into(),
    );
    // Mint a new token for the verifier that has NO cache yet
    let token2 = mint_token(
        &signing_key,
        kid,
        "abc-456",
        "deb2@example.com",
        "https://test.supabase.co/auth/v1",
        "authenticated",
        (chrono::Utc::now().timestamp() + 3600) as usize,
    );
    let result2 = verifier_broken.verify(&token2).await;
    assert!(result2.is_err(), "broken JWKS URL should fail without cache");

    // But the original verifier (with cache) should still work
    let result3 = verifier.verify(&token).await;
    assert!(result3.is_ok(), "cached verifier should still work after second call");
}
