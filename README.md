# Stremio Lite

A lightweight desktop client for Stremio, written in Rust with a native [Slint](https://slint.dev/) UI.

## Building

Requires a recent Rust toolchain plus the system libraries for Slint, libmpv, and libtorrent.

```sh
cargo build --release --package stremio-lite
```

To build and run directly:

```sh
cargo run --release --package stremio-lite
```

Settings, logs, and caches are kept in the platform data directory under `stremio-lite`.

## Credits

This program is an improved and updated version of the [stremio-native](https://github.com/stremio-native/stremio-native) project.

## License

GPL-3.0-only. See [LICENSE](LICENSE).
