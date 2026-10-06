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

        // ------------------------------------------------------------------
        // Que la ventana respete las barras del sistema.
        //
        // MEDIDO, con `dumpsys window` y con el log del dialogo, en el
        // moto g56 5G (2400x1080, densidad 2.4375):
        //
        //     InsetsSource type=statusBars      frame=[0,0][2400,59]
        //     InsetsSource type=navigationBars  frame=[2283,0][2400,1080]
        //     InsetsSource type=displayCutout   frame=[0,0][115,1080]
        //
        //     dialog newDocument: viewport [[0.0 0.0] - [937.4 443.1]]
        //                         ventana  [[12.3 -13.5] - [925.3 443.0]]
        //
        // La ventana del proceso mide 2286 de ancho —que es 2400 menos los 115 del
        // notch, o sea que **si** lo respeta— y **1080 de alto enteros, que es la
        // pantalla completa con los 59 de la barra de estado dentro**. De ahi el
        // `y = -13.5`: el borde superior del dialogo cae bajo la barra de
        // notificaciones, y las pestañas de plantillas (Mobile, Web, Print...)
        // quedan pegadas al reloj.
        //
        // Y el ancho util no son 937 puntos sino `937 - 115/2.4375 - 117/2.4375 =
        // 842`, que es por lo que el dialogo, que pedia 913, se salia por la derecha.
        //
        // MEDIDO tambien: winit 0.30 **no rellena `safe_area_insets` en Android**.
        // Solo lo hace en iOS:
        //
        //     $ grep -rl safe_area winit-0.30.13/src/
        //     winit-0.30.13/src/platform_impl/ios/app_state.rs
        //     winit-0.30.13/src/platform_impl/ios/window.rs
        //
        // Asi que `egui` recibe ceros y no puede reservar nada por su cuenta. La
        // ventana tiene que estar bien colocada antes.
        //
        // Comprobado tambien que `android-activity` **no** pone este flag al
        // arrancar: `LAYOUT_NO_LIMITS` solo aparece como constante y en el metodo
        // `AndroidApp::set_window_flags`, que nadie llama. Se quita igual, porque
        // ponerlo es idempotente y deja constancia de la intencion.
        getWindow().clearFlags(android.view.WindowManager.LayoutParams.FLAG_LAYOUT_NO_LIMITS);

        // API 30+: sustituye a `FLAG_LAYOUT_NO_LIMITS` su equivalente moderno. Con
        // `minSdk 24` hace falta el guard: `setDecorFitsSystemWindows` no existe
        // antes.
        if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.R) {
            getWindow().setDecorFitsSystemWindows(true);
        }

        // Y que el propio sistema diga cuales son, por logcat. Es la unica forma de
        // saber si lo de arriba ha funcionado sin adivinar: si el viewport que ve
        // egui sigue siendo 1080 de alto, aqui saldra un aviso.
        getWindow().getDecorView().setOnApplyWindowInsetsListener((v, insets) -> {
            int l, t, r, b;
            if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.R) {
                android.graphics.Insets bars =
                        insets.getInsets(android.view.WindowInsets.Type.systemBars()
                                | android.view.WindowInsets.Type.displayCutout());
                l = bars.left;
                t = bars.top;
                r = bars.right;
                b = bars.bottom;
            } else {
                l = insets.getSystemWindowInsetLeft();
                t = insets.getSystemWindowInsetTop();
                r = insets.getSystemWindowInsetRight();
                b = insets.getSystemWindowInsetBottom();
            }
            float d = getResources().getDisplayMetrics().density;
            android.util.Log.i("VCInsets", "insets px l=" + l + " t=" + t + " r=" + r + " b=" + b
                    + " | puntos l=" + (l / d) + " t=" + (t / d) + " r=" + (r / d) + " b=" + (b / d)
                    + " | density=" + d);
            return insets;
        });
    }
}
