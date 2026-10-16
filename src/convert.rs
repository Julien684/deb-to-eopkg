//! Pilote de conversion `.deb` -> `.eopkg`.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use sha1::{Digest, Sha1};

use crate::ar;
use crate::compress::{self, Format};
use crate::eopkg::{self, Dependency, FileRecord, PackageInfo, TarSource};
use crate::payload::PayloadWriter;
use crate::TOOL_NAME;

/// Niveau de compression xz du chargeur (le défaut de `tarfile`).
const XZ_PRESET: u32 = 6;

/// Fréquence des lignes de progression (mode verbeux).
const PROGRESS_BYTES: u64 = 32 * 1024 * 1024;
const PROGRESS_FILES: u64 = 200;

/// Options d'exécution de la conversion.
#[derive(Debug, Clone, Copy)]
pub struct Options {
    /// Conserver les dépendances `Depends` du `.deb` dans l'eopkg.
    pub keep_deps: bool,
    /// Ne montrer que les erreurs : ni progression, ni avertissements.
    pub quiet: bool,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            keep_deps: false,
            quiet: false,
        }
    }
}

/// Scripts de maintenance d'un paquet Debian, dans l'ordre canonique.
pub const MAINTAINER_SCRIPTS: [&str; 4] = ["preinst", "postinst", "prerm", "postrm"];

/// Un script de maintenance lu dans le `control.tar.*` du `.deb`.
///
/// eopkg n'exécute aucun script à l'installation — le mécanisme COMAR de pisi
/// n'existe plus sur Solus — donc ces scripts ne sont jamais convertis ; ils
/// sont seulement affichés à l'utilisateur, qui doit savoir ce qu'il perd.
#[derive(Debug, Clone)]
pub struct DebScript {
    /// `preinst`, `postinst`, `prerm` ou `postrm`.
    pub name: String,
    /// Contenu brut du script, tel qu'il est stocké dans le `.deb`.
    pub content: Vec<u8>,
}

impl DebScript {
    /// Taille du script en octets.
    pub fn size(&self) -> usize {
        self.content.len()
    }

    /// Contenu décodé pour l'affichage (les octets invalides sont remplacés).
    pub fn text(&self) -> Cow<'_, str> {
        String::from_utf8_lossy(&self.content)
    }
}

/// Journalisation sur la sortie d'erreur.
///
/// L'outil est verbeux par défaut (une conversion peut durer plusieurs
/// minutes : l'utilisateur doit voir qu'il se passe quelque chose) ; `--quiet`
/// ne laisse passer que les erreurs.
#[derive(Clone, Copy)]
struct Log {
    quiet: bool,
}

impl Log {
    fn info(self, msg: impl std::fmt::Display) {
        if !self.quiet {
            eprintln!("{TOOL_NAME} : {msg}");
        }
    }

    fn warn(self, msg: impl std::fmt::Display) {
        if !self.quiet {
            eprintln!("{TOOL_NAME} : avertissement : {msg}");
        }
    }
}

/// Taille lisible (`1.7 Mio`), pour les messages.
pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["o", "Kio", "Mio", "Gio"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} o")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// Résultat d'une conversion réussie.
#[derive(Debug)]
pub struct Summary {
    pub out: PathBuf,
    pub files: u64,
    pub bytes: u64,
    pub arch: String,
    /// Dépendances `Depends` lues dans le `.deb`. Toujours renseignées, pour
    /// information ; elles ne sont inscrites dans l'eopkg que si `--deps`.
    pub deps: Vec<Dependency>,
    /// Scripts de maintenance présents dans le `.deb`, pour information.
    /// eopkg ne peut pas les exécuter : ils ne sont jamais reportés.
    pub scripts: Vec<DebScript>,
}

