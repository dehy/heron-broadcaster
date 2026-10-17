// SPDX-License-Identifier: AGPL-3.0-or-later

// Rebuild when migrations change: `sqlx::migrate!` embeds them at compile time.
fn main() {
    println!("cargo:rerun-if-changed=../../migrations");
}
