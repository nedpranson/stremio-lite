# Stremio Nova

A fast, lightweight desktop client for Stremio — built in Rust with a native [Slint](https://slint.dev/) UI.

## Build

### Linux

Install dependencies (Debian/Ubuntu):

```bash
sudo apt-get install build-essential clang cmake pkg-config libssl-dev libclang-dev \
  libgtk-3-dev libayatana-appindicator3-dev libmpv-dev libtorrent-rasterbar-dev \
  libfontconfig1-dev libfreetype6-dev libxkbcommon-dev libwayland-dev \
  libx11-xcb-dev libxcb-shape0-dev libxcb-xfixes0-dev libgl1-mesa-dev libegl1-mesa-dev
```

Then:

```bash
cargo run --release --package stremio-native
```

### Windows

Prerequisites: [Rust (`msvc`)](https://rustup.rs/), Visual Studio 2022 C++ Build Tools (x64 MSVC v143), Windows SDK, LLVM/Clang.

Install native dependencies once:

```powershell
& "$env:VCPKG_ROOT\vcpkg.exe" install `
  --x-manifest-root="$PWD" `
  --x-install-root="$PWD\vcpkg_installed" `
  --triplet=x64-windows-v3-static-md-release `
  --overlay-triplets="$PWD\triplets"
```

Then:

```powershell
cargo run --release --package stremio-native
```

Settings, logs, and image caches are stored in `./storage/`.

## License

GPL-3.0-only — see [LICENSE](LICENSE).
