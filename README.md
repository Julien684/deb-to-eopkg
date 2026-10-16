# deb-to-eopkg

Un petit outil en **Rust** qui convertit un paquet Debian (`.deb`) en paquet
Solus (`.eopkg`) installable.

Il lit directement la structure `ar` + `tar` d'un `.deb` (aucun besoin de
`dpkg-deb`) — `data.tar` compressé en **gzip, xz, bzip2, lzma ou zstd** — puis
écrit un `.eopkg` conforme : une archive ZIP de trois membres (`metadata.xml`,
`files.xml`, `install.tar.xz`), avec pour chaque fichier le bon type
(`executable`, `doc`, `library`, `config`, …), le mode, l'uid/gid et le hash
SHA1. Les liens symboliques et physiques sont conservés.

Le binaire est **statique** (musl) : le paquet n'a aucune dépendance
d'exécution, ce qui est indispensable pour une installation depuis une URL.

Les messages existent en **français et en anglais** et suivent la langue du
système (`LC_ALL`, `LC_MESSAGES`, `LANG`, puis `/etc/locale.conf` ; l'anglais
sert de repli).

## Installation

Le paquet est autonome, il s'installe tel quel :

```bash
sudo eopkg install https://depoa.julien68.fr/Linuxon/deb-to-eopkg/raw/branch/main/deb-to-eopkg-2.6.0-6-1-x86_64.eopkg
```

Vérifier :

```bash
which deb-to-eopkg
deb-to-eopkg --help
```

## Utilisation

```
deb-to-eopkg paquet.deb
deb-to-eopkg paquet.deb destination/
deb-to-eopkg paquet.deb chemin/vers/paquet.eopkg
```

- `paquet.deb` : le fichier `.deb` à convertir.
- `destination` *(facultative)* : un **dossier** (l'outil y génère
  `nom-version-release-arch.eopkg`) **ou** le **chemin complet** du fichier
  `.eopkg` à produire. **Sans destination, le `.eopkg` est écrit dans le dossier
  du `.deb`.**

Exemple :

```bash
deb-to-eopkg mon-logiciel.deb
# -> ./mon-logiciel-1.2.3-1-1-x86_64.eopkg (à côté du .deb)

deb-to-eopkg mon-logiciel.deb /tmp/
# -> /tmp/mon-logiciel-1.2.3-1-1-x86_64.eopkg
```

Puis installer le résultat :

```bash
sudo eopkg install /tmp/mon-logiciel-1.2.3-1-1-x86_64.eopkg
```

## Options

- `-i`, `--install` : lance l'installation après la conversion, avec
  `sudo eopkg install <fichier>`. La conversion doit avoir réussi. eopkg
  affiche sa question de confirmation, vous pouvez donc renoncer à
  l'installation. À réserver aux systèmes Solus ; `sudo` peut demander le mot
  de passe.

- `--color WHEN` : `auto` (défaut), `always` ou `never`. En mode `auto`, les
  couleurs ne sont émises que si la sortie est un terminal, et jamais si
  `NO_COLOR` est définie ou `TERM=dumb`. Chaque type de script de maintenance a
  sa couleur (`preinst` cyan, `postinst` vert, `prerm` jaune, `postrm` magenta)
  et les dépendances sont en bleu.

- `-q`, `--quiet` : mode silencieux. Par défaut la conversion est **verbeuse**
  (fichier source, membres lus, progression régulière) — utile car une grosse
  conversion peut durer plusieurs minutes ; `--quiet` ne laisse passer que les
  erreurs. Le chemin du fichier produit reste affiché sur la sortie standard,
  ce qui permet de l'enchaîner :

  ```bash
  out=$(deb-to-eopkg --quiet mon-logiciel.deb /tmp/)
  ```

- `--deps` : conserve les dépendances `Depends` du `.deb` dans l'eopkg.
  Par défaut elles sont **ignorées** : les noms de paquets Debian (ex : `libc6`)
  ne correspondent pas aux noms eopkg de Solus (ex : `glibc`), et un paquet
  installé depuis une URL ne peut pas résoudre ses dépendances. C'est donc le
  comportement le plus sûr par défaut. La liste est de toute façon affichée à la
  fin de la conversion, pour information :

  ```
  deb-to-eopkg : dépendances du .deb (9) :
      libc6 (>= 2.38)
      zlib1g (>= 1:1.2.2)
      perl
      ...
  ```

- `-h`, `--help` / `-V`, `--version`.

## Compilation

