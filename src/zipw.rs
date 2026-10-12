//! Un écrivain ZIP minimal — juste de quoi produire les trois membres d'un
//! `.eopkg`.
//!
//! Pourquoi ne pas utiliser une caisse `zip` ? Parce qu'écrire un ZIP est
//! simple, que le résultat doit être **déterministe** (horodatages fixes, pas
//! de champs supplémentaires) et que cela garde l'arbre de dépendances réduit.
//! La sortie est lue par `zipfile` de Python côté eopkg/pisi, donc du ZIP tout
//! à fait standard suffit.

use std::fs::File;
use std::io::{self, Read, Write};
use std::path::Path;

use crc32fast::Hasher;
use flate2::write::DeflateEncoder;
use flate2::Compression;

const LOCAL_SIG: u32 = 0x0403_4b50;
const CENTRAL_SIG: u32 = 0x0201_4b50;
const EOCD_SIG: u32 = 0x0605_4b50;

/// 1er janvier 1980, 00:00 — la date la plus ancienne représentable en ZIP.
/// On l'utilise partout pour rendre la sortie reproductible.
const DOS_DATE: u16 = 0x0021;
const DOS_TIME: u16 = 0;

const METHOD_STORED: u16 = 0;
const METHOD_DEFLATE: u16 = 8;

struct CentralEntry {
    name: String,
    crc: u32,
    compressed_size: u32,
    uncompressed_size: u32,
    method: u16,
    offset: u32,
}

/// Écrivain ZIP séquentiel (pas d'archive ZIP64 : les membres d'un `.eopkg`
/// restent sous les 4 Gio).
pub struct ZipWriter<W: Write> {
    out: W,
    written: u64,
    entries: Vec<CentralEntry>,
}

impl<W: Write> ZipWriter<W> {
    pub fn new(out: W) -> ZipWriter<W> {
        ZipWriter {
            out,
            written: 0,
            entries: Vec::new(),
        }
    }

    /// Ajoute un membre stocké tel quel (aucune compression).
    pub fn add_stored(&mut self, name: &str, data: &[u8]) -> io::Result<()> {
        let crc = crc32(data);
        let len = data.len() as u64;
        self.write_entry(name, crc, len, len, METHOD_STORED, |out| {
            out.write_all(data)
        })
    }

    /// Ajoute un membre compressé en deflate.
    pub fn add_deflated(&mut self, name: &str, data: &[u8]) -> io::Result<()> {
        let mut enc = DeflateEncoder::new(Vec::new(), Compression::default());
        enc.write_all(data)?;
        let compressed = enc.finish()?;
        let crc = crc32(data);
        self.write_entry(
            name,
            crc,
            compressed.len() as u64,
            data.len() as u64,
            METHOD_DEFLATE,
            |out| out.write_all(&compressed),
        )
    }

    /// Ajoute un membre stocké lu depuis un fichier, en flux : le fichier est
    /// parcouru une première fois pour le CRC, puis recopié sans être chargé
    /// en mémoire (le chargeur `install.tar.xz` peut peser des centaines de
    /// mégaoctets).
    pub fn add_stored_file(&mut self, name: &str, path: &Path) -> io::Result<()> {
        let (crc, len) = crc32_of_file(path)?;
        self.write_entry(name, crc, len, len, METHOD_STORED, |out| {
            let mut f = File::open(path)?;
            io::copy(&mut f, out)?;
            Ok(())
        })
    }