/// Convertit `deb` en `.eopkg`.
///
/// `dest` est soit un répertoire (terminé par `/`, ou existant), auquel cas le
/// nom de fichier est déduit du paquet, soit le chemin complet du `.eopkg`.
pub fn convert(deb: &Path, dest: &Path, opts: Options) -> io::Result<Summary> {
    let log = Log { quiet: opts.quiet };
    let deb_size = std::fs::metadata(deb).map(|m| m.len()).unwrap_or(0);
    log.info(format!("source : {} ({})", deb.display(), human_size(deb_size)));

    let members = ar::list_members(deb)?;

    let control_member = members
        .iter()
        .find(|m| m.name.starts_with("control.tar"))
        .ok_or_else(|| invalid("pas de membre control.tar.* dans le .deb"))?;

    let data_members: Vec<_> = members
        .iter()
        .filter(|m| m.name.starts_with("data.tar"))
        .collect();
    let data_member = data_members
        .first()
        .ok_or_else(|| invalid("pas de membre data.tar.* dans le .deb"))?;
    if data_members.len() > 1 {
        log.warn(format!(
            "plusieurs membres data.tar.*, on utilise {:?}",
            data_member.name
        ));
    }

    let control_fmt = fmt_of(control_member)?;
    let data_fmt = fmt_of(data_member)?;
    log.info(format!(
        "membres : {} ({}) et {} ({})",
        control_member.name,
        control_fmt.name(),
        data_member.name,
        data_fmt.name()
    ));

    // ---- champs de contrôle (+ scripts de maintenance) ----
    let ControlTar { fields, scripts } = read_control_tar(deb, control_member, control_fmt)?;
    let name = fields
        .get("Package")
        .cloned()
        .ok_or_else(|| invalid("champ Package absent du .deb"))?;
    let version = eopkg::normalize_version(
        fields.get("Version").map(String::as_str).unwrap_or("0.0.0-1"),
    );
    let deb_arch = fields
        .get("Architecture")
        .map(String::as_str)
        .unwrap_or("all");
    let arch = eopkg::eopkg_arch(deb_arch);
    // Le paquet final est toujours étiqueté x86_64 (Solus n'utilise pas
    // `noarch`). Si la charge utile visait un autre CPU, elle ne tournera pas
    // sur Solus : on le signale, mais on convertit quand même (le contenu peut
    // être indépendant de l'architecture, ex. polices ou documentation).
    if deb_arch != "amd64" && deb_arch != "all" {
        log.warn(format!(
            "le .deb vise l'architecture « {deb_arch} » mais le paquet est étiqueté x86_64 : \
             la charge utile peut ne pas fonctionner sur Solus"
        ));
    }

    let out = resolve_output(dest, &name, &version, &arch);
    log.info(format!(
        "paquet {name} {version}, architecture {arch} -> {}",
        out.display()
    ));

    // ---- chargeur : on écrit install.tar.xz en flux vers un fichier tampon ----
    let spool = Spool::new()?;
    let mut records: Vec<FileRecord> = Vec::new();
    let mut total: u64 = 0;
    // Suivi de progression (mode verbeux).
    let mut processed: u64 = 0;
    let mut last_bytes: u64 = 0;
    let mut last_files: u64 = 0;

    {
        let spool_file = File::create(spool.path())?;
        let xz = xz2::write::XzEncoder::new(BufWriter::new(spool_file), XZ_PRESET);
        let mut pw = PayloadWriter::new(xz);

        let mut deb_file = File::open(deb)?;
        let region = data_member.reader(&mut deb_file)?;
        let decompressed = compress::decode(data_fmt, region)?;
        let mut archive = tar::Archive::new(BufReader::new(decompressed));

        for entry in archive.entries()? {
            let mut entry = entry?;
            let entry_type = entry.header().entry_type();
            if entry_type.is_dir() {
                continue; // les répertoires sont créés à l'extraction
            }

            let path = normalize_path(&entry.path()?.into_owned());
            if path.is_empty() {
                continue;
            }
            let mode = entry.header().mode().unwrap_or(0o644);
            let size = entry.header().size().unwrap_or(0);
            let ftype = eopkg::classify_type(&path);

            if entry_type.is_file() {
                // Hachage incrémental pendant la copie : pas besoin de garder
                // le contenu en mémoire (certains fichiers font > 100 Mo).
                let mut hashing = HashingReader::new(&mut entry);
                pw.add_regular(&path, mode, size, &mut hashing)?;
                let hash = hashing.hex();
                records.push(FileRecord {
                    path,
                    ftype,
                    size,
                    uid: 0,
                    gid: 0,
                    mode,
                    hash,
                });
                total += size;
            } else if entry_type.is_symlink() {
                let target = link_target(entry.link_name()?, &path, log);
                pw.add_symlink(&path, mode, &target)?;
                records.push(FileRecord {
                    path,
                    ftype,
                    size: 0,
                    uid: 0,
                    gid: 0,
                    mode,
                    hash: String::new(),
                });
            } else if entry_type.is_hard_link() {
                let target = link_target(entry.link_name()?, &path, log);
                pw.add_hardlink(&path, mode, &target)?;
                records.push(FileRecord {
                    path,
                    ftype,
                    size: 0,
                    uid: 0,
                    gid: 0,
                    mode,
                    hash: String::new(),
                });
            } else {
                log.warn(format!("entrée ignorée (type {:?}) : {path}", entry_type));
            }

            processed += 1;
            if !log.quiet
                && (total - last_bytes >= PROGRESS_BYTES
                    || processed - last_files >= PROGRESS_FILES)
            {
                log.info(format!(
                    "{} fichiers, {} écrits",
                    records.len(),
                    human_size(total)
                ));
                last_bytes = total;
                last_files = processed;
            }
        }

        let xz = pw.finish()?;
        let mut buffered = xz.finish()?;
        buffered.flush()?;
    }

    if records.is_empty() {
        return Err(invalid("aucun fichier dans data.tar.* du .deb"));
    }

    // ---- métadonnées ----
    let mut info = PackageInfo::from_control(&fields, &name, &version, &arch);
    // On analyse `Depends` dans tous les cas : la liste est reprise à la fin
    // de la conversion, pour information. Elle n'est inscrite dans l'eopkg que
    // si `--deps` (les noms Debian ne correspondent pas aux noms eopkg).
    let deps = parse_deps(
        fields.get("Depends").map(String::as_str).unwrap_or(""),
        log,
    );
    if opts.keep_deps {
        log.info("dépendances Depends conservées dans l'eopkg");
        info.deps = deps.clone();
    } else {
        log.info("dépendances Depends ignorées (--deps pour les conserver)");
    }

    eopkg::write_eopkg(&out, &info, &records, TarSource::File(spool.path()))?;

    Ok(Summary {
        out,
        files: records.len() as u64,
        bytes: total,
        arch,
        deps,
        scripts,
    })
}

