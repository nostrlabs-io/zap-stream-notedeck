#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
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
: "${KEYSTORE_FILE:?KEYSTORE_FILE is the path to the upload keystore}"
: "${KEYSTORE_PASSWORD:?}" "${KEY_ALIAS:?}"
export KEY_PASSWORD=${KEY_PASSWORD:-$KEYSTORE_PASSWORD}
export KEYSTORE_FILE=$(realpath "$KEYSTORE_FILE")
export ANDROID_HOME=${ANDROID_HOME:-$HOME/Android/Sdk}
export ANDROID_NDK_HOME=${ANDROID_NDK_HOME:-$ANDROID_HOME/ndk/28.2.13676358}
export ANDROID_NDK=$ANDROID_NDK_HOME
export CMAKE_ANDROID_NDK=$ANDROID_NDK_HOME

jni=android/app/src/main/jniLibs
rm -rf "$jni"
cargo ndk --platform 29 -t arm64-v8a -o "$jni" build --release --lib
cp "$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/sysroot/usr/lib/aarch64-linux-android/libc++_shared.so" "$jni/arm64-v8a/"
echo "sdk.dir=$ANDROID_HOME" > android/local.properties
(cd android && ./gradlew --no-daemon :app:assembleRelease :app:bundleRelease)

mkdir -p dist
apk=dist/zap_stream-$tag-android-arm64.apk
aab=dist/zap_stream-$tag-android.aab
cp android/app/build/outputs/apk/release/app-release.apk "$apk"
cp android/app/build/outputs/bundle/release/app-release.aab "$aab"
(cd dist && sha256sum "$(basename "$apk")" "$(basename "$aab")" | tee "zap_stream-$tag-android.sha256")
echo "$apk"
echo "$aab"
