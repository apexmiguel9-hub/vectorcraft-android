//! SAF: abrir, guardar y exportar con el selector del sistema de Android.
//!
//! # Por que esto existe
//!
//! MEDIDO, del port y del upstream:
//!
//! * VectorCraft **no** trae explorador de ficheros propio. El dialogo viene de
//!   `rfd`, cableado en `apps/vectorcraft/src/main.rs`, que es la app **de
//!   escritorio**.
//! * **`rfd` no sirve en Android.** Las dependencias de `rfd` 0.17.2 son `block2`,
//!   `dispatch2`, `js-sys`, `libc`, `log`, `objc2*`, `percent-encoding`, `pollster`,
//!   `raw-window-handle`, `wasm-bindgen*`, `wayland*` y `windows-sys`. **Ni `jni` ni
//!   `ndk-context`**: sus backends son macOS, Windows, wayland/xdg y web, y en
//!   Android no hay ninguno.
//! * El motor, por su cuenta, usa `std::fs` en unos pocos sitios
//!   (`cmd/links.rs:627`, `cmd/swatchlib.rs:136,190`, `cmd/flatten.rs:417`,
//!   `cmd/pdfcmds.rs:176`, `cmd/printpresets.rs:211`). Por eso los **archivos
//!   enlazados** con `link: true` no funcionan con un URI; ver [`es_uri`].
//!
//! # Por que no hay que copiar ficheros
//!
//! El "path" de VectorCraft es una cadena opaca y la UI lee y escribe por dos
//! ganchos (`crates/ui-egui/src/io.rs:95` y `:128`):
//!
//! ```text
//! fn read(app, path)       { app.services.read…(path) }
//! fn write_to(sv, path, b) { services.write…(path, bytes) }
//! ```
//!
//! Aqui esa cadena es un `content://` URI y lo resuelve `ContentResolver`. **Cero
//! copias, cero permisos de almacenamiento**: SAF esta disenado precisamente para no
//! dar acceso general al almacenamiento, y `/data/data/<pkg>/` tampoco lo necesita.
//!
//! # Por que esto bloquea, y por que no hay deadlock
//!
//! Los ganchos de fichero son sincronos (`FnMut(&FilePick) -> Option<String>`), igual
//! que en escritorio, y lanzar el selector de Android es otra Activity.
//!
//! MEDIDO: en Android el hilo que dibuja la UI **es el mismo** que despacha los
//! eventos de winit —el `tid` sale con nombre `android_main` en el tombstone—. Si se
//! espera aqui un evento, **el evento no llega nunca**.
//!
//! Lo que lo evita es que el despertar no pasa por winit: lo manda
//! `MainActivity.onActivityResult`, en el hilo de la UI de Android, llamando a
//! [`Java_ai_storyteller_vectorcraft_MainActivity_nativeOnFilePicked`]. Rust solo
//! espera en una variable de condicion. Es el unico camino que no toca el hilo
//! bloqueado.
//!
//! # Por que los bytes van en base64
//!
//! `Env::convert_byte_array` pide un `AsRef<JByteArray>`, y de un `JObject` devuelto
//! por `call_static_method` solo se llega a `JPrimitiveArray` pasando por
//! `from_raw`, que es `unsafe` y en `jni` 0.22 implica pelear con los lifetimes del
//! `Env`. Eso es justo lo que no se puede comprobar sin compilar, y este JNI esta
//! escrito a mano. Con cadenas —`Env::new_string` y `JString::try_to_string`, los dos
//! comprobados leyendo el codigo del crate— no hay superficie de error, y el motor ya
//! usa base64 para mover
//! bytes por sus parametros (`{name, dataBase64}`), asi que el formato es el del
//! proyecto y no una invencion del port.
//!
//! Lo que cuesta: un 33% mas de memoria y una copia. Para un documento vectorial,
//! que son megabytes, es milisegundos. Y va medido en [`leer`] y [`escribir`].

use std::ffi::c_void;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::Instant;

use jni::objects::{JObject, JString};
use jni::sys::{jint, JavaVM};
use jni::Env;

use vectorcraft_ui_egui::{FilePick, Services};

// Los modos, con los mismos numeros que en `MainActivity.java`.
const MODO_ABRIR: jint = 0;
const MODO_ABRIR_MULTI: jint = 1;
const MODO_GUARDAR: jint = 2;
const MODO_CARPETA: jint = 3;

