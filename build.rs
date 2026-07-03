fn main() {
    let target_arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    if target_arch != "wasm32" || option_env!("DOCS_RS").is_some() {
        return;
    }

    let rustflags = std::env::var("CARGO_ENCODED_RUSTFLAGS")
        .or_else(|_| std::env::var("RUSTFLAGS"))
        .unwrap_or_default();

    if !rustflags.contains("+atomics") {
        println!(
            "cargo:warning=wasm-bindgen-rayon: atomics target feature not detected in RUSTFLAGS. \
             Make sure to enable `-C target-feature=+atomics,+bulk-memory` (see crate README for details)."
        );
    }
}
