#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
root=$PWD

build=1
captions=1
only=""
while [ $# -gt 0 ]; do
  case $1 in
    --skip-build) build=0 ;;
    --no-captions) captions=0 ;;
    --only) only=$2; shift ;;
    --help|-h) sed -n '1,40p' docs/STORE-SHOTS.md; exit 0 ;;
    *) echo "unknown option $1" >&2; exit 2 ;;
  esac
  shift
done

export ANDROID_HOME=${ANDROID_HOME:-$HOME/Android/Sdk}
export ANDROID_NDK_HOME=${ANDROID_NDK_HOME:-$ANDROID_HOME/ndk/28.2.13676358}
export JAVA_HOME=${JAVA_HOME:-/usr/lib/jvm/java-21-openjdk-amd64}
export ANDROID_AVD_HOME=${STORE_SHOTS_AVD_HOME:-$HOME/.cache/zap-store-avd}
export PATH=$ANDROID_HOME/platform-tools:$ANDROID_HOME/emulator:$ANDROID_HOME/cmdline-tools/latest/bin:$PATH
port=${STORE_SHOTS_PORT:-5590}
export ANDROID_SERIAL=emulator-$port
image="system-images;android-34;google_apis;x86_64"
pkg=io.nostrlabs.zap_stream_flutter
activity=$pkg/stream.zap.app.MainActivity
ui="python3 $root/scripts/store_shots_ui.py"
out=$root/store
failures=0
emu_pid=""

adbs() { adb -s "$ANDROID_SERIAL" "$@"; }

cleanup() {
  [ $build = 1 ] && rm -rf "$root/android/app/src/main/jniLibs/x86_64"
  adbs shell am broadcast -a com.android.systemui.demo -e command exit >/dev/null 2>&1 || true
  adbs shell settings put global sysui_demo_allowed 0 >/dev/null 2>&1 || true
  adbs emu kill >/dev/null 2>&1 || true
  [ -n "$emu_pid" ] && kill "$emu_pid" >/dev/null 2>&1 || true
}
trap cleanup EXIT

build_apk() {
  jni=android/app/src/main/jniLibs
  cargo ndk --platform 29 -t x86_64 -o "$jni" build --release --lib --features store-shots
  cp "$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/sysroot/usr/lib/x86_64-linux-android/libc++_shared.so" "$jni/x86_64/"
  echo "sdk.dir=$ANDROID_HOME" > android/local.properties
  (cd android && ./gradlew :app:assembleDebug)
}

make_avd() {
  name=$1 device=$2
  [ -d "$ANDROID_AVD_HOME/$name.avd" ] && return
  mkdir -p "$ANDROID_AVD_HOME"
  echo no | avdmanager create avd -n "$name" -k "$image" -d "$device" --force >/dev/null
}

boot() {
  name=$1
  emulator -avd "$name" -port "$port" -no-window -no-audio -no-snapshot -no-boot-anim \
    -gpu "${STORE_SHOTS_GPU:-swiftshader_indirect}" -memory 3072 \
    -qemu -append "androidboot.qemu.hwcodec.avcdec=0" >/tmp/store-shots-emu.log 2>&1 &
  emu_pid=$!
  timeout 300 adb -s "$ANDROID_SERIAL" wait-for-device
  for _ in $(seq 120); do
    [ "$(adbs shell getprop sys.boot_completed | tr -d '\r')" = 1 ] && return
    sleep 3
  done
  echo "emulator $name did not boot" >&2
  exit 1
}

shutdown() {
  adbs emu kill >/dev/null 2>&1 || true
  wait "$emu_pid" 2>/dev/null || true
  emu_pid=""
}

demo_on() {
  adbs shell settings put global sysui_demo_allowed 1
  b="am broadcast -a com.android.systemui.demo -e command"
  adbs shell "$b enter" >/dev/null
  adbs shell "$b clock -e hhmm 0941" >/dev/null
  adbs shell "$b battery -e level 100 -e plugged false" >/dev/null
  adbs shell "$b network -e wifi show -e level 4 -e mobile hide" >/dev/null
  adbs shell "$b notifications -e visible false" >/dev/null
}

