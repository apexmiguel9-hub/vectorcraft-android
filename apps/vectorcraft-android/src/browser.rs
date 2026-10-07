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
use vectorcraft_ui_egui::{FilePick, Services};

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
    /// Guardar o exportar: el nombre propuesto se resuelve en la ruta elegida.
    Guardar,
}

/// Los bytes de una escritura esperando a que el usuario elija destino.
///
/// MEDIDO que hace falta un estado aparte, y por que el mapa de rutas no basta: `save`
/// pide la ruta y escribe **en la misma llamada** (`io.rs:251-263`), asi que cuando
/// `write` llega el dialogo sigue abierto —el usuario todavia no ha elegido— y lo unico
/// que se puede hacer es guardar los bytes y devolver `Ok(())`. Sin esto, guardar
/// escribiria en un fichero llamado `Untitled.png` en la raiz, que es el fallo que se veia.
static PENDIENTE: Mutex<Option<(String, Vec<u8>)>> = Mutex::new(None);

/// El nombre propuesto -> la ruta que el usuario eligio.
///
/// MEDIDO que hace falta para que el **segundo** guardado no vuelva a preguntar: el
/// documento guarda como `path` el nombre propuesto —es lo que devuelve `pick_path`
/// mientras el dialogo esta abierto— asi que sin este mapa, `Save` (no `Save As`) no
/// tendria donde escribir.
///
/// No se persiste a proposito, y se explica: en Android no se restauran los documentos
/// abiertos al arrancar, asi que `document.path` vuelve a ser `None` y siempre se pasa
/// por `Save As`. Un mapa en disco solo anadiria un fichero que nadie leeria.
static RUTAS: LazyLock<Mutex<std::collections::HashMap<String, String>>> =
    LazyLock::new(|| Mutex::new(std::collections::HashMap::new()));

/// El dialogo abierto y a donde va lo que salga.
struct Estado {
    dialogo: FileDialog,
    para: Para,
    /// El `FilePick` de la peticion, para `Guardar`: el nombre propuesto es la clave del
    /// mapa de rutas y el titulo del dialogo.
    pick: Option<FilePick>,
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

/// El directorio que se vio por ultima vez, para registrar **solo** los cambios.
///
/// MEDIDO por que hace falta: el bug que se midio en el movil es que al tocar una carpeta
/// a veces no se abre —uno de cada dos o tres— y sin esto no hay manera de saber si el
/// toque llego al dialogo o no:
/// * si el log **no** cambia de directorio, el toque no llego, y el problema es de entrada
/// * si cambia pero la rejilla se queda vacia, el problema es de lectura
static ULTIMO_DIR: Mutex<Option<PathBuf>> = Mutex::new(None);

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

        // MEDIDO, y este es el arreglo del bug de "toco una carpeta y a veces no se abre".
        //
        // No es una carrera ni un fallo de lectura: es que `egui_file` pide **doble
        // toque** para entrar en una carpeta (`src/lib.rs:813` y `:827`):
        //
        //     if response.clicked()        { … Command::Select(…) }            // selecciona
        //     if response.double_clicked() { … Command::BrowseDirectory(…) }  // entra
        //
        // En un dedo, el doble toque solo se registra si los dos caen dentro del tiempo de
        // egui. Tocar rapido entra; tocar despacio no. De ahi el "una de cada dos o
        // tres", y de ahi que con un toque y el boton **Open** fuera siempre: un toque
        // selecciona y el boton hace `OpenSelected`.
        //
        // El arreglo usa solo API publica, sin tocar el crate: tras un toque, la carpeta
        // queda en `selected_file`, asi que `path()` es el directorio tocado y
        // `directory()` sigue siendo el de antes. Si son distintos, se entra.
        //
        // MEDIDO que no hay bucle: al entrar, `set_path` llama a `refresh`, que hace
        // `selected_file = None` (`lib.rs:494`), asi que `path()` vuelve a dar `None`.
        if estado.dialogo.state() == State::Open
            && let Some(tocado) = estado.dialogo.path().map(std::path::Path::to_path_buf)
            && tocado.is_dir()
            && tocado != estado.dialogo.directory()
        {
            log::info!("browser: se entra en {tocado}", tocado = tocado.display());
            estado.dialogo.set_path(tocado);
        }

