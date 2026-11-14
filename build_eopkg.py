#!/usr/bin/env python3
"""build_eopkg.py — paquet .eopkg de deb-to-eopkg (binaire Rust + metainfo + icones)."""
import hashlib
import io
import sys
import tarfile
import zipfile
from datetime import date
from pathlib import Path

SRC_DIR = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(".")
OUT_DIR = Path(sys.argv[2]) if len(sys.argv) > 2 else SRC_DIR

PACKAGE_NAME   = "deb-to-eopkg"
VERSION        = "2.7.4"
RELEASE        = 15
DIST_RELEASE   = 1
ARCH           = "x86_64"
DIST_NAME      = "Solus"
BUILD_HOST     = "solus-local"
HOME_PAGE      = "https://depoa.julien68.fr/Linuxon/deb-to-eopkg"
PACKAGER_NAME  = "Julien"
PACKAGER_EMAIL = "julien_solus@use.startmail.com"
PART_OF        = "system.utilities"

FILE_LIST = [
    ("usr/bin/deb-to-eopkg", "target/release/deb-to-eopkg", 0o755),
    ("usr/share/doc/deb-to-eopkg/README.md", "README.md", 0o644),
    ("usr/share/licenses/deb-to-eopkg/LICENSE", "LICENSE", 0o644),
    ("usr/share/metainfo/fr.julien68.DebToEopkg.metainfo.xml",
     "fr.julien68.DebToEopkg.metainfo.xml", 0o644),
    ("usr/share/icons/hicolor/24x24/apps/deb-to-eopkg.png", "deb-to-eopkg-24.png", 0o644),
    ("usr/share/icons/hicolor/48x48/apps/deb-to-eopkg.png", "deb-to-eopkg-48.png", 0o644),
    ("usr/share/icons/hicolor/128x128/apps/deb-to-eopkg.png", "deb-to-eopkg-128.png", 0o644),
    ("usr/share/icons/hicolor/256x256/apps/deb-to-eopkg.png", "deb-to-eopkg-256.png", 0o644),
    ("usr/share/applications/deb-to-eopkg.desktop", "deb-to-eopkg.desktop", 0o644),
]

RUNTIME_DEPS = []

SUMMARY_EN = """Convert a Debian .deb package into a Solus .eopkg package."""
SUMMARY_FR = """Convertit un paquet Debian (.deb) en paquet Solus (.eopkg)."""
DESC_EN = """deb-to-eopkg is a small Rust tool that converts a Debian .deb package into an installable Solus .eopkg package. It reads the ar+tar structure of the .deb directly (no dpkg-deb) - data.tar compressed with gzip, xz, bzip2, lzma or zstd - and writes a conformant .eopkg (ZIP of metadata.xml, files.xml and install.tar.xz), preserving file types, modes and symlinks/hardlinks. Static binary, no runtime dependencies. Usage: deb-to-eopkg package.deb destination/"""
DESC_FR = """deb-to-eopkg est un petit outil en Rust qui convertit un paquet Debian (.deb) en paquet Solus (.eopkg) installable. Il lit directement la structure ar+tar du .deb (sans dpkg-deb) — data.tar compressé en gzip, xz, bzip2, lzma ou zstd — et écrit un .eopkg conforme (ZIP de metadata.xml, files.xml et install.tar.xz), en préservant les types de fichiers, les modes et les liens symboliques. Binaire statique, aucune dépendance à l'exécution. Utilisation : deb-to-eopkg paquet.deb destination/"""

OUT_FILENAME = f"{PACKAGE_NAME}-{VERSION}-{RELEASE}-{DIST_RELEASE}-{ARCH}.eopkg"


def sha1_hex(data):
    return hashlib.sha1(data).hexdigest()


def file_type_for(path):
    if path.startswith(("usr/bin/", "bin/", "usr/sbin/", "usr/libexec/")):
        return "executable"
    if path.startswith(("usr/share/doc/", "usr/share/help/")):
        return "doc"
    return "data"


