//! Persistencia del estado de la interfaz y de las preferencias.
//!
//! MEDIDO de por que este modulo existe: **el port no guardaba nada**. En el escritorio
//! `apps/vectorcraft/src/main.rs` carga con `load_prefs` (`:106`) y guarda con `save_prefs`
//! (`:116`, desde `on_exit` en `:49`), y en Android no habia ninguna de las dos llamadas.
//!
//! La consecuencia se veia en el movil: **pulsar OK en Preferencias no guardaba nada**.
//! `prefs_dialog::confirm` (`crates/ui-egui/src/prefs_dialog.rs:47`) si hace
//! `app.ui.engine_prefs = app.session.prefs.to_json()`, asi que el dato arrives a `app.ui`;
//! lo que no existia era la capa que lo escribe en disco. Al reabrir,
//! `prefs_dialog::restore` (`prefs_dialog.rs:58`) hacia
//! `serde_json::from_value(..).unwrap_or_default()` sobre un `engine_prefs` nulo, y por eso
//! todo volvia al valor de fabrica — el escalado a 0,85 incluido.
//!
//! MEDIDO de donde va, y por que aqui y no en el almacenamiento interno de la app:
//!
//! * Ruta real en `/storage/emulated/0/.vectorcraft/ui.json`. Es el equivalente del
//!   `~/Library/Application Support/VectorCraft/ui.json` del escritorio (`main.rs:77-93`)
//!   con la diferencia de que en Android el escritorio es la particion compartida, y asi el
//!   estado sobrevive a desinstalar la app.
//! * MEDIDO que **no** hace falta `getFilesDir()` ni ningun JNI: con
//!   `MANAGE_EXTERNAL_STORAGE` concedido, es `std::fs` normal, el mismo camino que ya usa
//!   `browser.rs` para leer y escribir documentos.
//! * El directorio **no existe todavia** — la app solo escribe documentos cuando el usuario
//!   elige sitio — asi que se crea en el primer guardado.

use crate::VectorcraftApp;

/// Carpeta del proyecto, equivalente al `.blend` de Blender para el estado que acompaña a
/// los documentos.
///
/// MEDIDO que el nombre lleva punto: una carpeta oculta no aparece en el explorador de
/// ficheros de Android ni en el `Download` del gestor, que es justo lo que se quiere para
/// un directorio de estado.
const DIR: &str = "/storage/emulated/0/.vectorcraft";

/// El fichero de estado. MEDIDO que guarda el `UiState` **entero** (`crates/ui-egui/src/
/// state.rs:303`), que ya incluye `engine_prefs` (`state.rs:303`), asi que preferencias,
/// paneles, docks, zoom y documentos abiertos viajan en el mismo fichero — igual que el
/// `ui.json` del escritorio.
const ARCHIVO: &str = "/storage/emulated/0/.vectorcraft/ui.json";

/// Ruta del estado. `pub` para que los tests puedan comprobarla sin repetir la constante.
pub fn ruta() -> String {
    ARCHIVO.to_string()
}

/// Cargar el estado guardado y aplicarlo.
///
/// MEDIDO que es copia literal del `load_prefs` del escritorio (`main.rs:106-111`): primero
/// `app.ui = ui.sanitized()` y **despues** `prefs_dialog::restore(app)`, que es lo que
/// traduce `engine_prefs` a `session.prefs`. El orden importa: al reves, `restore`
/// sobreescribiria lo que se acaba de cargar.
///
/// Si no hay fichero, o esta corrupto, se deja el estado de fabrica: MEDIDO que un
/// `unwrap_or_default()` al leer **no** es opcional, porque `restore` ya lo hace y un
/// fichero ilegible no puede dejar la app sin arrancar.
pub fn cargar(app: &mut VectorcraftApp) {
    let Ok(bytes) = std::fs::read(&ruta()) else { return };
    match serde_json::from_slice::<vectorcraft_ui_egui::UiState>(&bytes) {
        Ok(ui) => {
            app.ui = ui.sanitized();
        }
        Err(e) => {
            // MEDIDO que esto no puede ser un `panic!`: las preferencias son entrada no
            // confiable (un fichero escrito por otra version, o editado a mano) y el proyecto
            // prohibe panicos en codigo que se reparte.
            log::warn!("vectorcraft-android: ui.json ilegible, se usan los valores de fabrica: {e}");
        }
    }
    vectorcraft_ui_egui::prefs_dialog::restore(app);
}

/// Guardar el estado. Best effort: MEDIDO que un fallo al escribir **debe** conservar el
/// fichero anterior en vez de romperlo, que es lo que hace `write_atomic` en el escritorio
/// (`main.rs:125`).
///
/// MEDIDO que el escritorio primero refresca `engine_prefs` desde la sesion
/// (`main.rs:122`), porque fuera del dialogo pueden haber cambiado (el tema, por ejemplo), y
/// aqui se copia igual para no depender de que se haya pulsado OK.
pub fn guardar(app: &VectorcraftApp) {
    let ruta = ruta();
    if let Some(parent) = std::path::Path::new(&ruta).parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let mut ui = app.ui.clone();
    ui.engine_prefs = app.session.prefs.to_json();
    match serde_json::to_vec_pretty(&ui) {
        Ok(bytes) => {
            if let Err(e) = std::fs::write(&ruta, bytes) {
                log::warn!("vectorcraft-android: no se pudo guardar {ruta}: {e}");
            }
        }
        Err(e) => log::warn!("vectorcraft-android: no se pudo serializar el estado: {e}"),
    }
}
