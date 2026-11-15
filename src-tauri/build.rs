// Packs every assets/sounds/**/*.mp3 into one blob: deflate, then ChaCha20 with a
// per-build key. The key ships in the binary, so this stops casual extraction
// (no plain MP3s to carve out of the app), not a determined reverse engineer.
use chacha20::cipher::{KeyIvInit, StreamCipher};
use std::{fmt::Write as _, fs, io::Write, path::Path};

fn walk(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    for e in fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            walk(&p, out)
        } else if p.extension().is_some_and(|x| x == "mp3") {
            out.push(p)
        }
    }
}

fn main() {
    println!("cargo:rerun-if-changed=assets/sounds");
    println!("cargo:rerun-if-changed=revoked.txt");
    let root = Path::new("assets/sounds");
    let mut files = vec![];
    walk(root, &mut files);
    files.sort();

    let mut key = [0u8; 32];
    getrandom::fill(&mut key).unwrap();

    let mut blob = vec![];
    let mut index = String::new();
    for (i, f) in files.iter().enumerate() {
        let name = f.strip_prefix(root).unwrap().with_extension("");
        let name = name.to_string_lossy().replace('\\', "/");
        let mut z = flate2::write::DeflateEncoder::new(vec![], flate2::Compression::best());
        z.write_all(&fs::read(f).unwrap()).unwrap();
        let mut data = z.finish().unwrap();
        chacha20::ChaCha20::new(&key.into(), &nonce(i as u32).into()).apply_keystream(&mut data);
        writeln!(index, "    (\"{name}\", {}, {}),", blob.len(), data.len()).unwrap();
        blob.extend_from_slice(&data);
    }

    let revoked: Vec<String> = fs::read_to_string("revoked.txt")
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| format!("{l:?}"))
        .collect();

    let out = std::env::var("OUT_DIR").unwrap();
    fs::write(Path::new(&out).join("sounds.bin"), &blob).unwrap();
    fs::write(
        Path::new(&out).join("generated.rs"),
        format!(
            "pub const SOUND_KEY: [u8; 32] = {key:?};\n\
             pub static SOUND_PACK: &[u8] = include_bytes!(concat!(env!(\"OUT_DIR\"), \"/sounds.bin\"));\n\
             /// (name, offset, len); entry i was encrypted with nonce(i)\n\
             pub static SOUND_INDEX: &[(&str, usize, usize)] = &[\n{index}];\n\
             pub static REVOKED: &[&str] = &[{}];\n",
            revoked.join(", ")
        ),
    )
    .unwrap();

    tauri_build::build()
}

fn nonce(i: u32) -> [u8; 12] {
    let mut n = [0u8; 12];
    n[..4].copy_from_slice(&i.to_le_bytes());
    n
}
