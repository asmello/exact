//! Wire up the userlib-provided `user.x` linker script for the
//! `exact-job` binary. Mirrors `mono-os/examples/build.rs`: userlib's
//! own build script writes `user.x` to its OUT_DIR and exposes the
//! directory via `DEP_USERLIB_USER_X_DIR` (the `links = "userlib"`
//! metadata), and we add that to the linker search path.

use std::env;

fn main() {
    let dir = env::var("DEP_USERLIB_USER_X_DIR").expect("userlib build script exposes user_x_dir");
    println!("cargo:rustc-link-search={dir}");
    println!("cargo:rustc-link-arg-bins=-Tuser.x");
    println!("cargo:rerun-if-changed=build.rs");
}
