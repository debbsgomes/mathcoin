use base64::Engine;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwtClaims {
    pub sub: String,
    pub email: String,
}

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("unauthenticated: {0}")]
    Unauthenticated(String),
    #[error("internal error")]
    Internal,
}

#[async_trait::async_trait]
pub trait AuthVerifier: Send + Sync {
    async fn verify(&self, token: &str) -> Result<JwtClaims, AuthError>;
}

// ---- Mock verifier (for handler tests) ----

pub struct MockVerifier {
    sub: String,
    email: String,
    reject: bool,
}

impl MockVerifier {
    pub fn accepting(sub: String, email: String) -> Self {
        Self {
            sub,
            email,
            reject: false,
        }
    }

    pub fn rejecting() -> Self {
        Self {
            sub: String::new(),
            email: String::new(),
            reject: true,
        }
    }
}

#[async_trait::async_trait]
impl AuthVerifier for MockVerifier {
    async fn verify(&self, _token: &str) -> Result<JwtClaims, AuthError> {
        if self.reject {
            Err(AuthError::Unauthenticated("mock rejection".into()))
        } else {
            Ok(JwtClaims {
                sub: self.sub.clone(),
                email: self.email.clone(),
            })
        }
    }
}

// ---- JWKS types ----

#[derive(Debug, Deserialize, Clone)]
struct JwksResponse {
    keys: Vec<JwkKey>,
}

#[derive(Debug, Deserialize, Clone)]
struct JwkKey {
    kid: String,
    kty: String,
    crv: String,
    x: String,
    y: String,
}

// ---- Production adapter: ES256 / JWKS ----

struct JwksCache {
    data: JwksResponse,
    fetched_at: Instant,
}

pub struct JwksVerifier {
    jwks_url: String,
    issuer: String,
    audience: String,
    client: reqwest::Client,
    cache: RwLock<Option<JwksCache>>,
    cache_ttl: Duration,
}

impl JwksVerifier {
    pub fn new(jwks_url: String, issuer: String, audience: String) -> Self {
        // In production, JWKS and issuer must use HTTPS.
        // Localhost is exempt for testing/development.
        if !jwks_url.contains("localhost") && !jwks_url.contains("127.0.0.1") {
            assert!(
                jwks_url.starts_with("https://"),
                "JWKS_URL must use HTTPS in production"
            );
        }
        if !issuer.contains("localhost") && !issuer.contains("127.0.0.1") {
            assert!(
                issuer.starts_with("https://"),
                "JWT_ISS must use HTTPS in production"
            );
        }
        Self {
            jwks_url,
            issuer,
            audience,
            client: reqwest::Client::new(),
            cache: RwLock::new(None),
            cache_ttl: Duration::from_secs(3600),
        }
    }

    async fn fetch_jwks(&self) -> Result<JwksResponse, AuthError> {
        {
            let cache = self.cache.read().await;
            if let Some(ref entry) = *cache {
                if entry.fetched_at.elapsed() < self.cache_ttl {
                    return Ok(entry.data.clone());
                }
            }
        }

        let resp = self
            .client
            .get(&self.jwks_url)
            .send()
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "JWKS fetch failed");
                AuthError::Unauthenticated("authentication service unavailable".into())
            })?;

        let jwks: JwksResponse = resp
            .json()
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "JWKS parse failed");
                AuthError::Unauthenticated("authentication service unavailable".into())
            })?;

        {
            let mut cache = self.cache.write().await;
            *cache = Some(JwksCache {
                data: jwks.clone(),
                fetched_at: Instant::now(),
            });
        }

        Ok(jwks)
    }

    fn verify_token(&self, token: &str, jwks: &JwksResponse) -> Result<JwtClaims, AuthError> {
        use jsonwebtoken::{decode, DecodingKey, Validation};
        use serde_json::Value;

        let header = jsonwebtoken::decode_header(token)
            .map_err(|_| AuthError::Unauthenticated("malformed token".into()))?;

        let kid = header
            .kid
            .as_ref()
            .ok_or_else(|| AuthError::Unauthenticated("token missing kid".into()))?;

        let jwk = jwks
            .keys
            .iter()
            .find(|k| &k.kid == kid)
            .ok_or_else(|| {
                tracing::warn!(kid = %kid, "unknown kid in token");
                AuthError::Unauthenticated("unknown signing key".into())
            })?;

        let der_key = build_p256_spki_der(&jwk.x, &jwk.y)?;
        let pem_key = der_to_pem(&der_key, "PUBLIC KEY");

        let decoding_key = DecodingKey::from_ec_pem(pem_key.as_bytes())
            .map_err(|e| {
                tracing::error!(error = %e, "invalid public key from JWKS");
                AuthError::Unauthenticated("authentication service unavailable".into())
            })?;

        let mut validation = Validation::new(jsonwebtoken::Algorithm::ES256);
        validation.set_issuer(&[&self.issuer]);
        validation.set_audience(&[&self.audience]);

        let token_data = decode::<Value>(token, &decoding_key, &validation)
            .map_err(|e| {
                tracing::warn!(error = %e, "token verification failed");
                AuthError::Unauthenticated("token verification failed".into())
            })?;

        let claims = token_data.claims;
        let sub = claims["sub"]
            .as_str()
            .ok_or_else(|| AuthError::Unauthenticated("missing sub claim".into()))?
            .to_string();
        let email = claims["email"]
            .as_str()
            .unwrap_or("")
            .to_string();

        Ok(JwtClaims { sub, email })
    }
}

