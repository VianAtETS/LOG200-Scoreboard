//! `Ladder-Threads` dans le manifeste du jar : l'équipe y déclare jouer sur 2 coeur. La déclaration
//! voyage avec le jar, quelle que soit la façon de le soumettre ; un jar sans l'attribut joue sur 1.

use std::fs::File;
use std::io::{Read, Seek};
use std::path::Path;

use thiserror::Error;
use zip::ZipArchive;

const ATTRIBUTE: &str = "Ladder-Threads";
const MANIFEST_CAP: u64 = 64 * 1024;

#[derive(Debug, Error, PartialEq, Eq)]
#[error("{ATTRIBUTE} vaut {0:?} dans META-INF/MANIFEST.MF : 1 ou 2 attendu")]
pub struct BadThreads(String);

/// Sans manifeste lisible ni attribut, le jar est monothread.
pub fn multithread(jar: impl Read + Seek) -> Result<bool, BadThreads> {
    let Some(manifest) = read_manifest(jar) else {
        return Ok(false);
    };
    match main_attribute(&manifest, ATTRIBUTE).as_deref() {
        None | Some("1") => Ok(false),
        Some("2") => Ok(true),
        Some(other) => Err(BadThreads(other.into())),
    }
}

/// Pour un jar déjà accepté : sa valeur a été vérifiée à la soumission.
pub fn multithread_file(jar: &Path) -> bool {
    File::open(jar).is_ok_and(|file| multithread(file).unwrap_or(false))
}

fn read_manifest(jar: impl Read + Seek) -> Option<String> {
    let mut archive = ZipArchive::new(jar).ok()?;
    let entry = archive.by_name("META-INF/MANIFEST.MF").ok()?;
    let mut manifest = String::new();
    entry.take(MANIFEST_CAP).read_to_string(&mut manifest).ok()?;
    Some(manifest)
}

fn main_attribute(manifest: &str, name: &str) -> Option<String> {
    let mut lines: Vec<String> = Vec::new();
    for line in manifest.lines().take_while(|line| !line.is_empty()) {
        match (line.strip_prefix(' '), lines.last_mut()) {
            (Some(rest), Some(last)) => last.push_str(rest),
            _ => lines.push(line.to_string()),
        }
    }
    lines.iter().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        key.eq_ignore_ascii_case(name).then(|| value.trim().to_string())
    })
}

#[cfg(test)]
pub mod tests {
    use std::io::{Cursor, Write};

    use zip::ZipWriter;
    use zip::write::SimpleFileOptions;

    use super::*;

    /// Jar minimal dont le manifeste est `manifest`.
    pub fn jar_with_manifest(manifest: &str) -> Vec<u8> {
        let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
        zip.start_file("META-INF/MANIFEST.MF", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(manifest.as_bytes()).unwrap();
        zip.finish().unwrap().into_inner()
    }

    fn declared(manifest: &str) -> Result<bool, BadThreads> {
        multithread(Cursor::new(jar_with_manifest(manifest)))
    }

    #[test]
    fn reads_the_main_section_only() {
        let base = "Manifest-Version: 1.0\r\nMain-Class: Client\r\n";
        assert_eq!(declared(base), Ok(false));
        assert_eq!(declared(&format!("{base}Ladder-Threads: 1\r\n")), Ok(false));
        assert_eq!(declared(&format!("{base}Ladder-Threads: 2\r\n")), Ok(true));
        assert_eq!(declared(&format!("{base}ladder-threads:2\n")), Ok(true));
        assert_eq!(
            declared(&format!("{base}\r\nName: Client.class\r\nLadder-Threads: 2\r\n")),
            Ok(false)
        );
        assert_eq!(declared(&format!("{base}Ladder-Threads: \r\n 2\r\n")), Ok(true));
        assert_eq!(
            declared(&format!("{base}Ladder-Threads: 4\r\n")),
            Err(BadThreads("4".into()))
        );
    }

    #[test]
    fn a_jar_without_manifest_is_single_threaded() {
        assert_eq!(multithread(Cursor::new(b"PK\x03\x04 un jar".to_vec())), Ok(false));
        assert!(!multithread_file(Path::new("absent.jar")));
    }
}
