//! Écriture du chargeur `install.tar.xz` (le troisième membre d'un `.eopkg`).
//!
//! Toutes les entrées sont écrites avec `uid`/`gid` 0, `uname`/`gname` `root`
//! et un `mtime` de 0 : c'est ce qu'attend eopkg, et cela rend la sortie
//! reproductible quelle que soit la machine.

use std::io::{self, Read, Write};

use tar::{Builder, EntryType, Header};

/// Écrit un flux tar adapté au chargeur eopkg.
pub struct PayloadWriter<W: Write> {
    inner: Builder<W>,
}

impl<W: Write> PayloadWriter<W> {
    pub fn new(w: W) -> PayloadWriter<W> {
        PayloadWriter {
            inner: Builder::new(w),
        }
    }

    fn header(entry_type: EntryType, mode: u32, size: u64) -> Header {
        let mut h = Header::new_ustar();
        h.set_entry_type(entry_type);
        h.set_size(size);
        h.set_mode(mode & 0o7777);
        h.set_uid(0);
        h.set_gid(0);
        h.set_mtime(0);
        // « root » tient dans les champs ustar de 32 octets : l'erreur ne peut
        // pas se produire ici, on ignore donc le Result.
        let _ = h.set_username("root");
        let _ = h.set_groupname("root");
        h
    }

    /// Ajoute un fichier régulier. `data` doit fournir exactement `size` octets.
    pub fn add_regular<R: Read>(
        &mut self,
        path: &str,
        mode: u32,
        size: u64,
        data: R,
    ) -> io::Result<()> {
        let mut h = Self::header(EntryType::Regular, mode, size);
        self.inner.append_data(&mut h, path, data)
    }

    /// Ajoute un lien symbolique.
    pub fn add_symlink(&mut self, path: &str, mode: u32, target: &str) -> io::Result<()> {
        let mut h = Self::header(EntryType::Symlink, mode, 0);
        h.set_link_name(target)?;
        self.inner.append_data(&mut h, path, io::empty())
    }

    /// Ajoute un lien physique.
    pub fn add_hardlink(&mut self, path: &str, mode: u32, target: &str) -> io::Result<()> {
        let mut h = Self::header(EntryType::Link, mode, 0);
        h.set_link_name(target)?;
        self.inner.append_data(&mut h, path, io::empty())
    }

    /// Termine l'archive et rend l'écrivain sous-jacent.
    pub fn finish(self) -> io::Result<W> {
        self.inner.into_inner()
    }
}