def build():
    entries = []
    for dest, src_rel, mode in FILE_LIST:
        src = SRC_DIR / src_rel
        if not src.exists():
            print(f"  WARNING: source manquante: {src}", file=sys.stderr)
            continue
        data = src.read_bytes()
        entries.append((dest, data, mode))
        print(f"  + {dest} ({len(data)} octets)")

    buf = io.BytesIO()
    with tarfile.open(fileobj=buf, mode="w:xz") as tar:
        for dest, data, mode in entries:
            info = tarfile.TarInfo(name=dest)
            info.size = len(data)
            info.mode = mode
            info.uid = info.gid = 0
            info.mtime = 0
            tar.addfile(info, io.BytesIO(data))
    install_tar_xz = buf.getvalue()

    parts = ["<Files>"]
    for dest, data, mode in entries:
        parts += ["  <File>", f"    <Path>{dest}</Path>",
                  f"    <Type>{file_type_for(dest)}</Type>",
                  f"    <Size>{len(data)}</Size>", "    <Uid>0</Uid>", "    <Gid>0</Gid>",
                  f"    <Mode>{mode:04o}</Mode>", f"    <Hash>{sha1_hex(data)}</Hash>",
                  "  </File>"]
    parts.append("</Files>")
    files_xml = "\n".join(parts).encode()

    today = date.today().isoformat()
    installed_size = sum(len(d) for _, d, _ in entries)
    deps_xml = ""
    if RUNTIME_DEPS:
        deps_xml = ("        <RuntimeDependencies>\n"
                    + "".join(f"            <Dependency>{d}</Dependency>\n" for d in RUNTIME_DEPS)
                    + "        </RuntimeDependencies>\n")

    def a(s):
        return s.encode("ascii", "replace").decode("ascii")

    metadata_xml = f"""<PISI>
    <Source>
        <Name>{PACKAGE_NAME}</Name>
        <Homepage>{HOME_PAGE}</Homepage>
        <Packager>
            <Name>{PACKAGER_NAME}</Name>
            <Email>{PACKAGER_EMAIL}</Email>
        </Packager>
    </Source>
    <Package>
        <Name>{PACKAGE_NAME}</Name>
        <Summary xml:lang="en">{a(SUMMARY_EN)}</Summary>
        <Summary xml:lang="fr">{SUMMARY_FR}</Summary>
        <Description xml:lang="en">{a(DESC_EN)}</Description>
        <Description xml:lang="fr">{DESC_FR}</Description>
        <PartOf>{PART_OF}</PartOf>
        <License>GPL-3.0</License>
{deps_xml}        <History>
            <Update release="{RELEASE}">
                <Date>{today}</Date>
                <Version>{VERSION}</Version>
                <Comment>{PACKAGE_NAME} {VERSION} : metadonnees AppStream (fiche + icone) dans le paquet</Comment>
                <Name>{PACKAGER_NAME}</Name>
                <Email>{PACKAGER_EMAIL}</Email>
            </Update>
        </History>
        <BuildHost>{BUILD_HOST}</BuildHost>
        <Distribution>{DIST_NAME}</Distribution>
        <DistributionRelease>{DIST_RELEASE}</DistributionRelease>
        <Architecture>{ARCH}</Architecture>
        <InstalledSize>{installed_size}</InstalledSize>
        <PackageFormat>1.2</PackageFormat>
        <Source>
            <Name>{PACKAGE_NAME}</Name>
            <Homepage>{HOME_PAGE}</Homepage>
            <Packager>
                <Name>{PACKAGER_NAME}</Name>
                <Email>{PACKAGER_EMAIL}</Email>
            </Packager>
        </Source>
    </Package>
</PISI>""".encode("utf-8")

    OUT_DIR.mkdir(parents=True, exist_ok=True)
    out = OUT_DIR / OUT_FILENAME
    with zipfile.ZipFile(out, "w", compression=zipfile.ZIP_DEFLATED) as zf:
        zf.writestr("metadata.xml", metadata_xml)
        zf.writestr("files.xml", files_xml)
        zf.writestr("install.tar.xz", install_tar_xz)
    print(f"\n  OK {out} ({out.stat().st_size} octets)")
    print(f"  SHA1 {sha1_hex(out.read_bytes())}")


if __name__ == "__main__":
    build()
