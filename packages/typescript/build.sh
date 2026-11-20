#!/usr/bin/env zsh
# Builds the package into `dist`: the Rust library as a WebAssembly module
# with the JavaScript and the TypeScript declarations that wasm-bindgen
# writes for it, and the hand-written code. Run it in the development shell
# (`nix develop`), after `pnpm install`.
set -euo pipefail
cd "${0:A:h}"
root=${0:A:h:h:h}

# The command line tool writes the JavaScript side for one version of the
# crate only.
crate=$(awk '$0 == "name = \"wasm-bindgen\"" { getline; gsub(/[^0-9.]/, ""); print }' $root/Cargo.lock)
tool=$(wasm-bindgen --version)
tool=${tool##* }
if [[ $crate != $tool ]]; then
  print -u2 "The wasm-bindgen crate is $crate and the command line tool of the development shell is $tool."
  print -u2 "Set 'wasm-bindgen = \"=$tool\"' in crates/core/Cargo.toml and bindings/wasm/Cargo.toml and run 'cargo update -p wasm-bindgen'."
  exit 1
fi

metadata=$(cd $root && cargo metadata --format-version 1 --no-deps --locked)
target=$(node --print 'JSON.parse(process.argv[1]).target_directory' $metadata)
# The module names source files in its panic messages. It should not carry
# the directories of the machine that built it.
remap=(
  --remap-path-prefix=${CARGO_HOME:-$HOME/.cargo}/registry/src=/cargo
  --remap-path-prefix=$root=/publictransport
  --remap-path-prefix=$(rustc --print sysroot)=/rust
)
(cd $root && CARGO_ENCODED_RUSTFLAGS=${(pj:\x1f:)remap} cargo build --quiet --locked --profile dist \
  --target wasm32-unknown-unknown -p publictransport-wasm)

rm -rf src/generated dist
wasm-bindgen --target bundler --out-dir src/generated \
  $target/wasm32-unknown-unknown/dist/publictransport_wasm.wasm
wasm-opt -Oz src/generated/publictransport_wasm_bg.wasm -o src/generated/publictransport_wasm_bg.wasm

# A caller with `exactOptionalPropertyTypes` may pass `undefined` for an
# optional key, which the module reads as a missing one. tsify cannot
# declare that.
declarations=src/generated/publictransport_wasm.d.ts
perl -pi -e 's/(\w\?: )([^;}]*?)(;| \})/$1$2 | undefined$3/g' $declarations
if ! grep -q '?: .* | undefined' $declarations; then
  print -u2 "$declarations: found no optional key to widen"
  exit 1
fi

# wasm-bindgen's own module imports the `.wasm` file, which only some
# bundlers and runtimes can do, each after some setup. This one holds the
# code as text and works everywhere without any.
{
  print '/* @ts-self-types="./publictransport_wasm.d.ts" */'
  print 'import * as glue from "./publictransport_wasm_bg.js";'
  print -n 'const text = atob("'
  base64 < src/generated/publictransport_wasm_bg.wasm | tr -d '\n'
  print '");'
  cat <<'JS'
const bytes = new Uint8Array(text.length);
for (let index = 0; index < text.length; index++) {
  bytes[index] = text.charCodeAt(index);
}
const module = new WebAssembly.Module(bytes);
const instance = new WebAssembly.Instance(module, { "./publictransport_wasm_bg.js": glue });
glue.__wbg_set_wasm(instance.exports);
instance.exports.__wbindgen_start();
export * from "./publictransport_wasm_bg.js";
JS
} > src/generated/publictransport_wasm.js
rm src/generated/publictransport_wasm_bg.wasm src/generated/publictransport_wasm_bg.wasm.d.ts

pnpm exec tsc --project tsconfig.build.json
mkdir -p dist/generated
cp src/generated/* dist/generated
