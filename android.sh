#!/bin/bash
set -e

export ANDROID_HOME="${ANDROID_HOME:-$HOME/Android/Sdk}"
export ANDROID_NDK_HOME="${ANDROID_NDK_HOME:-$ANDROID_HOME/ndk/28.2.13676358}"
export ANDROID_NDK="$ANDROID_NDK_HOME"
export CMAKE_ANDROID_NDK="$ANDROID_NDK_HOME"

cargo ndk --platform 29 -t arm64-v8a -o android/app/src/main/jniLibs build --release --lib
cp "$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/sysroot/usr/lib/aarch64-linux-android/libc++_shared.so" android/app/src/main/jniLibs/arm64-v8a/

echo "sdk.dir=$ANDROID_HOME" > android/local.properties
cd android
./gradlew :app:assembleRelease
