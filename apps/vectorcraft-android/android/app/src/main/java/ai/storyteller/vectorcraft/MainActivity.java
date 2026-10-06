package ai.storyteller.vectorcraft;

import android.app.NativeActivity;
import android.os.Bundle;

/**
 * El Activity de VectorCraft en Android.
 *
 * Hereda de {@link NativeActivity} y no hace nada mas: la libreria nativa toma el
 * control desde {@code android_main}.
 *
 * <h2>Por que Java y no Kotlin</h2>
 *
 * MEDIDO: la primera version de esta clase estaba en Kotlin, y el APK salia sin
 * ella:
 *
 * <pre>
 *   java.lang.ClassNotFoundException: Didn't find class
 *   "ai.storyteller.vectorcraft.MainActivity"
 * </pre>
 *
 * porque el build solo aplicaba el plugin de Android y no el de Kotlin. Gradle
 * compila {@code .java} de serie y {@code .kt} no: sin el plugin, el fichero se
 * ignora en silencio, el APK se construye igual de bien y simplemente no lleva la
 * clase. Un fallo invisible hasta el ultimo momento.
 *
 * La alternativa a esto era anadir el plugin de Kotlin y su toolchain entero para
 * una clase que son tres lineas. La documentacion de {@code android-activity} dice
 * que hace falta "at least a small amount of Java <b>or</b> Kotlin", asi que Java
 * cumple y quita una dependencia.
 *
 * <h2>Por que NativeActivity</h2>
 *
 * Viene con el propio sistema operativo: no hay ninguna dependencia de Maven que
 * resolver ni codigo Java que compilar. winit localiza el {@code .so} por el
 * {@code android.app.lib_name} del manifiesto ({@code vectorcraft_android}) y llama
 * a {@code android_main} dentro de el.
 *
 * El limite conocido, escrito tambien en el modulo del crate: NativeActivity no
 * tiene soporte de metodos de entrada, asi que <b>el teclado blando no
 * aparece</b> en Android moderno. Lo arregla GameActivity con la libreria
 * {@code androidx.games:games-activity:4.4.0}. Ver el comentario de
 * {@code apps/vectorcraft-android/src/lib.rs}.
 *
 * <h2>Si la libreria faltara</h2>
 *
 * El fallo sale aqui como {@code UnsatisfiedLinkError}, que es el primer sitio
 * donde se ve un APK mal empaquetado. Por eso el CI comprueba con {@code unzip -l}
 * que la libreria va dentro del APK antes de darlo por bueno — y por eso el APK se
 * comprueba tambien con {@code aapt}: un APK que existe y no lleva la clase es
 * exactamente este caso.
 */
public class MainActivity extends NativeActivity {
    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
    }
}
