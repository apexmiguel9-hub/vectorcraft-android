//! VectorCraft en Android: el crate, no el binario.
//!
//! # Que hay aqui y por que
//!
//! `VectorcraftApp` es el MISMO editor que corre en escritorio y en la web. No hay
//! una UI para movil aparte. Este crate solo monta la ventana y conecta los tres
//! metodos que `eframe::App` necesita (`logic`, `raw_input_hook`, `ui`).
//!
//! ## Render: CPU, no GPU
//!
//! El motor rasteriza con `vello_cpu`: CPU pura, con SIMD para aarch64 y
//! multihilo con rayon. En un movil no depende de compute shader ni de lo que el
//! driver de la GPU sepa hacer. La GPU solo presenta pixeles ya calculados, a
//! traves de egui-wgpu.
//!
//! ## Gestos: ya estan hechos, y verificado por que
//!
//! egui 0.36 trae `zoom_delta()`, `zoom_delta_2d()`, `rotation_delta()`,
//! `translation_delta()` y `multi_touch()`. No hay codigo de gestos aqui porque no
//! hay que escribirlo.
//!
//! Que egui reciba varios dedos en Android esta verificado leyendo el backend de
//! winit v0.30.13, `src/platform_impl/android/mod.rs`:
//!
//! ```text
//! MotionAction::Down | PointerDown => TouchPhase::Started
//! MotionAction::Move                => TouchPhase::Moved, con motion_event.pointers()
//!                                                        (TODOS los dedos, no solo el primero)
//! id: pointer.pointer_id() as u64                            (id distinto por dedo)
//! ```
//!
//! egui-winit convierte cada `WindowEvent::Touch` en un `egui::Event::Touch` con su
//! `TouchId`, y `Context::multi_touch()` agrupa por id. Es exactamente el protocolo
//! que egui espera, asi que pinch-zoom y pan de dos dedos funcionan sin codigo
//! nuestro.
//!
//! ## Teclado: por que `android-game-activity`
//!
//! En el hilo de winit v0.30 hay un reporte reproducible de un Galaxy S23 con
//! Android 16: con `android-game-activity` el teclado blando abre y responde; con
//! `android-native-activity` no aparece. GameActivity da una jerarquia de vistas de
//! Android real, que es lo que necesita el InputMethodManager.
//!
//! La documentacion de Android es explicita en que no se puede depender de que el
//! teclado blando mande eventos de tecla, asi que el texto llega como `Event::Text`,
//! no como pulsaciones. egui lo gestiona.
//!
//! ## Insets
//!
//! `egui::InputState::safe_area_insets()` ya cubre status bar, barra de navegacion
//! y notch. `content_rect()` da el area segura, `viewport_rect()` la completa. La
//! UI no se mete debajo de la camara sin codigo extra.

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

/// Opciones de ventana.
///
/// Pantalla completa y sin marco: un canvas vectorial quiere toda la pantalla, y los
/// paneles van dentro de la UI en vez de en barras del sistema. El tamano minimo es
/// de un movil pequeno en vertical.
pub fn native_options() -> eframe::NativeOptions {
    eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([360.0, 480.0])
            .with_fullscreen(true),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    }
}

/// Construye el editor.
///
/// `Services` va sin los hooks de escritorio a proposito: sin `pick_open` (no hay
/// dialogo nativo de archivos), sin `system_clipboard` (egui hace el suyo), sin
/// `open_url`. Cada hueco degrada a un mensaje en vez de romper, que es el mismo
/// camino que usa la build web. Anadir SAF de Android es trabajo posterior y
/// localized, no un bloqueante para pintar.
pub fn build(_cc: &eframe::CreationContext<'_>) -> std::result::Result<Box<dyn eframe::App>, String> {
    Ok(Box::new(App(VectorcraftApp::new(Session::new(), Services::default()))))
}

/// Arranque en escritorio, para probar sin movil.
pub fn run() -> eframe::Result {
    eframe::run_native("VectorCraft", native_options(), Box::new(|cc| build(cc).map_err(|e| e.into())))
}