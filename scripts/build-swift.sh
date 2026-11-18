#!/usr/bin/env zsh
# Builds what the Swift package in packages/swift needs from the Rust code.
# Run it in the development shell (`nix develop`), which has the Rust targets.
#
#   scripts/build-swift.sh bindings      writes the generated Swift code and
#                                        the C header into the package
#   scripts/build-swift.sh check         fails if those files are not current
#   scripts/build-swift.sh xcframework   builds the Rust library for every
#                                        Apple platform into an XCFramework
#   scripts/build-swift.sh xcframework macos   the same for macOS only,
#                                        which is enough to run the tests
set -euo pipefail
cd "${0:A:h}/.."

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

package=packages/swift
library=libpublictransport_uniffi.a

# generate <output directory>
generate() {
  cargo build --quiet -p publictransport-uniffi
  local built=(target/debug/libpublictransport_uniffi.(dylib|so)(N))
  cargo run --quiet -p publictransport-uniffi --bin uniffi-bindgen -- \
    generate --library $built[1] --language swift --out-dir $1
  # UniFFI 0.32.2 renames the types named in crates/core/uniffi.toml, but
  # not their converters where a method of a record throws the type or a
  # trait of the type is exported. Without this the code does not compile.
  local renamed=(Error:PublicTransportError Protocol:NetworkProtocol Operation:NetworkOperation Method:HttpMethod)
  for pair in $renamed; do
    sed -i.bak -E "s/FfiConverterType${pair%%:*}_(lift|lower)/FfiConverterType${pair##*:}_\\1/g" $1/publictransport.swift
  done
  # The message of an error is the one from Rust, which the generated code
  # has as `description`, where it would print the case as Swift sees it.
  sed -i.bak 's/String(reflecting: self)/description/' $1/publictransport.swift
  grep --quiet 'errorDescription' $1/publictransport.swift
  rm $1/publictransport.swift.bak
}

# static <target> builds the static library alone: the shared one would
# need the SDK of the platform to link.
static() {
  cargo rustc --quiet -p publictransport-uniffi --lib --release --target $1 --crate-type staticlib
}

case ${1:-} in
  bindings)
    generate $work
    cp $work/publictransport.swift $package/Sources/PublicTransport/Generated/
    cp $work/publictransportFFI.h $package/Sources/publictransportFFI/include/
    ;;
  check)
    generate $work
    if ! diff -q $work/publictransport.swift $package/Sources/PublicTransport/Generated/publictransport.swift ||
      ! diff -q $work/publictransportFFI.h $package/Sources/publictransportFFI/include/publictransportFFI.h; then
      print -u2 "The generated code of the Swift package is not current. Run: ${0:t} bindings"
      exit 1
    fi
    ;;
  xcframework)
    # The oldest systems the package supports, see Package.swift.
    export MACOSX_DEPLOYMENT_TARGET=26.0 IPHONEOS_DEPLOYMENT_TARGET=26.0
    export TVOS_DEPLOYMENT_TARGET=26.0 WATCHOS_DEPLOYMENT_TARGET=26.0 XROS_DEPLOYMENT_TARGET=26.0
    # One line per slice of the framework: its name and the targets in it.
    slices=(
      "macos aarch64-apple-darwin x86_64-apple-darwin"
      "ios aarch64-apple-ios"
      "ios-simulator aarch64-apple-ios-sim x86_64-apple-ios"
      "tvos aarch64-apple-tvos"
      "tvos-simulator aarch64-apple-tvos-sim"
      "watchos aarch64-apple-watchos"
      "watchos-simulator aarch64-apple-watchos-sim"
      "visionos aarch64-apple-visionos"
      "visionos-simulator aarch64-apple-visionos-sim"
    )
    if [[ ${2:-} == macos ]]; then
      slices=("macos ${$(rustc --print host-tuple)}")
    fi
    arguments=()
    for slice in $slices; do
      parts=(${=slice})
      name=$parts[1]
      archives=()
      for target in $parts[2,-1]; do
        static $target
        archives+=(target/$target/release/$library)
      done
      mkdir -p $work/$name
      # Xcode's tools do not run with the SDK the development shell sets.
      env -i PATH=/usr/bin:/bin lipo -create $archives -output $work/$name/$library
      arguments+=(-library $work/$name/$library)
    done
    output=$package/PublicTransportBinary.xcframework
    # The work directory is removed at the end, and the old framework with it.
    [[ -d $output ]] && mv $output $work/previous.xcframework
    env -i PATH=/usr/bin:/bin HOME=$HOME xcodebuild -create-xcframework $arguments -output $output
    ;;
  *)
    print -u2 "usage: ${0:t} bindings | check | xcframework [macos]"
    exit 2
    ;;
esac
