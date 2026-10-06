//! VectorCraft en Android: el crate, no el binario.
//!
//! `VectorcraftApp` es el MISMO editor que corre en escritorio y en la web. No hay
//! una UI para movil aparte. Este crate solo monta la ventana y conecta los tres
//! metodos que `eframe::App` necesita.
//!
//! ## Render: CPU, no GPU
//!
//! El motor rasteriza con `vello_cpu`: CPU pura, con SIMD para aarch64 y multihilo
//! con rayon. En un movil no depende de compute shader ni de lo que el driver de la
//! GPU sepa hacer. La GPU solo presenta pixeles ya calculados, por egui-wgpu.
//!
//! ## Gestos: ya estan hechos, y verificado por que
//!
//! egui 0.36 trae `zoom_delta()`, `rotation_delta()`, `translation_delta()` y
//! `multi_touch()`. No hay codigo de gestos aqui porque no hay que escribirlo.
//!
//! Que Android le llegue varios dedos esta verificado leyendo winit 0.30.13,
//! `src/platform_impl/android/mod.rs`:
//!
//! ```text
//! MotionAction::Down | PointerDown => TouchPhase::Started
//! MotionAction::Move                => TouchPhase::Moved, con motion_event.pointers()
//!                                                        (TODOS los dedos, no solo el primero)
//! id: pointer.pointer_id() as u64                            (id distinto por dedo)
//! ```
//!
//! egui-winit convierte cada `WindowEvent::Touch` en un `egui::Event::Touch` con su
//! `TouchId`, y `Context::multi_touch()` agrupa por id. Pinch-zoom y pan de dos
//! dedos funcionan sin codigo nuestro.
//!
//! ## El punto de entrada, y por que este y no otro
//!
//! MEDIDO, y fue el fallo que mas tiempo costo: **un `cdylib` de Rust sin ningun
//! `#[no_mangle]` se queda vacio.** El enlazador borra todo lo que no sea alcanzable
//! desde las exportaciones, y en Rust lo unico que se exporta es lo marcado con
//! `#[no_mangle]`. Sin eso, el `.so` compila, Gradle lo empaqueta, la app instala
//! y muere al arrancar —sin `.eh_frame` ni pista.
//!
//! Lo que se veia era un `.so` de 5,8 MB con **566 KB de `.text` y 3 simbolos
//! dinamicos** para 225.484 lineas de codigo. El tamano no lo delata: la mayor parte
//! de esos 5,8 MB eran datos de depuracion comprimidos. "Es pequeno" y "esta roto"
//! se parecen, asi que el workflow comprueba la tabla de simbolos en vez de
//! fiarse del peso.
//!
//! De ahi sale la forma exacta de `android_main`, que es **el patron del ejemplo
//! oficial de egui** (`eframe/examples/hello_android`): el `AndroidApp` **no se
//! pasa como argumento aparte**, va en `NativeOptions::android_app`, porque es lo
//! que el `EventLoop` de winit necesita para construirse.
//!
//! ## Teclado: `native-activity` y lo que cuesta
//!
//! Hay un reporte reproducible de winit de que con `android-native-activity` el
//! teclado blando **no aparece** en Android moderno (Galaxy S23 / Android 16),
//! mientras que con `android-game-activity` si. El motivo es que el teclado lo
//! gestiona el `InputMethodManager` a traves de una vista, y `NativeActivity` no
//! tiene una.
//!
//! Se usa `android-native-activity` **aun asi**, porque es el unico camino con un
//! ejemplo oficial que se sabe que arranca, y lo prioritario es que la libreria se
//! vea entera y la app abra. El teclado es el siguiente problema, no el primero.
//!
//! Cuando se quiera, cambiar a `android-game-activity` es: la feature en el
//! `Cargo.toml`, el `android-activity` con `features = ["game-activity"]`, y el
//! `MainActivity` que herede de `GameActivity` en vez de `NativeActivity`.
//!
//! ## Insets
//!
//! `egui::InputState::safe_area_insets()` cubre status bar, barra de navegacion y
//! notch en las plataformas donde winit lo implementa. Ojo: en `winit` v0.30 esa
//! ruta solo esta implementada para iOS; en Android hay que reservarlo a mano (el
//! ejemplo de egui reserva 32 puntos arriba con un `Panel::top`).

use vectorcraft_engine::Session;
use vectorcraft_ui_egui::{Services, VectorcraftApp};

/// El `eframe::App` del port. Tres reenvios, porque todo el editor —50k lineas de
/// UI, 52 paneles, menus, canvas, atajos— ya vive en `VectorcraftApp`.
pub struct App(pub VectorcraftApp);

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.0.logic(ctx);
    }

    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw: &mut egui::RawInput) {
        self.0.raw_input_hook(raw);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.0.ui(ui);
    }
}

/// Construye el editor.
///
/// `Services` va sin los hooks de escritorio a proposito: sin `pick_open` (no hay
/// dialogo nativo), sin `system_clipboard` (egui hace el suyo), sin `open_url`.
/// Cada hueco degrada a un mensaje en vez de romper, que es el mismo camino que usa
/// la build web.
pub fn build(_cc: &eframe::CreationContext<'_>) -> std::result::Result<Box<dyn eframe::App>, String> {
    Ok(Box::new(App(VectorcraftApp::new(Session::new(), Services::default()))))
}

