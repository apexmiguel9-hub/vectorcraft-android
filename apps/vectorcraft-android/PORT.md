# VectorCraft en Android — mapa del port

Qué se creó, qué se tocó y por qué, con las líneas exactas. Para dos cosas:

1. **Portear el resto de apps** (`photocraft`, `printcraft`, …) con el mismo mapa.
2. **Quien haga un fork** sepa qué hay que deshacer y dónde, sin leer 84 commits.

Todo lo que hay aquí está **medido en el móvil**, no deducido. Las medidas van en el propio
sitio donde importan, con la salida del comando que las produjo.

- **Base:** `f9209e8` (merge-base con `upstream/main`); la fusión es `3a769da`, que se llevó los 188 commits de golpe (antes era `5a93e78`, 97 commits atrás)
- **Commits:** 84
- **Upstream por detrás:** 6 commits (un panel de Capas nuevo)
- **Motor y UI compartidos:** 6 ficheros, **290 líneas añadidas, 18 borradas**

---

## 1. Los 6 ficheros de `crates/` que se tocaron

Son los únicos que se comparten con el escritorio y la web, así que son los únicos que
tocan el upstream. Todos son aditivos o de recorte, ninguno cambia comportamiento en
escritorio.

| Fichero | +/- | Qué hace | Por qué |
|---|---:|---|---|
| `crates/ui-egui/src/canvas.rs` | +96 −1 | Arbitraje de gestos y el log `gesto:` de medición | En escritorio el ratón y los gestos nunca se mezclan. En un táctil, **un dedo es la herramienta y dos son el desplazamiento**, y eso hay que decidirlo en un sitio. El log es temporal: mide por qué el pan de dos dedos no llega. |
| `crates/ui-egui/src/lib.rs` | +5 −0 | Añade `Services::save_async` | La costura que faltaba para guardar en un host immediate-mode. Calcada de `open_async`, que ya existía para la web. |
| `crates/ui-egui/src/io.rs` | +8 −0 | `pick_path` consulta `save_async` | Devuelve el nombre propuesto mientras el destino se elige en el diálogo del host. Sin esto no hay forma de guardar sin panel nativo. |
| `crates/ui-egui/src/dialogs/mod.rs` | +24 −1 | Recorte de **alto** en el marco común + `ANCHOR_Y` | Solo recortaba el ancho. Un diálogo más alto que la ventana crecía por debajo y **se llevaba la fila de botones**: `Save for Web` era inusable en el móvil. |
| `crates/ui-egui/src/dialogs/save_for_web.rs` | +42 −7 | `anchos()` y `preview_h()` | Pedía 932 × ~586 pt en un viewport de 937 × 443. Ahora toma lo que hay. `PREVIEW_H` fuera: sin uso era error con `clippy -D warnings`. |
| `crates/ui-egui/src/dialogs/new_document.rs` | +115 −9 | Constantes de margen, alto y holgura | El diálogo no cabía en 985 pt de alto. Los `inner_margin` suman **encima** de `set_width`, no dentro. |

### Por qué el motor **no** se tocó

`Services::read` / `write` son exactamente el seam (`lib.rs:167,168`), y `open_async` +
`inbox` y `place_async` + `place_inbox` ya existían. Con un browser propio que devuelve
bytes, **el motor no necesita ni un cambio**.

Efectos colaterales buenos de usar rutas de verdad en vez de `content://`:

- **Los ficheros linked vuelven a funcionar.** El motor usa `std::fs` para ellos
  (`cmd/links.rs:627`, `cmd/swatchlib.rs:136,190`, `cmd/flatten.rs:417`,
  `cmd/pdfcmds.rs:176`, `cmd/printpresets.rs:211`), y con SAF un `content://` les hacía
  `stat` a una cadena rara.
- `note_recent` guarda rutas de verdad, así que los recientes se abren.

---

## 2. `apps/vectorcraft-android/` — lo que no existe en el upstream

Novedad entera. `472 + 547 + 582 + 7` líneas de Rust y `237` de Java.