app_files() { adbs shell "run-as $pkg sh -c '$1'"; }

start_app() {
  adbs shell am force-stop $pkg
  app_files "mkdir -p files/settings; echo 1 > files/settings/terms_accepted"
  adbs shell am start -n $activity >/dev/null
}

open_page() { app_files "echo $1 > files/settings/pending_page"; }

open_link() { adbs shell am start -a android.intent.action.VIEW -d "nostr:$1" $pkg >/dev/null; }

shot() {
  dir=$1 name=$2
  mkdir -p "$out/play/$dir"
  adbs exec-out screencap -p > "$out/play/$dir/$name.png"
}

need() {
  if ! $ui wait "$1" "${2:-45}"; then
    echo "WARNING: '$1' never appeared" >&2
    failures=$((failures + 1))
  fi
}

scenes() {
  dir=$1
  mkdir -p "$out/play/$dir"
  stream=$(python3 "$root/scripts/store_shots_pick.py")
  naddr=$(python3 -c "import json,sys;print(json.loads(sys.argv[1])['naddr'])" "$stream")
  host=$(python3 -c "import json,sys;print(json.loads(sys.argv[1])['host'])" "$stream")
  status=$(python3 -c "import json,sys;print(json.loads(sys.argv[1])['status'])" "$stream")
  echo "stream $status: $stream"
  echo "$stream" > "$out/play/$dir/stream.json"

  want() { [ -z "$only" ] || [ "$only" = "$1" ]; }

  start_app
  need "watching" 60
  sleep 6
  want home && shot "$dir" 01-home

  open_link "$naddr"
  need "watching" 60
  need "zapped" 60
  sleep 12
  want stream && shot "$dir" 02-stream-chat

  adbs shell input swipe 400 1500 400 1000 400
  sleep 2
  want chat && shot "$dir" 03-chat-zaps

  adbs shell settings put system accelerometer_rotation 0
  adbs shell settings put system user_rotation 1
  sleep 6
  want landscape && shot "$dir" 04-landscape-player
  adbs shell settings put system user_rotation 0
  sleep 4

  open_page categories
  need "Music" 40
  sleep 4
  want categories && shot "$dir" 05-categories

  open_page category:music
  sleep 12
  want category && shot "$dir" 06-category

  open_page search:radio
  need "radio" 30
  sleep 10
  want search && shot "$dir" 07-search

  open_page "profile:$host"
  sleep 12
  want profile && shot "$dir" 08-profile

  app_files "echo ja > files/settings/zap_locale"
  start_app
  sleep 30
  want language && shot "$dir" 09-japanese
  app_files "rm -f files/settings/zap_locale"

  start_app
  sleep 6
  open_page settings
  sleep 8
  want settings && shot "$dir" 10-settings
}

run_set() {
  avd=$1 device=$2 size=$3 density=$4 dir=$5
  make_avd "$avd" "$device"
  boot "$avd"
  adbs shell wm size "$size"
  adbs shell wm density "$density"
  adbs install -r -g "$root/android/app/build/outputs/apk/debug/app-debug.apk" >/dev/null
  demo_on
  scenes "$dir"
  adbs shell wm size reset
  adbs shell wm density reset
  adbs shell settings put system user_rotation 0
  adbs shell settings put system accelerometer_rotation 1
  shutdown
}

[ $build = 1 ] && build_apk
rm -rf "$out/play"
mkdir -p "$out/play"

run_set zs_phone pixel_7 1080x1920 420 phone
run_set zs_tab7 "Nexus 7 2013" 1200x1920 240 tablet-7
run_set zs_tab10 "Nexus 10" 1600x2560 320 tablet-10

[ $captions = 1 ] && python3 scripts/store_shots_captions.py
python3 scripts/store_shots_check.py
[ $failures = 0 ] || { echo "$failures scene waits failed, inspect the pictures" >&2; exit 1; }