        // MEDIDO, y es la instrumentacion del bug de las carpetas: un registro por
        // cambio de directorio, no uno por frame.
        let ahora = estado.dialogo.directory().to_path_buf();
        // MEDIDO, y el error lo decia:
        //     `MutexGuard<'_, Option<PathBuf>>` does not implement `PartialEq<Option<PathBuf>>`
        // El guard no implementa `PartialEq`, asi que hay que sacar el valor antes de
        // comparar. Y el `if` en vez de `replace(...).is_none()` porque `replace` devuelve
        // el valor viejo, no un booleano —eso si compila, pero no dice lo que parece.
        let cambio = ULTIMO_DIR.lock().unwrap_or_else(|e| e.into_inner()).as_ref() != Some(&ahora);
        if cambio {
            ULTIMO_DIR.lock().unwrap_or_else(|e| e.into_inner()).replace(ahora.clone());
            // MEDIDO que no se puede contar las entradas: `egui_file` no expone el
            // listado, solo `selection()`. Asi que el log dice donde estamos y no cuanto
            // hay, que es lo que hace falta para distinguir "el toque no llego" de "llego
            // y no se pudo leer".
            log::info!("browser: directorio -> {}", ahora.display());
        }

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
                let nombre = estado.pick.as_ref().map(|p| p.name.clone());
                *caja = None;
                if para == Para::Guardar {
                    // MEDIDO: en un dialogo de guardar `selection()` viene vacia y el
                    // nombre va en el campo de texto, asi que el camino de aqui es el de
                    // `path()`: la ruta completa. Ver `destino_elegido`.
                    match (elegidas.first(), nombre) {
                        (Some(ruta), Some(nombre)) => destino_elegido(&nombre, ruta),
                        _ => log::info!("browser: guardado cancelado o sin nombre"),
                    }
                } else {
                    resolver(para, elegidas);
                }
            }
            State::Cancelled | State::Closed => {
                // MEDIDO: si se cancela, los bytes se tiran. Si se dejaran, el siguiente
                // guardado escribiria el fichero anterior donde el usuario acaba de decir
                // que no.
                if estado.pick.is_some()
                    && let Ok(mut p) = PENDIENTE.lock()
                {
                    p.take();
                    log::info!("browser: destino cancelado; se descartan los bytes en espera");
                }
                *caja = None;
            }
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
            // MEDIDO: `Para::Guardar` no llega aqui. `resolver` es para ficheros **de
            // entrada**, y guardar no lee: su camino esta en `destino_elegido`, porque
            // la ruta la decide el dialogo y no el fichero.
            Para::Guardar => {}
        }
    }
}