| Fichero | Líneas | De las que son comentario | Qué hace |
|---|---:|---:|---|
| `src/lib.rs` | 472 | ~367 | Punto de entrada, opciones de ventana, `eframe::App`, insets, banner de permiso |
| `src/browser.rs` | 547 | ~305 | El browser (Open, Place, Save) con `egui_file`, los `Services`, `std::fs` |
| `src/permiso.rs` | 582 | ~299 | Lo único que queda de JNI: permiso de almacenamiento e insets del sistema |
| `src/main.rs` | 7 | ~5 | El mismo `main` de escritorio, para probar sin móvil |
| `android/…/MainActivity.java` | 237 | ~133 | `System.loadLibrary`, `nativeListo`, `hayPermiso`, `pedirPermiso`, `insets` |

`src/lib.rs` también trae el proyecto Gradle entero:

| Fichero | Líneas |
|---|---:|
| `android/build.gradle` | 41 |
| `android/settings.gradle` | 17 |
| `android/gradle.properties` | 5 |
| `android/app/build.gradle` | 188 |
| `android/app/proguard-rules.pro` | 3 |
| `android/app/src/main/AndroidManifest.xml` | 67 |
| `android/app/src/main/res/drawable/ic_launcher.xml` | 52 |
| `android/app/src/main/res/values/strings.xml` | 4 |
| `android/app/src/main/res/values/themes.xml` | 9 |
| `android/app/src/main/res/xml/backup_rules.xml` | 4 |

---

## 3. Los 4 ficheros de la raíz que chocan con upstream

De los 315 ficheros que cambia upstream, el port se solapa con **10**, y solo uno dio conflicto:
`.gitignore` (2 líneas de upstream contra 5 del port; se quedan las dos). El resto auto-mergeó,
incluido `canvas.rs` con 20 commits de upstream por encima.

| Fichero | Cambio |
|---|---|
| `Cargo.toml` | `android_logger`, `winit` y el bloque `[target.'cfg(target_os = "android")']` del port |
| `Cargo.lock` | `egui_file` + `dyn-clone`, y `jni`. **No se toca a mano**: hay un workflow, `.github/workflows/regen-lock.yml`, que lo regenera con `dtolnay/rust-toolchain@stable` y lo sube como artefacto. Local no vale porque el cargo de aquí es más antiguo que el `rust-version` del workspace (1.95) y **baja versiones** al resolver |
| `.gitignore` | `local.properties`, `.gradle/`, `jniLibs/` |
| `ASSETS.md` | La fila del icono del launcher: vector original en XML |

---

## 4. CI

| Workflow | Pasos | Qué hace |
|---|---:|---|
| `build-android-so.yml` | 14 | Compila la `.so` para `aarch64` con `cargo ndk`, quita **solo** la tabla DWARF, y **comprueba la tabla de símbolos** |
| `build-apk.yml` | 12 | Se **dispara al terminar** el del `.so`, baja el artefacto y monta el APK |
| `ci-ui.yml` | 6 | `cargo test -p vectorcraft-ui-egui`, `-p vectorcraft-engine`, `cargo xtask layers` |

**Dos reglas que costaron sangre:**

- **`.so` y APK en workflows separados.** `javac` corre en el del APK y su código **no está
  en la `.so`**, así que el del `.so` dio **verde** dos veces seguidas con el Java roto.
- **El workflow del APK busca la última `.so` con éxito**, no la de su commit. Con
  artefactos y workflows encadenados eso es lo único que funciona, pero significa que **el
  APK puede llevar una `.so` vieja**: comprobar con `strings` lo que se acaba de instalar.

---

## 5. Lo que hay que saber para portar las demás apps

### 5.1 El ANR que estructura todo

**Abrir cualquier segunda Activity da ANR.** No es de SAF: es de abrir cualquier Activity.

`android-activity` 0.6 bloquea el hilo principal de Java hasta que el hilo del bucle
confirma el estado (`native_activity/glue.rs:511`, `set_activity_state` escribe un comando
en un pipe y espera en un condvar). El hilo del bucle es `android_main`, que es donde corre
el update de egui. Ciclo cerrado:

```
el update espera al selector → el selector pausa la Activity →
la pausa espera al update → el selector nunca devuelve
```

```
Reason: Input dispatching timed out (…MainActivity is not responding.
  Waited 10001ms for FocusEvent(hasFocus=false))
mLastPausedActivity: com.android.documentsui/…picker.PickActivity
```

**Conclusión: nada que espere dentro del update.** Por eso todo el I/O del port es
immediate-mode y los bytes llegan por un inbox que el motor drena cada frame.

### 5.2 Las cinco vías de JNI que fallaron

