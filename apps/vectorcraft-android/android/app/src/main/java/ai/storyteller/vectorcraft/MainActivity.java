package ai.storyteller.vectorcraft;

import android.app.NativeActivity;
import android.content.ContentResolver;
import android.content.Intent;
import android.graphics.Insets;
import android.net.Uri;
import android.os.Build;
import android.os.Bundle;
import android.util.Log;
import android.view.WindowInsets;
import android.view.WindowManager;

/**
 * El Activity de VectorCraft en Android.
 *
 * Hereda de {@link NativeActivity} y hace dos cosas: deja que la libreria nativa
 * tome el control desde {@code android_main}, y habla con Android en nombre de Rust
 * para el Storage Access Framework.
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
 * <h2>SAF</h2>
 *
 * MEDIDO: VectorCraft <b>no</b> trae explorador de ficheros propio, y su
 * {@code rfd} no sirve en Android: las dependencias de {@code rfd} 0.17.2 no
 * incluyen {@code jni} ni {@code ndk-context}, o sea que sus backends son macOS,
 * Windows, wayland/xdg y web, y en Android no hay ninguno. Asi que SAF es
 * obligatorio y esta es su implementacion.
 *
 * El "path" de VectorCraft es una cadena opaca y la UI lee y escribe por los
 * ganchos {@code Services::read} / {@code Services::write}
 * ({@code crates/ui-egui/src/io.rs}). Aqui esa cadena es un
 * {@code content://} URI y se resuelve con {@link ContentResolver}. Por eso
 * <b>no hace falta ni copiar ficheros ni pedir permiso de almacenamiento</b>: SAF
 * esta disenado justo para no dar acceso general al almacenamiento, y el
 * almacenamiento privado de la app tampoco lo necesita.
 *
 * <h2>El bloqueo y por que no hay deadlock</h2>
 *
 * Los ganchos de fichero son sincronos ({@code FnMut(&FilePick) -> Option<String>}),
 * igual que en escritorio. Lanzar el selector de Android es otra Activity, asi que
 * el hilo de Rust tiene que esperar.
 *
 * MEDIDO: el hilo que dibuja la UI en Android es el mismo que despacha los eventos
 * de winit, asi que <b>esperar aqui sin mas deadlock</b>: el resultado nunca llegaria.
 *
 * Lo que lo evita es que el despertar lo manda <b>Java</b>, desde
 * {@code onActivityResult}, en el hilo de la UI de Android, llamando a
 * {@link #nativeOnFilePicked()}. Ese camino no pasa por winit. Rust solo
 * espera en una variable de condicion.
 */
public class MainActivity extends NativeActivity {
    private static final String TAG = "VectorCraft";
    /** Request code del selector de ficheros. Debe caber en los 16 bits bajos. */
    private static final int RC_PICK = 0x5643; // 'VC'

    // Modos de `request`, los mismos numeros que en `saf.rs`.
    static final int MODE_OPEN = 0;
    static final int MODE_OPEN_MULTI = 1;
    static final int MODE_SAVE = 2;
    static final int MODE_OPEN_TREE = 3;

    private static MainActivity instance;

    /**
     * El resultado del ultimo selector, a la espera de que Rust lo recoja.
     *
     * MEDIDO por que NO se pasa el URI como argumento del callback: para leer un
     * {@code jstring} dentro de la funcion JNI hace falta un {@code Env}, y en
     * `jni` 0.22 `Env::from_raw` con su lifetime hace que eso se
     * convierta en un error de compilacion. Guardandolo aqui y dejandolo a Rust la
     * lectura de la cadena, Rust lo recoge con un `Env` de verdad, que si esta
     * comprobado que funciona.
     */
    private static String lastResult;

    /**
     * Llega el resultado de un selector. {@code uri} es {@code null} si el usuario
     * cancelo. La implementa {@code apps/vectorcraft-android/src/saf.rs}.
     *
     * MEDIDO: tiene que ser {@code static}, para que el simbolo JNI sea
     * {@code Java_ai_storyteller_vectorcraft_MainActivity_nativeOnFilePicked} y lo
     * resuelva {@code System.loadLibrary} sin registrar nada a mano.
     */
    private static native void nativeOnFilePicked();

