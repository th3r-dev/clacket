//! Built-in sounds, embedded by build.rs as deflate + ChaCha20. Decoded on demand.
use chacha20::cipher::{KeyIvInit, StreamCipher};
use std::io::Read;

include!(concat!(env!("OUT_DIR"), "/generated.rs"));

/// Raw MP3 bytes of a built-in clip, e.g. "cream/press/GENERIC_R0" or "fx/scroll".
pub fn clip(name: &str) -> Option<Vec<u8>> {
    let i = SOUND_INDEX.iter().position(|(n, _, _)| *n == name)?;
    let (_, off, len) = SOUND_INDEX[i];
    let mut data = SOUND_PACK[off..off + len].to_vec();
    let mut nonce = [0u8; 12];
    nonce[..4].copy_from_slice(&(i as u32).to_le_bytes());
    chacha20::ChaCha20::new(&SOUND_KEY.into(), &nonce.into()).apply_keystream(&mut data);
    let mut out = Vec::with_capacity(len * 2);
    flate2::read::DeflateDecoder::new(&data[..]).read_to_end(&mut out).ok()?;
    Some(out)
}

pub fn exists(name: &str) -> bool {
    SOUND_INDEX.iter().any(|(n, _, _)| *n == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_clip_unpacks_to_mp3() {
        assert!(SOUND_INDEX.len() > 150);
        for (name, _, _) in SOUND_INDEX {
            let b = clip(name).unwrap_or_else(|| panic!("{name} failed to unpack"));
            let is_mp3 = b.starts_with(b"ID3") || (b[0] == 0xFF && b[1] & 0xE0 == 0xE0);
            assert!(is_mp3, "{name} is not mp3 after unpack");
        }
    }

    #[test]
    fn pack_holds_no_plain_mp3() {
        // the first 8 plain bytes of a real clip must not appear anywhere in the blob
        let head = &clip(SOUND_INDEX[0].0).unwrap()[..8];
        assert!(!SOUND_PACK.windows(8).any(|w| w == head));
    }
}