Rust stable, aucune bibliothèque système requise (les sources C de xz, bzip2 et
zstd sont compilées statiquement par cargo) :

```bash
# binaire statique, portable
rustup target add x86_64-unknown-linux-musl   # une seule fois
cargo build --release --target x86_64-unknown-linux-musl

# paquet .eopkg installable (déposé à la racine : c'est l'artefact publié)
./target/x86_64-unknown-linux-musl/release/pack-eopkg \
    target/x86_64-unknown-linux-musl/release/deb-to-eopkg .

# pour republier une même version, le release doit augmenter (voir ci-dessous)
./target/x86_64-unknown-linux-musl/release/pack-eopkg --release 3 \
    target/x86_64-unknown-linux-musl/release/deb-to-eopkg .
```

### ⚠️ Le *release* doit augmenter à chaque publication

Le nom du paquet est `deb-to-eopkg-<version>-<release>-1-x86_64.eopkg`, et
**eopkg ne compare que le release**, jamais la chaîne de version
(`pisi/operations/upgrade.py` : `int(release_installe) < int(release_depot)` ;
le numéro est lu depuis l'attribut `release` de `<Update>` dans `<History>`).
Republier en laissant `release` inchangé — même avec une version supérieure —
rend le paquet invisible à `sudo eopkg up` : eopkg répond alors
« already at the latest release ». C'est pourquoi la publication d'un nouveau
build doit toujours incrémenter ce numéro (ou passer `--release N`).

Tests unitaires :

```bash
cargo test
```

Contrôle croisé d'une conversion (le `.eopkg` correspond-il au `.deb` source ?
chemins, types, modes, SHA1, cibles de liens) :

```bash
python3 tools/verify_deb_vs_eopkg.py mon-logiciel.deb /tmp/mon-logiciel-1.2.3-1-1-x86_64.eopkg
```

## Limites connues

- **Les scripts de maintenance du `.deb` ne sont pas reportés** (`preinst`,
  `postinst`, `prerm`, `postrm`). Ils sont détectés et **affichés en entier**
  (chacun dans sa couleur) à la fin de la conversion, mais eopkg **n'exécute
  aucun script à l'installation** : le seul mécanisme prévu par pisi (COMAR,
  scripts Python déclarés dans `<Provides><COMAR>` et enregistrés via D-Bus)
  n'existe plus sur Solus — aucun paquet `comar` dans le dépôt, démon absent,
  et le module qui les lançait (`pisi/comariface.py`) n'est plus appelé. Un
  paquet converti se comporte donc comme un simple paquet de fichiers.
