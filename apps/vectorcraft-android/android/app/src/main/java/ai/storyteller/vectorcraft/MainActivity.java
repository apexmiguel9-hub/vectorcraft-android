package ai.storyteller.vectorcraft;

import android.app.NativeActivity;
import android.content.Intent;
import android.net.Uri;
import android.os.Bundle;
import android.os.Environment;
import android.provider.Settings;
import android.util.Log;

/**
 * La Activity del port.
 *
 * <p>Es la de AOSP, {@link NativeActivity}, porque el meta-data
 * {@code android.app.lib_name} es lo que hace que {@code ANativeActivity_onCreate} arranque.
 * De ahi sale todo el ciclo de vida de la ventana, que es el que usa
 * {@code android-activity} por debajo.
 *
 * <h3>Lo unico que hay aqui es el permiso</h3>
 *
 * <p>SAF se ha ido. El browser de ficheros lo dibuja el port dentro de egui
 * ({@code src/browser.rs}), y el I/O es {@code std::fs}. El motivo esta medido: abrir
 * <em>cualquier</em> segunda Activity da ANR en este port, porque {@code android-activity}
 * bloquea el hilo principal de Java hasta que el hilo del bucle confirma el estado, y ese
 * hilo es el que corre el update de egui.
 *
 * <p>Lo que queda es {@link #hayPermiso()} y {@link #pedirPermiso()}, que son dos lineas:
 * {@code MANAGE_EXTERNAL_STORAGE} es un <em>appop</em>, no un permiso normal, asi que no
 * hay dialogo y hay que abrir los Ajustes.
 */
public class MainActivity extends NativeActivity {

    private static final String TAG = "VectorCraft";

    private static MainActivity instance;

    /**
     * MEDIDO, y era el fallo entero que hacia que SAF no abriera nada: las metodos
     * {@code native} de esta clase NO resolvian:
     *
     * <pre>
     * java.lang.UnsatisfiedLinkError: No implementation found for void
     * ai.storyteller.vectorcraft.MainActivity.nativeListo()
     * </pre>
     *
     * con la libreria <b>cargada</b> y Rust <b>arrancando</b>. Y no era la firma: dos
     * nativas distintas fallaban igual, una {@code extern "C"} con punteros y otra
     * {@code extern "system"} con tipos, y desde dos sitios distintos del ciclo de vida.
     *
     * <p>La razon es que {@link NativeActivity} (la de AOSP) carga la libreria por su
     * cuenta leyendo el meta-data {@code android.app.lib_name}, y lo hace por el camino
     * <b>nativo</b> de ANativeActivity, que <b>no se le comunica a ART</b>. El
     * {@code dlsym} que ART hace para resolver un metodo {@code native} de una clase Java
     * busca entre las librerias que el {@code ClassLoader} tiene registradas, y esa no lo
     * esta. Para ART, sencillamente, "la libreria no esta cargada".
     *
     * <p>{@code System.loadLibrary} si se lo comunica. Y cargar dos veces la misma
     * libreria no hace nada, asi que se llama antes de {@code super.onCreate()} sin miedo.
     *
     * <p>El nombre sale del propio meta-data del manifiesto
     * ({@code android.app.lib_name = vectorcraft_android}).
     */
    private static final String LIB = "vectorcraft_android";

    static {
        System.loadLibrary(LIB);
    }

    /**
     * Java -> Rust: la app esta viva y le pasa su propia clase.
     *
     * <p>MEDIDO que hace falta, y es la quinta via probada. {@code Env.find_class} y
     * {@code Env.load_class} fallan desde el hilo nativo de {@code android-activity} con
     * {@code NoClassDefFound}, y {@code LoaderContext::FromObject} tambien fallo. En vez
     * de buscar la clase se <b>toma}: JNI nos pasa el {@code jclass}, que es su propia
     * {@code Class}, y a partir de ahi hay una referencia global.
     *
     * <p>Va con {@code try/catch} porque una app no puede morir porque una nativa no haya
     * resuelto.
     */
    private static native void nativeListo();

    /**
     * MEDIDO, y no es obvio: un {@code null} de Java <b>no</b> es una cadena vacia.
     *
     * <p>Los dos casos normales de este modulo devuelven cadena vacia, nunca
     * {@code null}. La razon esta en el lado de Rust, en {@code texto()}: envolver un
     * {@code null} en un {@code JString} lo convierte en un puntero nulo, y
     * {@code get_string_utf_chars(null)} da justo
     * {@code NullPtr("get_string_utf_chars obj argument")}.
     *
     * @return {@code true} si el permiso esta concedido, {@code false} si no, o si
     *         todavia no se pudo preguntar (y entonces {@code false} es lo seguro).
     */
    private static boolean hayPermiso() {
        try {
            return Environment.isExternalStorageManager();
        } catch (Throwable t) {
            Log.w(TAG, "hayPermiso: " + t);
            return false;
        }
    }

    /**
     * Abre los Ajustes de "Acceso a todos los archivos".
     *
     * <p>MEDIDO que esto no da ANR: se lanza y se vuelve, sin esperar nada. El bloqueo
     * que lo daba era esperar <em>dentro</em> del update de egui, y ese ya no existe.
     *
     * <p>MEDIDO que hace falta el {@code package:} delante: sin el, en Android 11 o
     * superior los Ajustes abren la lista de <em>todas</em> las apps y no la de esta, y
     * el interruptor correcto hay que buscarlo a mano.
     *
     * @return Cadena vacia si se pudiera abrir; si no, el motivo.
     */
    private static String pedirPermiso() {
        try {
            if (instance == null) {
                return "la Activity todavia no esta lista";
            }
            Intent i = new Intent(
                    Settings.ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION,
                    Uri.parse("package:" + instance.getPackageName()));
            i.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
            instance.startActivity(i);
            return "";
        } catch (Throwable t) {
            Log.w(TAG, "pedirPermiso: " + t);
            return String.valueOf(t);
        }
    }

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        instance = this;
        // MEDIDO: con `System.loadLibrary` explicito en el bloque estatico esto resuelve;
        // sin el, ninguna nativa de la clase resolvia.
        try {
            nativeListo();
        } catch (Throwable t) {
            Log.w(TAG, "nativeListo: " + t);
        }
        Log.i(TAG, "vectorcraft: Activity creada, permiso=" + hayPermiso());
    }

    @Override
    protected void onDestroy() {
        instance = null;
        super.onDestroy();
    }
}