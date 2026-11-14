#!/usr/bin/env bash
# On-device APK build (aarch64): aapt2 + kotlinc + compose plugin + d8 + apksigner.
# Requires aapt2/zipalign/apksigner (apt), JDK, kotlinc, r8.jar, androidx jars,
# and the API 37 framework jar (/root/kt/android-all-37.jar).
set -euo pipefail
APP=$(cd "$(dirname "$0")" && pwd)
MAIN=$APP/app/src/main
if [ -z "${OUT:-}" ]; then OUT=/tmp/apkbuild; fi
if [ -z "${KT:-}" ]; then KT=/root/kt; fi
if [ -z "${JAVA_HOME:-}" ]; then export JAVA_HOME=/usr/lib/jvm/java-17-openjdk-arm64; fi
export PATH=$JAVA_HOME/bin:$KT/kotlinc/kotlinc/bin:$PATH
if [ -z "${SDK_PLATFORM:-}" ]; then SDK_PLATFORM=/usr/lib/android-sdk/platforms/android-23/android.jar; fi
rm -rf "$OUT"; mkdir -p "$OUT/gen" "$OUT/classes" "$OUT/kotlin" "$OUT/dex"
cd "$OUT"
python3 - "$MAIN/AndroidManifest.xml" <<'PY'
import pathlib, sys
src = pathlib.Path(sys.argv[1]).read_text()
# Manual aapt2 link needs the package attribute; AGP injects it from the namespace.
src = src.replace('<manifest xmlns:android="http://schemas.android.com/apk/res/android">',
                  '<manifest xmlns:android="http://schemas.android.com/apk/res/android" package="com.dvda.maker">', 1)
src = src.replace('        android:requestLegacyExternalStorage="true"\n', '')
src = src.replace('        android:roundIcon="@mipmap/ic_launcher"\n', '')
pathlib.Path("AndroidManifest.xml").write_text(src)
PY
aapt2 compile --dir "$MAIN/res" -o res.zip
aapt2 link -o base.apk -I "$SDK_PLATFORM" --manifest AndroidManifest.xml \
  --min-sdk-version 24 --target-sdk-version 37 --version-code 1 --version-name 0.1.0 \
  --java gen res.zip
javac --release 8 -d classes $(find gen -name "*.java")
CP="classes:$KT/android-all-37.jar"
mapfile -t KOTLIN_SOURCES < <(find "$MAIN/java" -type f -name "*.kt" -print)
kotlinc -classpath "$CP" -jvm-target 1.8 -d kotlin "${KOTLIN_SOURCES[@]}"
jar cf classes.jar -C classes . -C kotlin .
java -cp "$KT/r8.jar" com.android.tools.r8.D8 --min-api 24 --lib "$KT/android-all-37.jar" \
  --output dex classes.jar "$KT/kotlinc/kotlinc/lib/kotlin-stdlib.jar"
cp base.apk app.unsigned.apk
mkdir -p stage/lib/arm64-v8a
cp dex/*.dex stage/
cp "$APP/app/src/main/jniLibs/arm64-v8a/libdvda_core.so" stage/lib/arm64-v8a/
( cd stage && zip -q -j ../app.unsigned.apk *.dex && \
  zip -q -0 ../app.unsigned.apk lib/arm64-v8a/libdvda_core.so )
zipalign -f -p 4 app.unsigned.apk app.aligned.apk
KS="$APP/debug.keystore"
if [ ! -f "$KS" ]; then keytool -genkeypair -keystore "$KS" \
  -storepass android -keypass android -alias androiddebugkey -keyalg RSA \
  -keysize 2048 -validity 10000 -dname "CN=Android Debug,O=Android,C=US"; fi
apksigner sign --ks "$KS" --ks-pass pass:android --key-pass pass:android \
  --ks-key-alias androiddebugkey --out app-signed.apk app.aligned.apk
apksigner verify --print-certs app-signed.apk | head -3
ls -lh app-signed.apk