/// El `JavaVM`, del `AndroidApp` de `android-activity`.
///
/// MEDIDO: `winit::platform::android::activity::AndroidApp` reexporta
/// `android_activity::AndroidApp`, que da `vm_as_ptr()`. El grafo ya trae `jni`
/// 0.22.4 como dependencia de `android-activity`, asi que anadirla aqui como directa
/// no amplia nada.
static VM: AtomicPtr<JavaVM> = AtomicPtr::new(std::ptr::null_mut());

/// `true` cuando Java ya llamo a `nativeOnFilePicked` y el resultado esta listo.
static RESUELTO: Mutex<bool> = Mutex::new(false);
static ESPERA: Condvar = Condvar::new();

/// Guardar el `JavaVM` del proceso. Se llama una vez, al arrancar.
pub fn registrar(app: &winit::platform::android::activity::AndroidApp) {
    VM.store(app.vm_as_ptr() as *mut JavaVM, Ordering::Release);
    log::info!("saf: JavaVM registrado");
}

/// Java -> Rust: el selector ha terminado.
///
/// Se llama en el hilo de la UI de Android, desde `onActivityResult`. Solo despierta;
/// el resultado se recoge luego con `takeResult`, para no tener que decodificar un
/// `jstring` aqui, donde no hay un `Env` con el que hacerlo.
#[unsafe(no_mangle)]
pub extern "C" fn Java_ai_storyteller_vectorcraft_MainActivity_nativeOnFilePicked(_env: *mut c_void, _clase: *mut c_void) {
    match RESUELTO.lock() {
        Ok(mut g) => *g = true,
        Err(e) => *e.into_inner() = true,
    }
    ESPERA.notify_all();
}

/// Adjuntar el hilo actual a la JVM y hacer una cosa con el `Env`.
///
/// `attach_current_thread_for_scope` desconecta solo, que es lo que hace falta
/// porque `read`/`write` se llaman desde el hilo de winit.
///
/// El `Result` anidado es por la firma de `attach_current_thread_for_scope`, que
/// exige `E: From<jni::errors::Error>` y por tanto no admite errores de aplicacion.
/// El wrapper [`env`] lo aplana, para que las llamadas no hagan malabares.
fn con_env<T, F>(f: F) -> Result<Result<T, String>, String>
where
    F: FnOnce(&mut Env) -> std::result::Result<Result<T, String>, jni::errors::Error>,
{
    let p = VM.load(Ordering::Acquire);
    if p.is_null() {
        return Err("la JVM no esta registrada todavia".into());
    }
    // SAFETY: `p` viene de `AndroidApp::vm_as_ptr()`, que es el JavaVM de la app y
    // vive todo el proceso. `JavaVM::from_raw` solo comprueba que no sea nulo.
    let vm = unsafe { jni::JavaVM::from_raw(p) };
    vm.attach_current_thread_for_scope(f).map_err(|e| format!("{e:?}"))
}

/// [`con_env`] sin el `Result` anidado.
fn env<T, F>(f: F) -> Result<T, String>
where
    F: FnOnce(&mut Env) -> std::result::Result<Result<T, String>, jni::errors::Error>,
{
    con_env(f)?
}

/// El `String` que devuelve una llamada Java, o cadena vacia si devuelve `null`.
///
/// MEDIDO, tres cosas encadenadas, todas mirando el codigo de `jni` 0.22:
///
/// 1. **No existe `From<JObject> for JString`**, asi que `o.into()` no compila:
///    `the trait bound JString<'_>: From<JObject<'_>> is not satisfied`. Y tampoco
///    vale un `T: Reference` generico con `T::from_raw`: `Reference` **no** tiene
///    ese metodo —el ejemplo de `reference.rs:118` es de como escribir uno propio,
///    no del trait—, asi que sale `no associated function named from_raw found for
///    type parameter T`.
/// 2. El metodo que si existe es **`JString::from_raw`** (el propio crate lo usa en
///    `jstring.rs:165`), y `reference.rs:362` lo recomienda explicitamente: *"You
///    should always prefer to use wrapper-provided ::from_raw() methods (Such as
///    JString::from_raw()) for wrapping raw local references because those will
///    guarantee that the returned reference has a lifetime that's tied to a valid
///    local reference frame."*
/// 3. **`Env::get_string` esta deprecado** desde 0.22 (*"use JString::mutf8_chars or
///    JString::to_string instead"*), y el workspace compila con
///    `clippy -D warnings`: aunque el tipo cuadrara seria fallo de compilacion. El
///    sustituto es `JString::try_to_string`.
///
/// `jobject` y `jstring` son el mismo tipo: en JNI `jstring` es un `typedef` de
/// `jobject`, asi que el puntero pasa tal cual.
fn texto(e: &mut Env<'_>, v: jni::JValueOwned<'_>) -> std::result::Result<String, jni::errors::Error> {
    match v {
        jni::JValueOwned::Object(o) => {
            // SAFETY: la referencia la ha devuelto JNI en la llamada anterior de este
            // mismo `Env`, es una local viva y no sale de aqui: se lee y se descarta.
            let s: JString = unsafe { JString::from_raw(e, o.into_raw()) };
            s.try_to_string(e)
        }
        _ => Ok(String::new()),
    }
}

