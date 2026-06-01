# Dev setup (Fedora 44)

The Rust shell uses **iced** (wgpu renderer) + **iced_layershell** (wlr-layer-shell
via smithay-client-toolkit). One command installs everything needed to build:

```sh
sudo dnf install -y gcc gcc-c++ make cmake pkgconf-pkg-config \
    wayland-devel wayland-protocols-devel libxkbcommon-devel \
    vulkan-loader-devel mesa-vulkan-drivers \
  && curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y \
  && . "$HOME/.cargo/env" \
  && cargo install cargo-generate-rpm
```

What each piece is for:

| Package(s)                                   | Why                                            |
| -------------------------------------------- | ---------------------------------------------- |
| `gcc gcc-c++ make cmake`                     | C/C++ toolchain some `*-sys` crates build with |
| `pkgconf-pkg-config`                         | locate system libs at build time               |
| `wayland-devel wayland-protocols-devel`      | layer-shell client (taskbar, Start menu)        |
| `libxkbcommon-devel`                         | keyboard handling in the Wayland clients        |
| `vulkan-loader-devel mesa-vulkan-drivers`    | wgpu (iced's GPU renderer) + drivers            |
| rustup (rustc/cargo)                         | the Rust toolchain                              |
| `cargo-generate-rpm`                         | builds the RPM from Cargo metadata              |

Font rendering is pure-Rust (cosmic-text) — no freetype/fontconfig dev packages
required. TLS for the asset fetcher is rustls — no `openssl-devel` required.

## Build

```sh
cd rust
cargo build --release          # produces target/release/mde
cargo test                     # unit tests + accuracy harness (needs a session)
cargo generate-rpm -p mde      # -> target/generate-rpm/mde-*.rpm
```

> Dependencies are pinned and the workspace builds clean against a live toolchain
> (`Cargo.lock` is committed). The shell renders with **iced 0.13** + **iced_layershell
> 0.13**; window control is wlr-foreign-toplevel (`wayland-client` / `wayland-protocols-wlr`).
>
> **Runtime (to actually run the shell, not just build it):** the session is
> **labwc**, and the shell shells out to several tools — install the runtime set
> too: `sudo dnf install -y labwc wlr-randr wlopm swaybg swayidle swaylock foot grim`.
> (The RPM declares these in its `requires`; for a dev session install them by hand.)