/// Format de compression d'un membre `*.tar*`.
fn fmt_of(member: &ar::ArMember) -> io::Result<Format> {
    let suffix = member.name.strip_prefix("control.tar").or_else(|| member.name.strip_prefix("data.tar"));
    match suffix.map(Format::from_suffix) {
        Some(Some(fmt)) => Ok(fmt),
        _ => Err(invalid(format!(
            "compression non prise en charge pour le membre {:?}",
            member.name
        ))),
    }
}

/// Contenu utile d'un `control.tar.*`.
struct ControlTar {
    fields: BTreeMap<String, String>,
    scripts: Vec<DebScript>,
}

/// Lit le `control.tar.*` : champs du fichier `control` et scripts de
/// maintenance.
///
/// Les lignes de continuation du fichier `control` (celles qui commencent par
/// une espace) sont rattachées au champ précédent, séparées par un `\n` : c'est
/// ainsi que la description longue de Debian est préservée.
fn read_control_tar(deb: &Path, member: &ar::ArMember, fmt: Format) -> io::Result<ControlTar> {
    let mut f = File::open(deb)?;
    let region = member.reader(&mut f)?;
    let mut decompressed = compress::decode(fmt, region)?;

    let mut blob = Vec::new();
    decompressed.read_to_end(&mut blob)?;
    collect_control_entries(&blob)
}

/// Extraits d'un `control.tar` déjà décompressé : champs de `control` et
/// scripts de maintenance.
fn collect_control_entries(blob: &[u8]) -> io::Result<ControlTar> {
    let mut fields: Option<BTreeMap<String, String>> = None;
    let mut scripts: Vec<DebScript> = Vec::new();

    let mut archive = tar::Archive::new(blob);
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        if path.to_string_lossy().ends_with('/') {
            continue;
        }
        let Some(base) = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
        else {
            continue;
        };
        let mut content = Vec::new();
        entry.read_to_end(&mut content)?;

        if base == "control" {
            fields = Some(parse_control_fields(&String::from_utf8_lossy(&content)));
        } else if MAINTAINER_SCRIPTS.contains(&base.as_str()) {
            scripts.push(DebScript {
                name: base,
                content,
            });
        }
    }

    let fields =
        fields.ok_or_else(|| invalid("fichier `control` introuvable dans control.tar.*"))?;
    scripts.sort_by_key(|s| {
        MAINTAINER_SCRIPTS
            .iter()
            .position(|name| *name == s.name)
            .unwrap_or(usize::MAX)
    });
    Ok(ControlTar { fields, scripts })
}

