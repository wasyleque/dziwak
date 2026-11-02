# Dziwak

[Polski](README.md) | [English](README.en.md) | **Español**

Editor de imágenes rasterizadas ligero, escrito en Rust, con una interfaz al estilo de GIMP 2.10 — para Linux (Omarchy, Wayland/Hyprland) y Windows 10/11.
Licencia: GPL-3.0-or-later.

Aproximadamente 100 MB de RAM, 0% de CPU cuando está inactivo, sin requisitos de tarjeta gráfica.

## Características

- **Capas:** opacidad, 6 modos de mezcla, miniaturas, menú contextual (fusionar hacia abajo, alfa a selección, color a alfa, aplanar)
- **Selecciones:** rectangular, elíptica, a mano alzada (lazo), selección difusa, por color; combinando (Shift = añadir, Ctrl = restar, Shift+Ctrl = intersección); contorno de «hormigas en marcha» que sigue la forma real
- **Editar:** limpiar selección (Supr), rellenar con color de primer plano / fondo
- **Pintura:** pincel, lápiz, aerógrafo, borrador, cubo de relleno, degradado, clonar, emborronar, blanquear/ensombrecer, desenfocar/enfocar
- **Transformaciones:** mover, recortar, rotar, escalar, voltear; escalado de imagen y tamaño del lienzo
- **Texto:** estilo GIMP — arrastra un cuadro y escribe directamente en la imagen; la capa de texto se crea inmediatamente; fuentes del sistema, tamaño, color, alineación, espacio entre líneas
- **Pestañas:** varias imágenes a la vez (Ctrl+O, múltiples archivos en el diálogo o `dziwak a.png b.png`), cada una con sus propias capas, historial, selección y vista; las pestañas inactivas liberan memoria de GPU, el historial de deshacer comparte un presupuesto de memoria
- **Filtros con vista previa en vivo:** brillo/contraste, matiz/saturación, desaturar (luminancia, luma, claridad, promedio, valor — como en GIMP), desenfoque gaussiano, enfocar
- **Eliminación de fondo:** uniforme (desde los bordes) y IA (U²-Net-p localmente en CPU, ~1–2 s, sin GPU)
- **Archivos:** PNG, JPEG, WebP y formato personalizado `.dziwak` (capas, compresión LZ4)
- **Interfaz:** diseño oscuro GIMP 2.10 con acento de tema Omarchy; polaco, inglés, español (Editar → Idioma)
- Deshacer/rehacer con límite de memoria (instantáneas de mosaicos copy-on-write)
- Pregunta si guardar al cerrar una pestaña o la aplicación (también Super+W en Hyprland); las causas de los fallos se guardan en `~/.cache/dziwak/crash.log`

## Compilación

Se requiere Rust estable (por ejemplo, `mise use -g rust@stable`).

```bash
cargo run -p dziwak --release
```

Versión sin IA (binario más pequeño, compilación más rápida): `cargo build --release -p dziwak --no-default-features`.

## Instalación

**Windows 10/11:** descarga `dziwak-…-windows-x64.zip` desde [Releases](https://github.com/wasyleque/dziwak/releases), extrae y ejecuta `dziwak.exe` (portable, sin instalación; mantén la carpeta `models` junto a él).

**Linux:**

```bash
scripts/install-local.sh
```

Instala en `~/.local` (programa, icono, entrada del lanzador) y descarga el modelo IA (4.6 MB, Apache-2.0) a `~/.local/share/dziwak/models/`.

## Atajos de teclado

Las herramientas tienen atajos como en GIMP 2.10.

| Atajo | Herramienta | Atajo | Herramienta |
|---|---|---|---|
| R | Selección rectangular | P | Pincel |
| E | Selección elíptica | N | Lápiz |
| F | Selección a mano alzada | A | Aerógrafo |
| U | Selección difusa | Shift+E | Borrador |
| Shift+O | Selección por color | Shift+B | Cubo de relleno |
| M | Mover | G | Degradado |
| Shift+C | Recortar | C | Clonar (Ctrl+clic = origen) |
| Shift+R | Rotar | Shift+S | Emborronar |
| Shift+T | Escalar | Shift+D | Blanquear/Ensombrecer |
| Shift+F | Voltear | Shift+U | Desenfocar/Enfocar |
| O | Pipeta | T | Texto (cuadro en la imagen; Ctrl+Enter = aplicar, Esc = cancelar) |
| Z | Ampliación | Shift+M | Medida |

| Atajo | Acción |
|---|---|
| Ctrl+O / Ctrl+Shift+S | Abrir (en una nueva pestaña) / Guardar como |
| Ctrl+Tab / Ctrl+Shift+Tab | Siguiente / anterior pestaña |
| Ctrl+W | Cerrar pestaña (o clic medio en la pestaña) |
| Ctrl+Q | Salir (pregunta por las imágenes sin guardar) |
| Ctrl+Z / Ctrl+Shift+Z, Ctrl+Y | Deshacer / Rehacer |
| Ctrl+A / Ctrl+Shift+A / Ctrl+I | Seleccionar todo / Deseleccionar / Invertir selección |
| Supr | Limpiar selección (sin selección: toda la capa) |
| Ctrl+, / Ctrl+. | Rellenar con color FG / BG |
| X / D | Intercambiar colores / Colores por defecto |
| Tab | Mostrar/Ocultar docks |
| Shift+J / Shift+Ctrl+J | Centrar imagen / Ajustar a ventana |
| Rueda del ratón / Shift+Rueda | Desplazar arriba–abajo / izquierda–derecha |
| Ctrl+Rueda | Acercar alrededor del cursor |
| Botón central, Espacio+botón izquierdo | Mover vista |

## Arquitectura

- `crates/core`: Lógica independiente de la UI. Capas como mosaicos de 64×64 (copy-on-write), píxeles RGBA8 premultiplicados, historial de instantáneas de mosaicos, filtros, selecciones, transformaciones, texto, formato `.dziwak`.
- `crates/app`: Interfaz `egui`/`eframe` (backend `glow`); solo los mosaicos cambiados van a la GPU.
- `crates/ai`: Eliminación de fondo con modelo U²-Net-p vía `tract` (puro Rust, sin ONNX Runtime).

## Desarrollo

Reglas del proyecto y hoja de ruta: `AGENTS.md`. Pruebas: `cargo test --workspace` (prueba del modelo IA: `cargo test -p dziwak-ai -- --ignored`).

## Autor y soporte

Autor: **Piotr Wasilewski**. Si te gusta Dziwak, puedes apoyar su desarrollo mediante [PayPal](https://www.paypal.com/donate/?business=wasyl%40o2.pl&item_name=Dziwak) (también en la aplicación: Ayuda → Apoyar el proyecto).