    fn write_entry<F>(
        &mut self,
        name: &str,
        crc: u32,
        compressed_size: u64,
        uncompressed_size: u64,
        method: u16,
        payload: F,
    ) -> io::Result<()>
    where
        F: FnOnce(&mut W) -> io::Result<()>,
    {
        if name.len() > u16::MAX as usize
            || compressed_size > u32::MAX as u64
            || uncompressed_size > u32::MAX as u64
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("ZIP64 requis pour le membre {name:?} (non pris en charge)"),
            ));
        }

        let offset = self.written;
        let mut hdr = Vec::with_capacity(30 + name.len());
        push_u32(&mut hdr, LOCAL_SIG);
        push_u16(&mut hdr, 20); // version minimale pour extraire
        push_u16(&mut hdr, 0); // drapeaux : noms ASCII, pas de descripteur
        push_u16(&mut hdr, method);
        push_u16(&mut hdr, DOS_TIME);
        push_u16(&mut hdr, DOS_DATE);
        push_u32(&mut hdr, crc);
        push_u32(&mut hdr, compressed_size as u32);
        push_u32(&mut hdr, uncompressed_size as u32);
        push_u16(&mut hdr, name.len() as u16);
        push_u16(&mut hdr, 0); // longueur des données supplémentaires
        hdr.extend_from_slice(name.as_bytes());

        self.out.write_all(&hdr)?;
        self.written += hdr.len() as u64;
        payload(&mut self.out)?;
        self.written += compressed_size;

        self.entries.push(CentralEntry {
            name: name.to_string(),
            crc,
            compressed_size: compressed_size as u32,
            uncompressed_size: uncompressed_size as u32,
            method,
            offset: offset as u32,
        });
        Ok(())
    }

    /// Écrit le répertoire central et le marqueur de fin d'archive.
    pub fn finish(mut self) -> io::Result<W> {
        let cd_offset = self.written;
        for e in &self.entries {
            let mut hdr = Vec::with_capacity(46 + e.name.len());
            push_u32(&mut hdr, CENTRAL_SIG);
            push_u16(&mut hdr, 20); // version de création
            push_u16(&mut hdr, 20); // version minimale
            push_u16(&mut hdr, 0); // drapeaux
            push_u16(&mut hdr, e.method);
            push_u16(&mut hdr, DOS_TIME);
            push_u16(&mut hdr, DOS_DATE);
            push_u32(&mut hdr, e.crc);
            push_u32(&mut hdr, e.compressed_size);
            push_u32(&mut hdr, e.uncompressed_size);
            push_u16(&mut hdr, e.name.len() as u16);
            push_u16(&mut hdr, 0); // données supplémentaires
            push_u16(&mut hdr, 0); // commentaire
            push_u16(&mut hdr, 0); // n° de disque
            push_u16(&mut hdr, 0); // attributs internes
            push_u32(&mut hdr, 0o644 << 16); // attributs externes
            push_u32(&mut hdr, e.offset);
            hdr.extend_from_slice(e.name.as_bytes());
            self.out.write_all(&hdr)?;
            self.written += hdr.len() as u64;
        }
        let cd_size = self.written - cd_offset;

        let mut eocd = Vec::with_capacity(22);
        push_u32(&mut eocd, EOCD_SIG);
        push_u16(&mut eocd, 0); // n° de ce disque
        push_u16(&mut eocd, 0); // disque du répertoire central
        push_u16(&mut eocd, self.entries.len() as u16);
        push_u16(&mut eocd, self.entries.len() as u16);
        push_u32(&mut eocd, cd_size as u32);
        push_u32(&mut eocd, cd_offset as u32);
        push_u16(&mut eocd, 0); // commentaire
        self.out.write_all(&eocd)?;
        self.out.flush()?;
        Ok(self.out)
    }
}

fn push_u16(v: &mut Vec<u8>, x: u16) {
    v.extend_from_slice(&x.to_le_bytes());
}

fn push_u32(v: &mut Vec<u8>, x: u32) {
    v.extend_from_slice(&x.to_le_bytes());
}

fn crc32(data: &[u8]) -> u32 {
    let mut h = Hasher::new();
    h.update(data);
    h.finalize()
}

fn crc32_of_file(path: &Path) -> io::Result<(u32, u64)> {
    let mut f = File::open(path)?;
    let mut h = Hasher::new();
    let mut len: u64 = 0;
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
        len += n as u64;
    }
    Ok((h.finalize(), len))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32_matches_reference() {
        // Valeurs de référence du CRC-32 (polynôme IEEE 802.3).
        assert_eq!(crc32(b""), 0x0000_0000);
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(crc32(b"The quick brown fox jumps over the lazy dog"), 0x414F_A339);
    }

    #[test]
    fn produces_a_readable_zip_container() {
        let mut z = ZipWriter::new(Vec::new());
        z.add_stored("a.txt", b"hello").unwrap();
        z.add_deflated("b.txt", b"hello hello hello").unwrap();
        let out = z.finish().unwrap();

        // Le répertoire central est présent…
        assert!(out.windows(4).any(|w| w == [0x50, 0x4b, 0x01, 0x02]));
        // …et le marqueur de fin d'archive occupe les 22 derniers octets.
        let eocd = &out[out.len() - 22..];
        assert_eq!(&eocd[0..4], [0x50, 0x4b, 0x05, 0x06]);
        assert_eq!(u16::from_le_bytes([eocd[10], eocd[11]]), 2); // 2 entrées
        assert_eq!(u16::from_le_bytes([eocd[20], eocd[21]]), 0); // pas de commentaire
    }
}