/// Analyse le contenu d'un fichier `control` (`Clé: valeur`, continuations).
fn parse_control_fields(text: &str) -> BTreeMap<String, String> {
    let mut fields: BTreeMap<String, String> = BTreeMap::new();
    let mut current: Option<String> = None;

    for line in text.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if line.is_empty() {
            continue;
        }
        if line.starts_with(' ') || line.starts_with('\t') {
            // Ligne de continuation : appartient au champ précédent.
            if let Some(key) = &current {
                if let Some(value) = fields.get_mut(key) {
                    value.push('\n');
                    value.push_str(line.trim_start());
                }
            }
            continue;
        }
        if let Some((key, value)) = line.split_once(':') {
            let key = key.trim().to_string();
            fields.insert(key.clone(), value.trim().to_string());
            current = Some(key);
        }
    }
    fields
}

/// Dépendances `Depends` d'un `.deb`, au format eopkg.
///
/// Les dépendances qualifiées par architecture (`arch:nom`) sont ignorées :
/// elles n'ont pas d'équivalent eopkg.
fn parse_deps(deps: &str, log: Log) -> Vec<Dependency> {
    let mut out = Vec::new();
    for part in deps.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let (name, constraint) = match part.split_once('(') {
            Some((name, rest)) => {
                let c = rest.trim().trim_end_matches(')').trim();
                (
                    name.trim(),
                    if c.is_empty() {
                        None
                    } else {
                        Some(c.to_string())
                    },
                )
            }
            None => (part, None),
        };
        if name.is_empty() {
            continue;
        }
        if name.contains(':') {
            log.warn(format!("dépendance par architecture ignorée : {part}"));
            continue;
        }
        out.push(Dependency {
            name: name.to_string(),
            constraint,
        });
    }
    out
}

/// Chemin relatif dans le paquet : sans `./` ni `/` de tête.
fn normalize_path(path: &Path) -> String {
    path.to_string_lossy()
        .trim_start_matches(|c| c == '.' || c == '/')
        .to_string()
}

/// Cible d'un lien, normalisée en relatif si elle était absolue.
fn link_target(raw: Option<Cow<'_, Path>>, path: &str, log: Log) -> String {
    let target = raw.map(|c| c.to_string_lossy().into_owned()).unwrap_or_default();
    if let Some(rest) = target.strip_prefix('/') {
        log.warn(format!("lien absolu normalisé en relatif : {path} -> {target}"));
        rest.to_string()
    } else {
        target
    }
}

/// Chemin de sortie : un répertoire (`dest/`, ou un répertoire existant)
/// reçoit le nom de fichier déduit du paquet.
fn resolve_output(dest: &Path, name: &str, version: &str, arch: &str) -> PathBuf {
    let looks_like_dir = dest.to_string_lossy().ends_with('/') || dest.is_dir();
    if looks_like_dir {
        dest.join(eopkg::eopkg_filename(name, version, "1", arch))
    } else {
        dest.to_path_buf()
    }
}

fn invalid(msg: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg.into())
}

/// Fichier temporaire supprimé à la destruction.
struct Spool {
    path: PathBuf,
}

