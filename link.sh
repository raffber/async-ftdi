#!/bin/bash

set -euxfo pipefail

rustc --crate-name request_response --edition=2021 examples/request_response.rs \
    --crate-type bin --emit=dep-info,link -C embed-bitcode=no -C debuginfo=2 -C metadata=5f6b71943dc1ce55 \
    -C extra-filename=-5f6b71943dc1ce55 --out-dir /home/raff/coding/async-ftdi/target/x86_64-pc-windows-gnu/debug/examples --target x86_64-pc-windows-gnu -C incremental=/home/raff/coding/async-ftdi/target/x86_64-pc-windows-gnu/debug/incremental -L dependency=/home/raff/coding/async-ftdi/target/x86_64-pc-windows-gnu/debug/deps -L dependency=/home/raff/coding/async-ftdi/target/debug/deps --extern async_ftdi=/home/raff/coding/async-ftdi/target/x86_64-pc-windows-gnu/debug/deps/libasync_ftdi-8f865ab308b505e1.rlib --extern libftd2xx=/home/raff/coding/async-ftdi/target/x86_64-pc-windows-gnu/debug/deps/liblibftd2xx-9f7c38419d87cb49.rlib --extern libftd2xx_ffi=/home/raff/coding/async-ftdi/target/x86_64-pc-windows-gnu/debug/deps/liblibftd2xx_ffi-727d60eecc69543f.rlib --extern tokio=/home/raff/coding/async-ftdi/target/x86_64-pc-windows-gnu/debug/deps/libtokio-20b84d3b6c62f525.rlib --extern windows_sys=/home/raff/coding/async-ftdi/target/x86_64-pc-windows-gnu/debug/deps/libwindows_sys-7ec0436877393c30.rlib -L native=/home/raff/coding/libftd2xx-ffi/vendor/windows/Static/amd64 -L native=/home/raff/.cargo/registry/src/github.com-1ecc6299db9ec823/windows_x86_64_gnu-0.36.1/lib \
    -C link-args='-l:ftd2xx.lib'
