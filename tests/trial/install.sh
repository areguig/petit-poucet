#!/bin/sh
# Installs what the trial needs with the distro's own package manager, and Syncthing as $1 asks.
set -eu
syncthing=$1
sha256=$2
packages="git curl ca-certificates bash procps"
if [ "$syncthing" = package ]; then packages="$packages syncthing"; fi
if command -v apt-get >/dev/null; then
  apt-get update -qq && DEBIAN_FRONTEND=noninteractive apt-get install -y -qq $packages
elif command -v dnf >/dev/null; then
  dnf -q -y install $packages shadow-utils
fi
if [ "$syncthing" != package ]; then
  file=syncthing-linux-amd64-$syncthing.tar.gz
  curl -fsSL --retry 3 -o "/tmp/$file" "https://github.com/syncthing/syncthing/releases/download/$syncthing/$file"
  test "$(sha256sum "/tmp/$file" | cut -d' ' -f1)" = "$sha256"
  tar -xzf "/tmp/$file" -C /tmp
  install -m 755 "/tmp/syncthing-linux-amd64-$syncthing/syncthing" /usr/local/bin/syncthing
fi
useradd --create-home --shell /bin/bash dev
