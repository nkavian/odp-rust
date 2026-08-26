#!/usr/bin/env bash
set -euo pipefail

version=${1:?version is required}
release_directory=.release
mkdir -p "$release_directory"
rm -f "$release_directory"/*.crate

for package in odp-core odp-directory odp-agent odp-service; do
  cargo package --locked -p "$package"
  cp "target/package/$package-$version.crate" "$release_directory/"
done