    /**
     * La app esta viva y le pasa su propia {@code Class} a Rust.
     *
     * <p>MEDIDO por que esto existe, y son las cinco vias que fallaron antes:
     *
     * <ol>
     *   <li>{@code Env::find_class} — desde un hilo nativo atado no ve clases de la app.</li>
     *   <li>{@code Env::load_class} — prueba el classloader del hilo y luego
     *       {@code FindClass}; en el hilo de {@code android-activity} no hay ninguno.</li>
     *   <li>{@code LoaderContext::FromObject(&activity)} — deberia haber bastado, porque
     *       la Activity <i>es</i> un objeto de la clase, y seguia dando
     *       {@code NoClassDefFound { requested: "ai/storyteller/vectorcraft/MainActivity" }}.</li>
     * </ol>
     *
     * <p>Un metodo nativo {@code static} recibe como segundo parametro el
     * {@code jclass}, que es su propia {@code Class}. O sea: en vez de <b>buscar</b> la
     * clase, se <b>toma</b>. Con eso hay una referencia global y no hace falta buscar
     * nada mas.
     */
    private static native void nativeListo();

    /**
     * MEDIDO: una segunda nativa, con el estilo viejo (`extern "C"` y punteros crudos,
     * como `nativeOnFilePicked`), para separar "las nativas de esta clase no resuelven"
     * de "esta en concreto no resuelve".
     */
    private static native void nativaDeEstiloViejo();

    /** Guarda el resultado y despierta a Rust, que esta esperando. */
    private static void publicar(String uri) {
        lastResult = uri;
        Log.i(TAG, "selector: " + (uri == null ? "cancelado" : uri));
        nativeOnFilePicked();
    }

    /** Rust recoge el resultado. Lo vacia, para no repetirlo. */
    private static String takeResult() {
        String r = lastResult;
        lastResult = null;
        return r;
    }

    /**
     * Rust pide abrir el selector. Se devuelve de inmediato; el resultado llega a
     * {@link #nativeOnFilePicked}.
     *
     * @param mode  uno de los {@code MODE_*}
     * @param title titulo del selector
     * @param mimes MIME types separados por comas, ya traducidos desde las extensiones
     * @return {@code null}, o el error si la Activity no esta lista
     */
    private static String request(int mode, String title, String mimes) {
        MainActivity a = instance;
        if (a == null) {
            return "la Activity no esta lista todavia";
        }
        // MEDIDO: `startActivityForResult` tiene que correr en el hilo de la UI. Rust
        // llama a esto desde el hilo de winit, y sin esto salta
        // `CalledFromWrongThreadException`.
        a.runOnUiThread(new Runnable() {
            @Override
            public void run() {
                a.startPicker(mode, title, mimes);
            }
        });
        return null;
    }

    private void startPicker(int mode, String title, String mimes) {
        Intent intent;
        if (mode == MODE_OPEN_TREE) {
            intent = new Intent(Intent.ACTION_OPEN_DOCUMENT_TREE);
        } else if (mode == MODE_SAVE) {
            intent = new Intent(Intent.ACTION_CREATE_DOCUMENT)
                    .addCategory(Intent.CATEGORY_OPENABLE);
        } else if (mode == MODE_OPEN_MULTI) {
            intent = new Intent(Intent.ACTION_OPEN_DOCUMENT)
                    .addCategory(Intent.CATEGORY_OPENABLE)
                    .putExtra(Intent.EXTRA_ALLOW_MULTIPLE, true);
        } else {
            intent = new Intent(Intent.ACTION_OPEN_DOCUMENT)
                    .addCategory(Intent.CATEGORY_OPENABLE);
        }

        // MEDIDO: sin estos dos flags el permiso es solo temporal y
        // `takePersistableUriPermission` falla con `SecurityException`. Es lo que
        // permite que un fichero abierto siga siendo legible **despues de cerrar y
        // volver a abrir la app**, que es lo que necesita `note_recent`.
        intent.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION
                | Intent.FLAG_GRANT_WRITE_URI_PERMISSION
                | Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION);