impl Spool {
    fn new() -> io::Result<Spool> {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let mut path = std::env::temp_dir();
        path.push(format!(
            "deb-to-eopkg-{}-{nanos}.tar.xz.tmp",
            std::process::id()
        ));
        Ok(Spool { path })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for Spool {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Lecteur qui met à jour un SHA1 au fil des octets lus.
struct HashingReader<R> {
    inner: R,
    hasher: Sha1,
}

impl<R: Read> HashingReader<R> {
    fn new(inner: R) -> HashingReader<R> {
        HashingReader {
            inner,
            hasher: Sha1::new(),
        }
    }

    fn hex(self) -> String {
        hex_lower(&self.hasher.finalize())
    }
}

impl<R: Read> Read for HashingReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.hasher.update(&buf[..n]);
        Ok(n)
    }
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn champs_de_controle() {
        let text = "Package: foo\nVersion: 1.2.3-1\nDescription: short desc\n long line one\n long line two\n\nDepends: a, b\n";
        let f = parse_control_fields(text);
        assert_eq!(f.get("Package").unwrap(), "foo");
        assert_eq!(f.get("Version").unwrap(), "1.2.3-1");
        assert_eq!(
            f.get("Description").unwrap(),
            "short desc\nlong line one\nlong line two"
        );
        assert_eq!(f.get("Depends").unwrap(), "a, b");
    }

    #[test]
    fn dependances() {
        let d = parse_deps("libc6 (>= 2.34), zlib1g, arch:foo", Log { quiet: true });
        assert_eq!(d.len(), 2);
        assert_eq!(d[0].name, "libc6");
        assert_eq!(d[0].constraint.as_deref(), Some(">= 2.34"));
        assert_eq!(d[1].name, "zlib1g");
        assert_eq!(d[1].constraint, None);
    }

    #[test]
    fn chemins() {
        assert_eq!(normalize_path(Path::new("./usr/bin/tool")), "usr/bin/tool");
        assert_eq!(normalize_path(Path::new("usr/bin/tool")), "usr/bin/tool");
    }

    #[test]
    fn sortie() {
        let p = resolve_output(Path::new("/tmp/out/"), "foo", "1.2.3", "x86_64");
        assert_eq!(p, PathBuf::from("/tmp/out/foo-1.2.3-1-1-x86_64.eopkg"));
        let p = resolve_output(Path::new("/tmp/x.eopkg"), "foo", "1.2.3", "x86_64");
        assert_eq!(p, PathBuf::from("/tmp/x.eopkg"));
    }

    /// Construit un `control.tar` minimal en mémoire.
    fn control_tar(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut builder = tar::Builder::new(Vec::new());
        for (name, data) in entries {
            let mut header = tar::Header::new_ustar();
            header.set_entry_type(tar::EntryType::Regular);
            header.set_size(data.len() as u64);
            header.set_mode(0o644);
            header.set_uid(0);
            header.set_gid(0);
            header.set_mtime(0);
            builder.append_data(&mut header, name, &data[..]).unwrap();
        }
        builder.into_inner().unwrap()
    }

    #[test]
    fn scripts_de_maintenance_detectes_et_ordonnes() {
        let tar = control_tar(&[
            ("./control", b"Package: foo\nVersion: 1.0\n"),
            ("./postrm", b"#!/bin/sh\nexit 0\n"),
            ("./md5sums", b"abcd  usr/bin/foo\n"),
            ("./postinst", b"#!/bin/sh\necho hi\n"),
            ("./preinst", b"#!/bin/sh\nexit 0\n"),
        ]);
        let parsed = collect_control_entries(&tar).unwrap();
        assert_eq!(parsed.fields.get("Package").unwrap(), "foo");

        // Seuls les quatre scripts de maintenance sont retenus (`md5sums` et
        // `conffiles` sont ignorés), et ils ressortent dans l'ordre canonique.
        let names: Vec<&str> = parsed.scripts.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["preinst", "postinst", "postrm"]);
        assert!(parsed.scripts.iter().all(|s| s.size() > 0));
        // Le contenu est conservé tel quel, pour l'afficher.
        let postinst = parsed
            .scripts
            .iter()
            .find(|s| s.name == "postinst")
            .unwrap();
        assert_eq!(postinst.text(), "#!/bin/sh\necho hi\n");
    }

    #[test]
    fn paquet_sans_script_de_maintenance() {
        let tar = control_tar(&[("./control", b"Package: bar\n")]);
        let parsed = collect_control_entries(&tar).unwrap();
        assert!(parsed.scripts.is_empty());
    }

    #[test]
    fn control_absent_est_une_erreur() {
        let tar = control_tar(&[("./postinst", b"#!/bin/sh\n")]);
        assert!(collect_control_entries(&tar).is_err());
    }
}
