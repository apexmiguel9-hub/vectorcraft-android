# El .so de Rust se moduliza con `cargo ndk strip` para release, no desde aqui.
# ProGuard solo toca bytecode, y el .so ya viene compilado.
-keep class ai.storyteller.vectorcraft.MainActivity { *; }
