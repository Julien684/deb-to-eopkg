//! Internationalisation (français / anglais).
//!
//! Aucune dépendance externe : chaque langue est un jeu de gabarits
//! `&'static str` et [`fill`] remplace les `{}` dans l'ordre. La langue est
//! choisie d'après la locale du système (`LC_ALL`, `LC_MESSAGES`, `LANG`, puis
//! `/etc/locale.conf`), l'anglais servant de repli.

/// Langue de l'interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Fr,
    En,
}

impl Lang {
    /// Déduit la langue de la locale du système.
    pub fn detect() -> Lang {
        for var in ["LC_ALL", "LC_MESSAGES", "LANG"] {
            if let Some(value) = std::env::var_os(var) {
                let value = value.to_string_lossy();
                if !value.trim().is_empty() {
                    return from_locale(&value);
                }
            }
        }
        // Session sans locale (ssh non interactif, cron…) : on lit la
        // configuration système.
        if let Ok(conf) = std::fs::read_to_string("/etc/locale.conf") {
            for line in conf.lines() {
                let line = line.trim();
                for key in ["LANG=", "LC_MESSAGES="] {
                    if let Some(value) = line.strip_prefix(key) {
                        let value = value.trim().trim_matches('"');
                        if !value.is_empty() {
                            return from_locale(value);
                        }
                    }
                }
            }
        }
        Lang::En
    }

    /// Jeu de textes de cette langue.
    pub fn texts(self) -> &'static Texts {
        match self {
            Lang::Fr => &FR,
            Lang::En => &EN,
        }
    }
}

/// Interprète une valeur de locale (`fr_FR.UTF-8`, `en_US`, `C`, …).
fn from_locale(value: &str) -> Lang {
    let primary = value.split(['.', '@']).next().unwrap_or("");
    let code = primary.split(['_', '-']).next().unwrap_or("");
    if code.eq_ignore_ascii_case("fr") {
        Lang::Fr
    } else {
        Lang::En
    }
}

/// Remplace les `{}` de `template` par `args`, dans l'ordre.
pub fn fill(template: &str, args: &[&str]) -> String {
    let mut out = String::with_capacity(template.len() + 32);
    let mut rest = template;
    for arg in args {
        match rest.split_once("{}") {
            Some((before, after)) => {
                out.push_str(before);
                out.push_str(arg);
                rest = after;
            }
            None => break,
        }
    }
    out.push_str(rest);
    out
}

