#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
workspace_root="$(cd "$project_root/../../.." && pwd)"
deveco_contents="${DEVECO_STUDIO_CONTENTS:-/Applications/DevEco-Studio.app/Contents}"
sdk_root="${DEVECO_SDK_HOME:-$deveco_contents/sdk}"
native_root="$sdk_root/default/openharmony/native"
target="aarch64-unknown-linux-ohos"
target_dir="${CARGO_TARGET_DIR:-$workspace_root/target}"
rust_linker="$native_root/llvm/bin/aarch64-unknown-linux-ohos-clang"
rust_cxx="$native_root/llvm/bin/aarch64-unknown-linux-ohos-clang++"
rust_ar="$native_root/llvm/bin/llvm-ar"
cmake="$native_root/build-tools/cmake/bin/cmake"
library_dir="$project_root/entry/libs/arm64-v8a"
har_root="$project_root/engine"
har_library_dir="$har_root/libs/arm64-v8a"
declaration_source="$workspace_root/bindings/uc-ohos-napi/ohos/index.d.ts"
entry_declaration_dir="$project_root/entry/src/main/cpp/types/libuc_ohos_napi"
har_declaration_dir="$har_root/src/main/cpp/types/libuc_ohos_napi"
dist_dir="${UC_OHOS_DIST_DIR:-$target_dir/uc-ohos-napi-dist/ohos}"
debug_dir="$(dirname "$dist_dir")/debug-symbols/ohos"

har_only=false
case "${1:-}" in
  --har-only) har_only=true ;;
  "") ;;
  *) echo "Usage: $0 [--har-only]" >&2; exit 2 ;;
esac
python3 - "$sdk_root" <<'SDK_CHECK'
import json, pathlib, sys
metadata = pathlib.Path(sys.argv[1]) / 'default/sdk-pkg.json'
if not metadata.is_file():
    raise SystemExit('SDK metadata unavailable; set DEVECO_SDK_HOME')
api = int(json.loads(metadata.read_text())['data']['apiVersion'])
if api < 24:
    raise SystemExit(f'Engine HAR requires API 24 or later; installed SDK is API {api}')
SDK_CHECK

for executable in "$rust_linker" "$rust_cxx" "$rust_ar" "$cmake"; do
  if [[ ! -x "$executable" ]]; then
    echo "required DevEco tool is unavailable: $executable" >&2
    exit 1
  fi
done

export PATH="${CARGO_HOME:-$HOME/.cargo}/bin:$deveco_contents/tools/node/bin:$PATH"
export DEVECO_SDK_HOME="$sdk_root"
printf 'sdk.dir=%s\n' "$sdk_root" > "$project_root/local.properties"
cd "$project_root"
# ohpm resolves the local N-API package before the native build starts. Seed
# both package directories from the single checked-in declaration so a clean
# checkout can install dependencies without relying on a previous build.
mkdir -p "$entry_declaration_dir" "$har_declaration_dir"
cp "$declaration_source" "$entry_declaration_dir/index.d.ts"
cp "$declaration_source" "$har_declaration_dir/index.d.ts"
cp "$entry_declaration_dir/oh-package.json5" "$har_declaration_dir/oh-package.json5"
cp "$entry_declaration_dir/package.json" "$har_declaration_dir/package.json"
"$deveco_contents/tools/ohpm/bin/ohpm" install --all
cd "$workspace_root"
manifest_tool="$workspace_root/bindings/uc-ohos-napi/scripts/build-manifest.py"
source_fingerprint="$(python3 "$manifest_tool" --root "$workspace_root" --fingerprint)"

CARGO_TARGET_DIR="$target_dir" \
CARGO_TARGET_AARCH64_UNKNOWN_LINUX_OHOS_LINKER="$rust_linker" \
CC_aarch64_unknown_linux_ohos="$rust_linker" \
CXX_aarch64_unknown_linux_ohos="$rust_cxx" \
AR_aarch64_unknown_linux_ohos="$rust_ar" \
CMAKE="$cmake" \
  cargo build \
    --manifest-path "$workspace_root/Cargo.toml" \
    -p uc-ohos-napi \
    --target "$target" \
    --locked

