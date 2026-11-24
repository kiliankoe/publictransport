#!/bin/sh
# Runs the test of the Android library on the connected device or the
# running emulator: installs the app that `just android` has built, an app
# with the library that R8 has shrunk and renamed, and its test. Uses the
# adb on the PATH, so that CI can take the one that started its emulator.
set -eu
apks=packages/kotlin/android/smoke/build/outputs/apk
adb install -r "$apks/release/smoke-release.apk"
adb install -r "$apks/androidTest/release/smoke-release-androidTest.apk"
package=io.github.kiliankoe.publictransport.smoke
# `am instrument` ends with success whatever the tests did.
result=$(adb shell am instrument -w "$package.test/androidx.test.runner.AndroidJUnitRunner")
echo "$result"
case $result in *"OK (1 test)"*) ;; *) exit 1 ;; esac
