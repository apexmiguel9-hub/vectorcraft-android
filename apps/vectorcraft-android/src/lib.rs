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
//! ## El browser de ficheros
//!
//! Lo dibujamos nosotros con `egui_file`, dentro de la UI, y no con SAF. No es
//! capricho: **abrir una segunda Activity da ANR en este port**, porque
//! `android-activity` bloquea el hilo principal de Java hasta que el hilo del bucle
//! confirma la pausa, y ese hilo es el que corre el update de egui. La traza entera
//! esta en el modulo [`browser`].
//!
//! ## Insets
//!
//! `egui::InputState::safe_area_insets()` cubre status bar, barra de navegacion y
//! notch en las plataformas donde winit lo implementa. Ojo: en `winit` v0.30 esa
//! ruta solo esta implementada para iOS; en Android hay que reservarlo a mano (el
//! ejemplo de egui reserva 32 puntos arriba con un `Panel::top`).

use vectorcraft_engine::Session;
use vectorcraft_ui_egui::VectorcraftApp;

#[cfg(target_os = "android")]
mod browser;
#[cfg(target_os = "android")]
mod permiso;

/// El `eframe::App` del port. Tres reenvios, porque todo el editor —50k lineas de
/// UI, 52 paneles, menus, canvas, atajos— ya vive en `VectorcraftApp`.
pub struct App(pub VectorcraftApp);

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // MEDIDO, y el orden importa: el browser va **antes** que el editor. Si acaba de
        // recoger un fichero, lo vuelca en el inbox aqui y `drain_inbox`
        // (`crates/ui-egui/src/lib.rs:676`) lo abre en este mismo frame. Al reves habia
        // un frame de retraso, que en un movil de 120 fps se ve.
        #[cfg(target_os = "android")]
        browser::logic(ctx);
        self.0.logic(ctx);
    }

    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw: &mut egui::RawInput) {
        self.0.raw_input_hook(raw);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.0.ui(ui);
        // MEDIDO: el aviso va en una `Window` propia y **no** en el `Ui` del editor.
        //
        // Dibujado en el `Ui` salia pegado a la barra de estado, encima del texto de
        // ayuda, y se leia todo junto:
        //
        //     Click thumbnail to select · Ctrl/Cmd-click to switch multiple objects ·
        //     Alt+… Permitir  ⎌ Ahora no  :ate
        //
        // MEDIDO tambien el desplazamiento: los 24 pt de la barra de estado se reservan a
        // mano porque winit 0.30 solo rellena `safe_area_insets` en iOS.
        #[cfg(target_os = "android")]
        egui::Window::new("permiso de almacenamiento")
            .anchor(egui::Align2::CENTER_TOP, egui::Vec2::new(0.0, 44.0))
            .collapsible(false)
            .resizable(false)
            .title_bar(false)
            .show(ui.ctx(), browser::aviso);
    }
}

/// Construye el editor.
///
/// `Services` va sin los hooks de escritorio a proposito: sin `pick_open` (no hay
/// dialogo nativo), sin `system_clipboard` (egui hace el suyo), sin `open_url`.
/// Cada hueco degrada a un mensaje en vez de romper, que es el mismo camino que usa
/// la build web.
pub fn build(_cc: &eframe::CreationContext<'_>) -> std::result::Result<Box<dyn eframe::App>, String> {
    #[cfg(target_os = "android")]
    let services = browser::services();
    // MEDIDO: el import de `Services` sobra en Android porque aqui solo se usa para el
    // `default()` de escritorio, y el de Android lo construye `browser::services()`. Por
    // eso va con la ruta completa y no en el `use`.
    #[cfg(not(target_os = "android"))]
    let services = vectorcraft_ui_egui::Services::default();

    Ok(Box::new(App(VectorcraftApp::new(Session::new(), services))))
}

