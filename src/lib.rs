//! `deb-to-eopkg` — convertit un paquet Debian (`.deb`) en paquet Solus (`.eopkg`).
//!
//! Copyright (C) 2026 Julien — licence GNU GPL v3 (voir le fichier LICENSE).
//!
//! Le crate est découpé en petits modules :
//!
//! * [`ar`] — lecture de l'archive `ar` (le conteneur d'un `.deb`) ;
//! * [`compress`] — décompression des membres `control.tar.*` / `data.tar.*` ;
//! * [`payload`] — écriture du chargeur `install.tar.xz` ;
//! * [`eopkg`] — le format eopkg lui-même (XML + ZIP de trois membres) ;
//! * [`zipw`] — un écrivain ZIP minimal (mais valide) ;
//! * [`convert`] — le pilote de conversion.
//!
//! Aucune dépendance à l'exécution : le binaire visé est statique
//! (`x86_64-unknown-linux-musl`), ce qui est requis pour installer le paquet
//! depuis une URL sur Solus.

pub mod ar;
pub mod color;
pub mod compress;
pub mod convert;
pub mod eopkg;
pub mod i18n;
pub mod payload;
pub mod zipw;

/// Nom de l'outil, tel qu'affiché dans les messages.
pub const TOOL_NAME: &str = "deb-to-eopkg";

/// Version du crate (issue de `Cargo.toml`).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