Solo funciona la quinta, y conviene saber por qué fallaron las otras para no repetirlas:

| Vía | Resultado |
|---|---|
| `Env::find_class` | `NoClassDefFound` desde un hilo nativo atado |
| `Env::load_class` con `LoaderContext::None` | ídem: prueba el classloader del hilo |
| `LoaderContext::FromObject(&activity)` | ídem, **aunque la Activity sea de la clase** |
| — | — |
| **`onCreate` pasa su propia clase** | ✅ referencia global, ya no hay que buscar nada |

Y **`System.loadLibrary` explícito** en el `onCreate` es obligatorio: `android.app.NativeActivity`
carga la `.so` por su cuenta leyendo el meta-data `android.app.lib_name`, pero lo hace por
el camino **nativo** de ANativeActivity, que **no se le comunica a ART**, y el `dlsym` de ART
busca entre las librerías que el `ClassLoader` tiene registradas.

### 5.3 Insets: recorta `screen_rect`, no rellenes los de egui

Este costó lo suyo y **el nombre de la API engaña**:

- `winit` 0.30 rellena `safe_area_insets` **solo en iOS**.
- La ventana llega **edge-to-edge**: el viewport medido es 2400 × 1080 enteros. Desde
  Android 15 con `targetSdk 35+` es obligatorio y `setDecorFitsSystemWindows` no hace nada.
- `content_rect() = viewport_rect - safe_area_insets` lo usan los **`Window`** →
  `safe_area_insets` **sí** arregla los diálogos.
- Los paneles se colocan en `available_rect()`, que sale de **`screen_rect`**, y ahí los
  insets no intervienen → hay que **recortar `screen_rect`**.

Medida, con `safe_area_insets` puesto y correcto (`t=24.205128`), los paneles **no** se movían:

```
y=  3 px =  1.23 pt   ← el panel empezaba en y=0
inset esperado: 24.205 pt = 59 px
```

Y después de recortar:

```
y= 59 px = 24.21 pt   ✅
```

### 5.4 Permiso de almacenamiento

`MANAGE_EXTERNAL_STORAGE` es un **appop**, no un permiso: no hay diálogo, hay que abrir
`Settings.ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION`. Se pide **una vez** al primer
arranque y **se puede saltar**: sin él el browser sigue sirviendo para lo que es de la app.

Ojo al leer el estado del paquete:

```
install permissions:
  android.permission.MANAGE_EXTERNAL_STORAGE: granted=false, flags=[USER_SET]
appops:
  MANAGE_EXTERNAL_STORAGE: allow
```

`granted=false` **no** significa denegado. Lo que decide es el appop.

**Lo que decide el `targetSdk`, no la versión del móvil.** Con `targetSdk 34` hace falta en
Android 11 igual que en 16; bajar el `minSdk` no ahorra nada. En Android 7-10 no se pide.

### 5.5 Un `match` mal escrito que devuelve `false` en vez de fallar

El bug más caro de la sesión, y no daba ningún aviso:

```rust
// jni-0.22.4/src/jvalue.rs:24
pub enum JValueOwned<'local> {
    Int(jint),
    Bool(jboolean),   // ← un boolean de Java llega AQUÍ, no como Int
    …
}
```

Con `match v { JValueOwned::Int(n) => n != 0, _ => false }` la rama `_` se llevaba siempre,
así que `concedido()` era `false` **para siempre**. Explicaba tres síntomas a la vez: el
aviso que no se iba nunca, el salto a Ajustes en cada arranque, y `Permission denied`
estando el permiso concedido.

### 5.6 `env()` aplana el `Result` — y sale tres veces

`con_env` devuelve `Result<Result<T, String>, String>` porque `attach_current_thread_for_scope`
exige `E: From<jni::errors::Error>`. **`env()` lo aplana.** El `match` de fuera tiene un
solo nivel; los `Ok(Ok(..))` **dentro** de las closures sí son correctos.

### 5.7 `cdylib` sin `#[no_mangle]` sale vacío

El enlazador borra lo que no sea alcanzable desde las exportaciones. Se vio una `.so` de
**5,8 MB con 566 KB de `.text` y 3 símbolos dinámicos** para 225.484 líneas: el tamaño no lo
delata porque casi todo eran datos de depuración. **Hay que medir `.text`, no el peso.**

### 5.8 Vulkan es un callejón en este móvil

