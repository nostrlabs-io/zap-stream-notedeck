#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
attach=
for arg in "$@"; do
  case $arg in
    --attach) attach=1 ;;
    *) echo "usage: $0 [--attach]" >&2; exit 2 ;;
  esac
done
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
tag=${TAG:-v$version}
if [ "$tag" != "v$version" ]; then
  echo "tag $tag does not match the Cargo version $version" >&2
  exit 1
fi
notes=$(awk -v v="$version" '/^## \[/{p=index($0,"[" v "]")>0;next} /^\[[^]]*\]: /{p=0} p' CHANGELOG.md)
if ! grep -q '[^[:space:]]' <<< "$notes"; then
  echo "CHANGELOG.md has no entry for $version" >&2
  exit 1
fi
IFS=. read -r major minor patch <<< "${version%%[-+]*}"
build=${BUILD_NUMBER:-$(( major * 1000000 + minor * 10000 + patch * 100 ))}

if ! ssh -o ConnectTimeout=8 -o BatchMode=yes mac true 2>/dev/null; then
  cat >&2 <<'EOF'
The Mac is not reachable over ssh. If it rebooted, FileVault is waiting at the pre-boot screen:
  1. From a machine on the LAN run: ssh -o ControlPath=none kieran@<mac ip>
  2. Type the Mac login password. The session drops and macOS finishes booting.
  3. Wait a few minutes, check with: ssh -o ConnectTimeout=8 mac true
See ~/.agents/memory/mac-dev-server.md.
EOF
  exit 1
fi

dir=git/zap-stream-app-release
rsync -az --delete --exclude target --exclude .git --exclude dist --exclude review --exclude android/app/build \
  --exclude android/app/src/main/jniLibs --exclude '*.whl' ./ "mac:$dir/"
ssh mac "export PATH=\$HOME/.cargo/bin:/opt/homebrew/bin:\$PATH; cd ~/$dir && BUILD_NUMBER=$build ./ios-appstore.sh"

mkdir -p dist
ipa=dist/zap_stream-$tag-ios.ipa
scp -q "mac:$dir/target/ios/ZapStream.ipa" "$ipa"

read -r ipa_short ipa_build < <(unzip -p "$ipa" 'Payload/ZapStream.app/Info.plist' \
  | python3 -c 'import plistlib,sys; p=plistlib.loads(sys.stdin.buffer.read()); print(p["CFBundleShortVersionString"], p["CFBundleVersion"])')
if [ "$ipa_short" != "${version%%[-+]*}" ]; then
  echo "IPA CFBundleShortVersionString is $ipa_short, Cargo version is $version" >&2
  exit 1
fi
if [ "$ipa_build" != "$build" ]; then
  echo "IPA CFBundleVersion is $ipa_build, expected $build" >&2
  exit 1
fi
ssh mac "codesign --verify --deep --strict --verbose=2 ~/$dir/target/ios/ZapStream.app 2>&1; codesign -dvv ~/$dir/target/ios/ZapStream.app 2>&1 | grep -E '^(Authority|TeamIdentifier|Identifier)='"

(cd dist && sha256sum "$(basename "$ipa")" | tee "zap_stream-$tag-ios.sha256")
if [ -n "$attach" ]; then
  gh release upload "$tag" "$ipa" "dist/zap_stream-$tag-ios.sha256" --clobber --repo nostrlabs-io/zap-stream-notedeck
fi
echo "$ipa"