        String[] types = mimes == null || mimes.isEmpty() ? null : mimes.split(",");
        if (types != null && types.length > 0) {
            // `setType` con varios no vale: hay que dar el mas general y luego
            // `EXTRA_MIME_TYPES`, que es la via que SAF entiende de verdad.
            intent.setType("*/*");
            intent.putExtra(Intent.EXTRA_MIME_TYPES, types);
        }
        if (title != null && !title.isEmpty()) {
            intent.putExtra(Intent.EXTRA_TITLE, title);
        }
        try {
            startActivityForResult(intent, RC_PICK);
        } catch (Exception e) {
            Log.e(TAG, "el selector no arranca: " + e);
            publicar(null);
        }
    }

    @Override
    protected void onActivityResult(int requestCode, int resultCode, Intent data) {
        super.onActivityResult(requestCode, resultCode, data);
        if (requestCode != RC_PICK) {
            return;
        }
        if (resultCode != RESULT_OK || data == null || data.getData() == null) {
            publicar(null);
            return;
        }

        // MEDIDO: `ACTION_OPEN_DOCUMENT` con `EXTRA_ALLOW_MULTIPLE` devuelve el
        // primero en `getData()` y el resto en `getClipData()`. Sin esto solo se
        // abriria uno de los N.
        StringBuilder uris = new StringBuilder();
        java.util.ArrayList<Uri> all = new java.util.ArrayList<>();
        ClipDataHolder holder = collect(data);
        if (holder != null) {
            for (int i = 0; i < holder.count; i++) {
                all.add(holder.items.get(i));
            }
        }
        if (all.isEmpty()) {
            publicar(null);
            return;
        }
        for (Uri u : all) {
            if (uris.length() > 0) {
                uris.append('\n');
            }
            uris.append(u.toString());
        }

        // El permiso persistente, por cada URI. Sin esto, la segunda vez que se
        // abra uno de estos ficheros desde "recientes" salta `SecurityException`.
        for (Uri u : all) {
            persist(u, data.getFlags());
        }
        publicar(uris.toString());
    }

    /** Estructura minima para no arrastrar `ClipData` a la firma. */
    private static final class ClipDataHolder {
        final int count;
        final java.util.List<Uri> items;

        ClipDataHolder(int count, java.util.List<Uri> items) {
            this.count = count;
            this.items = items;
        }
    }

    private static ClipDataHolder collect(Intent data) {
        java.util.List<Uri> list = new java.util.ArrayList<>();
        android.content.ClipData clip = data.getClipData();
        if (clip != null) {
            for (int i = 0; i < clip.getItemCount(); i++) {
                Uri u = clip.getItemAt(i).getUri();
                if (u != null) {
                    list.add(u);
                }
            }
        } else if (data.getData() != null) {
            list.add(data.getData());
        }
        return list.isEmpty() ? null : new ClipDataHolder(list.size(), list);
    }

    private void persist(Uri u, int resultFlags) {
        int flags = resultFlags & (Intent.FLAG_GRANT_READ_URI_PERMISSION
                | Intent.FLAG_GRANT_WRITE_URI_PERMISSION);
        if (flags == 0) {
            // El proveedor no devolvio flags: se Assume lectura, que es lo que
            // necesita un fichero abierto.
            flags = Intent.FLAG_GRANT_READ_URI_PERMISSION;
        }
        try {
            getContentResolver().takePersistableUriPermission(u, flags);
        } catch (Exception e) {
            // No todos los proveedores lo dan. Se avisa y se sigue: el fichero sera
            // leible mientras la app viva, y solo fallara al reabrirlo en otra
            // sesion, que es mejor que negarse a abrirlo.
            Log.w(TAG, "sin permiso persistente para " + u + ": " + e);
        }
    }

    /**
     * Lee un fichero por su URI y lo devuelve en base64.
     *
     * <p>MEDIDO por que base64 y no {@code byte[]}: la API de JNI de Rust que lee un
     * array de bytes pide un {@code AsRef<JByteArray>}, y de un {@code JObject}
     * devuelto por una llamada solo se llega a uno pasando por {@code from_raw}, que es
     * {@code unsafe} y en {@code jni} 0.24 implica pelear con los lifetimes del
     * {@code Env}. Las cadenas si estan comprobadas. Y el propio motor ya mueve bytes
     * en base64 por sus parametros ({@code {name, dataBase64}}), asi que el formato es
     * el del proyecto.
     *
     * @return los bytes en base64, o lanza con el mensaje de Android
     */
    private static String readBase64(String uri) {
        try {
            ContentResolver cr = instance.getContentResolver();
            Uri u = Uri.parse(uri);
            try (java.io.InputStream in = cr.openInputStream(u)) {
                if (in == null) {
                    throw new IllegalStateException("el proveedor no abre " + uri);
                }
                java.io.ByteArrayOutputStream out = new java.io.ByteArrayOutputStream(1 << 16);
                byte[] buf = new byte[1 << 16];
                int n;
                while ((n = in.read(buf)) > 0) {
                    out.write(buf, 0, n);
                }
                return android.util.Base64.encodeToString(out.toByteArray(), android.util.Base64.NO_WRAP);
            }
        } catch (RuntimeException e) {
            throw e;
        } catch (Exception e) {
            throw new IllegalStateException(uri + ": " + e, e);
        }
    }

    /**
     * Escribe un fichero por su URI. Lo llama Rust desde {@code Services::write}.
     *
     * MEDIDO: se escribe con un buffer y se cierra explicitamente, en vez de dejar
     * que el {@code ContentResolver} lo haga por su cuenta. Un fichero a medias es
     * peor que un fallo: el motor avisa de que nunca escribe a medias, y aqui no hay
     * escritura atomica posible con SAF, asi que al menos se garantiza que el
     * buffer llega entero o que el error es explicito.
     */
    private static void writeBase64(String uri, String base64) {
        byte[] data = android.util.Base64.decode(base64, android.util.Base64.NO_WRAP);
        try {
            ContentResolver cr = instance.getContentResolver();
            Uri u = Uri.parse(uri);
            java.io.OutputStream out = cr.openOutputStream(u, "wt");
            if (out == null) {
                // MEDIDO: algunos proveedores no aceptan el modo "wt" y con "w"
                // los añade al final en vez de truncarlos, que un fichero de
                // documento nunca quiere.
                out = cr.openOutputStream(u, "w");
            }
            if (out == null) {
                throw new IllegalStateException("el proveedor no escribe " + uri);
            }
            try {
                out.write(data);
                out.flush();
            } finally {
                out.close();
            }
        } catch (RuntimeException e) {
            throw e;
        } catch (Exception e) {
            throw new IllegalStateException(uri + ": " + e, e);
        }
    }

    /**
     * El nombre que DocumentsUI muestra para un URI. Lo usa Rust para el titulo del
     * documento, que con un URI entero seria ilegible.
     *
     * @return el nombre, o {@code null} si no se puede
     */
    private static String displayName(String uri) {
        // MEDIDO: la primera version de esto estaba escrita con `.let(c -> …)`, que es
        // Kotlin:
        //
        //     MainActivity.java:371: error: cannot find symbol
        //         .let(c -> {
        //                ^
        //         symbol: method let((c)->{ try[...]ll; })
        //
        // Java no tiene `let`. Y no es un descuido de sintaxis: el fichero entero de
        // esta clase esta en Java **a proposito** (ver el `why Java y no Kotlin` de
        // arriba), asi que escribir Kotlin aqui habria sido el fallo de verdad.
        //
        // try-with-resources en vez de cerrar a mano, que es lo que hacia el `let`.
        try (android.database.Cursor c = instance.getContentResolver()
                .query(Uri.parse(uri),
                        new String[] { android.provider.OpenableColumns.DISPLAY_NAME },
                        null, null, null)) {
            if (c != null && c.moveToFirst()) {
                return c.getString(0);
            }
        } catch (Exception e) {
            android.util.Log.w(TAG, "sin nombre para " + uri + ": " + e);
        }
        return null;
    }

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        instance = this;
        // MEDIDO: esto es una PRUEBA, no el sitio final.
        //
        // En `onCreate` falla con
        //
        //     java.lang.UnsatisfiedLinkError: No implementation found for void
        //     ai.storyteller.vectorcraft.MainActivity.nativeListo()
        //     (tried Java_…_nativeListo and Java_…_nativeListo__)
        //
        // con la libreria **cargada** —MEDIDO: en el mismo lanzamiento, Rust corre y
        // registra `saf: JavaVM registrado`, pero en el hilo nativo 8315, mientras el
        // crash es en el hilo principal 8297— y el simbolo esta en `.dynsym` como
        // `FUNC GLOBAL DEFAULT` (MEDIDO con `llvm-readelf --dyn-syms` sobre la `.so`
        // **sacada del movil**). O sea: simbolo ahi, libreria cargada, y `dlsym`
        // devuelve null. Sin explicacion todavia.
        //
        // Se intenta en los dos sitios y con `try/catch`, para (a) que la app no muera
        // por esto, y (b) medir cual de los dos funciona:
        //
        // * `onCreate`, por el hilo principal de Java, lo antes posible.
        // * `onWindowFocusChanged(true)`, cuando la ventana ya esta y la libreria
        //   cargada de seguro.
        probar("onCreate");

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
        // notificaciones, y las pestañas de plantillas quedan pegadas al reloj.
        //
        // MEDIDO tambien: winit 0.30 **no rellena `safe_area_insets` en Android**.
        // Solo lo hace en iOS:
        //
        //     $ grep -rl safe_area winit-0.30.13/src/
        //     winit-0.30.13/src/platform_impl/ios/app_state.rs
        //     winit-0.30.13/src/platform_impl/ios/window.rs
        //
        // Y MEDIDO que los flags **no** redimensionan la superficie: tras ponerlos,
        // el viewport que ve egui seguia siendo de 937.4 x 443.1 puntos, o sea 1080
        // de alto enteros. Con `NativeActivity` el buffer que ve winit no cambia, asi
        // que la UI no puede reservar nada por su cuenta. Por eso los ultimos 48
        // puntos a la derecha siguen bajo la barra de gestos: hace falta pasarle los
        // insets a egui por JNI, que es lo siguiente.
        getWindow().clearFlags(WindowManager.LayoutParams.FLAG_LAYOUT_NO_LIMITS);

        // API 30+: sustituye a `FLAG_LAYOUT_NO_LIMITS` su equivalente moderno. Con
        // `minSdk 24` hace falta el guard: `setDecorFitsSystemWindows` no existe
        // antes.
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            getWindow().setDecorFitsSystemWindows(true);
        }

        // Y que el propio sistema diga cuales son, por logcat. Es la unica forma de
        // saber si lo de arriba ha funcionado sin adivinar.
        getWindow().getDecorView().setOnApplyWindowInsetsListener((v, insets) -> {
            int l, t, r, b;
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                Insets bars = insets.getInsets(
                        WindowInsets.Type.systemBars() | WindowInsets.Type.displayCutout());
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

    /** MEDIDO: la medicion de que llamada nativa resuelve y cual no. */
    private static void probar(String donde) {
        try {
            nativaDeEstiloViejo();
            android.util.Log.i(TAG, "nativaDeEstiloViejo desde " + donde + ": RESUELTA");
        } catch (Throwable t) {
            android.util.Log.w(TAG, "nativaDeEstiloViejo desde " + donde + ": " + t);
        }
        try {
            nativeListo();
            android.util.Log.i(TAG, "nativeListo desde " + donde + ": RESUELTA");
        } catch (Throwable t) {
            android.util.Log.w(TAG, "nativeListo desde " + donde + ": " + t);
        }
    }

    @Override
    public void onWindowFocusChanged(boolean tieneFoco) {
        super.onWindowFocusChanged(tieneFoco);
        if (tieneFoco) {
            probar("onWindowFocusChanged");
        }
    }

    @Override
    protected void onDestroy() {
        instance = null;
        super.onDestroy();
    }
}