wgpu elige Vulkan (PRIMARY) y el driver aborta **mientras compila el fragment shader**:

```
#04-06 vulkan.mtk.so   FragmentShaderCompileState::CompileUF()
#02-03 libufwriter.so  BILParseStream()
#00      libc.so       abort()
```

Sin panic de Rust: el proceso lo mata el driver. Se usa **solo OpenGL ES**
(`NativeOptions::wgpu_options`), que no cuesta nada porque el motor rasteriza en CPU con
`vello_cpu`.

**Y `clippy -D warnings` convierte en fallo cualquier API deprecada** —`Env::get_string` lo
está desde `jni` 0.22.

---

## 6. El browser: por qué `egui_file` y no SAF

| | SAF (sistema) | `egui_file` (propio) |
|---|---|---|
| ¿Segunda Activity? | **Sí** → `onPause` → **ANR** | **No** → **no hay ANR** |
| Lee `/sdcard/0` | Sí, sin permisos | **No**, sin el permiso |
| Deps de Android | `jni`, `ContentResolver` | **ninguna** |
| E/S | — | **0**, MIT, `egui ^0.36` (la nuestra) |

Medido, y es lo que hace posible no escribir ningún `Vfs`:

```
egui_file/src/lib.rs:231   fs: Box::new(Fs {})
egui_file/src/fs.rs        std::fs::create_dir / read_dir / rename
```

### Dos comportamientos del crate que parecen bugs y no lo son

**Entrar en una carpeta pide doble toque** (`lib.rs:813` selecciona, `:827` entra). En un
dedo solo se registra si los dos caen dentro del tiempo de egui, así que tocar despacio no
entra: "a veces sí, a veces no". Arreglado **sin tocar el crate**: tras un toque la carpeta
queda en `selected_file`, así que `path()` es el directorio tocado y `directory()` el
anterior; si difieren, se entra.

**`selection()` viene vacío en un diálogo de guardar**, porque solo devuelve ficheros
*marcados* del listado. Por eso el guardado usa `path()`, que ahí sí es la **ruta
completa** (`self.path.join(filename)` → `select` → `path()`).

---

## 7. Estado medido en el dispositivo

**moto g56 5G**, 2400 × 1080, densidad 390 → **2,4375 px/pt**.

| | |
|---|---|
| Viewport de egui en horizontal | 937,4 × 443,1 pt |
| Insets medidos | `l=0 t=24,205128 r=48 b=0` |
| 24,2 pt arriba | 59 px (barra de estado) |
| 48 pt a la derecha | 117 px (barra de navegación) |
| Render | `render 5,4 ms · ui 1,5 ms · 124 fps` |

El renderer es **100 % CPU**: `crates/render` depende de `vello_cpu`, `vello_common`,
`image`, `flate2`, `jpeg-encoder`, `gif`, `weezl`, `log`. Ni `wgpu`, ni `vello` de GPU. La
GPU solo sube una textura y dibuja un quad
(`crates/ui-egui/src/canvas.rs:1571`, `ColorImage::from_rgba_premultiplied`).

---

## 8. Los gestos: el bug que se anulaba solo

Es el hallazgo más caro de este port, así que va con su aritmética. **Un dedo es la
herramienta, dos dedos pan y zoom a la vez** — como Figma, Procreate, Illustrator y
Affinity.

### Qué pasaba

Con dos dedos, el **zoom iba bien y el pan no**. Y el pan solo aparecía si dejabas los dedos
quietos ~1 s y luego deslizabas.

### Por qué: no faltaba el gesto, se cancelaban entre sí

Todo en `crates/ui-egui/src/canvas.rs`, en `handle_input`. La transformada se construye
**una vez**, arriba del todo:

```rust
let xf = Xf::new(rect, &v);          // centro VIEJO
```

y luego, en este orden dentro del mismo frame:

1. **el pan de dos dedos** — `canvas.rs:496`, `vm.center -= d;`
2. **el zoom alrededor del puntero** — `canvas.rs:534-540`:

```rust
let before = xf.to_doc(p);                 // centro VIEJO (pre-pan)
vm.zoom = (vm.zoom * factor).clamp(0.0313, 640.0);
let nx = Xf { rect, zoom: vm.zoom, center: vm.center, rot: vm.rotation.to_radians() };
let after = nx.to_doc(p);                  // centro NUEVO (post-pan)
vm.center += before - after;
```

