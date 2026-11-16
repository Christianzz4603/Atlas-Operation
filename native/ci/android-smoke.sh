#!/usr/bin/env bash
# Installs the APK on the emulator, launches it, and prints whatever helps explain a crash.
APK=dist/atlas-miniwars.apk
PKG=com.atlas.miniwars
adb install -r "$APK" || { echo "INSTALL FAILED"; exit 1; }
adb logcat -c
adb shell monkey -p $PKG -c android.intent.category.LAUNCHER 1
sleep 35
if adb shell pidof $PKG >/dev/null; then echo "=== PROCESS ALIVE ==="; else echo "=== PROCESS DEAD ==="; fi
adb exec-out screencap -p > shot.png || true
adb logcat -d -v time > logcat.txt || true
echo "=== app log file ==="
adb shell run-as $PKG cat files/atlas-log.txt 2>&1 | tail -40 || true
echo "=== crash-related logcat ==="
grep -E "AndroidRuntime|FATAL|Fatal signal|SIGSEGV|SIGABRT|panicked|PANIC|RustPanic|RustStdoutStderr|UnsatisfiedLink|libatlas|atlas:|miniquad|DEBUG  |backtrace|ActivityManager.*(atlas|miniwars)" logcat.txt | head -150
echo "=== screenshot stats ==="
pip install --quiet pillow >/dev/null 2>&1
python3 - <<'PY'
from PIL import Image
try:
    im = Image.open("shot.png").convert("RGB")
    w, h = im.size
    px = list(im.getdata())
    nonblack = sum(1 for p in px if sum(p) > 60) / len(px)
    colors = len(set((r // 32, g // 32, b // 32) for r, g, b in px[:: max(1, len(px) // 20000)]))
    print("size", w, h, "non-black fraction", round(nonblack, 3), "distinct coarse colors", colors)
except Exception as e:
    print("screenshot analysis failed:", e)
PY
if adb shell pidof $PKG >/dev/null; then exit 0; else exit 1; fi
