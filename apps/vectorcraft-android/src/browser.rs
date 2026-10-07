//! El browser de ficheros, dibujado dentro de la app.
//!
//! Sustituye a SAF entero, y la razon esta medida, no supuesta.
//!
//! ## Por que no SAF
//!
//! MEDIDO: abrir una segunda Activity da ANR en este port. El selector del sistema
//! abre una Activity, eso pausa la nuestra, y `android-activity` **bloquea el hilo
//! principal de Java hasta que el hilo del bucle confirma el estado**. Traza textual
//! del ANR, del propio movil:
//!
//! ```text
//! "main" prio=5 tid=1 Native
//!   native: #01 <std::sys::sync::condvar::futex::Condvar>::wait+112
//!   native: #02 <WaitableNativeActivityState>::set_activity_state+160
//!   native: #03 try_with_waitable_activity_ref::<glue::on_pause::{closure#0}…>+240
//!   native: #05 android_activity::activity_impl::glue::on_pause+20
//!   native: #08 android.app.NativeActivity.onPause+16
//! ```
//!
//! `set_activity_state` (`glue.rs:511`) escribe el comando en un pipe y espera en un
//! condvar a que `activity_state` cambie, y **solo el hilo del bucle puede cambiarlo**.
//! Ese hilo es el de `android_main`, que es donde corre el update de egui. O sea: si el
//! update espera al resultado del selector, el hilo nunca confirma la pausa y el
//! resultado nunca llega. Deadlock cerrado, y el ANR a los 10 s:
//!
//! ```text
//! Reason: Input dispatching timed out (…MainActivity is not responding.
//!   Waited 10001ms for FocusEvent(hasFocus=false))
//! mLastPausedActivity: com.android.documentsui/…picker.PickActivity
//! ```
//!
//! **No es un problema de SAF: es de abrir cualquier segunda Activity.** Por eso un
//! browser propio, pintado en egui, no abre ninguna y el ANR desaparece de raiz.
//!
//! ## Por que `egui_file`
//!
//! MEDIDO, leyendo el crate:
//!
//! * `grep -rniE 'android|jni|SAF' src/` **no devuelve ni una linea**: es egui puro.
//! * `src/lib.rs:231` — `fs: Box::new(Fs {})`, y `src/fs.rs` es `std::fs` a pelo
//!   (`std::fs::create_dir`, `read_dir`, `rename`). **No hay que escribir ningun `Vfs`.**
//! * Pide `egui ^0.36`, que es exactamente la version del proyecto. Una sola
//!   dependencia (`dyn-clone`), licencia MIT, 4,1 M de descargas.
//! * Inmediate mode: `FileDialog::show(ctx)` se llama cada frame y el resultado se lee
//!   con `state()` / `path()` / `selection()`. **Ninguna espera.**
//!
//! ## Los ficheros: por que `std::fs` y de donde salen
//!
//! Con `MANAGE_EXTERNAL_STORAGE` concedido, un `content://` sobra: se leen y se
//! escriben rutas de verdad. Y eso tiene dos ventajas que SAF no daba:
//!
//! * **Los ficheros linked vuelven a funcionar.** El motor usa `std::fs` para ellos
//!   (`cmd/links.rs:627`, `cmd/swatchlib.rs:136,190`, `cmd/flatten.rs:417`,
//!   `cmd/pdfcmds.rs:176`, `cmd/printpresets.rs:211`, `cmd/fileio/mod.rs:686`), asi que
//!   con SAF un `content://` les hacia `stat` a una cadena rara. Con rutas de verdad
//!   vuelven a entrar, sin tocar el motor.
//! * **`note_recent` guarda rutas de verdad**, asi que los recientes se abren.
//!
//! ## El permiso
//!
//! `MANAGE_EXTERNAL_STORAGE` no se concede con un dialogo: hay que abrir
//! `Settings.ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION` y el usuario lo activa a
//! mano. Se pide **una vez y se puede saltar**: sin el, el navegador sigue siendo
//! utilizable para lo que es nuestro (documentos, plantillas, assets, swatches) y
//! solo no alcanza `/storage/emulated/0/`.
//!
//! MEDIDO el porque, en el movil:
//!
//! ```text
//! $ adb shell run-as ai.storyteller.vectorcraft ls /sdcard
//! ls: /sdcard: Permission denied
//! ```
//!
//! Y el otro lado: con un build de ArmourPaint **sin ningun** permiso de almacenamiento,
//! su browser muestra la **rejilla vacia** en `/Download` — mientras que con el
//! permiso concedido lista y entra. Sin permiso se ven los nombres de las carpetas de
//! primer nivel, y nada mas.