`before` usa el centro **anterior** al pan y `after` el **posterior**. Desarrollando, con
`center_nuevo = center_viejo − d`:

```
before − after = d + (p−c)·(1/zoom − 1/zoom′)

center_nuevo + (before − after)
  = (center_viejo − d) + d + (p−c)·(1/zoom − 1/zoom′)
  = center_viejo + (p−c)·(1/zoom − 1/zoom′)
```

El `−d` y el `+d` **se cancelan exactamente**. Del pan no queda nada; solo sobrevive la
corrección de zoom. `before` y `after` asumían el mismo centro, y el pan se encarga de
romper esa suposición un frame antes.

### Por qué la pausa de un segundo lo "arreglaba"

El bloque de zoom está dentro de `if (factor - 1.0).abs() > 1e-6` (`canvas.rs:534`). Con
los dedos quietos el log da `translation=[0.0 0.0]` y `zoom=1.0`, el bloque **se salta
entero**, nadie deshace el pan, y al deslizar funciona. Durante el pellizco `zoom ≠ 1.0`,
el bloque entra, y el pan se anula cada frame.

### El arreglo

Una línea, `canvas.rs:515` (`xf` pasa a `mut`):

```rust
xf = Xf::new(rect, vm);      // justo después de `vm.center -= d`
```

para que `before` y `after` partan del mismo centro, que es la condición que la expresión
anterior asumía en silencio y que el pan incumplía.

### Cómo está medido

Con el log denso del port, en el móvil, con los dedos:

| | Antes | Después |
|---|---:|---:|
| Frames con pan **y** zoom a la vez | — | **2555** |
| `translation_delta` ≠ 0 con 2 dedos | 74,9 % | 74,9 % |

Que el `translation_delta` tuviera el **mismo** porcentaje antes y después es la prueba de
que el gesto siempre llegó igual: **egui no era el problema**, el gasto estaba en el
cálculo del centro. Antes esos 2555 frames de pan+zoom se perdían todos.

### Un detalle que no es un detalle

