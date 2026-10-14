#!/usr/bin/env python3
"""Contrôle croisé d'une conversion : le .eopkg correspond-il au .deb source ?

Usage :
    python3 tools/verify_deb_vs_eopkg.py <source.deb> <produit.eopkg>

Outil de développement (indépendant du binaire Rust) : il déplie le .deb *et*
le .eopkg avec la seule bibliothèque standard et compare, fichier par fichier,
le chemin, le type, la taille, le mode, le SHA1 et la cible des liens. Il sert
à attraper les régressions de format sans passer par la VM Solus.

Sortie : un rapport sur stdout, code 0 si tout concorde, 1 sinon.
"""

import hashlib
import io
import re
import sys
import tarfile
import zipfile

AR_MAGIC = b"!<arch>\n"


def ar_members(path):
    """Membres d'une archive `ar`, dans l'ordre, sans les décompresser."""
    data = open(path, "rb").read()
    if data[:8] != AR_MAGIC:
        raise SystemExit(f"{path} : ce n'est pas une archive ar")
    out = {}
    off = 8
    while off + 60 <= len(data):
        head = data[off : off + 60].decode("ascii", "replace")
        # GNU ar termine le nom par « / » ; dpkg-deb n'écrit que des espaces.
        name = head[0:16].strip().rstrip("/")
        size = int(head[48:58].strip())
        out[name] = data[off + 60 : off + 60 + size]
        off += 60 + size
        if off % 2:
            off += 1
    return out


def _field(block, tag):
    """Valeur d'une balise XML, chaîne vide si absente."""
    found = re.search(rf"<{tag}>(.*?)</{tag}>", block, re.S)
    return found.group(1) if found else ""


def deb_entries(path):
    """Dictionnaire chemin -> (mode, taille, sha1, cible) pour un .deb."""
    members = ar_members(path)
    data_name = next(n for n in members if n.startswith("data.tar"))
    with tarfile.open(fileobj=io.BytesIO(members[data_name])) as tar:
        entries = {}
        for member in tar:
            if member.isdir():
                continue
            name = member.name.lstrip("./")
            if member.issym() or member.islnk():
                entries[name] = (member.mode & 0o7777, 0, None, member.linkname)
            elif member.isfile():
                handle = tar.extractfile(member)
                assert handle is not None
                blob = handle.read()
                entries[name] = (
                    member.mode & 0o7777,
                    member.size,
                    hashlib.sha1(blob).hexdigest(),
                    None,
                )
        return entries


def eopkg_entries(path):
    """Dictionnaire chemin -> (mode, taille, sha1, cible) pour un .eopkg."""
    archive = zipfile.ZipFile(path)
    assert archive.namelist() == [
        "metadata.xml",
        "files.xml",
        "install.tar.xz",
    ], f"membres inattendus : {archive.namelist()}"
    files_xml = archive.read("files.xml").decode()

    declared = {}
    for block in re.findall(r"<File>(.*?)</File>", files_xml, re.S):
        declared[_field(block, "Path")] = (
            int(_field(block, "Mode"), 8),
            int(_field(block, "Size")),
            _field(block, "Hash"),
        )

    with tarfile.open(fileobj=io.BytesIO(archive.read("install.tar.xz"))) as tar:
        payload = {}
        for member in tar:
            if member.isdir():
                continue
            mode, size, sha1 = declared[member.name]
            if member.issym() or member.islnk():
                payload[member.name] = (mode, size, sha1, member.linkname)
            else:
                handle = tar.extractfile(member)
                assert handle is not None
                got = hashlib.sha1(handle.read()).hexdigest()
                if got != sha1:
                    print(f"  SHA1 divergent : {member.name}")
                payload[member.name] = (mode, size, sha1, None)
        return payload


def main():
    if len(sys.argv) != 3:
        raise SystemExit(__doc__)
    deb, eopkg = sys.argv[1], sys.argv[2]

    want = deb_entries(deb)
    got = eopkg_entries(eopkg)
    print(f"deb: {len(want)} entrées   eopkg files.xml: {len(got)}   tar: {len(got)}")

    errors = []
    for name in sorted(set(want) - set(got)):
        errors.append(f"absent de l'eopkg : {name}")
    for name in sorted(set(got) - set(want)):
        errors.append(f"en trop dans l'eopkg : {name}")
    for name in sorted(set(want) & set(got)):
        w_mode, w_size, w_hash, w_link = want[name]
        g_mode, g_size, g_hash, g_link = got[name]
        if w_mode != g_mode:
            errors.append(f"{name} : mode {oct(w_mode)} != {oct(g_mode)}")
        if w_size != g_size:
            errors.append(f"{name} : taille {w_size} != {g_size}")
        if w_hash and w_hash != g_hash:
            errors.append(f"{name} : SHA1 {w_hash} != {g_hash}")
        if (w_link or None) != (g_link or None):
            errors.append(f"{name} : lien {w_link!r} != {g_link!r}")

    if errors:
        print("ERREURS :")
        for err in errors[:20]:
            print("  ", err)
        sys.exit(1)
    print("OK : le .eopkg correspond exactement au .deb")


if __name__ == "__main__":
    main()
