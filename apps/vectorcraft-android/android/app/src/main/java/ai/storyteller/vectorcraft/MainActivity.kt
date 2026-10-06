package ai.storyteller.vectorcraft

import android.app.NativeActivity
import android.os.Bundle

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
        // MEDIDO: esto es todo lo que hace la clase, y es a proposito.
        //
        // `android.app.NativeActivity` viene con el propio sistema operativo, asi
        // que no hay ni una dependencia de Maven que resolver y ni una linea de Java
        // que compilar. `super.onCreate` localiza el `.so` por el
        // `android.app.lib_name` del manifiesto (`vectorcraft_android`) y winit
        // llama a `android_main` dentro de el.
        //
        // El limite conocido, y esta escrito en el modulo del crate: `NativeActivity`
        // no tiene soporte de metodos de entrada, asi que **el teclado blando no
        // aparece** en Android moderno. Lo arregla `GameActivity` con la libreria
        // `androidx.games:games-activity:4.4.0`. Ver el comentario del modulo.
        //
        // Si el `.so` no estuviera ahi, el fallo sale aqui como
        // `UnsatisfiedLinkError`, que es el primer sitio donde se ve un APK mal
        // empaquetado. Por eso el CI comprueba con `unzip -l` que la libreria va
        // dentro del APK antes de darlo por bueno.
        super.onCreate(savedInstanceState)
    }
}