`egui-winit` **emula el ratón con un dedo** (`egui-winit-0.36.2/src/lib.rs:904`, *"emit
PointerButton resp. PointerMoved events to emulate mouse"*), y solo para el primer puntero
(`:902`). Por eso el dedo emulado te abre el menú de clic derecho al mantener pulsado y por
eso parece que hay un "botón derecho fantasma". No viene de Android: es egui. Y con dos
dedos, el segundo **no** se emula, que es justo lo que hace que el gesto separe limpio.

### Los tres logs que no servían

En el mismo fichero, tres instrumentos seguidos que no podían decir nada. Se documentan
porque el patrón se repite y el cuarto sí funcionó:

| | Por qué no decía nada |
|---|---|
| Log dentro del `if` de dos dedos | Solo imprimía si ya había multitáctil: "no llegan eventos" y "el bloque no corre" daban el **mismo silencio** |
| Log muestreado cada 30 frames | El gesto dura **uno o dos frames** (`touch_state.rs:249-250`, sin margen), así que el muestreo pasaba de largo |
| Log con `PAN_LOG == 0` | `PAN_LOG` arranca en **200**, así que la condición era falsa y **no imprimía nunca** |

El que funciona gasta **una línea por frame mientras hay dedo**, con presupuesto, y añade
una línea base incondicional al arrancar — porque si no llega ningún toque el log denso no
diría nada, que es justo el fallo que buscaba detectar.

---

## 9. Fusiones de upstream: el mapa de los conflictos

MEDIDO con `git merge-tree --write-tree` en seco, que **no toca la rama**. Es la forma de
saber lo que cuesta una fusión antes de empezar, y el script `scripts/actualizar-upstream.sh`
no lo hace.

### Las dos que llevamos

| Fusión | Commits | Conflictos |
|---|---:|---:|
| 188 commits | 188 | **1** (`.gitignore`) |
| 336 commits (2 releases + i18n italiano) | 336 | **7** |

Always **merge**, no rebase: con 111 commits del port un rebase son 111 oportunidades de
conflicto en vez de 7, y además reescribe historia ya publicada.

### Dónde caen siempre

| Fichero | Conflictos | Por qué |
|---|---:|---|
| `crates/ui-egui/src/canvas.rs` | 7 | El archivo con más commits de los dos lados: 29 de upstream, 15 del port |
| `crates/ui-egui/src/dialogs/*.rs` | 3 | Upstream refactorizó los diálogos a un helper `modal::show` |
| `Cargo.lock` | 1 | **Nunca se resuelve a mano** — se regenera con `.github/workflows/regen-lock.yml`, en CI, porque el cargo local es más antiguo que el `rust-version` del workspace (1.95) y baja versiones |
| `crates/tools/src/*.rs` | 1 | Upstream extrajo la tolerancia de acierto a `cx.pick_tol()` |

### Lo que costó de más, y por qué

* **`new_document.rs`**: tomar el lado de upstream en un conflicto dejó mis variables
  (`presets_w`, `height`) mezcladas con las suyas, y los 5 identificadores que quedaron
  huérfanos. **Error mío**: un conflicto de imports y otro de bloque se resuelven **uniendo**,
  no eligiendo. La salida fue tomar el fichero entero de upstream: **−130 líneas**, todas
  constantes de márgenes que su `modal::show` ya hace.
* **`prefs_dialog.rs`**: −`CHROME_H` muerto, que `clippy -D warnings` rechaza.
* **El recorte de alto de los diálogos** pasó de estar en `dialogs/mod.rs` a `dialogs/modal.rs`,
  que es mejor: `modal::show` es por donde pasan **todos** los diálogos, así que ahora el
  recorte se aplica a los nuevos de upstream sin tocar nada.

---

## 10. Los defaults de preferencias: cambiados, y upstream los puede volver a cambiar

**Este repo cambia defaults de `Prefs`, en `crates/engine/src/lib.rs`, a propósito.** No es
un descuido: es lo que hace el port utilizable en un dedo. Y como upstream toca ese fichero,
**cualquier fusión puede devolverlos a los de escritorio y romper tests.** Es lo primero que
hay que mirar cuando `ci-ui` salga rojo tras una fusión.

### Los tres

| Preferencia | Upstream | Aquí | Por qué |
|---|---:|---:|---|
| `ui_scaling` | 1.0 | **0.85** | MEDIDO: el diálogo de Preferencias pide 418,5 pt-UI de alto y un móvil en horizontal da 418,9 de `content_rect`. A 1,0 cabe por 0,4 pt y el `anchor` de −20 lo empujaba fuera: título bajo la barra de estado y fila de botones cortada. A 0,85 sobran 74,3 pt |
| `anchor_size` | 3 | **7** | MEDIDO: 3 deja `grow = 0` y los nodos se dibujan de 4 pt — 3,4 pt-UI a 0,85, contra los **48 dp** que Android pide. 7 es el **techo** de `clamp(1, 7)` |
| `selection_tolerance` | 3.0 | 3.0 **sin tocar** | **Probado a 8.0 y revertido**: rompe **12 tests del motor**. Ver abajo |

### `selection_tolerance`: probado, revertido, y por qué

Es la **tolencia del hit test en píxeles de pantalla**: `cx.pick_tol()` es
`selection_tolerance / zoom` (`tools/src/lib.rs:354`). MEDIDO: subirla de 3.0 a 8.0 rompe
**12 tests del motor**, que hacen clic a una distancia concreta y comprueban qué se
selecciona:

    snap_to_pixel_rounds_drawing_and_moves   (37.3, 34.8) en vez de (17, 16)
    selection_tool_drags_a_ruler_guide         110.0 en vez de 120.0

Con 3 px esos clics fallan y seleccionan lo que el test espera; con 8 px alcanzan y
seleccionan otra cosa. **No son tests malos**: miden precisión de selección, y el valor está
en medio de ella.

Y el techo es 8, así que por la vía de la preferencia no se llega al **24 px** que el hitbox
táctil necesita en realidad:

    "invalid parameters for `prefs.set`: `selectionTolerance` must be between 1 and 8 (got 24)"
    crates/engine/src/cmd/prefscmds.rs:138    num(1.0, 8.0, "px")

**Intercambio medido: 3 → 8 contra 12 tests rotos.** Se queda en 3.0.

Si hay que hacerlo bien, el camino es tocar **los hit tests de los handles**, que son los que
el dedo no alcanza, y no el radio global. Eso se mide aparte.

### Qué hacer cuando upstream rompa esto

1. `git diff <antes-de-la-fusion>..HEAD -- crates/engine/src/lib.rs` y mira las tres líneas.
2. Si upstream bajó los defaults, **vuelve a subirlos**: son 3 líneas.
3. Los tests de handles (`canvas.rs`) fallarán con un mensaje como
   `anchors 4, handles 6 by default: [8.0, 10.0, 800.0]`. **Están bien**: son de *escalado* y
   de *estilo*, no del default. Se arreglan fijando `anchorSize` a 3 explícitamente en el test,
   que es como ya están.

### La lección del tamaño de los handles

MEDIDO que upstream **ya había hecho** esto mejor que el port: extrajeron el tamaño a la
preferencia `anchor_size` y crean `cx.pick_tol()`. El port tenía hardcodeados `8.0`, `12.0` y
`24.0` en nueve sitios, lo cual además de ser peor obliga a rehacerlo en cada fusión. **Cuando
se toque esto, primero leer si upstream ya lo hizo.**

---

## 11. Lo que NO está hecho

| | |
|---|---|
| **Teclado blando** | No aparece con `android-native-activity`. `NativeActivity` no tiene vista para el `InputMethodManager`. El arreglo es `androidx.games:games-activity:4.4.0` + la feature `game-activity` + `MainActivity extends GameActivity` |
| ~~Rebase de upstream~~ | **Hecho**: fusionados los 188 commits con `git merge`, **un solo conflicto** (`.gitignore`). `ci-ui` verde (3776 tests) y `.so` compilando sobre el upstream nuevo. Quedan 6 commits de un panel de Capas, con un unico solape en `dialogs/mod.rs` |
| **Export web** | Genera ficheros muy grandes. Es del motor, no del port |
| **`pick_folder`, `pick_open_multi`** | Sin conectar: son **síncronos** (`FnMut(…) -> Option<String>`) y un diálogo immediate-mode no devuelve una ruta en la misma llamada. Afecta a *Relink to Folder* y *Package* |
| **Jugador de vídeo / texturas** | Sin probar en el móvil |
| **Menú que parpadea con el dedo** | **Sin causa identificada.** Una hipótesis anterior lo atribuía a `canvas.rs:189`, y **es falsa**, descartada leyendo el fuente: (a) `Popup::context_menu` (`egui-0.36.2/src/containers/popup.rs:248-259`) abre con `secondary_clicked()` y **no hay temporizador de pulsación larga**; (b) `egui-winit` solo traduce un botón derecho **real** a `Secondary` (`lib.rs:1439`), y `on_touch` nunca lo emite —emite `Left` (`lib.rs:912`)—, y (c) el backend de Android de winit no produce eventos de ratón (`winit-0.30.13/.../android/mod.rs:387`, `_ => None // TODO mouse events`). O sea que con un dedo **`secondary_clicked()` no puede ser cierto**, y la rama de `canvas.rs:189` (además `any_pressed()` es de *este* frame, `input_state/mod.rs:1369`, y `context_menu()` corre después, línea 197, así que la ventana no existe) no se ejecuta. Lo que se ve queda sin explicar |

---

## 12. Reproducir

```bash
gh workflow run build-android-so.yml        # o push a main; tarda ~10 min
gh workflow run build-apk.yml               # se dispara al terminar el anterior
```

Para bajar artefactos, **`gh run download` no funciona**; va por API:

```bash
A=$(gh api "repos/apexmiguel9-hub/vectorcraft-android/actions/runs/$RUN/artifacts" \
      --jq '.artifacts[]|select(.name=="vectorcraft-android-debug")|.id')
gh api "repos/apexmiguel9-hub/vectorcraft-android/actions/artifacts/$A/zip" > a.zip
```

**Para portear otra app, el orden que funcionó:**

1. El mismo proyecto Gradle y los mismos tres workflows, cambiando `lib_name` y el paquete.
2. `src/lib.rs`: opciones de ventana con **solo OpenGL ES**, `log::set_max_level`, gancho de
   panicos al logcat, y `raw_input_hook` recortando `screen_rect`.
3. `src/browser.rs`: el browser y los `Services` con `std::fs`.
4. `src/permiso.rs`: permiso e insets.
5. `MainActivity.java`: `System.loadLibrary`, `nativeListo`, y los estáticos de Java.
6. **Medir en el móvil antes de dar algo por bueno.** Cada bug de esta lista parecía
   improbable y cuatro de ellos no habrían salido leyendo el código.