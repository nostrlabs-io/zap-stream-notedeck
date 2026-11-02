#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
DIST=${DIST:-$HOME/kgdist}
PROVISION=${PROVISION:-$DIST/zap.mobileprovision}
KC=$DIST/kg.keychain-db
PW=${KEYCHAIN_PASSWORD:-kgbuild}
security unlock-keychain -p "$PW" "$KC"
IDENTITY=$(security find-identity -v -p codesigning "$KC" | grep -o 'Apple Distribution[^"]*' | head -1)
TARGET=aarch64-apple-ios KEYCHAIN="$KC" SIGN_IDENTITY="$IDENTITY" PROVISION="$PROVISION" ./ios.sh
rm -rf target/ios/ipa target/ios/ZapStream.ipa
mkdir -p target/ios/ipa/Payload
cp -R target/ios/ZapStream.app target/ios/ipa/Payload/
(cd target/ios/ipa && /usr/bin/zip -qry ../ZapStream.ipa Payload)
echo target/ios/ZapStream.ipa