/// Tous les textes visibles de l'outil, dans une langue donnée.
pub struct Texts {
    /// Aide complète (`--help`).
    pub help: &'static str,
    /// Unités de taille : octets, Kio, Mio, Gio.
    pub units: [&'static str; 4],

    // ---- lignes d'information ----
    pub info_source: &'static str,
    pub info_members: &'static str,
    pub info_package: &'static str,
    pub info_progress: &'static str,
    pub info_deps_kept: &'static str,
    pub info_deps_dropped: &'static str,
    pub info_generation: &'static str,
    pub info_deps_none: &'static str,
    pub info_deps_header: &'static str,
    pub info_scripts_header: &'static str,
    pub info_scripts_footer: &'static str,
    pub info_install: &'static str,

    // ---- avertissements ----
    pub warn_not_deb: &'static str,
    pub warn_multi_data: &'static str,
    pub warn_entry_skipped: &'static str,
    pub warn_abs_link: &'static str,
    pub warn_arch_dep: &'static str,
    pub warn_foreign_arch: &'static str,

    // ---- erreurs ----
    pub err_generic: &'static str,
    pub err_unknown_option: &'static str,
    pub err_extra_arg: &'static str,
    pub err_missing_deb: &'static str,
    pub err_deb_not_found: &'static str,
    pub err_invalid_color: &'static str,
    pub err_install_failed: &'static str,
    pub err_sudo_spawn: &'static str,
    pub err_no_control_member: &'static str,
    pub err_no_data_member: &'static str,
    pub err_no_package_field: &'static str,
    pub err_no_files: &'static str,
    pub err_unsupported_compression: &'static str,
    pub err_no_control_file: &'static str,
    pub err_not_ar: &'static str,
    pub err_bad_member_size: &'static str,
    /// Rappel affiché après une erreur d'usage.
    pub hint_help: &'static str,
    /// Préfixe des avertissements (`avertissement : {}` / `warning: {}`).
    pub warn_prefix: &'static str,
}

static FR: Texts = Texts {
    help: "\
deb-to-eopkg — convertit un paquet Debian (.deb) en paquet Solus (.eopkg).

USAGE :
    deb-to-eopkg [OPTIONS] <paquet.deb> [destination]

ARGUMENTS :
    <paquet.deb>    le fichier .deb à convertir
    [destination]   un dossier (le nom du .eopkg est déduit du paquet)
                    ou le chemin complet du fichier .eopkg à produire.
                    Par défaut : le dossier du .deb.

OPTIONS :
    -i, --install   installe le paquet avec eopkg après la conversion
                    (`sudo eopkg install <fichier>`). La conversion doit avoir
                    réussi. eopkg demande confirmation, vous pouvez donc
                    renoncer à l'installation. À réserver aux systèmes Solus :
                    sudo est requis, donc le mot de passe peut être demandé.
    --deps          conserve les dépendances Depends du .deb dans l'eopkg.
                    Par défaut elles sont ignorées : les noms de paquets
                    Debian (libc6) ne correspondent pas à ceux de Solus
                    (glibc), et un paquet installé depuis une URL ne peut pas
                    résoudre ses dépendances. La liste reste affichée à la fin
                    de la conversion, pour information.
    -q, --quiet     mode silencieux : n'affiche ni progression ni
                    avertissements. Le chemin du fichier produit reste
                    affiché sur la sortie standard.
    --color WHEN    coloration : auto (défaut, seulement si la sortie est un
                    terminal), always ou never. Respecte NO_COLOR et TERM=dumb.
    -h, --help      affiche cette aide
    -V, --version   affiche la version

La conversion est verbeuse par défaut : elle indique le fichier source, les
membres lus et une progression régulière (elle peut durer plusieurs minutes).
Les dépendances du .deb et, s'il y en a, le CONTENU de ses scripts de
maintenance sont affichés à la fin. Ces scripts ne sont PAS reportés : eopkg
n'exécute aucun script à l'installation (le mécanisme COMAR de pisi n'existe
plus sur Solus).

EXEMPLES :
    deb-to-eopkg mon-logiciel.deb
    # -> ./mon-logiciel-1.2.3-1-1-x86_64.eopkg (à côté du .deb)
    deb-to-eopkg mon-logiciel.deb /tmp/
    # -> /tmp/mon-logiciel-1.2.3-1-1-x86_64.eopkg
    deb-to-eopkg --install mon-logiciel.deb   # ... puis l'installe
",
    units: ["o", "Kio", "Mio", "Gio"],

    info_source: "source : {} ({})",
    info_members: "membres : {} ({}) et {} ({})",
    info_package: "paquet {} {}, architecture {} -> {}",
    info_progress: "{} fichiers, {} écrits",
    info_deps_kept: "dépendances Depends conservées dans l'eopkg",
    info_deps_dropped: "dépendances Depends ignorées (--deps pour les conserver)",
    info_generation: "génération : {} ({} fichiers, {} de données, arch={})",
    info_deps_none: "dépendances du .deb : aucune",
    info_deps_header: "dépendances du .deb ({}) :",
    info_scripts_header: "scripts de maintenance du .deb ({}), NON reportables :",
    info_scripts_footer: "eopkg ne lance aucun script à l'installation (le mécanisme \
                          COMAR n'existe plus sur Solus) : ces scripts sont ignorés.",
    info_install: "installation : sudo eopkg install {}",

    warn_not_deb: "avertissement : {} ne finit pas par .deb",
    warn_multi_data: "plusieurs membres data.tar.*, on utilise {}",
    warn_entry_skipped: "entrée ignorée (type {}) : {}",
    warn_abs_link: "lien absolu normalisé en relatif : {} -> {}",
    warn_arch_dep: "dépendance par architecture ignorée : {}",
    warn_foreign_arch: "le .deb vise l'architecture « {} » mais le paquet est étiqueté \
                        x86_64 : la charge utile peut ne pas fonctionner sur Solus",

    err_generic: "erreur : {}",
    err_unknown_option: "erreur : option inconnue : {}",
    err_extra_arg: "erreur : argument en trop : {}",
    err_missing_deb: "erreur : il faut un fichier .deb à convertir.",
    err_deb_not_found: "erreur : fichier .deb introuvable : {}",
    err_invalid_color: "erreur : --color attend auto, always ou never.",
    err_install_failed: "erreur : `sudo eopkg install` a échoué (code {})",
    err_sudo_spawn: "erreur : impossible de lancer `sudo` : {}",
    err_no_control_member: "pas de membre control.tar.* dans le .deb",
    err_no_data_member: "pas de membre data.tar.* dans le .deb",
    err_no_package_field: "champ Package absent du .deb",
    err_no_files: "aucun fichier dans data.tar.* du .deb",
    err_unsupported_compression: "compression non prise en charge pour le membre {}",
    err_no_control_file: "fichier `control` introuvable dans control.tar.*",
    err_not_ar: "ce n'est pas une archive `ar` (signature \"!<arch>\\n\" absente)",
    err_bad_member_size: "taille de membre `ar` invalide ({}) pour {}",
    hint_help: "Essayez `deb-to-eopkg --help`.",
    warn_prefix: "avertissement : {}",
};

static EN: Texts = Texts {
    help: "\
deb-to-eopkg — convert a Debian package (.deb) into a Solus package (.eopkg).

USAGE:
    deb-to-eopkg [OPTIONS] <package.deb> [destination]

ARGUMENTS:
    <package.deb>   the .deb file to convert
    [destination]   a directory (the .eopkg name is derived from the package)
                    or the full path of the .eopkg file to produce.
                    Default: the directory holding the .deb.

OPTIONS:
    -i, --install   install the package with eopkg after the conversion
                    (`sudo eopkg install <file>`). The conversion must have
                    succeeded. eopkg then asks for confirmation, so you can
                    still back out. Meant for Solus systems: sudo is required,
                    so your password may be asked.
    --deps          keep the .deb's Depends dependencies in the eopkg. They are
                    dropped by default: Debian package names (libc6) do not
                    match Solus ones (glibc), and a package installed from a
                    URL cannot resolve its dependencies. The list is still
                    printed at the end of the conversion, for information.
    -q, --quiet     quiet mode: no progress and no warnings. The produced file
                    path is still printed on standard output.
    --color WHEN    colouring: auto (default, only when the output is a
                    terminal), always or never. Honours NO_COLOR and TERM=dumb.
    -h, --help      show this help
    -V, --version   show the version

The conversion is verbose by default: it reports the source file, the members
read and regular progress (it can take several minutes). The .deb dependencies
and, if any, the CONTENT of its maintainer scripts are printed at the end.
Those scripts are NOT carried over: eopkg runs no script at install time
(pisi's COMAR mechanism no longer exists on Solus).

EXAMPLES:
    deb-to-eopkg my-app.deb
    # -> ./my-app-1.2.3-1-1-x86_64.eopkg (next to the .deb)
    deb-to-eopkg my-app.deb /tmp/
    # -> /tmp/my-app-1.2.3-1-1-x86_64.eopkg
    deb-to-eopkg --install my-app.deb   # ... then installs it
",
    units: ["B", "KiB", "MiB", "GiB"],

    info_source: "source: {} ({})",
    info_members: "members: {} ({}) and {} ({})",
    info_package: "package {} {}, architecture {} -> {}",
    info_progress: "{} files, {} written",
    info_deps_kept: "Depends dependencies kept in the eopkg",
    info_deps_dropped: "Depends dependencies dropped (--deps to keep them)",
    info_generation: "generated: {} ({} files, {} of data, arch={})",
    info_deps_none: ".deb dependencies: none",
    info_deps_header: ".deb dependencies ({}):",
    info_scripts_header: ".deb maintainer scripts ({}), NOT transferable:",
    info_scripts_footer: "eopkg runs no script at install time (pisi's COMAR mechanism \
                          no longer exists on Solus): these scripts are ignored.",
    info_install: "installing: sudo eopkg install {}",

    warn_not_deb: "warning: {} does not end with .deb",
    warn_multi_data: "several data.tar.* members, using {}",
    warn_entry_skipped: "ignored entry (type {}): {}",
    warn_abs_link: "absolute link made relative: {} -> {}",
    warn_arch_dep: "architecture-qualified dependency ignored: {}",
    warn_foreign_arch: "the .deb targets architecture \"{}\" but the package is tagged \
                        x86_64: the payload may not work on Solus",

    err_generic: "error: {}",
    err_unknown_option: "error: unknown option: {}",
    err_extra_arg: "error: unexpected extra argument: {}",
    err_missing_deb: "error: a .deb file to convert is required.",
    err_deb_not_found: "error: .deb file not found: {}",
    err_invalid_color: "error: --color expects auto, always or never.",
    err_install_failed: "error: `sudo eopkg install` failed (code {})",
    err_sudo_spawn: "error: could not run `sudo`: {}",
    err_no_control_member: "no control.tar.* member in the .deb",
    err_no_data_member: "no data.tar.* member in the .deb",
    err_no_package_field: "Package field missing from the .deb",
    err_no_files: "no file in the .deb's data.tar.*",
    err_unsupported_compression: "unsupported compression for member {}",
    err_no_control_file: "`control` file not found in control.tar.*",
    err_not_ar: "not an `ar` archive (missing \"!<arch>\\n\" magic)",
    err_bad_member_size: "invalid `ar` member size ({}) for {}",
    hint_help: "Try `deb-to-eopkg --help`.",
    warn_prefix: "warning: {}",
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locales_reconnues() {
        assert_eq!(from_locale("fr_FR.UTF-8"), Lang::Fr);
        assert_eq!(from_locale("fr"), Lang::Fr);
        assert_eq!(from_locale("fr-CA"), Lang::Fr);
        assert_eq!(from_locale("en_US.UTF-8"), Lang::En);
        assert_eq!(from_locale("C"), Lang::En);
        assert_eq!(from_locale("POSIX"), Lang::En);
        assert_eq!(from_locale("de_DE"), Lang::En);
        assert_eq!(from_locale(""), Lang::En);
    }

    #[test]
    fn remplissage_des_gabarits() {
        assert_eq!(fill("a {} b {}", &["1", "2"]), "a 1 b 2");
        assert_eq!(fill("rien", &["x"]), "rien");
        assert_eq!(fill("{} seul", &["v"]), "v seul");
        // Plus d'arguments que de trous : le surplus est ignoré.
        assert_eq!(fill("{}", &["v", "ignore"]), "v");
        // Les accolades non vides ne sont pas touchées.
        assert_eq!(fill("{x}", &["v"]), "{x}");
    }

    #[test]
    fn chaque_langue_a_tous_ses_textes() {
        for texts in [Lang::Fr.texts(), Lang::En.texts()] {
            assert!(texts.help.contains("deb-to-eopkg"));
            assert_eq!(texts.units.len(), 4);
        }
        // Les deux aides ne sont pas identiques.
        assert_ne!(FR.help, EN.help);
        assert_ne!(FR.units, EN.units);
    }

    #[test]
    fn gabarits_coherents_entre_langues() {
        // Même nombre de `{}` de chaque côté : une traduction ne doit pas
        // perdre ni ajouter d'argument.
        let count = |s: &str| s.matches("{}").count();
        for (fr, en) in [
            (FR.info_source, EN.info_source),
            (FR.info_members, EN.info_members),
            (FR.info_package, EN.info_package),
            (FR.info_progress, EN.info_progress),
            (FR.info_generation, EN.info_generation),
            (FR.info_deps_header, EN.info_deps_header),
            (FR.info_scripts_header, EN.info_scripts_header),
            (FR.info_install, EN.info_install),
            (FR.warn_not_deb, EN.warn_not_deb),
            (FR.warn_multi_data, EN.warn_multi_data),
            (FR.warn_entry_skipped, EN.warn_entry_skipped),
            (FR.warn_abs_link, EN.warn_abs_link),
            (FR.warn_arch_dep, EN.warn_arch_dep),
            (FR.warn_foreign_arch, EN.warn_foreign_arch),
            (FR.err_generic, EN.err_generic),
            (FR.err_unknown_option, EN.err_unknown_option),
            (FR.err_extra_arg, EN.err_extra_arg),
            (FR.err_deb_not_found, EN.err_deb_not_found),
            (FR.err_install_failed, EN.err_install_failed),
            (FR.err_sudo_spawn, EN.err_sudo_spawn),
            (FR.err_unsupported_compression, EN.err_unsupported_compression),
            (FR.err_bad_member_size, EN.err_bad_member_size),
        ] {
            assert_eq!(count(fr), count(en), "gabarits différents : {fr:?} / {en:?}");
        }
    }
}
