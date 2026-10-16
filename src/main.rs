//! `deb-to-eopkg` — convertit un paquet Debian (`.deb`) en paquet Solus
//! (`.eopkg`).
//!
//! Usage :
//!
//! ```text
//! deb-to-eopkg paquet.deb
//! deb-to-eopkg paquet.deb destination/
//! deb-to-eopkg paquet.deb chemin/vers/paquet.eopkg
//! ```
//!
//! Sans destination, le `.eopkg` est écrit dans le dossier du `.deb`.
//!
//! Les messages suivent la langue du système : français ou anglais (voir
//! `deb_to_eopkg::i18n`).

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use deb_to_eopkg::{color, convert, i18n, TOOL_NAME, VERSION};

fn main() -> ExitCode {
    let lang = i18n::Lang::detect();
    let texts = lang.texts();

    let mut deb = None;
    let mut dest = None;
    let mut keep_deps = false;
    let mut quiet = false;
    let mut install = false;
    let mut color = color::Choice::Auto;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print!("{}", texts.help);
                return ExitCode::SUCCESS;
            }
            "-V" | "--version" => {
                println!("{TOOL_NAME} {VERSION}");
                return ExitCode::SUCCESS;
            }
            "--deps" => keep_deps = true,
            "-i" | "-it" | "--install" => install = true,
            "-q" | "--quiet" => quiet = true,
            "--color" => match args.next().as_deref().map(str::parse::<color::Choice>) {
                Some(Ok(choice)) => color = choice,
                _ => {
                    eprintln!("{TOOL_NAME} : {}", texts.err_invalid_color);
                    return ExitCode::from(2);
                }
            },
            _ if arg.starts_with("--color=") => {
                match arg["--color=".len()..].parse::<color::Choice>() {
                    Ok(choice) => color = choice,
                    Err(()) => {
                        eprintln!("{TOOL_NAME} : {}", texts.err_invalid_color);
                        return ExitCode::from(2);
                    }
                }
            }
            _ if arg.starts_with('-') && arg != "-" => {
                eprintln!(
                    "{TOOL_NAME} : {}",
                    i18n::fill(texts.err_unknown_option, &[&arg])
                );
                eprintln!("{TOOL_NAME} : {}", texts.hint_help);
                return ExitCode::from(2);
            }
            _ => {
                if deb.is_none() {
                    deb = Some(arg);
                } else if dest.is_none() {
                    dest = Some(arg);
                } else {
                    eprintln!("{TOOL_NAME} : {}", i18n::fill(texts.err_extra_arg, &[&arg]));
                    eprintln!("{TOOL_NAME} : {}", texts.hint_help);
                    return ExitCode::from(2);
                }
            }
        }
    }

    let Some(deb) = deb else {
        eprintln!("{TOOL_NAME} : {}", texts.err_missing_deb);
        eprintln!("{TOOL_NAME} : {}", texts.hint_help);
        return ExitCode::from(2);
    };

    let deb_path = Path::new(&deb);
    if !deb_path.is_file() {
        eprintln!(
            "{TOOL_NAME} : {}",
            i18n::fill(texts.err_deb_not_found, &[&deb])
        );
        return ExitCode::from(2);
    }
    if !deb.to_lowercase().ends_with(".deb") && !quiet {
        eprintln!("{TOOL_NAME} : {}", i18n::fill(texts.warn_not_deb, &[&deb]));
    }

    // Sans destination, on écrit à côté du `.deb`.
    let dest = match dest {
        Some(dest) => PathBuf::from(dest),
        None => default_destination(deb_path),
    };

    let opts = convert::Options {
        keep_deps,
        quiet,
        lang,
    };
    // Couleurs : une par type de script, une pour les dépendances.
    let palette = color::Palette::for_stderr(color);

    let summary = match convert::convert(deb_path, &dest, opts) {
        Ok(summary) => summary,
        Err(err) => {
            let err = err.to_string();
            eprintln!("{TOOL_NAME} : {}", i18n::fill(texts.err_generic, &[&err]));
            return ExitCode::from(1);
        }
    };

    if !quiet {
        eprintln!(
            "{TOOL_NAME} : {}",
            i18n::fill(
                texts.info_generation,
                &[
                    &summary.out.display().to_string(),
                    &summary.files.to_string(),
                    &convert::human_size(summary.bytes, lang),
                    &summary.arch,
                ]
            )
        );
        // La liste des dépendances du .deb, pour information : elle n'est pas
        // inscrite dans l'eopkg sans `--deps`, mais elle est utile à l'œil.
        if summary.deps.is_empty() {
            eprintln!("{TOOL_NAME} : {}", texts.info_deps_none);
        } else {
            eprintln!(
                "{TOOL_NAME} : {}",
                i18n::fill(texts.info_deps_header, &[&summary.deps.len().to_string()])
            );
            for dep in &summary.deps {
                match &dep.constraint {
                    Some(constraint) => eprintln!(
                        "    {}",
                        palette.dependency(&format!("{} ({constraint})", dep.name))
                    ),
                    None => eprintln!("    {}", palette.dependency(&dep.name)),
                }
            }
        }
        // Scripts de maintenance : eopkg n'exécute aucun script à
        // l'installation, ils ne peuvent pas être reportés. On affiche leur
        // contenu en entier pour que l'utilisateur voie précisément ce qu'il
        // perd (et puisse le reprendre à la main si nécessaire).
        if !summary.scripts.is_empty() {
            eprintln!(
                "{TOOL_NAME} : {}",
                i18n::fill(
                    texts.info_scripts_header,
                    &[&summary.scripts.len().to_string()]
                )
            );
            for script in &summary.scripts {
                let header = format!(
                    "--- {} ({}) ---",
                    script.name,
                    convert::human_size(script.size() as u64, lang)
                );
                eprintln!(
                    "{TOOL_NAME} : {}",
                    palette.script_header(&script.name, &header)
                );
                let text = script.text();
                // On retire le saut de ligne final avant de colorer : la
                // séquence de remise à zéro reste ainsi sur la dernière ligne
                // du script au lieu de flotter sur la ligne suivante.
                let body = text.strip_suffix('\n').unwrap_or(&text);
                eprintln!("{}", palette.script_body(&script.name, body));
            }
            eprintln!("{TOOL_NAME} : {}", texts.info_scripts_footer);
        }
    }
    // Le chemin produit sur la sortie standard, pour l'enchaîner dans
    // un script (`out=$(deb-to-eopkg …)`).
    println!("{}", summary.out.display());

    if install {
        return install_package(&summary.out, quiet, texts);
    }

    ExitCode::SUCCESS
}

