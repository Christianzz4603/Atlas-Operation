# Playing Atlas MiniWars (native) on Android

## A) Winlator (recommended) - use the *Windows-x64* download
1. Unzip it and copy the whole folder somewhere Winlator can see (e.g. Downloads).
2. In Winlator: create/open a container, then launch `atlas-miniwars.exe` from the file manager (or add it as a shortcut).
   - No Visual C++ runtime is needed (it is built in).
3. Container settings that usually work:
   - Screen size 1280x720
   - Graphics driver: try **Turnip + Zink** first (Snapdragon); if the window is black/white, try **VirGL**
   - Box64 preset: *Performance*
4. Controls (touch = mouse): drag to pan, tap a country to select it, **+ / -** buttons (bottom right) to zoom,
   **MAP** switches map, **NEW** restarts, **||** pauses, **+ / -** in the top bar changes speed.
   (With a keyboard: WASD pan, wheel zoom, Space pause, R restart.)
5. If it is slow: copy `atlas.cfg.example` to `atlas.cfg` and set `fps_cap=30`.
6. If it crashes or shows nothing, open `atlas-log.txt` (next to the exe) and send it to me.

## B) Termux + proot + VNC - use the *Linux-arm64* download (your phone is ARM)
    pkg install proot-distro
    proot-distro install debian
    proot-distro login debian
    apt update && apt install -y libx11-6 libxcursor1 libxi6 libgl1 libgl1-mesa-dri libasound2 tigervnc-standalone-server xterm
    vncserver :1 -geometry 1280x720
    # copy the unzipped folder into the proot (e.g. from /sdcard via the proot bind), then:
    chmod +x atlas-miniwars run-termux.sh
    ./run-termux.sh
Connect a VNC viewer app to `localhost:5901`. Software rendering (llvmpipe) is used, so expect lower FPS;
set `fps_cap=30` in `atlas.cfg`. `atlas-log.txt` is written next to the binary.
On Debian 13 the sound library is called `libasound2t64`.
