package ai.storyteller.vectorcraft

import android.os.Bundle
import androidx.games.activity.GameActivity

/**
 * El Activity de VectorCraft en Android.
 *
 * Lo único que hace es heredar de [GameActivity] y dejar que la librería nativa
 * arranque. Ese es el motivo de la choice de backend, y está medido:
 *
 * **Por que `GameActivity` y no `NativeActivity`.**
 *
 * Con `native-activity` el teclado blando **no aparece** en Android moderno. Hay un
 * reporte reproducible de un Galaxy S23 con Android 16 en el hilo de winit v0.30:
 * *"the software keyboard does not show at all"*. La razón es que el teclado lo
 * hace el `InputMethodManager` a través de una vista de Android, y `NativeActivity`
 * no tiene una.
 *
 * `GameActivity` sí tiene una: trae una jerarquía de vistas de verdad. Por eso el
 * teclado abre y responde.
 *
 * **Segunda razón, igual de importante.** `GameActivity` aporta un `SurfaceView` y
 * un `Surface` de Android de verdad, que es lo que egui-wgpu necesita para
 * presentar píxeles. Con `NativeActivity` hay que arrancar el swapchain a mano.
 *
 * **Y esto no estropea el render.** El documento se rasteriza en CPU con `vello_cpu`;
 * la GPU solo presenta píxeles ya calculados. No hay compute shader en el camino, así
 * que el mismo binario funciona en un PowerVR de gama baja igual que en un
 * Snapdragon.
 *
 * **Teclado: llega como texto, no como teclas.** La documentación de Android es
 * explícita: *"You should never rely on receiving KeyEvents for any key on a soft
 * input method"*. egui gestiona `Event::Text` por su cuenta, así que escribir texto
 * en una capa funciona; lo que no hay es atajos de teclado, y en un móvil no los
 * hay.
 */
class MainActivity : GameActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        // `GameActivity.onCreate` carga el .so declarado en el manifest
        // (`android.app.lib_name` = `vectorcraft_android`) y le pasa el control al
        // entry point de Rust. Si el `.so` no existe o no compila, `System.loadLibrary`
        // lanza aquí y Android mata el proceso con `UnsatisfiedLinkError`. Es el
        // primer sitio donde falla un APK mal empaquetado, y conviene verlo en el
        // logcat como esto y no como un "la app no arranca".
        super.onCreate(savedInstanceState)
    }
}