mkdir -p "$library_dir" "$har_library_dir"
cp "$target_dir/$target/debug/libuc_ohos_napi.so" "$library_dir/libuc_ohos_napi.so"
cp "$target_dir/$target/debug/libuc_ohos_napi.so" "$har_library_dir/libuc_ohos_napi.so"
cp "$declaration_source" "$entry_declaration_dir/index.d.ts"
cp "$declaration_source" "$har_declaration_dir/index.d.ts"
cp "$entry_declaration_dir/oh-package.json5" "$har_declaration_dir/oh-package.json5"
cp "$entry_declaration_dir/package.json" "$har_declaration_dir/package.json"
mkdir -p "$dist_dir" "$debug_dir"
cp "$library_dir/libuc_ohos_napi.so" "$dist_dir/libuc_ohos_napi.so"
cp "$library_dir/libuc_ohos_napi.so" "$debug_dir/arm64-v8a.so"
cp "$declaration_source" "$dist_dir/index.d.ts"
shasum -a 256 "$dist_dir/libuc_ohos_napi.so" | awk '{print $1}' \
  > "$dist_dir/uc-ohos-napi.checksum.txt"
version="$(cargo pkgid --manifest-path "$workspace_root/Cargo.toml" -p uc-ohos-napi)"
version="${version##*#}"
commit="$(git -C "$workspace_root" rev-parse HEAD)"
printf 'v%s\n' "$version" > "$dist_dir/version.txt"
printf '%s\n' "$commit" > "$dist_dir/source-commit.txt"
cd "$project_root"
if [[ "$har_only" == false ]]; then
  DEVECO_SDK_HOME="$sdk_root" \
    "$deveco_contents/tools/hvigor/bin/hvigorw" \
    --mode module \
    -p product=default \
    -p module=entry@default \
    -p buildMode=debug \
    assembleHap \
    --no-daemon

  "$project_root/sign-emulator.sh"
fi

DEVECO_SDK_HOME="$sdk_root" \
  "$deveco_contents/tools/hvigor/bin/hvigorw" \
  --mode module \
  -p product=default \
  -p module=engine@default \
  -p buildMode=release \
  assembleHar \
  --no-daemon

har_path="$(python3 - "$har_root/build/default/outputs/default" <<'HAR_PATH'
import pathlib, sys
candidates = list(pathlib.Path(sys.argv[1]).glob('*.har'))
if len(candidates) != 1 or not candidates[0].is_file():
    raise SystemExit('Expected exactly one HarmonyOS HAR in the module output directory')
print(candidates[0])
HAR_PATH
)"
cp "$har_path" "$dist_dir/UniClipboardEngine.har"
if [[ "$har_only" == false ]]; then
  cp "$project_root/entry/build/default/outputs/default/entry-default-signed.hap" \
    "$dist_dir/UniClipboardEngineProbe.hap"
fi

python3 "$manifest_tool" --root "$workspace_root" \
  --expected-fingerprint "$source_fingerprint" --sdk "$sdk_root" --version "$version" \
  --har "$dist_dir/UniClipboardEngine.har" --declaration "$declaration_source" \
  --native "$target_dir/$target/debug/libuc_ohos_napi.so"

shasum -a 256 "$dist_dir/UniClipboardEngine.har" | awk '{print $1}' \
  > "$dist_dir/UniClipboardEngine.har.checksum.txt"
if [[ "$har_only" == false ]]; then
  shasum -a 256 "$dist_dir/UniClipboardEngineProbe.hap" | awk '{print $1}' \
    > "$dist_dir/UniClipboardEngineProbe.checksum.txt"
  echo "OK: $dist_dir/UniClipboardEngineProbe.hap"
fi

echo "OK: $dist_dir/UniClipboardEngine.har"