/// Guardar/exportar: destino elegido.
///
/// MEDIDO la API del crate, que es lo que hace esto posible:
///
/// ```text
/// lib.rs:775  let path = self.path.join(filename);   // el nombre del campo de fichero
/// lib.rs:870  Command::Open(path) => self.select(Some(path));
/// lib.rs:430  pub fn path(&self) -> &Path { selected_file.path() }
/// ```
///
/// O sea: al guardar, `path()` es la **ruta completa**, no el directorio. Y
/// `selection()` solo devuelve ficheros marcados del listado, asi que en un dialogo de
/// guardar viene vacia y por eso `logic` cae a `path()`.
///
/// Y `default_filename` se **consume** al construir el dialogo (`lib.rs:243`), asi que el
/// nombre propuesto hay que guardarlo aparte —de ahi el `pick` en [`Estado`].
fn destino_elegido(nombre: &str, ruta: &std::path::Path) {
    RUTAS.lock().unwrap_or_else(|e| e.into_inner()).insert(nombre.to_string(), ruta.to_string_lossy().into_owned());
    let esperando = PENDIENTE.lock().unwrap_or_else(|e| e.into_inner()).take();
    match esperando {
        Some((clave, bytes)) => {
            // MEDIDO: se escribe aqui y no antes, que es justo lo que pedia el
            // `PENDIENTE`. Y si el mapa tiene otra clave —el usuario guardo otra cosa
            // mientras— se avisa en vez de escribir donde no es.
            if clave != nombre {
                log::warn!("browser: los bytes en espera son de {clave}, no de {nombre}; no se escriben");
                return;
            }
            match std::fs::write(ruta, &bytes) {
                Ok(()) => log::info!("browser: guardado {} ({} bytes)", ruta.display(), bytes.len()),
                Err(e) => log::error!("browser: no se pudo escribir {}: {e}", ruta.display()),
            }
        }
        // MEDIDO: sin bytes esperando significa que la escritura ya habia salido por el
        // mapa —un segundo `Save`—. No es un fallo.
        None => log::info!("browser: destino elegido {}", ruta.display()),
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
    mostrar_con(para, nuevo, filtros, multi, None)
}

/// Igual que [`mostrar`], y ademas con la peticion a la que va a servir.
///
/// MEDIDO que hace falta la peticion y no solo el dialogo: el `FilePick` trae el nombre
/// propuesto, y ese nombre es la **clave** del mapa de rutas de [`destino_elegido`] y lo
/// que cierra [`PENDIENTE`]. Sin el, al confirmar solo habria una ruta y ninguna idea de
/// a que nombre pertenece.
fn mostrar_con(para: Para, nuevo: fn() -> FileDialog, filtros: Filtros, multi: bool, pick: Option<FilePick>) {
    if ESTADO.with(|c| c.borrow().is_some()) {
        // Ya hay un modal abierto. Sustituirlo perderia lo elegido hasta ahora, asi que
        // la peticion se ignora, que es lo que hace un modal de verdad.
        log::info!("browser: ya hay un dialogo abierto, peticion ignorada");
        return;
    }
    let mut dialogo = nuevo()
        .title(match para {
            Para::Abrir => "Abrir",
            Para::Poner => "Colocar",
            // MEDIDO: faltaba este brazo y por eso el dialogo de guardar se titulaba
            // "Abrir" —los tres botones de abajo si decian Save y Cancel—. Un titulo
            // equivocado en un modal es peor que ninguno: dice lo contrario de lo que
            // va a pasar.
            Para::Guardar => "Guardar",
        })
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
    // MEDIDO: `default_filename` se consume al construir (`egui_file/src/lib.rs:243`),
    // asi que el nombre propuesto se le pasa aqui y no en `resolver`.
    if let Some(p) = &pick {
        dialogo = dialogo.default_filename(p.name.clone());
    }
    dialogo = dialogo.initial_path(PathBuf::from("/storage/emulated/0"));
    // MEDIDO, y lo dice el propio error de compilacion:
    //     expected `FileDialog`, found `()`
    //     note: method `open` modifies its receiver in-place
    // O sea que `open()` es `fn open(&mut self)`, no un constructor de cadena. Encadenar
    // `.open()` al final devolvia `()` y no el dialogo.
    dialogo.open();
    ESTADO.with(|c| *c.borrow_mut() = Some(Estado { dialogo, para, pick }));
}


/// `save_async`: abrir el dialogo de destino.
///
/// MEDIDO que `pick_path` (`io.rs:167`) devuelve el **nombre propuesto** mientras este
/// dialogo esta abierto, y que despues `write` llega con esos mismos bytes. De ahi el
/// par `PENDIENTE` mas abajo: es el unico sitio donde se pueden guardar.
///
/// El motivo de que esto sea un gancho y no `pick_save` es que `pick_save` es
/// **sincrono** (`FnMut(&FilePick) -> Option<String>`) y un dialogo en immediate mode no
/// puede devolver una ruta en la misma llamada en la que el usuario la elige.
fn guardar(pick: &FilePick) {
    // Un `FilePick` de guardar trae un filtro por formato; el dialogo lo usa para el
    // nombre y para avisar si el usuario escribe otra extension.
    let filtros: Filtros =
        pick.filters.iter().map(|(etiqueta, exts)| (*etiqueta, *exts)).collect();
    mostrar_con(Para::Guardar, || FileDialog::save_file(), filtros, false, Some(pick.clone()));
}

/// Los `Services` del port: el browser y `std::fs`.
///
/// MEDIDO que `read`/`write` son exactamente el seam (`crates/ui-egui/src/lib.rs:134,135`):
/// `Fn(&str) -> Result<Vec<u8>, String>` y `FnMut(&str, &[u8]) -> Result<(), String>`.
///
/// `write` tiene tres caminos, y el primero es el que hace que guardar funcione:
///
/// 1. Hay bytes esperando a destino —el dialogo de guardar esta abierto—: se guardan y se
///    devuelve `Ok(())`. **Sin esto se escribiria en un fichero llamado `Untitled.png` en
///    la raiz**, que es el fallo que se veia antes.
/// 2. El nombre ya esta en [`RUTAS`], o sea un segundo `Save` del mismo documento: se
///    escribe en la ruta elegida la primera vez y no se vuelve a preguntar.
/// 3. Cualquier otra cosa: `std::fs` normal.
pub fn services() -> Services {
    Services {
        // MEDIDO: `open_async` y `place_async` devuelven `()` y no reciben nada
        // (`crates/ui-egui/src/lib.rs:159,168`). Lanzan el browser y vuelven; el resultado
        // entra por el inbox y lo drena el motor en su propio frame.
        open_async: Some(Box::new(abrir)),
        place_async: Some(Box::new(poner)),
        // MEDIDO: el gancho nuevo, que es `FnMut(&FilePick)`, y a diferencia de
        // `pick_open` **si recibe filtros**. `pick_path` devuelve el nombre propuesto
        // mientras el dialogo esta abierto.
        save_async: Some(Box::new(guardar)),
        inbox: Some((*INBOX).clone()),
        place_inbox: Some((*PLACE_INBOX).clone()),
        read: Some(Box::new(|path: &str| std::fs::read(path).map_err(|e| format!("{path}: {e}")))),
        write: Some(Box::new(|path: &str, bytes: &[u8]| {
            // Camino 1: hay un destino por elegir, y estos son sus bytes.
            //
            // MEDIDO, y el error lo decia:
            //     expected `Option<String>`, found `&str`
            // Asi que la comparacion es con `as_deref()`. Y de paso se quitan las dos
            // comprobaciones que sobraban: que el mapa **no** tenga el nombre ya lo
            // cubre el camino 2 de abajo, y que el dialogo abierto sea de guardar ya lo
            // dice `el_nombre_pendiente`, que devuelve `None` si no lo es.
            if el_nombre_pendiente().as_deref() == Some(path) {
                *PENDIENTE.lock().unwrap_or_else(|e| e.into_inner()) = Some((path.to_string(), bytes.to_vec()));
                log::info!("browser: {path} espera destino ({} bytes)", bytes.len());
                return Ok(());
            }
            // Camino 2: segunda vez del mismo documento.
            if let Some(destino) = RUTAS.lock().unwrap_or_else(|e| e.into_inner()).get(path).cloned() {
                return std::fs::write(&destino, bytes).map_err(|e| format!("{destino}: {e}"));
            }
            // Camino 3.
            std::fs::write(path, bytes).map_err(|e| format!("{path}: {e}"))
        })),
        ..Default::default()
    }
}

/// El nombre propuesto del dialogo de guardar abierto, si lo hay.
fn el_nombre_pendiente() -> Option<String> {
    ESTADO.with(|c| c.borrow().as_ref().and_then(|e| e.pick.as_ref()).map(|p| p.name.clone()))
}

/// Si hay que mostrar el aviso de permiso.
///
/// MEDIDO que hace falta por separado de [`aviso`]: la `Window` se dibuja antes de llamar
/// a `aviso`, asi que sin esto se creaba una ventana **vacia** y salia su marco en el
/// lienzo —un punto con sombra— aunque no hubiera nada que decir.
pub fn hay_aviso() -> bool {
    permiso::hay_que_preguntar()
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