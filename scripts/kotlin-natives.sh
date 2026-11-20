#!/usr/bin/env bash
# Collects the Rust library of every platform for the jar of the Kotlin
# package: kotlin-natives.sh <directory with wheels> <output directory>
#
# The wheels of the Python package hold the same library, built for each
# platform and tested there, so the jar takes it from them. Each library
# goes into the directory that JNA searches for its platform.
set -euo pipefail

wheels=$1
output=$2

for wheel in "$wheels"/*.whl; do
  case $wheel in
    *macosx_*_arm64.whl) platform=darwin-aarch64 ;;
    *macosx_*_x86_64.whl) platform=darwin-x86-64 ;;
    *linux*_x86_64.whl) platform=linux-x86-64 ;;
    *linux*_aarch64.whl) platform=linux-aarch64 ;;
    *win_amd64.whl) platform=win32-x86-64 ;;
    *) echo "no platform known for $wheel" >&2; exit 1 ;;
  esac
  mkdir -p "$output/$platform"
  # -j drops the path inside the wheel.
  unzip -q -j -o "$wheel" 'publictransport/_uniffi/*publictransport_uniffi.*' \
    -d "$output/$platform"
done

expected="darwin-aarch64/libpublictransport_uniffi.dylib
darwin-x86-64/libpublictransport_uniffi.dylib
linux-aarch64/libpublictransport_uniffi.so
linux-x86-64/libpublictransport_uniffi.so
win32-x86-64/publictransport_uniffi.dll"
found=$(cd "$output" && find . -type f | sed 's|^\./||' | sort)
if [ "$found" != "$expected" ]; then
  printf 'expected the libraries\n%s\nfound\n%s\n' "$expected" "$found" >&2
  exit 1
fi
