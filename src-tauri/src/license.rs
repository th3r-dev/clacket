//! Offline license check. A key is "CLACKET-" + base32(payload_json ‖ ed25519_signature).
//! The Worker signs with the private key; the app only holds the public key. No network.
use data_encoding::BASE32_NOPAD;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};

/// Production public key. Replace with the output of `node server/scripts/keygen.mjs`.
pub const PUBLIC_KEY: [u8; 32] = *include_bytes!("../license-public.key");
pub const PREFIX: &str = "CLACKET-";

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct License {
    pub v: u8,
    pub lid: String,
    pub name: String,
    pub plan: String,
    pub iat: u64,
}

#[derive(Debug, PartialEq)]
pub enum LicenseError {
    Invalid,
    Revoked,
}

/// Accepts the key with any whitespace/line breaks pasted around or inside it, any case.
pub fn verify_with(key: &str, public: &[u8; 32], revoked: &[&str]) -> Result<License, LicenseError> {
    let clean: String = key.chars().filter(|c| !c.is_whitespace()).collect::<String>().to_ascii_uppercase();
    let body = clean.strip_prefix(PREFIX).ok_or(LicenseError::Invalid)?;
    let raw = BASE32_NOPAD.decode(body.as_bytes()).map_err(|_| LicenseError::Invalid)?;
    if raw.len() <= 64 {
        return Err(LicenseError::Invalid);
    }
    let (payload, sig) = raw.split_at(raw.len() - 64);
    let vk = VerifyingKey::from_bytes(public).map_err(|_| LicenseError::Invalid)?;
    let sig = Signature::from_slice(sig).map_err(|_| LicenseError::Invalid)?;
    vk.verify(payload, &sig).map_err(|_| LicenseError::Invalid)?;
    let lic: License = serde_json::from_slice(payload).map_err(|_| LicenseError::Invalid)?;
    if lic.plan != "pro" {
        return Err(LicenseError::Invalid);
    }
    if revoked.contains(&lic.lid.as_str()) {
        return Err(LicenseError::Revoked);
    }
    Ok(lic)
}

pub fn verify(key: &str) -> Result<License, LicenseError> {
    verify_with(key, &PUBLIC_KEY, crate::pack::REVOKED)
}

/// Pulls a key out of a dropped/opened clacket.license file or a clacket://activate?key= link.
pub fn extract(text: &str) -> Option<String> {
    let t = text.trim();
    if let Some(q) = t.strip_prefix("clacket://activate?key=") {
        return Some(q.split('&').next()?.to_string());
    }
    let start = t.to_ascii_uppercase().find(PREFIX)?;
    Some(t[start..].split(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c.is_whitespace())).next()?.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn sign(sk: &SigningKey, lic: &License) -> String {
        let payload = serde_json::to_vec(lic).unwrap();
        let sig = sk.sign(&payload);
        let mut raw = payload;
        raw.extend_from_slice(&sig.to_bytes());
        format!("{PREFIX}{}", BASE32_NOPAD.encode(&raw))
    }

    fn lic() -> License {
        License { v: 1, lid: "lic_1".into(), name: "Jane D.".into(), plan: "pro".into(), iat: 1_800_000_000 }
    }

    #[test]
    fn valid_tampered_wrong_key_revoked() {
        let sk = SigningKey::from_bytes(&[7; 32]);
        let pk = sk.verifying_key().to_bytes();
        let key = sign(&sk, &lic());
        assert_eq!(verify_with(&key, &pk, &[]).unwrap(), lic());
        // pasted with line breaks and lowercase still works
        let messy = format!("  {}\n{}  ", &key[..40].to_lowercase(), &key[40..]);
        assert!(verify_with(&messy, &pk, &[]).is_ok());

        let mut bad = key.clone().into_bytes();
        let i = PREFIX.len() + 10;
        bad[i] = if bad[i] == b'A' { b'B' } else { b'A' };
        assert_eq!(verify_with(std::str::from_utf8(&bad).unwrap(), &pk, &[]), Err(LicenseError::Invalid));
        assert_eq!(verify_with(&key[..key.len() - 5], &pk, &[]), Err(LicenseError::Invalid));
        assert_eq!(verify_with("CLACKET-", &pk, &[]), Err(LicenseError::Invalid));

        let other = sign(&SigningKey::from_bytes(&[9; 32]), &lic());
        assert_eq!(verify_with(&other, &pk, &[]), Err(LicenseError::Invalid));
        assert_eq!(verify_with(&key, &pk, &["lic_1"]), Err(LicenseError::Revoked));

        let free = License { plan: "free".into(), ..lic() };
        assert_eq!(verify_with(&sign(&sk, &free), &pk, &[]), Err(LicenseError::Invalid));
    }

    #[test]
    fn extract_from_link_and_file() {
        assert_eq!(extract("clacket://activate?key=CLACKET-ABC&x=1").as_deref(), Some("CLACKET-ABC"));
        assert_eq!(extract("Your key:\nCLACKET-ABCD\n").as_deref(), Some("CLACKET-ABCD"));
        assert_eq!(extract("nothing here"), None);
    }

    /// A key signed by the Worker's own code (server/test/fixture.json) verifies here.
    #[test]
    fn worker_fixture_verifies() {
        let f: serde_json::Value = serde_json::from_str(include_str!("../../server/test/fixture.json")).unwrap();
        let pk: [u8; 32] = data_encoding::HEXLOWER.decode(f["public_hex"].as_str().unwrap().as_bytes()).unwrap().try_into().unwrap();
        let l = verify_with(f["key"].as_str().unwrap(), &pk, &[]).unwrap();
        assert_eq!(l.plan, "pro");
    }
}