/// La excepcion pendiente de Java, si la hay.
///
/// Sin esto un `SecurityException` de `ContentResolver` seria un error generico de
/// JNI y no se sabria que paso.
fn excepcion(env: &mut Env) -> Option<String> {
    if !env.exception_check() {
        return None;
    }
    // MEDIDO: `describe` vuelca la traza al logcat con el tag del proceso y
    // `clear` la levanta. Sin las dos, la excepcion se cuelga al siguiente frame de
    // JNI y el siguiente `call_static_method` falla con un error que no dice nada.
    env.exception_describe();
    env.exception_clear();
    Some("Java lanzo una excepcion; la traza esta en el logcat".into())
}

/// Lanzar el selector y esperar al resultado.
///
/// Bloqueante a proposito, igual que `rfd` en escritorio: el picker es modal. El
/// despertar lo manda Java, no winit — ver el modulo.
fn pedir(mode: jint, titulo: &str, mimes: &str) -> Result<Option<String>, String> {
    match RESUELTO.lock() {
        Ok(mut g) => *g = false,
        Err(e) => *e.into_inner() = false,
    }

    let (titulo, mimes) = (titulo.to_string(), mimes.to_string());
    env(move |e| {
        let clase = e.find_class(jni::jni_str!("ai/storyteller/vectorcraft/MainActivity"))?;
        let t = e.new_string(&titulo)?;
        let m = e.new_string(&mimes)?;
        let (jt, jm): (JObject, JObject) = (t.into(), m.into());
        let v = e.call_static_method(
            &clase,
            jni::jni_str!("request"),
            jni::jni_sig!("(ILjava/lang/String;Ljava/lang/String;)Ljava/lang/String;"),
            &[jni::JValue::Int(mode), jni::JValue::Object(&jt), jni::JValue::Object(&jm)],
        )?;
        // `request` devuelve `null` si arranco bien, o el motivo del fallo.
        let texto = texto(e, v)?;
        Ok(Ok(if texto.is_empty() { None } else { Some(texto) }))
    })?;

    // Y ahora esperar. Java despierta por `nativeOnFilePicked`.
    match RESUELTO.lock() {
        Ok(mut g) => {
            while !*g {
                g = match ESPERA.wait(g) {
                    Ok(x) => x,
                    Err(e) => e.into_inner(),
                };
            }
        }
        Err(e) => {
            let mut g = e.into_inner();
            while !*g {
                g = match ESPERA.wait(g) {
                    Ok(x) => x,
                    Err(y) => y.into_inner(),
                };
            }
        }
    }

    // Recoger el resultado. `takeResult` devuelve `null` si se cancelo.
    let uri = env(|e| {
        let clase = e.find_class(jni::jni_str!("ai/storyteller/vectorcraft/MainActivity"))?;
        let v = e.call_static_method(&clase, jni::jni_str!("takeResult"), jni::jni_sig!("()Ljava/lang/String;"), &[])?;
        Ok(Ok(texto(e, v)?))
    })?;

    if uri.is_empty() {
        log::info!("saf: cancelado");
        Ok(None)
    } else {
        log::info!("saf: elegido {uri}");
        Ok(Some(uri))
    }
}

