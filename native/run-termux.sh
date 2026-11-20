#!/bin/sh
# Run inside a proot Debian/Ubuntu (Termux) with a VNC server on display :1
#   proot-distro login debian
#   vncserver :1 -geometry 1280x720
#   ./run-termux.sh
export DISPLAY="${DISPLAY:-:1}"
export LIBGL_ALWAYS_SOFTWARE=1
export GALLIUM_DRIVER=llvmpipe
cd "$(dirname "$0")" || exit 1
exec ./country-warzone "$@"
