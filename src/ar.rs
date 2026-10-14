//! Lecture minimale d'une archive `ar` — juste ce qu'il faut pour un `.deb`.
//!
//! Un `.deb` n'est rien d'autre qu'une archive `ar` contenant
//! `debian-binary`, `control.tar.*` et `data.tar.*`. On se contente de repérer
//! les membres (nom, décalage, taille) : les gros membres sont ensuite lus en
//! flux, jamais chargés entièrement en mémoire.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

/// Octets magiques de toute archive `ar`.
pub const AR_MAGIC: &[u8] = b"!<arch>\n";

/// Un membre d'une archive `ar`, localisé par son décalage.
#[derive(Debug, Clone)]
pub struct ArMember {
    /// Nom du membre, sans l'espace de remplissage ni le `/` final.
    pub name: String,
    /// Décalage absolu des données dans le fichier.
    pub offset: u64,
    /// Taille déclarée des données.
    pub size: u64,
}

impl ArMember {
    /// Un lecteur limité aux octets de ce membre.
    pub fn reader<'a>(&self, f: &'a mut File) -> io::Result<io::Take<&'a mut File>> {
        f.seek(SeekFrom::Start(self.offset))?;
        Ok(f.take(self.size))
    }
}

/// Liste les membres d'une archive `ar`, dans l'ordre du fichier.
///
/// Erreur si la signature `!<arch>\n` est absente (ce n'est pas un `.deb`).
pub fn list_members(path: &Path) -> io::Result<Vec<ArMember>> {
    let mut f = File::open(path)?;
    let end = f.seek(SeekFrom::End(0))?;

    f.seek(SeekFrom::Start(0))?;
    let mut magic = [0u8; 8];
    f.read_exact(&mut magic)?;
    if &magic != AR_MAGIC {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "ce n'est pas une archive `ar` (signature \"!<arch>\\n\" absente)",
        ));
    }

    let mut members = Vec::new();
    let mut off: u64 = 8;
    while off + 60 <= end {
        f.seek(SeekFrom::Start(off))?;
        let mut hdr = [0u8; 60];
        f.read_exact(&mut hdr)?;

        // GNU `ar` termine le nom par un `/` (ex. `control.tar.xz/`) et complète
        // avec des espaces ; dpkg-deb, lui, n'écrit que des espaces. On retire
        // les deux, comme le prévoit le format `ar`.
        let name = String::from_utf8_lossy(&hdr[0..16])
            .trim()
            .trim_end_matches('/')
            .to_string();
        let size_str = String::from_utf8_lossy(&hdr[48..58]).trim().to_string();
        let size: u64 = size_str.parse().map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("taille de membre `ar` invalide ({size_str:?}) pour {name:?}"),
            )
        })?;

        members.push(ArMember {
            name,
            offset: off + 60,
            size,
        });

        // Les membres sont alignés sur 2 octets (un octet de remplissage si
        // la taille est impaire).
        off += 60 + size;
        if off % 2 != 0 {
            off += 1;
        }
    }
    Ok(members)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_ar_file() {
        let dir = std::env::temp_dir();
        let p = dir.join("deb-to-eopkg-ar-test-not-ar");
        std::fs::write(&p, b"not an ar archive").unwrap();
        assert!(list_members(&p).is_err());
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn strips_gnu_ar_name_terminator() {
        // Certains .deb (assemblés avec GNU `ar`, ex. electron-builder) écrivent
        // le nom suivi d'un `/` : il doit être retiré comme le prévoit le format.
        let mut a = Vec::new();
        a.extend_from_slice(AR_MAGIC);
        let body = b"xzdata";
        let mut h = [b' '; 60];
        let name = "control.tar.xz/";
        h[0..name.len()].copy_from_slice(name.as_bytes());
        h[48..49].copy_from_slice(b"6");
        a.extend_from_slice(&h);
        a.extend_from_slice(body);

        let dir = std::env::temp_dir();
        let p = dir.join("deb-to-eopkg-ar-test-gnu");
        std::fs::write(&p, &a).unwrap();
        let m = list_members(&p).unwrap();
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].name, "control.tar.xz");
        assert_eq!(m[0].size, 6);
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn reads_member_offsets() {
        // Archive `ar` de 2 membres, tailles impaire et paire.
        let mut a = Vec::new();
        a.extend_from_slice(AR_MAGIC);
        let push = |a: &mut Vec<u8>, name: &str, body: &[u8]| {
            let mut h = [b' '; 60];
            h[0..name.len()].copy_from_slice(name.as_bytes());
            let size = format!("{}", body.len());
            h[48..48 + size.len()].copy_from_slice(size.as_bytes());
            a.extend_from_slice(&h);
            a.extend_from_slice(body);
            if body.len() % 2 != 0 {
                a.push(b'\n');
            }
        };
        push(&mut a, "debian-binary", b"2.0\n");
        push(&mut a, "data.tar.xz", b"abc");

        let dir = std::env::temp_dir();
        let p = dir.join("deb-to-eopkg-ar-test");
        std::fs::write(&p, &a).unwrap();
        let m = list_members(&p).unwrap();
        assert_eq!(m.len(), 2);
        assert_eq!(m[0].name, "debian-binary");
        assert_eq!(m[0].size, 4);
        assert_eq!(m[0].offset, 8 + 60);
        assert_eq!(m[1].name, "data.tar.xz");
        assert_eq!(m[1].size, 3);
        // 8 (magic) + 60 + 4 (membre 1, taille paire : pas de remplissage) + 60
        assert_eq!(m[1].offset, 8 + 60 + 4 + 60);
        let _ = std::fs::remove_file(&p);
    }
}
