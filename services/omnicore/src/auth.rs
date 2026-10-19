//! Identity helpers: offline-mode UUIDs and the Mojang session-server join
//! used by authenticated (online-mode) connections.

use anyhow::{bail, Context, Result};
use md5::{Digest as _, Md5};
use num_bigint::BigInt;
use sha1::Sha1;
use uuid::Uuid;

/// Offline-mode UUID exactly as the vanilla server derives it:
/// a name-based (version 3) UUID of `"OfflinePlayer:" + name`.
pub fn offline_uuid(username: &str) -> Uuid {
    let mut hash: [u8; 16] = Md5::digest(format!("OfflinePlayer:{username}").as_bytes()).into();
    hash[6] = (hash[6] & 0x0f) | 0x30;
    hash[8] = (hash[8] & 0x3f) | 0x80;
    Uuid::from_bytes(hash)
}

/// Validates a Minecraft username (3-16 chars of [A-Za-z0-9_]).
pub fn valid_username(name: &str) -> bool {
    (3..=16).contains(&name.len()) && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Minecraft's non-standard "server hash": SHA-1 digest interpreted as a signed
/// two's complement big integer, printed in hex.
pub fn server_hash(server_id: &str, shared_secret: &[u8], public_key: &[u8]) -> String {
    let mut h = Sha1::new();
    sha1::Digest::update(&mut h, server_id.as_bytes());
    sha1::Digest::update(&mut h, shared_secret);
    sha1::Digest::update(&mut h, public_key);
    let digest = sha1::Digest::finalize(h);
    BigInt::from_signed_bytes_be(&digest).to_str_radix(16)
}

/// Credentials for an authenticated (Microsoft-owned) Java account.
#[derive(Clone, Debug)]
pub struct OnlineSession {
    pub access_token: String,
    pub profile_id: Uuid,
    pub username: String,
}

/// Tells the Mojang session server that this account is joining the server
/// identified by `hash`. The server then verifies it with `hasJoined`.
pub fn session_join(session: &OnlineSession, hash: &str) -> Result<()> {
    let base = std::env::var("OMNI_SESSION_SERVER").unwrap_or_else(|_| "https://sessionserver.mojang.com".into());
    let body = serde_json::json!({
        "accessToken": session.access_token,
        "selectedProfile": session.profile_id.simple().to_string(),
        "serverId": hash,
    });
    let resp = ureq::post(&format!("{base}/session/minecraft/join"))
        .set("Content-Type", "application/json")
        .send_json(body);
    match resp {
        Ok(r) if r.status() == 204 || r.status() == 200 => Ok(()),
        Ok(r) => bail!("session join returned HTTP {}", r.status()),
        Err(ureq::Error::Status(code, r)) => {
            let text = r.into_string().unwrap_or_default();
            bail!("session join rejected (HTTP {code}): {text}")
        }
        Err(e) => Err(e).context("session join request failed"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offline_uuid_matches_vanilla() {
        // Reference values computed with Java UUID.nameUUIDFromBytes (what vanilla uses).
        assert_eq!(offline_uuid("Notch").to_string(), "b50ad385-829d-3141-a216-7e7d7539ba7f");
        assert_eq!(offline_uuid("jeb_").to_string(), "a762f560-4fce-3236-812a-b80efff0b62b");
        assert_eq!(offline_uuid("NovaCraft").to_string(), "57648c4a-7a2c-396b-989c-611e6abb621e");
    }

    #[test]
    fn server_hash_vectors() {
        // Well-known test vectors for the Minecraft hex digest.
        let h = |s: &str| server_hash(s, &[], &[]);
        assert_eq!(h("Notch"), "4ed1f46bbe04bc756bcb17c0c7ce3e4632f06a48");
        assert_eq!(h("jeb_"), "-7c9d5b0044c130109a5d7b5fb5c317c02b4e28c1");
        assert_eq!(h("simon"), "88e16a1019277b15d58faf0541e11910eb756f6");
    }

    #[test]
    fn usernames() {
        assert!(valid_username("NovaCraft"));
        assert!(!valid_username("ab"));
        assert!(!valid_username("bad name"));
    }
}
