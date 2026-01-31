#[cfg(not(target_arch = "wasm32"))]

#[actix_web::main]
async fn main() {
    use nix_autobuild::backend::main;

    main().await.unwrap();
}

#[cfg(target_arch = "wasm32")]
fn main() {
    use nix_autobuild::frontend::main;
    main();
}