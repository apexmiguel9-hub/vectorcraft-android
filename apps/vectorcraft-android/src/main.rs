//! Binario de escritorio para el crate del port.
//!
//! El mismo codigo corre en el `.so` de Android (ver `lib.rs`, que es lo que carga
//! `System.loadLibrary`) y en este binario. Así la lógica se depura en local, que es
//! mucho más rápido que compilar para el móvil cada vez.
fn main() -> eframe::Result {
    vectorcraft_android::run()
}