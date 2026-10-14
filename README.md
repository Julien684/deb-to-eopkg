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

## Installation

Le paquet est autonome, il s'installe tel quel :

```bash
sudo eopkg install https://depoa.julien68.fr/Linuxon/deb-to-eopkg/raw/branch/main/deb-to-eopkg-2.2.0-1-1-x86_64.eopkg
```

Vérifier :

```bash
which deb-to-eopkg
deb-to-eopkg --help
```

## Utilisation

```
deb-to-eopkg paquet.deb destination/
deb-to-eopkg paquet.deb chemin/vers/paquet.eopkg
```

- `paquet.deb` : le fichier `.deb` à convertir.
- `destination` : un **dossier** (l'outil y génère
  `nom-version-release-arch.eopkg`) **ou** le **chemin complet** du fichier
  `.eopkg` à produire.

Exemple :

```bash
deb-to-eopkg mon-logiciel.deb /tmp/
# -> /tmp/mon-logiciel-1.2.3-1-1-x86_64.eopkg
```

Puis installer le résultat :

```bash
sudo eopkg install -y /tmp/mon-logiciel-1.2.3-1-1-x86_64.eopkg
```

## Options

- `-i`, `--install` : lance l'installation après la conversion, avec
  `sudo eopkg install -y <fichier>`. La conversion doit avoir réussi. À
  réserver aux systèmes Solus ; `sudo` peut demander le mot de passe.

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
```

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