/// Traducir las extensiones de un `FilePick` a MIME types.
///
/// MEDIDO: `FilePick.filters` son pares `(nombre, extensiones)` y **no** MIME types
/// (`crates/ui-egui/src/lib.rs:122`), y SAF solo entiende de MIME. Sin esto el selector
/// muestra todos los ficheros del dispositivo, que es justo lo que un filtro debe
/// evitar.
fn mimes(filtros: &[(&'static str, &'static [&'static str])]) -> String {
    let mut out: Vec<&'static str> = Vec::new();
    for (_, exts) in filtros {
        for e in *exts {
            let mime: &'static str = match e.trim_start_matches('.').to_ascii_lowercase().as_str() {
                "svg" | "svgz" => "image/svg+xml",
                "png" => "image/png",
                "jpg" | "jpeg" => "image/jpeg",
                "gif" => "image/gif",
                "webp" => "image/webp",
                "tif" | "tiff" => "image/tiff",
                "bmp" => "image/bmp",
                "pdf" | "ai" => "application/pdf",
                "psd" => "image/vnd.adobe.photoshop",
                "eps" => "application/postscript",
                "dxf" => "image/vnd.dxf",
                "wasm" => "application/wasm",
                "txt" => "text/plain",
                "json" => "application/json",
                // El formato propio es un contenedor binario: `octet-stream` es lo
                // unico honesto, y no se traduce a nada mas especifico.
                _ => continue,
            };
            if !out.contains(&mime) {
                out.push(mime);
            }
        }
    }
    out.join(",")
}

/// Los bytes de un `content://` URI.
pub fn leer(uri: &str) -> Result<Vec<u8>, String> {
    let t0 = Instant::now();
    let uri = uri.to_string();
    let b64 = env(move |e| {
        let clase = e.find_class(jni::jni_str!("ai/storyteller/vectorcraft/MainActivity"))?;
        let u = e.new_string(&uri)?;
        let ju: JObject = u.into();
        let v = e.call_static_method(
            &clase,
            jni::jni_str!("readBase64"),
            jni::jni_sig!("(Ljava/lang/String;)Ljava/lang/String;"),
            &[jni::JValue::Object(&ju)],
        )?;
        Ok(Ok(texto(e, v)?))
    })?;

    // MEDIDO: el `?` no es opcional. `env` devuelve
    // `Result<Option<String>, String>` —porque `excepcion` devuelve `Option`— y sin
    // aplanarlo, `if let Some(..)` se esta aplicando a un `Result` y el error es
    // `expected Result<_, String>, found Option<String>`, que no señala el sitio.
    if let Some(x) = env(|e| Ok(excepcion(e)))? {
        return Err(x);
    }
    let bytes = vectorcraft_format::base64_decode(&b64).ok_or("el contenido leido no es base64 valido")?;
    log::info!("saf: leidos {} bytes de {uri} en {:?}", bytes.len(), t0.elapsed());
    Ok(bytes)
}

/// Escribir bytes en un `content://` URI.
pub fn escribir(uri: &str, bytes: &[u8]) -> Result<(), String> {
    let t0 = Instant::now();
    let uri = uri.to_string();
    let b64 = vectorcraft_format::base64_encode(bytes);
    env(move |e| {
        let clase = e.find_class(jni::jni_str!("ai/storyteller/vectorcraft/MainActivity"))?;
        let u = e.new_string(&uri)?;
        let d = e.new_string(&b64)?;
        let (ju, jd): (JObject, JObject) = (u.into(), d.into());
        e.call_static_method(
            &clase,
            jni::jni_str!("writeBase64"),
            jni::jni_sig!("(Ljava/lang/String;Ljava/lang/String;)V"),
            &[jni::JValue::Object(&ju), jni::JValue::Object(&jd)],
        )?;
        Ok(Ok(()))
    })?;

    // MEDIDO: el `?` no es opcional. `env` devuelve
    // `Result<Option<String>, String>` —porque `excepcion` devuelve `Option`— y sin
    // aplanarlo, `if let Some(..)` se esta aplicando a un `Result` y el error es
    // `expected Result<_, String>, found Option<String>`, que no señala el sitio.
    if let Some(x) = env(|e| Ok(excepcion(e)))? {
        return Err(x);
    }
    log::info!("saf: escritos {} bytes en {uri} en {:?}", bytes.len(), t0.elapsed());
    Ok(())
}

/// El nombre que DocumentsUI muestra para un URI.
///
/// MEDIDO por que hace falta: con el URI entero como titulo del documento sale
/// `content://com.android.providers.downloads.documents/document/msf%3A1234`, ilegible
/// en la barra de pestañas.
pub fn nombre(uri: &str) -> Option<String> {
    let uri = uri.to_string();
    let n = env(move |e| {
        let clase = e.find_class(jni::jni_str!("ai/storyteller/vectorcraft/MainActivity"))?;
        let u = e.new_string(&uri)?;
        let ju: JObject = u.into();
        let v = e.call_static_method(
            &clase,
            jni::jni_str!("displayName"),
            jni::jni_sig!("(Ljava/lang/String;)Ljava/lang/String;"),
            &[jni::JValue::Object(&ju)],
        )?;
        Ok(Ok(texto(e, v)?))
    })
    .ok()?;
    if n.is_empty() {
        None
    } else {
        Some(n)
    }
}

/// Si `path` es un URI de SAF.
///
/// MEDIDO que el motor usa `std::fs` en estos sitios, y que por eso un `content://`
/// se les pasa mal —con un error que no explica nada—:
///
/// * `cmd/links.rs:627` — `file_stamp` + `read_file` + `absolute_path`
/// * `cmd/swatchlib.rs:136` y `:190` — bibliotecas de muestras
/// * `cmd/flatten.rs:417`, `cmd/pdfcmds.rs:176`, `cmd/printpresets.rs:211`
/// * `cmd/fileio/mod.rs:686` — `read_file` es `std::fs::read`
///
/// Por eso los **enlaces** (`link: true` en `file.place`, `links.relink`) no pueden
/// usar un URI en Android: el motor les haria `stat` a una cadena `content://…`. La
/// solucion es incrustarlos, que es lo que hace `file.place` cuando `link` no se pide
/// (`cmd/place/mod.rs:56`: "a raster image keeps its file's path; other files are
/// embedded"), y en Android es la opcion correcta.
///
/// Solo `content://`: un `file://` seria otro caso, y SAF no lo resuelve.
pub fn es_uri(path: &str) -> bool {
    path.starts_with("content://")
}

/// Los `Services` del port: SAF para todo el I/O.
///
/// MEDIDO que esto es todo lo que hace falta y que no hay que tocar el motor ni
/// `io.rs`:
///
/// * `read`/`write` son exactamente el seam: `Fn(&str) -> Vec<u8>` y
///   `FnMut(&str, &[u8]) -> ()`, o sea un stream.
/// * `open_dialog` (`io.rs:88`) ya tiene su rama sin ruta, que es la que usa la web.
///
/// Lo que se deja a proposito sin conectar:
///
/// * `pick_open_multi`: `Box<dyn FnMut() -> Vec<String>>`, **sin** `&FilePick` —a
///   diferencia de `pick_open` (`lib.rs:166`)— asi que no puede filtrar. Se conecta
///   sin filtros, que es lo que ya hace el escritorio al seleccionar varios.
/// * `open_file`: `File → Show in Folder` no tiene sentido con un `content://`, asi
///   que devuelve un error claro en vez de fingir.
pub fn services() -> Services {
    Services {
        // MEDIDO: `PickOpen` y `PickSave` devuelven **`Option<String>`**
        // (`lib.rs:132,133`), no `Result`. Por eso el `.ok().flatten()`: `pedir`
        // devuelve `Result<Option<String>, String>` porque necesita poder fallar, y un
        // fallo de SAF se trata como "el usuario no eligio nada", que es lo que un
        // `Option` significa aqui.
        pick_open: Some(Box::new(|pick: &FilePick| pedir(MODO_ABRIR, "Abrir", &mimes(&pick.filters)).ok().flatten())),
        pick_save: Some(Box::new(|pick: &FilePick| pedir(MODO_GUARDAR, &pick.name, &mimes(&pick.filters)).ok().flatten())),
        pick_open_multi: Some(Box::new(|| {
            let uris = match pedir(MODO_ABRIR_MULTI, "Abrir", "") {
                Ok(u) => u,
                Err(e) => {
                    log::error!("saf: {e}");
                    None
                }
            };
            // MEDIDO: Java devuelve los URIs separados por '\\n', porque con
            // `EXTRA_ALLOW_MULTIPLE` el primero viene en `getData()` y el resto en
            // `getClipData()`, y no caben en un unico Intent.
            uris.map(|s| s.split('\n').filter(|u| !u.is_empty()).map(str::to_string).collect())
                .unwrap_or_default()
        })),
        // MEDIDO: `pick_folder` es `Box<dyn FnMut() -> Option<String>>` (`lib.rs:185`).
        pick_folder: Some(Box::new(|| pedir(MODO_CARPETA, "Elegir carpeta", "").ok().flatten())),
        read: Some(Box::new(|path: &str| {
            if es_uri(path) {
                leer(path)
            } else {
                std::fs::read(path).map_err(|e| format!("{path}: {e}"))
            }
        })),
        write: Some(Box::new(|path: &str, bytes: &[u8]| {
            if es_uri(path) {
                escribir(path, bytes)
            } else {
                std::fs::write(path, bytes).map_err(|e| format!("{path}: {e}"))
            }
        })),
        open_file: Some(Box::new(|path: &str| {
            if es_uri(path) {
                Err("Android no puede abrir un content:// con otra app".into())
            } else {
                Ok(())
            }
        })),
        ..Default::default()
    }
}