/// Opciones de ventana.
///
/// Pantalla completa y sin marco: un canvas vectorial quiere toda la pantalla, y los
/// paneles van dentro de la UI en vez de en barras del sistema.
fn window_options() -> eframe::NativeOptions {
    eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([360.0, 480.0])
            .with_fullscreen(true),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    }
}

/// Arranque en escritorio, para probar sin movil.
pub fn run() -> eframe::Result {
    eframe::run_native("VectorCraft", window_options(), Box::new(|cc| build(cc).map_err(|e| e.into())))
}

/// Punto de entrada en Android.
///
/// Sin esto el `.so` sale VACIO. Ver el comentario del modulo: un `cdylib` sin
/// `#[no_mangle]` se queda sin codigo porque el enlazador tira lo que no sea
/// alcanzable desde las exportaciones.
// MEDIDO: `#[unsafe(no_mangle)]` y no `#[no_mangle]`. En Rust moderno `no_mangle`
// es un atributo `unsafe`, y el workspace prohibe `unsafe_code`. El ejemplo oficial
// de egui desactiva los lints del workspace por el mismo motivo; este crate hace
// lo mismo, y es el unico sitio del proyecto donde hace falta `unsafe`.
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "C" fn android_main(app: winit::platform::android::activity::AndroidApp) {
    // Al log de Android. Sin esto, `log::info!` del motor no aparece en el logcat y
    // no hay ni un solo dato de lo que hace la libreria en el movil.
    android_logger::init_once(android_logger::Config::default().with_max_level(log::LevelFilter::Info));

    // MEDIDO que hace falta, y el motivo es concreto.
    //
    // `log` **descarta todo en silencio hasta que se llama a `set_max_level`**: su
    // `MAX_LOG_LEVEL_FILTER` empieza en `LevelFilter::Off`. Sin esto, ni un solo
    // `log::info!` de esta libreria ni del motor llega al logcat.
    //
    // Se ha comprobado que `android_logger` 0.14.1 lo hace, pero solo en una
    // condicion muy estrecha:
    //
    //     pub fn init_once(config: Config) {
    //         let log_level = config.log_level;
    //         let logger = ANDROID_LOGGER.get_or_init(|| AndroidLogger::new(config));
    //         if let Err(err) = log::set_logger(logger) {
    //             log::debug!("...set_logger failed: {err}");
    //         } else if let Some(level) = log_level {
    //             log::set_max_level(level);      // <- solo si no hay logger puesto
    //         }
    //     }
    //
    // Es decir: **si ya hay un logger instalado, `set_max_level` no se llama nunca**
    // y el nivel se queda en `Off` para siempre. Depender de esa rama para tener
    // logs es depender de que nadie haya puesto un logger antes, que no es una
    // garantia: el orden de inicializacion dentro de un grafo de 393 crates no lo
    // tiene uno.
    //
    // Ponerlo aqui lo hace explicito y no condicional. Y el `log::info!` siguiente
    // imprime el nivel resuelto, que es la medicion que faltaba.
    log::set_max_level(log::LevelFilter::Info);
    log::info!("nivel de log resuelto: {}", log::max_level());

    // MEDIDO que hace falta, y de paso una leccion del otro repo: **en Android, un
    // panic de Rust es invisible.** El gancho por defecto escribe en `stderr`, y
    // `stderr` no llega a logcat si el proceso no tiene stdout asociado. Ya se ha
    // visto ahi: un `eprintln!` en un bucle de diez mil llamadas no aparecio ni una
    // vez.
    //
    // Aqui el sintoma es peor: el proceso muere con
    //
    //     Fatal signal 6 (SIGABRT) ... in tid ... (android_main)
    //
    // y el backtrace acaba en `android_main+320` — dentro de la funcion, sin decir
    // que linea. Se sabe que se ha panicado y no se sabe por que.
    //
    // Este hook manda el panic al logcat, que si llega, y de ahi se ve el mensaje
    // entero con su fichero y su linea.
    std::panic::set_hook(Box::new(|info| {
        log::error!("PANIC de Rust: {info}");
        // Y tambien a `stderr`, que en un debugger o en un `adb logcat` con el
        // proceso asociado si aparece, y no cuesta nada.
        eprintln!("PANIC de Rust: {info}");
    }));

    // El `AndroidApp` va DENTRO de las opciones, no como argumento aparte: es lo que
    // el `EventLoop` de winit necesita para construirse, y sin el se queda sin
    // bucle de eventos.
    let options = eframe::NativeOptions { android_app: Some(app), ..window_options() };

    log::info!("vectorcraft-android: arrancando, version {}", env!("CARGO_PKG_VERSION"));

    let resultado = eframe::run_native("VectorCraft", options, Box::new(|cc| build(cc).map_err(|e| e.into())));

    // MEDIDO por que esto NO es un `panic!`: el proyecto prohibe panicos en codigo que
    // se reparte, y este es ese codigo. `run_native` devuelve `Err` si no puede
    // crear la ventana o el contexto de render, y un abort aqui sale como "Fatal
    // signal 6", que no dice nada de por que.
    if let Err(e) = resultado {
        log::error!("vectorcraft-android: no se pudo arrancar: {e:?}");
    }
}