- **Le paquet produit est toujours étiqueté `x86_64`.** Solus n'utilise pas
  `noarch` (aucun paquet de son dépôt, aucun paquet installé) et refuse
  d'installer un paquet ainsi étiqueté :
  `Package X (noarch) is not compatible with your x86_64 architecture.`
  Un `.deb` en `Architecture: all` (polices, documentation, données, pas mal
  d'applications Java/Python) est donc étiqueté `x86_64` pour rester
  installable. Si la charge utile visait réellement un autre CPU (arm64,
  i386…), un avertissement est émis : le contenu ne fonctionnera pas sur Solus.
- Les **noms de dépendances** ne sont pas traduits (Debian → Solus), d'où le
  comportement par défaut qui les supprime.
- Les dépendances par architecture (`arch:nom`) sont ignorées.
- Les **répertoires** du `.deb` ne sont pas listés dans `files.xml` : ils sont
  créés à l'extraction du chargeur. Un répertoire vide n'est donc pas préservé.
- Les permissions spéciales (setuid/setgid) sont conservées telles quelles.
- Le texte des métadonnées est repris tel quel du `.deb` : une description
  non-ASCII peut faire échouer `eopkg info` sur un système dont la locale n'est
  pas UTF-8 (l'installation, elle, fonctionne). Les métadonnées du paquet
  `deb-to-eopkg` lui-même sont en ASCII pour cette raison.

## Licence

GPL v3. Voir `LICENSE`.

---

# deb-to-eopkg (English)

A small **Rust** tool that converts a Debian package (`.deb`) into an
installable Solus package (`.eopkg`).

It reads the `ar` + `tar` structure of the `.deb` directly (no `dpkg-deb`
needed) — `data.tar` compressed with **gzip, xz, bzip2, lzma or zstd** — then
writes a conformant `.eopkg`: a ZIP of three members (`metadata.xml`,
`files.xml`, `install.tar.xz`), carrying the right type for every file
(`executable`, `doc`, `library`, `config`, …), its mode, uid/gid and SHA1
hash. Symlinks and hard links are preserved.

The binary is **static** (musl): the package has no runtime dependency, which
is required to install it from a URL.

Messages exist in **French and English** and follow the system language
(`LC_ALL`, `LC_MESSAGES`, `LANG`, then `/etc/locale.conf`; English is the
fallback).

## Installation

The package is self-contained, it installs as-is:

```bash
sudo eopkg install https://depoa.julien68.fr/Linuxon/deb-to-eopkg/raw/branch/main/deb-to-eopkg-2.6.0-6-1-x86_64.eopkg
```

Check it:

```bash
which deb-to-eopkg
deb-to-eopkg --help
```

## Usage

```
deb-to-eopkg package.deb
deb-to-eopkg package.deb destination/
deb-to-eopkg package.deb path/to/package.eopkg
```

- `package.deb`: the `.deb` file to convert.
- `destination` *(optional)*: a **directory** (the tool writes
  `name-version-release-arch.eopkg` inside it) **or** the **full path** of the
  `.eopkg` to produce. **With no destination, the `.eopkg` is written next to
  the `.deb`.**

```bash
deb-to-eopkg my-app.deb
# -> ./my-app-1.2.3-1-1-x86_64.eopkg (next to the .deb)

deb-to-eopkg my-app.deb /tmp/
# -> /tmp/my-app-1.2.3-1-1-x86_64.eopkg
```

Then install the result:

```bash
sudo eopkg install /tmp/my-app-1.2.3-1-1-x86_64.eopkg
```

## Options

- `-i`, `--install`: run the installation after the conversion, with
  `sudo eopkg install <file>`. The conversion must have succeeded. eopkg then
  asks for confirmation, so you can still back out. Meant for Solus systems;
  `sudo` may ask for your password.
- `--color WHEN`: `auto` (default), `always` or `never`. In `auto` mode colours
  are only emitted when the output is a terminal, and never when `NO_COLOR` is
  set or `TERM=dumb`. Each maintainer script has its own colour (`preinst`
  cyan, `postinst` green, `prerm` yellow, `postrm` magenta) and dependencies
  are blue.
- `-q`, `--quiet`: quiet mode. The conversion is **verbose** by default
  (source file, members read, regular progress) — useful because a large
  conversion can take several minutes; `--quiet` lets only errors through. The
  produced file path is still printed on standard output, so it can be
  chained:

  ```bash
  out=$(deb-to-eopkg --quiet my-app.deb /tmp/)
  ```

- `--deps`: keep the `.deb`'s `Depends` dependencies in the eopkg. They are
  **dropped** by default: Debian package names (e.g. `libc6`) do not match
  Solus eopkg names (e.g. `glibc`), and a package installed from a URL cannot
  resolve its dependencies. The list is printed at the end of the conversion
  either way, for information.
- `-h`, `--help` / `-V`, `--version`.

## Known limitations

- **The `.deb` maintainer scripts are not carried over** (`preinst`,
  `postinst`, `prerm`, `postrm`). They are detected and **printed in full**
  (each in its own colour) at the end of the conversion, but eopkg **runs no
  script at install time**: the only mechanism pisi provided (COMAR, Python
  scripts declared in `<Provides><COMAR>` and registered over D-Bus) no longer
  exists on Solus — no `comar` package in the repository, no daemon, and the
  module that used to run them (`pisi/comariface.py`) is never called. A
  converted package therefore behaves as a plain file package.
- **The produced package is always tagged `x86_64`.** Solus does not use
  `noarch` (no such package in its repository, none installed) and refuses to
  install a package tagged that way:
  `Package X (noarch) is not compatible with your x86_64 architecture.`
  An `Architecture: all` `.deb` (fonts, documentation, data, a fair number of
  Java/Python applications) is therefore tagged `x86_64` to stay installable.
  If the payload really targets another CPU (arm64, i386…), a warning is
  issued: the content will not work on Solus.
- **Dependency names** are not translated (Debian → Solus), hence the default
  that drops them.
- Architecture-qualified dependencies (`arch:name`) are ignored.
- The `.deb`'s **directories** are not listed in `files.xml`: they are created
  while extracting the payload, so an empty directory is not preserved.
- Special permissions (setuid/setgid) are kept as they are.
- Metadata text is taken verbatim from the `.deb`: a non-ASCII description can
  make `eopkg info` fail on a system whose locale is not UTF-8 (installing
  still works). The `deb-to-eopkg` package's own metadata is ASCII for that
  reason.

## License

GPL v3. See `LICENSE`.