use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::{Arc, LazyLock, Mutex};

use egui_file::{FileDialog, State};
use vectorcraft_engine::cmd::fileio;
use vectorcraft_ui_egui::place::PlaceArrival;
use vectorcraft_ui_egui::Services;

use crate::permiso;

/// Filtros del browser: `(etiqueta, extensiones)`.
type Filtros = Vec<(&'static str, &'static [&'static str])>;

/// MEDIDO: `LazyLock` y no `Arc::new` en un `static`.
///
/// `Arc::new` no es `const`, asi que un `static INBOX: Arc<Mutex<…>>` no compila, y
/// hacerlos por separado —uno para escribir y otro para el `Services`— seria un fallo
/// silencioso: los bytes irian a una caja y el motor drenaria otra, y no se veria
/// nunca un fichero abierto. `LazyLock` da **la misma** `Arc` en los dos sitios, que es
/// justo lo que hace falta.
static INBOX: LazyLock<Arc<Mutex<Vec<(String, Vec<u8>)>>>> =
    LazyLock::new(|| Arc::new(Mutex::new(Vec::new())));

/// Los bytes elegidos para colocar, igual que arriba pero por `place_inbox`.
static PLACE_INBOX: LazyLock<Arc<Mutex<Vec<PlaceArrival>>>> =
    LazyLock::new(|| Arc::new(Mutex::new(Vec::new())));

/// Que esta esperando el browser. Uno cada vez, que es lo correcto: es modal.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Para {
    /// `File → Open…`: los bytes van a `inbox` y la app los abre.
    Abrir,
    /// `File → Place…`: los bytes van a `place_inbox` y la app los coloca.
    Poner,
}

/// El dialogo abierto y a donde va lo que salga.
struct Estado {
    dialogo: FileDialog,
    para: Para,
}

// MEDIDO: estado por hilo y no un `static`, y el motivo concreto.
//
// El update de egui corre en el hilo de `android_main`, y ahi se llaman **todos** los
// ganchos que nos importan: `open_async` y `place_async` desde el menu, y `logic`
// desde `eframe::App::logic`. Con un `static Mutex<Option<Estado>>` haria falta que
// `FileDialog` fuera `Send`, y eso no se puede comprobar sin compilar. Con
// `thread_local!` no hace falta ningun `Send`, y `RefCell` avisa en vez de corromper si
// alguna vez se reentra.
thread_local! {
    static ESTADO: RefCell<Option<Estado>> = const { RefCell::new(None) };
}

/// Un frame del update.
///
/// Se llama **antes** de `VectorcraftApp::logic`, a proposito: si el browser acaba de
/// recoger un fichero, se vuelca en el inbox aqui y el motor lo drena en el mismo frame,
/// dentro de su `drain_inbox` (`lib.rs:676`). Si fuera al reves, habia un frame de
/// retraso.
pub fn logic(ctx: &egui::Context) {
    permiso::cada_frame();

    ESTADO.with(|caja| {
        let mut caja = caja.borrow_mut();
        let Some(estado) = caja.as_mut() else { return };

        estado.dialogo.show(ctx);

        match estado.dialogo.state() {
            State::Selected => {
                // MEDIDO: `selection()` cubre los dos casos —con `multi_select` da todos
                // los marcados y sin el da el unico—, pero se cae a `path()` si viene
                // vacia.
                let mut elegidas: Vec<PathBuf> =
                    estado.dialogo.selection().iter().map(|p| p.to_path_buf()).collect();
                if elegidas.is_empty()
                    && let Some(p) = estado.dialogo.path()
                {
                    elegidas.push(p.to_path_buf());
                }
                let para = estado.para;
                *caja = None;
                resolver(para, elegidas);
            }
            State::Cancelled | State::Closed => *caja = None,
            // `Open`: sigue abierto.
            State::Open => {}
        }
    });
}

/// Entregar lo elegido.
fn resolver(para: Para, elegidas: Vec<PathBuf>) {
    for ruta in elegidas {
        // MEDIDO: leer aqui y no en el drenado es lo que permite que un fallo sea un
        // mensaje y no un fichero a medias. Y con `std::fs` el error dice el camino y el
        // motivo, cosa que un `content://` no daba.
        let bytes = match std::fs::read(&ruta) {
            Ok(b) => b,
            Err(e) => {
                log::error!("browser: no se pudo leer {}: {e}", ruta.display());
                continue;
            }
        };
        let nombre = nombre_de(&ruta);
        match para {
            Para::Abrir => INBOX.lock().unwrap_or_else(|e| e.into_inner()).push((nombre, bytes)),
            // `drop: None` = elegido en el dialogo, no soltado sobre el lienzo.
            Para::Poner => PLACE_INBOX
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(PlaceArrival { name: nombre, bytes, drop: None }),
        }
    }
}

/// El nombre del fichero, que es lo que la UI muestra.
///
/// MEDIDO que el motor lo saca del nombre y no del contenido: `io::open_bytes` llama a
/// `fileio::extension(name)`, asi que sin nombre no hay deteccion de formato.
fn nombre_de(ruta: &std::path::Path) -> String {
    ruta.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| ruta.to_string_lossy().into_owned())
}

