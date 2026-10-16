//! Décompression des membres `control.tar.*` et `data.tar.*` d'un `.deb`.
//!
//! C'est ici que se joue la robustesse de l'outil : un `.deb` peut comprimer
//! son `data.tar` en gzip (le plus ancien), xz (défaut de dpkg), bzip2, lzma
//! ou zstd (défaut d'Ubuntu). Tous ces formats sont pris en charge.

use std::io::{self, Read};

/// Compression d'un membre, déduite de son suffixe (`control.tar.xz` → `Xz`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Plain,
    Gzip,
    Bzip2,
    Xz,
    Lzma,
    Zstd,
}

impl Format {
    /// Reconnaît la compression d'après ce qui suit `control.tar` / `data.tar`.
    ///
    /// Renvoie `None` pour un suffixe inconnu (l'appelant peut alors produire
    /// un message d'erreur explicite).
    pub fn from_suffix(suffix: &str) -> Option<Format> {
        Some(match suffix {
            "" => Format::Plain,
            ".gz" => Format::Gzip,
            ".bz2" => Format::Bzip2,
            ".xz" => Format::Xz,
            ".lzma" => Format::Lzma,
            ".zst" | ".zstd" => Format::Zstd,
            _ => return None,
        })
    }

    /// Nom court, pour les messages.
    pub fn name(self) -> &'static str {
        match self {
            Format::Plain => "tar",
            Format::Gzip => "gzip",
            Format::Bzip2 => "bzip2",
            Format::Xz => "xz",
            Format::Lzma => "lzma",
            Format::Zstd => "zstd",
        }
    }
}

/// Enveloppe `r` dans le décompresseur correspondant au format.
pub fn decode<'a, R: Read + 'a>(fmt: Format, r: R) -> io::Result<Box<dyn Read + 'a>> {
    Ok(match fmt {
        Format::Plain => Box::new(r),
        Format::Gzip => Box::new(flate2::read::MultiGzDecoder::new(r)),
        Format::Bzip2 => Box::new(bzip2::read::MultiBzDecoder::new(r)),
        Format::Xz => Box::new(xz2::read::XzDecoder::new(r)),
        Format::Zstd => Box::new(zstd::stream::read::Decoder::new(r)?),
        Format::Lzma => {
            // `.lzma` = flux LZMA seul (« lzma_alone »), sans en-tête xz.
            let stream = xz2::stream::Stream::new_lzma_decoder(u64::MAX)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
            Box::new(xz2::read::XzDecoder::new_stream(r, stream))
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suffixes() {
        assert_eq!(Format::from_suffix(""), Some(Format::Plain));
        assert_eq!(Format::from_suffix(".gz"), Some(Format::Gzip));
        assert_eq!(Format::from_suffix(".bz2"), Some(Format::Bzip2));
        assert_eq!(Format::from_suffix(".xz"), Some(Format::Xz));
        assert_eq!(Format::from_suffix(".lzma"), Some(Format::Lzma));
        assert_eq!(Format::from_suffix(".zst"), Some(Format::Zstd));
        assert_eq!(Format::from_suffix(".zstd"), Some(Format::Zstd));
        assert_eq!(Format::from_suffix(".lz4"), None);
    }
}
