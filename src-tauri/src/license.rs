//! License and activation.
//!
//! A **key** ("CLACKET-" + base32(payload ‖ ed25519_sig)) proves purchase and is checked offline.
//! To USE Pro the app POSTs the key to the Worker's /activate, which binds it to ONE device and
//! returns an **activation token** ("CLACKTOK-" + base32(payload ‖ ed25519_sig)) with
//! `{v,typ:"act",lid,device,plan,iat}`. The token is verified offline for THIS machine, so after
//! activation the app runs without the network. Both are signed by the Worker's private key; the
//! app only holds the matching public key. Moving to another device needs an explicit /deactivate.
use data_encoding::BASE32_NOPAD;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::Duration;

/// Production public key. Replace with the output of `node server/scripts/keygen.mjs`.
pub const PUBLIC_KEY: [u8; 32] = *include_bytes!("../license-public.key");
pub const PREFIX: &str = "CLACKET-";
pub const TOKEN_PREFIX: &str = "CLACKTOK-";

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct License {
    pub v: u8,
    pub lid: String,
    pub name: String,
    pub plan: String,
    pub iat: u64,
}

/// A device-bound activation token's payload (distinct from a key's).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Activation {
    pub v: u8,
    pub typ: String,
    pub lid: String,
    pub device: String,
    pub plan: String,
    pub iat: u64,
}

#[derive(Debug, PartialEq)]
pub enum LicenseError {
    Invalid,
    Revoked,
}

/// What activation can go wrong with. The UI shows a distinct message for each.
#[derive(Debug, PartialEq)]
pub enum ActivateError {
    /// The key is active on another device (server said 409).
    InUse,
    /// Couldn't reach the license server (network/timeout). Nothing was changed.
    Offline,
    /// The key isn't valid, or the returned token didn't verify for this device.
    Invalid,
    /// The key was refunded.
    Revoked,
}

impl ActivateError {
    /// A short code the UI maps to a message.
    pub fn code(&self) -> &'static str {
        match self {
            ActivateError::InUse => "in_use",
            ActivateError::Offline => "offline",
            ActivateError::Invalid => "invalid",
            ActivateError::Revoked => "revoked",
        }
    }
}