/// Abrir el browser de apertura.
///
/// MEDIDO que `open_async` **no recibe filtros** (`lib.rs:159`, `Box<dyn FnMut()>`), al
/// contrario que `pick_open`. Se pran los del motor aqui, que son los mismos que usa
/// `io::open_dialog`.
fn abrir() {
    mostrar(Para::Abrir, || FileDialog::open_file(), fileio::open_filters().collect(), false);
}

/// Abrir el browser de colocar.
fn poner() {
    mostrar(Para::Poner, || FileDialog::open_file(), fileio::place_filters().collect(), true);
}

/// Poner el dialogo en pantalla.
///
/// MEDIDO que hay que construir uno nuevo cada vez y no reutilizar: el motor ya dejo el
/// ultimo destino en su sitio, y es lo que hace el ejemplo de la libreria. Ademas
/// `FileDialog::new` es privado: los unicos constructores publicos son `open_file`,
/// `save_file` y `select_folder`, asi que el tipo se elige con la funcion que se pasa.
///
/// MEDIDO tambien que `initial_path` se consume al construirse, asi que el directorio de
/// arranque se decide aqui y no en el primer `show`.
fn mostrar(para: Para, nuevo: fn() -> FileDialog, filtros: Filtros, multi: bool) {
    if ESTADO.with(|c| c.borrow().is_some()) {
        // Ya hay un modal abierto. Sustituirlo perderia lo elegido hasta ahora, asi que
        // la peticion se ignora, que es lo que hace un modal de verdad.
        log::info!("browser: ya hay un dialogo abierto, peticion ignorada");
        return;
    }
    let mut dialogo = nuevo()
        .title(if para == Para::Poner { "Colocar" } else { "Abrir" })
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .default_size(egui::vec2(760.0, 460.0))
        .show_system_files(false)
        .multi_select(multi)
        .show_new_folder(false)
        .show_rename(false);
    if !filtros.is_empty() {
        // MEDIDO: `Filter<T>` es `Box<dyn Fn(&T::Target) -> bool + Send + Sync>` y para
        // `PathBuf` el objetivo es `Path`. Se comparan en minusculas porque el motor las
        // da asi (`fileio`) pero los ficheros no siempre.
        let permitidas: Vec<String> =
            filtros.iter().flat_map(|(_, exts)| exts.iter().map(|e| e.to_ascii_lowercase())).collect();
        dialogo = dialogo.show_files_filter(Box::new(move |p: &std::path::Path| {
            match p.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()) {
                Some(e) => permitidas.contains(&e),
                // Sin extension son las carpetas: siempre se dejan pasar.
                None => true,
            }
        }));
    }
    // MEDIDO, y aqui se corrigio una decision previa: el arranque va **siempre** a
    // `/storage/emulated/0`, sin mirar el permiso.
    //
    // Estaba condicionado a `permiso::concedido()`, y con el permiso denial apareceria
    // `Permission denied` en el navegador. Que es peor que vacio, porque no dice que
    // hacer: "sin permiso" no es un error de la app, es lo que pasa.
    //
    // Y el dialogo tiene campo de path, asi que se puede escribir cualquiera. El
    // unico sitio al que hay que ir con cuidado es este.
    dialogo = dialogo.initial_path(PathBuf::from("/storage/emulated/0"));
    // MEDIDO, y lo dice el propio error de compilacion:
    //     expected `FileDialog`, found `()`
    //     note: method `open` modifies its receiver in-place
    // O sea que `open()` es `fn open(&mut self)`, no un constructor de cadena. Encadenar
    // `.open()` al final devolvia `()` y no el dialogo.
    dialogo.open();
    ESTADO.with(|c| *c.borrow_mut() = Some(Estado { dialogo, para }));
}