#[async_trait::async_trait]
impl AuthVerifier for JwksVerifier {
    async fn verify(&self, token: &str) -> Result<JwtClaims, AuthError> {
        let jwks = self.fetch_jwks().await?;
        self.verify_token(token, &jwks)
    }
}

/// Build DER-encoded SubjectPublicKeyInfo for a P-256 public key from raw x, y coordinates.
/// Fixed format for P-256 (91 bytes total):
///   30 59  SEQUENCE
///     30 13  AlgorithmIdentifier SEQUENCE
///       06 07 2A86 48CE 3D02 01  OID: EC (1.2.840.10045.2.1)
///       06 08 2A86 48CE 3D03 0107 OID: P-256 (1.2.840.10045.3.1.7)
///     03 42  BIT STRING
///       00    0 unused bits
///       04 || x || y  uncompressed point (65 bytes)
fn build_p256_spki_der(x_b64: &str, y_b64: &str) -> Result<Vec<u8>, AuthError> {
    let x = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(x_b64)
        .map_err(|_| AuthError::Unauthenticated("invalid x coordinate encoding".into()))?;
    let y = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(y_b64)
        .map_err(|_| AuthError::Unauthenticated("invalid y coordinate encoding".into()))?;

    if x.len() != 32 || y.len() != 32 {
        return Err(AuthError::Unauthenticated(
            "invalid P-256 key coordinates".into(),
        ));
    }

    let mut der = Vec::with_capacity(91);
    der.push(0x30);
    der.push(0x59);
    der.push(0x30);
    der.push(0x13);
    der.extend_from_slice(&[0x06, 0x07, 0x2A, 0x86, 0x48, 0xCE, 0x3D, 0x02, 0x01]);
    der.extend_from_slice(&[0x06, 0x08, 0x2A, 0x86, 0x48, 0xCE, 0x3D, 0x03, 0x01, 0x07]);
    der.push(0x03);
    der.push(0x42);
    der.push(0x00);
    der.push(0x04);
    der.extend_from_slice(&x);
    der.extend_from_slice(&y);

    Ok(der)
}

/// Wrap DER bytes in PEM armor.
fn der_to_pem(der: &[u8], label: &str) -> String {
    let b64 = base64::engine::general_purpose::STANDARD.encode(der);
    let mut pem = String::new();
    pem.push_str(&format!("-----BEGIN {}-----\n", label));
    for chunk in b64.as_bytes().chunks(64) {
        pem.push_str(std::str::from_utf8(chunk).expect("base64 output is always valid UTF-8"));
        pem.push('\n');
    }
    pem.push_str(&format!("-----END {}-----\n", label));
    pem
}

#[cfg(test)]
mod tests {
    use super::*;
    use p256::ecdsa::SigningKey;
    use p256::elliptic_curve::pkcs8::{EncodePrivateKey, EncodePublicKey};
    use p256::SecretKey;
    use rand_core::OsRng;

    #[tokio::test]
    async fn roundtrip_p256_key_pem() {
        let secret = SecretKey::random(&mut OsRng);
        let signing_key: SigningKey = secret.into();
        let verifying_key = signing_key.verifying_key();

        let pkcs8_pem = signing_key.to_pkcs8_pem(Default::default()).unwrap();
        let enc_key = jsonwebtoken::EncodingKey::from_ec_pem(pkcs8_pem.as_bytes()).unwrap();

        let spki_pem = verifying_key.to_public_key_pem(Default::default()).unwrap();
        let dec_key = jsonwebtoken::DecodingKey::from_ec_pem(spki_pem.as_bytes()).unwrap();

        let mut header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::ES256);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as usize;
        let claims = serde_json::json!({
            "sub": "test",
            "email": "test@test.com",
            "exp": now + 3600,
            "iat": now,
        });

        let token = jsonwebtoken::encode(&header, &claims, &enc_key).unwrap();
        let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::ES256);
        validation.required_spec_claims.remove("exp");
        let decoded =
            jsonwebtoken::decode::<serde_json::Value>(&token, &dec_key, &validation).unwrap();

        assert_eq!(decoded.claims["sub"], "test");
    }
}