/// Destination par défaut : le dossier qui contient le `.deb`.
///
/// `Path::parent()` renvoie une chaîne vide pour un fichier situé dans le
/// dossier courant (`deb-to-eopkg paquet.deb`) — on écrit alors dans `.`.
fn default_destination(deb_path: &Path) -> PathBuf {
    deb_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Installe le `.eopkg` fraîchement produit avec `sudo eopkg install`.
///
/// On ne passe volontairement pas `-y` : eopkg affiche alors sa question de
/// confirmation et l'utilisateur peut renoncer à l'installation. La sortie
/// d'eopkg est reliée au terminal, donc la question reste interactive même
/// quand notre propre sortie est en mode `--quiet`.
fn install_package(eopkg: &Path, quiet: bool, texts: &i18n::Texts) -> ExitCode {
    if !quiet {
        eprintln!(
            "{TOOL_NAME} : {}",
            i18n::fill(texts.info_install, &[&eopkg.display().to_string()])
        );
    }
    match Command::new("sudo")
        .arg("eopkg")
        .arg("install")
        .arg(eopkg)
        .status()
    {
        Ok(status) if status.success() => ExitCode::SUCCESS,
        Ok(status) => {
            let code = status.code().unwrap_or(-1).to_string();
            eprintln!(
                "{TOOL_NAME} : {}",
                i18n::fill(texts.err_install_failed, &[&code])
            );
            ExitCode::from(1)
        }
        Err(err) => {
            let err = err.to_string();
            eprintln!("{TOOL_NAME} : {}", i18n::fill(texts.err_sudo_spawn, &[&err]));
            ExitCode::from(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn destination_par_defaut_est_le_dossier_du_deb() {
        assert_eq!(
            default_destination(Path::new("/paquets/mon-logiciel.deb")),
            PathBuf::from("/paquets")
        );
        assert_eq!(
            default_destination(Path::new("/tmp/a/b/c.deb")),
            PathBuf::from("/tmp/a/b")
        );
    }

    #[test]
    fn destination_par_defaut_dans_le_dossier_courant() {
        // `parent()` donne "" pour un fichier sans dossier : on écrit dans `.`.
        assert_eq!(
            default_destination(Path::new("mon-logiciel.deb")),
            PathBuf::from(".")
        );
        assert_eq!(
            default_destination(Path::new("./mon-logiciel.deb")),
            PathBuf::from(".")
        );
    }
}
