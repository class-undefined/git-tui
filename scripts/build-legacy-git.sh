#!/bin/sh
# Build only the local Git command used by compatibility tests.
set -eu
prefix=${1:-"$PWD/.tools/git-1.8.3.1"}
mkdir -p "$prefix/bin"
prefix=$(cd "$prefix" && pwd)
build_dir=$(mktemp -d)
trap 'rm -rf "$build_dir"' EXIT HUP INT TERM
archive="$build_dir/git.tar.xz"
curl --fail --location --retry 3 --connect-timeout 20 \
  https://www.kernel.org/pub/software/scm/git/git-1.8.3.1.tar.xz -o "$archive"
expected=40f785ec3e9e2f476f592ce3310344fc03e8d4acce8ce2a39c11441297de3d75
if command -v sha256sum >/dev/null 2>&1; then
  actual=$(sha256sum "$archive" | cut -d ' ' -f 1)
else
  actual=$(shasum -a 256 "$archive" | cut -d ' ' -f 1)
fi
[ "$actual" = "$expected" ] || { echo 'Git source checksum mismatch' >&2; exit 1; }
tar -xJf "$archive" -C "$build_dir"
cd "$build_dir/git-1.8.3.1"
make -j2 git NO_OPENSSL=YesPlease NO_CURL=YesPlease NO_EXPAT=YesPlease \
  NO_GETTEXT=YesPlease NO_TCLTK=YesPlease NO_PERL=YesPlease \
  NO_PYTHON=YesPlease \
  CFLAGS='-O2 -Wno-deprecated-declarations -Wno-implicit-function-declaration -Wno-incompatible-pointer-types' >&2
cp git "$prefix/bin/git"
"$prefix/bin/git" --version