/// Opciones de ventana.
///
/// Pantalla completa y sin marco: un canvas vectorial quiere toda la pantalla, y los
/// paneles van dentro de la UI en vez de en barras del sistema.
/// La configuracion de wgpu que se usa en Android: **solo OpenGL ES**.
///
/// MEDIDO por que, y por que no es "Vulkan con menos cosas".
///
/// Vulkan en este movil esta **completo**, y conviene decirlo porque es lo que
/// hace el crash desconcertante. Verificado con el perfil de Vulkan del
/// `vp_gpuinfo` del Moto G56 5G (Android 16):
///
/// | | |
/// |---|---|
/// | `apiVersion` | **1.3.1023** (el informe de la base de datos lo da como 1.3.303) |
/// | extensiones | **129**, de las que 61 son `VK_KHR` |
/// | `VK_ANDROID_external_memory_android_hardware_buffer` | si |
/// | `driverID` | `VK_DRIVER_ID_IMAGINATION_PROPRIETARY`, "PowerVR B-Series" 25.1 |
/// | subgroup | 128, con todas las operaciones |
///
/// O sea: **no falta ninguna feature.** El problema es otro.
///
/// wgpu elige solo, y elige Vulkan, que en Android es el backend primario:
///
///     There are 2 available wgpu adapters: {backend: Vulkan, "PowerVR B-Series
///     BXM-8-256"}, {backend: Gl, "PowerVR B-Series BXM-8-256"}
///
/// Y al crear el primer pipeline del editor el proceso muere. Los seis frames
/// interiores del tombstone no son de Rust, son del driver:
///
///     #00 libc.so           (abort+156)
///     #01-03 libufwriter.so  (BILParseStream+164)
///     #04-06 vulkan.mtk.so   (FragmentShaderCompileState::CompileUF()+1272)
///     #07 wgpu_hal::vulkan::device::Device::create_render_pipeline
///     #14 egui_wgpu::renderer::Renderer::new
///     #15 egui_wgpu::Painter::set_window
///
/// Leyendolo de abajo arriba: `egui_wgpu::Painter` pide su primer pipeline, `wgpu`
/// lo pasa a Vulkan, y **`vulkan.mtk.so` aborta mientras compila el fragment
/// shader** —`FragmentShaderCompileState::CompileUF`— dentro de `libufwriter.so`,
/// que es el escritor de binarios del compilador propietario (`BIL` es su IR).
///
/// **El driver no loguea nada**: no hay mensaje de asercion en el logcat, solo el
/// `abort()`. Esta compilado en release y el assert es un `abort()` a pelo. Por eso
/// no se puede saber que construccion concreta del SPIR-V le molesta: no lo dice.
///
/// ## Lo que cuesta esta decision, que es cero aqui
///
/// VectorCraft **rasteriza en CPU**, con `vello_cpu`. La GPU no dibuja vectores:
/// solo presenta los pixeles que la CPU ya ha pintado, a traves de egui. Toda la
/// parte de Vulkan que de verdad importa —compute shaders, para rasterizar en GPU—
/// es justo la que este proyecto no usa, y lo hizo a proposito para no depender del
/// driver. aqui el coste de GL es cero.
///
/// ## Lo que se deja anotado
///
/// Si algun dia el driver de PowerVR deja de abortar, esto es **una linea**: quitar
/// la funcion y dejar `renderer: eframe::Renderer::Wgpu`. Y si se quiere probar sin
/// recompilar, el propio egui-wgpu tiene la variable de entorno:
///
///     WGPU_BACKEND=opengl
///
/// que se lee en `WgpuSetupCreateNew::without_display_handle()`, de donde sale el
/// `WgpuConfiguration::default()`. Esta que se fija a mano, y no solo por la variable,
/// para que no dependa de cuando se lea el entorno.
fn wgpu_solo_opengl() -> eframe::egui_wgpu::WgpuConfiguration {
    // `WgpuSetupCreateNew` no es `#[non_exhaustive]`, pero en vez de usar sintaxis de
    // actualizacion se parte de su propio constructor y se cambia el campo. Es lo
    // que menos puede romperse al actualizar egui: no depende de que campos existan
    // los que no se nombran.
    let mut setup = eframe::egui_wgpu::WgpuSetupCreateNew::without_display_handle();
    setup.instance_descriptor.backends = eframe::egui_wgpu::wgpu::Backends::GL;
    eframe::egui_wgpu::WgpuConfiguration {
        wgpu_setup: eframe::egui_wgpu::WgpuSetup::CreateNew(setup),
        ..Default::default()
    }
}

