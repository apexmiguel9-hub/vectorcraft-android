package ai.storyteller.vectorcraft

import android.os.Bundle
import org.mozilla.android.activity.NativeActivity

/**
 * El Activity de VectorCraft en Android.
 *
 * Hereda de [NativeActivity] y no hace nada mas: la libreria nativa toma el
 * control desde `android_main`.
 *
 * **Por que `NativeActivity` y no `GameActivity`.**
 *
 * Es lo que usa el ejemplo oficial de egui, y es el unico camino con un
 * `android_main` que se sabe que arranca. `GameActivity` busca el punto de entrada
 * por otro mecanismo y no tiene ejemplo equivalente.
 *
 * **Lo que cuesta, medido.** Con `NativeActivity` el teclado blando no aparece en
 * Android moderno: hay un reporte reproducible de un Galaxy S23 con Android 16 en el
 * hilo de winit v0.30 —*"the software keyboard does not show at all"*— porque el
 * teclado lo gestiona el `InputMethodManager` a traves de una vista, y
 * `NativeActivity` no tiene ninguna.
 *
 * Se acepta ese problema a proposito: primero que la app abra y se vea el editor
 * entero, despues el teclado. Cuando se quiera arreglar, son tres cambios y estan
 * anotados en `apps/vectorcraft-android/src/lib.rs`: la feature del `Cargo.toml`,
 * el `android-activity` con `features = ["game-activity"]`, y esta clase pasando a
 * `GameActivity`.
 *
 * **El render no se resiente.** El documento se rasteriza en CPU con `vello_cpu`; la
 * GPU solo presenta pixeles ya calculados. No hay compute shader en el camino, asi
 * que el mismo binario funciona igual en un PowerVR de gama baja que en un
 * Snapdragon.
 *
 * **Textos sin acentos en los comentarios** a proposito: se compilan con el NDK sin
 * depender de la codificacion de ficheros.
 */
class MainActivity : NativeActivity() {
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