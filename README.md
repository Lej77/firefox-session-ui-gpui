# Firefox Session Data GUI using gpui

This is a graphical user interface for interacting with Firefox's session store
file that contains info about currently opened tabs and windows.

The GUI is implemented using [`gpui`](https://www.gpui.rs/): "A fast, productive UI framework for Rust from the creators of Zed".

Note that this program simply makes use of the code exposed by the CLI tool at <https://github.com/Lej77/firefox_session_data>.

## Usage

- Download a precompiled executable from the [latest GitHub release](https://github.com/Lej77/firefox-session-ui-gpui/releases).
- Or clone the git repository and:
    - Build a release version locally using `cargo build --release` then run `target/release/firefox-session-ui-gpui.exe`.
    - When developing use: `cargo run`

### `cargo install`

You can use `cargo install` to easily build from source without manually cloning the repo:

```bash
cargo install --git https://github.com/Lej77/firefox-session-ui-gpui.git
```

You can use [`cargo-binstall`](https://github.com/cargo-bins/cargo-binstall) to easily download the precompiled executables from a GitHub release:

```bash
cargo binstall --git https://github.com/Lej77/firefox-session-ui-gpui.git firefox-session-ui-gpui
```

After installing you can update the program using [nabijaczleweli/cargo-update: A cargo subcommand for checking and applying updates to installed executables](https://github.com/nabijaczleweli/cargo-update):

```bash
cargo install-update --git firefox-session-ui-gpui

# OR update all installed programs:
cargo install-update --git --all
```

You can uninstall uisng:

```bash
 cargo uninstall firefox-session-ui-gpui
```

## References

- [GPUI](https://www.gpui.rs/)
    - [GitHub](https://github.com/zed-industries/zed/tree/main/crates/gpui)
    - [docs.rs](https://docs.rs/gpui/latest/gpui/)
- [GPUI Component](https://longbridge.github.io/gpui-component/)
    - [GitHub](https://github.com/longbridge/gpui-component)
    - [docs.rs](https://docs.rs/gpui-component/latest/gpui_component/)

## License

This project is released under [Apache License (Version 2.0)](./LICENSE-APACHE).

### Contribution

Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in the work by you, as defined in the Apache-2.0
license, shall be licensed as above, without any additional terms or
conditions.
