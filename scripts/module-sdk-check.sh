#!/usr/bin/env bash
# The module SDK alone builds a module: the example module, its back end and its web part, copied
# with the SDK (sdk/web, src-tauri/crates/module) into a folder of its own, nothing else of
# Prism. Then what a module may not import fails at its build: another Svelte module, a file out
# of its folder (Prism's sources).
#   scripts/module-sdk-check.sh        (`just module-sdk-check`; needs rustup's wasm32-wasip2 and pnpm)
set -euo pipefail
prism=$(cd "$(dirname "$0")/.." && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

mkdir -p "$work/sdk" "$work/crates"
cp -R "$prism/sdk/web" "$work/sdk/web"
cp -R "$prism/src-tauri/crates/module" "$work/crates/module"
rsync -a --exclude node_modules --exclude dist --exclude target "$prism/examples/modules/hello/" "$work/hello/"
# The SDK where it is now: next to the module, as a team's repository would pin it.
sed -i.bak 's|"link:../../../sdk/web"|"link:../sdk/web"|' "$work/hello/package.json"
sed -i.bak 's|path = "../../../../src-tauri/crates/module"|path = "../../crates/module"|' "$work/hello/backend/Cargo.toml"
# Its lockfile names the SDK where it was: resolved again (`--frozen-lockfile` would refuse it).
rm -f "$work/hello/package.json.bak" "$work/hello/backend/Cargo.toml.bak" "$work/hello/pnpm-lock.yaml"

echo "== back end (wasm32-wasip2)"
cargo build --quiet --release --target wasm32-wasip2 --manifest-path "$work/hello/backend/Cargo.toml"
echo "== web part"
(cd "$work/hello" && pnpm install --silent --no-frozen-lockfile && pnpm exec vite build --logLevel warn)
for f in module.toml backend.wasm web/defs.js web/components.js; do
  test -f "$work/hello/dist/hello/$f" || { echo "no dist/hello/$f"; exit 1; }
done

refused() {
  local what=$1 line=$2 expect=$3
  cp "$work/hello/src/components.ts" "$work/components.ts.bak"
  printf '%s\nconsole.log(probe)\n' "$line" | cat - "$work/components.ts.bak" > "$work/hello/src/components.ts"
  if out=$(cd "$work/hello" && pnpm exec vite build --logLevel error 2>&1); then
    echo "$what: built, it should not"; exit 1
  fi
  grep -q "$expect" <<<"$out" || { echo "$what: refused, but not saying \"$expect\":"; echo "$out" | tail -5; exit 1; }
  cp "$work/components.ts.bak" "$work/hello/src/components.ts"
  echo "refused at the build: $what"
}
refused "a Svelte module Prism does not give" "import { run as probe } from 'svelte/legacy'" "Prism gives modules"
refused "Prism's internals" "import { live as probe } from '\$prism/live.svelte'" "through @prism/sdk only"
mkdir -p "$work/elsewhere" && echo "export const probe = 1" > "$work/elsewhere/x.ts"
refused "a file out of the module's folder" "import { probe } from '../../elsewhere/x'" "out of the module's folder"
echo "ok: the SDK alone builds the example module"
