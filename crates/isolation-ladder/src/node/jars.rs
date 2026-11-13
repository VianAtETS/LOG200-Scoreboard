//! Jars reçus par un nœud, rangés par sha256 : un build déjà vu n'est jamais renvoyé, un nouveau
//! build n'est jamais confondu avec l'ancien. La date de modification sert d'horloge LRU : relue à
//! chaque usage, elle survit au redémarrage du nœud.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::PathBuf;
use std::time::SystemTime;

use thiserror::Error;

use isolation_core::digest::sha256_hex;

use super::protocol::is_sha256;

#[derive(Debug, Error)]
pub enum JarError {
    #[error("sha256 invalide")]
    BadHash,
    #[error("jar trop gros")]
    TooBig,
    #[error("le contenu ne correspond pas au sha256 annoncé")]
    Mismatch,
    #[error(transparent)]
    Io(#[from] io::Error),
}

pub struct JarCache {
    dir: PathBuf,
    /// Taille totale au-delà de laquelle les jars les moins récemment servis partent.
    max_bytes: u64,
    max_jar: u64,
}

impl JarCache {
    pub fn new(dir: PathBuf, max_bytes: u64, max_jar: u64) -> io::Result<Self> {
        fs::create_dir_all(&dir)?;
        Ok(Self {
            dir,
            max_bytes,
            max_jar,
        })
    }

    fn path(&self, sha256: &str) -> PathBuf {
        self.dir.join(format!("{sha256}.jar"))
    }

    /// Le jar s'il est là ; il devient le plus récent.
    pub fn get(&self, sha256: &str) -> Option<PathBuf> {
        if !is_sha256(sha256) {
            return None;
        }
        let path = self.path(sha256);
        let file = File::options().write(true).open(&path).ok()?;
        // Faute de pouvoir dater, le jar reste utilisable ; il partira seulement plus tôt.
        let _ = file.set_modified(SystemTime::now());
        Some(path)
    }

    /// Range `bytes` sous `sha256` après vérification, puis fait de la place.
    pub fn put(&self, sha256: &str, bytes: &[u8]) -> Result<PathBuf, JarError> {
        if !is_sha256(sha256) {
            return Err(JarError::BadHash);
        }
        if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > self.max_jar {
            return Err(JarError::TooBig);
        }
        if sha256_hex(bytes) != sha256 {
            return Err(JarError::Mismatch);
        }
        let path = self.path(sha256);
        let partial = self.dir.join(format!("{sha256}.part"));
        File::create(&partial)?.write_all(bytes)?;
        fs::rename(&partial, &path)?;
        self.evict(&path)?;
        Ok(path)
    }

    /// Retire les jars les moins récents tant que le total dépasse `max_bytes`, sauf `keep`.
    fn evict(&self, keep: &PathBuf) -> io::Result<()> {
        let mut jars = Vec::new();
        for entry in fs::read_dir(&self.dir)? {
            let entry = entry?;
            let meta = entry.metadata()?;
            if entry.path().extension().is_some_and(|e| e == "jar") {
                jars.push((meta.modified()?, meta.len(), entry.path()));
            }
        }
        let mut total: u64 = jars.iter().map(|(_, len, _)| len).sum();
        jars.sort();
        for (_, len, path) in jars {
            if total <= self.max_bytes {
                break;
            }
            if &path != keep {
                fs::remove_file(&path)?;
                total -= len;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use isolation_core::digest::sha256_hex;

    use super::*;

    fn cache(name: &str, max_bytes: u64, max_jar: u64) -> JarCache {
        let dir = std::env::temp_dir().join(format!("isolation-jars-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        JarCache::new(dir, max_bytes, max_jar).unwrap()
    }

    #[test]
    fn cache_is_keyed_by_content() {
        let jars = cache("content", 1 << 20, 1 << 20);
        let (v1, v2) = (b"jar v1".as_slice(), b"jar v2".as_slice());
        let (h1, h2) = (sha256_hex(v1), sha256_hex(v2));
        assert_eq!(jars.get(&h1), None);
        let path = jars.put(&h1, v1).unwrap();
        assert_eq!(fs::read(&path).unwrap(), v1);
        assert_eq!(jars.get(&h1), Some(path));
        assert_eq!(
            jars.get(&h2),
            None,
            "un nouveau build n'est jamais confondu avec l'ancien"
        );
        jars.put(&h2, v2).unwrap();
        assert_ne!(jars.get(&h1), jars.get(&h2));
        let _ = fs::remove_dir_all(&jars.dir);
    }

    #[test]
    fn a_mismatched_hash_is_refused_and_nothing_written() {
        let jars = cache("mismatch", 1 << 20, 1 << 20);
        let claimed = sha256_hex(b"autre chose");
        assert!(matches!(jars.put(&claimed, b"jar"), Err(JarError::Mismatch)));
        assert_eq!(jars.get(&claimed), None);
        assert!(matches!(jars.put("../../etc/passwd", b"jar"), Err(JarError::BadHash)));
        assert_eq!(fs::read_dir(&jars.dir).unwrap().count(), 0);
        let _ = fs::remove_dir_all(&jars.dir);
    }

    #[test]
    fn oversized_jars_are_refused() {
        let jars = cache("big", 1 << 20, 4);
        let bytes = b"12345";
        assert!(matches!(jars.put(&sha256_hex(bytes), bytes), Err(JarError::TooBig)));
        assert!(jars.put(&sha256_hex(b"1234"), b"1234").is_ok());
        let _ = fs::remove_dir_all(&jars.dir);
    }

    #[test]
    fn oldest_jars_are_evicted_first() {
        let jars = cache("evict", 10, 10);
        let (a, b, c) = (b"aaaa".as_slice(), b"bbbb".as_slice(), b"cccc".as_slice());
        jars.put(&sha256_hex(a), a).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        jars.put(&sha256_hex(b), b).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        assert!(
            jars.get(&sha256_hex(a)).is_some(),
            "a vient de servir : il passe devant b"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
        jars.put(&sha256_hex(c), c).unwrap();
        assert!(
            jars.get(&sha256_hex(b)).is_none(),
            "b, le moins récemment servi, est parti"
        );
        assert!(jars.get(&sha256_hex(a)).is_some() && jars.get(&sha256_hex(c)).is_some());
        let _ = fs::remove_dir_all(&jars.dir);
    }
}