fn to_activate_err(e: LicenseError) -> ActivateError {
    match e {
        LicenseError::Revoked => ActivateError::Revoked,
        LicenseError::Invalid => ActivateError::Invalid,
    }
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

/// Verifies an activation token offline: signature, `typ=="act"`, `plan=="pro"`, the token's
/// `device` equals `device`, and the lid isn't revoked. This is the proof Pro is on for a device.
pub fn verify_token_with(token: &str, public: &[u8; 32], device: &str, revoked: &[&str]) -> Result<Activation, LicenseError> {
    let clean: String = token.chars().filter(|c| !c.is_whitespace()).collect::<String>().to_ascii_uppercase();
    let body = clean.strip_prefix(TOKEN_PREFIX).ok_or(LicenseError::Invalid)?;
    let raw = BASE32_NOPAD.decode(body.as_bytes()).map_err(|_| LicenseError::Invalid)?;
    if raw.len() <= 64 {
        return Err(LicenseError::Invalid);
    }
    let (payload, sig) = raw.split_at(raw.len() - 64);
    let vk = VerifyingKey::from_bytes(public).map_err(|_| LicenseError::Invalid)?;
    let sig = Signature::from_slice(sig).map_err(|_| LicenseError::Invalid)?;
    vk.verify(payload, &sig).map_err(|_| LicenseError::Invalid)?;
    let act: Activation = serde_json::from_slice(payload).map_err(|_| LicenseError::Invalid)?;
    if act.typ != "act" || act.plan != "pro" || act.device != device {
        return Err(LicenseError::Invalid);
    }
    if revoked.contains(&act.lid.as_str()) {
        return Err(LicenseError::Revoked);
    }
    Ok(act)
}

/// Verifies a stored token for THIS machine. `is_pro` is true iff this returns Ok.
pub fn verify_token(token: &str) -> Result<Activation, LicenseError> {
    verify_token_with(token, &PUBLIC_KEY, &device_hash(), crate::pack::REVOKED)
}

/// A stable, non-reversible id for this machine: `hex(sha256("clacket-device-v1:" ‖ machine_uid))`.
/// The raw machine id never leaves the device; only this salted hash is sent to the server.
pub fn device_hash() -> String {
    let uid = machine_uid::get().unwrap_or_else(|_| "clacket-unknown-machine".to_string());
    let mut h = Sha256::new();
    h.update(b"clacket-device-v1:");
    h.update(uid.as_bytes());
    data_encoding::HEXLOWER.encode(&h.finalize())
}

/// The host name, for a human-readable device label. Display only; falls back to "This device".
pub fn device_name() -> String {
    hostname::get().ok().and_then(|h| h.into_string().ok()).filter(|s| !s.is_empty()).unwrap_or_else(|| "This device".to_string())
}

/// POSTs JSON to `url` with a short timeout, returning (status, body). A transport failure
/// (DNS, refused, timeout) maps to Offline; any HTTP response (incl. 4xx) comes back as Ok.
fn post_json(url: &str, body: serde_json::Value) -> Result<(u16, serde_json::Value), ActivateError> {
    let agent = ureq::AgentBuilder::new().timeout(Duration::from_secs(12)).build();
    match agent.post(url).send_json(body) {
        Ok(r) => Ok((r.status(), r.into_json().unwrap_or(serde_json::Value::Null))),
        Err(ureq::Error::Status(code, r)) => Ok((code, r.into_json().unwrap_or(serde_json::Value::Null))),
        Err(ureq::Error::Transport(_)) => Err(ActivateError::Offline),
    }
}

/// Activates a key against the server, binding it to this device. On success returns the license
/// (for its name) and the device-bound token to persist. The token is verified offline before it's
/// trusted, so a server that returned a token for the wrong device or a bad signature is rejected.
pub fn activate(url: &str, key: &str) -> Result<(License, String), ActivateError> {
    let (status, json) = post_json(url, serde_json::json!({ "key": key, "device_hash": device_hash(), "device_name": device_name() }))?;
    if status == 200 {
        let token = json.get("token").and_then(|t| t.as_str()).ok_or(ActivateError::Invalid)?;
        verify_token(token).map_err(to_activate_err)?; // the token must verify for THIS device
        let lic = verify(key).map_err(to_activate_err)?; // and the key must be a real pro key (for the name)
        return Ok((lic, token.to_string()));
    }
    let err = json.get("error").and_then(|e| e.as_str()).unwrap_or("");
    Err(match (status, err) {
        (409, _) => ActivateError::InUse,
        (_, "revoked") => ActivateError::Revoked,
        _ => ActivateError::Invalid,
    })
}

/// Frees this device's seat on the server so the key can activate elsewhere. Offline means the
/// seat wasn't freed, so the caller must NOT clear the local token (the user couldn't transfer).
pub fn deactivate(url: &str, key: &str) -> Result<(), ActivateError> {
    // Any HTTP response means the server was reached and the seat is released (or was never ours);
    // only a transport failure must stop us from clearing Pro locally.
    post_json(url, serde_json::json!({ "key": key, "device_hash": device_hash() })).map(|_| ())
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

    fn sign_token(sk: &SigningKey, act: &Activation) -> String {
        let payload = serde_json::to_vec(act).unwrap();
        let sig = sk.sign(&payload);
        let mut raw = payload;
        raw.extend_from_slice(&sig.to_bytes());
        format!("{TOKEN_PREFIX}{}", BASE32_NOPAD.encode(&raw))
    }

    fn act(device: &str) -> Activation {
        Activation { v: 1, typ: "act".into(), lid: "lic_1".into(), device: device.into(), plan: "pro".into(), iat: 1_800_000_000 }
    }

    #[test]
    fn token_verifies_for_the_right_device_only() {
        let sk = SigningKey::from_bytes(&[7; 32]);
        let pk = sk.verifying_key().to_bytes();
        let dev = "7f3adevicehash";
        let token = sign_token(&sk, &act(dev));

        // right device, right key, not revoked
        assert_eq!(verify_token_with(&token, &pk, dev, &[]).unwrap(), act(dev));
        // whitespace/case around the token is tolerated like a key
        assert!(verify_token_with(&format!("  {}\n", token.to_lowercase()), &pk, dev, &[]).is_ok());
        // wrong device: a token copied to another machine is useless
        assert_eq!(verify_token_with(&token, &pk, "other-device", &[]), Err(LicenseError::Invalid));
        // wrong signing key (forged token)
        let other_pk = SigningKey::from_bytes(&[9; 32]).verifying_key().to_bytes();
        assert_eq!(verify_token_with(&token, &other_pk, dev, &[]), Err(LicenseError::Invalid));
        // revoked lid
        assert_eq!(verify_token_with(&token, &pk, dev, &["lic_1"]), Err(LicenseError::Revoked));
        // a license key is not a token (wrong prefix)
        assert_eq!(verify_token_with(&sign(&sk, &lic()), &pk, dev, &[]), Err(LicenseError::Invalid));
        // wrong typ is rejected even with a good signature
        let mut bad = act(dev);
        bad.typ = "lic".into();
        assert_eq!(verify_token_with(&sign_token(&sk, &bad), &pk, dev, &[]), Err(LicenseError::Invalid));
    }

    /// is_pro(): a key alone is NOT enough — Pro needs a token bound to THIS device
    /// (mirrors main.rs `load_license`). Copying the key to another machine can't turn Pro on.
    #[test]
    fn pro_needs_a_device_bound_token_not_just_a_key() {
        let sk = SigningKey::from_bytes(&[7; 32]);
        let pk = sk.verifying_key().to_bytes();
        let dev = "this-machine";
        let is_pro = |token: Option<&str>| token.map(|t| verify_token_with(t, &pk, dev, &[]).is_ok()).unwrap_or(false);

        let _key = sign(&sk, &lic()); // a valid key, but on its own:
        assert!(!is_pro(None)); // no token -> not Pro
        assert!(is_pro(Some(&sign_token(&sk, &act(dev))))); // token for this device -> Pro
        assert!(!is_pro(Some(&sign_token(&sk, &act("some-other-machine"))))); // token for another device -> not Pro
    }
}