fn window_options() -> eframe::NativeOptions {
    eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([360.0, 480.0])
            .with_fullscreen(true),
        // MEDIDO: el render **no** es `eframe::Renderer::Wgpu` pelado, sino el mismo
        // renderizador con el backend de **OpenGL ES** y solo ese.
        //
        // Con `Renderer::Wgpu`, egui-wgpu deja el backend a su cuenta:
        //
        //     backends: wgpu::Backends::from_env()
        //         .unwrap_or(wgpu::Backends::PRIMARY | wgpu::Backends::GL),
        //
        // y en Android `PRIMARY` es Vulkan, que gana. O sea: el port iba a Vulkan.
        //
        // ## Por que Vulkan es un callejon en este movil
        //
        // MEDIDO, con la cadena de 35 frames del tombstone. wgpu encuentra los dos
        // adaptadores y elige el de Vulkan:
        //
        //     There are 2 available wgpu adapters: {backend: Vulkan, ... "PowerVR
        //     B-Series BXM-8-256"}, {backend: Gl, ...}
        //
        // y al crear el primer pipeline revienta. Los seis frames interiores no son
        // de Rust, son del driver:
        //
        //     #00 libc.so              (abort+156)
        //     #01-03 libufwriter.so     (BILParseStream+164)
        //     #04-06 vulkan.mtk.so      (FragmentShaderCompileState::CompileUF()+1272)
        //     #07 wgpu_hal::vulkan::device::Device::create_render_pipeline
        //     #08 wgpu_hal::dynamic::device::vulkan::DynDevice::create_render_pipeline
        //     #09 wgpu_core::device::resource::Device::create_render_pipeline_inner
        //     #10 wgpu_core::device::resource::Device::create_render_pipeline
        //     #11 wgpu_core::device::global::Global::device_create_general_render_pipeline
        //     #12 wgpu_core::device::global::Global::device_create_render_pipeline
        //     #13 wgpu_core::backend::wgpu_core::CoreDevice::<dispatch>::DeviceInterface::create_render_pipeline
        //     #14 egui_wgpu::renderer::Renderer::new
        //     #15 egui_wgpu::Painter::set_window
        //     #16 eframe::native::wgpu_integration::WgpuWinitApp::resumed
        //
        // Se puede leer de abajo arriba y es inequivoco: `egui_wgpu::Painter` pide su
        // primer pipeline, `wgpu` lo pasa a Vulkan, y **`vulkan.mtk.so` aborta
        // mientras compila el fragment shader** —`FragmentShaderCompileState::CompileUF`—
        // dentro de `libufwriter.so`, que es el escritor de binarios del compilador
        // propietario. `abort()` en `libc`, no un panic.
        //
        // Eso explica por que el hook de panics no decia nada: **no hay ningun panic
        // de Rust**. El proceso lo mata el driver del sistema.
        //
        // ## Lo que se hace
        //
        // Quedarse **solo con OpenGL ES**, que el mismo logCat dice que tambien esta
        // disponible y que es el camino que si funciona en estos PowerVR sobre Android:
        // el motor de VectorCraft rasteriza en CPU con `vello_cpu`, asi que la GPU
        // solo presenta pixeles y no tiene nada que hacer con esto. Passarse por
        // OpenGL no cuesta nada al dibujo.
        // MEDIDO: en eframe 0.36 `Renderer::Wgpu` es una variante **unitaria**, no
        // lleva configuracion dentro. Sale:
        //
        //     error[E0559]: variant `eframe::Renderer::Wgpu` has no field named
        //                    `wgpu_configuration`
        //
        // La configuracion va en su propio campo de `NativeOptions`:
        //
        //     /// Configures wgpu instance/device/adapter/surface creation and renderloop.
        //     pub wgpu_options: egui_wgpu::WgpuConfiguration,
        renderer: eframe::Renderer::Wgpu,
        wgpu_options: wgpu_solo_opengl(),
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

    // MEDIDO: el permiso de almacenamiento necesita el `JavaVM`, y `android-activity`
    // solo lo expone a traves del `AndroidApp`. Se registra aqui, antes de construir las
    // opciones, porque `Services` lo consulta en cuanto el usuario abre un fichero.
    #[cfg(target_os = "android")]
    permiso::registrar(&app);

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