#!/bin/sh
set -eu
[ "$(git --version)" = 'git version 1.8.3.1' ]
rpm -q git glibc
/artifacts/git-tui --version
GIT_TUI_TEST_GIT=/usr/bin/git /artifacts/git-workflows-tests --test-threads=1
fixture=$(mktemp -d)
trap 'rm -rf "$fixture"' EXIT HUP INT TERM
cd "$fixture"
git init
printf 'CentOS 7 runtime\n' > sample.txt
/artifacts/git-tui --check .