/// Los `Services` del port: el browser y `std::fs`.
///
/// MEDIDO que `read`/`write` son exactamente el seam (`lib.rs:134,135`): `Fn(&str) ->
/// Result<Vec<u8>, String>` y `FnMut(&str, &[u8]) -> Result<(), String>`.
///
/// Lo que se deja **sin** conectar, y por que:
///
/// * `pick_open`, `pick_save`, `pick_folder`, `pick_open_multi` son **sincronos**
///   (`FnMut(…) -> Option<String>`), y un browser en immediate mode no puede devolver
///   una ruta en la misma llamada en la que el usuario la elige. Es el paso siguiente y
///   necesita una costura que hoy no existe en upstream. `open_async` y `place_async` si
///   son asincronos y ya estan.
///
/// **Ahora mismo guardar y exportar siguen sin funcionar**, y no es un descuido: es que
/// `save` escribe en la misma llamada en la que se pide la ruta (`io.rs:251-263`). Cada
/// hueco degrada a un mensaje en vez de romper, que es el mismo camino que la web.
pub fn services() -> Services {
    Services {
        // MEDIDO: `open_async` y `place_async` devuelven `()` y no reciben nada
        // (`lib.rs:159,168`). Lanzan el browser y vuelven; el resultado entra por el inbox
        // y lo drena el motor en su propio frame.
        open_async: Some(Box::new(abrir)),
        place_async: Some(Box::new(poner)),
        inbox: Some((*INBOX).clone()),
        place_inbox: Some((*PLACE_INBOX).clone()),
        read: Some(Box::new(|path: &str| std::fs::read(path).map_err(|e| format!("{path}: {e}")))),
        write: Some(Box::new(|path: &str, bytes: &[u8]| std::fs::write(path, bytes).map_err(|e| format!("{path}: {e}")))),
        ..Default::default()
    }
}

/// El aviso de permiso, si falta.
///
/// MEDIDO que no hace falta pedirlo solo: el usuario lo salta y la app sigue siendo
/// utilizable para lo suyo. Por eso son dos botones y no un dialogo modal.
pub fn aviso(ui: &mut egui::Ui) {
    if !permiso::hay_que_preguntar() {
        return;
    }
    ui.horizontal(|ui| {
        ui.small("Sin acceso a los archivos no se pueden abrir ni guardar los del telefono.");
        if ui.small_button("Permitir").clicked() {
            permiso::pedir();
        }
        if ui.small_button("Ahora no").clicked() {
            permiso::descartar();
        }
    });
}