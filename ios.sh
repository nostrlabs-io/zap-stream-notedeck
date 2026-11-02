#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
TARGET=${TARGET:-aarch64-apple-ios-sim}
BUNDLE_ID=${BUNDLE_ID:-io.nostrlabs.zap-stream}
PROFILE=${PROFILE:-release}
APP=target/ios/ZapStream.app
cargo build --profile "$PROFILE" --bin zap_stream_app --target "$TARGET"
rm -rf "$APP"
mkdir -p "$APP"
cp "target/$TARGET/$PROFILE/zap_stream_app" "$APP/ZapStream"
sed "s/__BUNDLE_ID__/$BUNDLE_ID/" ios/Info.plist > "$APP/Info.plist"
mkdir -p target/ios
sips -z 1024 1024 -s format jpeg assets/logo.png --out target/ios/icon.jpg >/dev/null
sips -s format png target/ios/icon.jpg --out target/ios/icon-1024.png >/dev/null
icons=target/ios/Assets.xcassets/AppIcon.appiconset
rm -rf target/ios/Assets.xcassets
mkdir -p "$icons"
images=""
for spec in iphone:20:2 iphone:20:3 iphone:29:2 iphone:29:3 iphone:40:2 iphone:40:3 iphone:60:2 iphone:60:3 ipad:20:1 ipad:20:2 ipad:29:1 ipad:29:2 ipad:40:1 ipad:40:2 ipad:76:1 ipad:76:2 ipad:83.5:2; do
  IFS=: read -r idiom size scale <<<"$spec"
  px=$(python3 -c "print(round($size*$scale))")
  name="icon-$idiom-$size@${scale}x.png"
  sips -z "$px" "$px" target/ios/icon-1024.png --out "$icons/$name" >/dev/null
  images="$images{\"filename\":\"$name\",\"idiom\":\"$idiom\",\"scale\":\"${scale}x\",\"size\":\"${size}x${size}\"},"
done
cp target/ios/icon-1024.png "$icons/icon-marketing.png"
echo "{\"images\":[$images{\"filename\":\"icon-marketing.png\",\"idiom\":\"ios-marketing\",\"scale\":\"1x\",\"size\":\"1024x1024\"}],\"info\":{\"author\":\"xcode\",\"version\":1}}" > "$icons/Contents.json"
echo '{"info":{"author":"xcode","version":1}}' > target/ios/Assets.xcassets/Contents.json
if [ "$TARGET" = aarch64-apple-ios ]; then
  actool_platform=iphoneos
else
  actool_platform=iphonesimulator
fi
xcrun actool target/ios/Assets.xcassets --compile "$APP" --platform "$actool_platform" --minimum-deployment-target 15.0 \
  --app-icon AppIcon --output-partial-info-plist target/ios/icons.plist --target-device iphone --target-device ipad >/dev/null
/usr/libexec/PlistBuddy -c "Merge target/ios/icons.plist" "$APP/Info.plist" 2>/dev/null || true
if [ "$TARGET" = aarch64-apple-ios ]; then
  sdk_build=$(xcrun --sdk iphoneos --show-sdk-build-version)
  sdk_ver=$(xcrun --sdk iphoneos --show-sdk-version)
  xc_ver=$(/usr/libexec/PlistBuddy -c "Print :DTXcode" /Applications/Xcode.app/Contents/Info.plist)
  xc_build=$(/usr/libexec/PlistBuddy -c "Print :ProductBuildVersion" /Applications/Xcode.app/Contents/version.plist)
  for kv in "DTPlatformName:iphoneos" "DTSDKName:iphoneos$sdk_ver" "DTSDKBuild:$sdk_build" "DTPlatformVersion:$sdk_ver" "DTPlatformBuild:$sdk_build" "DTXcode:$xc_ver" "DTXcodeBuild:$xc_build" "DTCompiler:com.apple.compilers.llvm.clang.1_0" "BuildMachineOSBuild:$(sw_vers -buildVersion)"; do
    /usr/libexec/PlistBuddy -c "Add :${kv%%:*} string ${kv#*:}" "$APP/Info.plist" 2>/dev/null || true
  done
fi
echo "APPL????" > "$APP/PkgInfo"
if [ "$TARGET" = aarch64-apple-ios ]; then
  if [ -n "${PROVISION:-}" ]; then
    cp "$PROVISION" "$APP/embedded.mobileprovision"
    security cms -D -i "$PROVISION" > target/ios/profile.plist
    plutil -extract Entitlements xml1 -o target/ios/entitlements.plist target/ios/profile.plist
    team=$(plutil -extract TeamIdentifier.0 raw -o - target/ios/profile.plist)
    plutil -replace application-identifier -string "$team.$BUNDLE_ID" target/ios/entitlements.plist
    ENTITLEMENTS=target/ios/entitlements.plist
  fi
  codesign --force --timestamp --sign "${SIGN_IDENTITY:?set SIGN_IDENTITY for a device build}" \
    ${KEYCHAIN:+--keychain "$KEYCHAIN"} ${ENTITLEMENTS:+--entitlements "$ENTITLEMENTS"} "$APP"
else
  codesign --force --sign - "$APP"
fi
echo "$APP"
