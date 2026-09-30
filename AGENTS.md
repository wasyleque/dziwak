# Dziwak — reguły projektu (czytaj ZAWSZE przed zmianą kodu)

Dziwak to lekki edytor grafiki rastrowej w stylu GIMP-a, w Ruście, dla Omarchy Linux (Wayland/Hyprland).
Licencja GPL-3.0-or-later. Priorytet: **mało RAM-u, mało CPU, szybki start.** Docelowa maszyna ma 4 rdzenie i 7 GB RAM.

## Architektura (NIE ZMIENIAĆ bez zgody nadzorcy)

- `crates/core` (`dziwak-core`): czysta logika, **zero zależności od UI**. Wszystko testowalne `cargo test -p dziwak-core`.
- `crates/app` (`dziwak`): UI w `eframe`/`egui` (backend `glow`, Wayland). Tylko wyświetlanie i wejście, żadnej logiki pikseli.
- **Piksele:** RGBA8, premultiplied alpha, sRGB. Typ `Rgba8([u8; 4])`.
- **Kafle:** obraz warstwy = siatka kafli 64×64 px. `Tile = Arc<[Rgba8; 4096]>`, pusty kafel = `None` (nie zajmuje pamięci).
  Zapis do kafla przez `Arc::make_mut` (copy-on-write).
- **Historia (undo/redo):** migawka warstwy = sklonowana siatka `Arc`-ów (tanie). Pamiętamy tylko zmienione kafle. Limit pamięci historii konfigurowalny.
- **Renderowanie:** kompozycja tylko „brudnych” kafli (`dirty` set), wysyłanie do GPU tylko zmienionych kafli.
- Operacje na pikselach: pętle bez alokacji w środku; `rayon` dopiero gdy pomiar pokaże potrzebę.
- Zależności: minimalne, zawsze `default-features = false` z jawną listą funkcji. Nowa zależność = uzasadnienie w commicie.

## Zasady pracy dla modeli

1. Jedno zadanie = jedna mała zmiana. Nie ruszaj plików spoza zadania.
2. Każda funkcja w `core` ma test jednostkowy w tym samym pliku (`#[cfg(test)] mod tests`).
3. Przed zakończeniem MUSI przejść: `cargo fmt && cargo clippy --workspace -- -D warnings && cargo test --workspace`.
4. Nie używaj `unwrap()` w kodzie aplikacji poza testami (używaj `Result`, błędy przez `thiserror` w core jeśli potrzebne).
5. Komentarze i nazwy w UI po polsku, identyfikatory w kodzie po angielsku.
6. Nie twierdź, że test przeszedł, jeśli nie widziałeś tego w wyjściu komendy.

## Etapy (status: [ ] do zrobienia, [x] zrobione)

- [x] E0 Okno `eframe` „Dziwak” z pustym płótnem; zoom kółkiem (wokół kursora), przesuwanie środkowym przyciskiem/spacja+LPM.
- [x] E1 core: `Rgba8`, `Tile`, `TiledLayer { width, height, tiles: Vec<Option<Tile>> }`, get/set pikseli, testy.
- [x] E2 core: `Document { width, height, layers: Vec<Layer> }`, `Layer { name, visible, opacity, blend, pixels: TiledLayer }`, kompozycja do bufora dla prostokąta.
- [x] E3 app: wyświetlanie dokumentu jako tekstury per kafel (tylko brudne kafle są wysyłane).
- [x] E4 Otwieranie/zapis PNG/JPEG/WebP (crate `image`, tylko te formaty) + okno dialogowe `rfd` (portal XDG).
- [x] E5 core: pędzel okrągły (rozmiar, twardość, krycie), interpolacja pociągnięć; app: narzędzie Pędzel i Gumka.
- [x] E6 core: historia undo/redo na migawkach kafli; app: Ctrl+Z / Ctrl+Shift+Z / Ctrl+Y, menu Edycja.
- [x] E7 Panel warstw: dodaj/usuń/przesuń/widoczność/krycie.
- [x] E8 Tryby mieszania: Normal, Multiply, Screen, Overlay, Darken, Lighten.
- [x] E9 Zaznaczenie prostokątne/eliptyczne (maska), przycinanie operacji do zaznaczenia.
- [x] E10 Filtry: jasność/kontrast, odcień/nasycenie, rozmycie Gaussa (separowalne), wyostrzanie.
- [x] E11 Wypełnianie (kubełek), pipeta, gradient liniowy.
- [x] E12 Kolory z motywu Omarchy (`~/.config/omarchy/current/theme/`), plik `.desktop`, ikona.
- [x] E13 Własny format `.dziwak` (warstwy, kafle, kompresja lz4_flex — czysty Rust, szybszy i lżejszy w kompilacji niż zstd).

## Mikrozadania (dla aidera + Ollama; jedno na raz, maksymalnie uproszczone)
Zasada: 1 plik, 1 funkcja, czysta matematyka/logika, wejście i wyjście ściśle zdefiniowane, obowiązkowy test jednostkowy w tym samym pliku.

- [x] M1 `blend.rs`: dodać `opacity_factor(opacity: f32) -> u32` i `apply_opacity_factor(pixel, factor: u32)`; w `document.rs` liczyć factor raz na warstwę, nie per piksel. Testy bez zmian muszą przejść.
- [x] M2 `app/src/main.rs` TileRenderer: przy zoom < 1.0 używaj `TextureOptions::LINEAR`, przy zoom >= 1.0 `NEAREST`; przy przekroczeniu progu wywołaj `mark_all_dirty()` (ponowne wysłanie kafli z nową opcją). Dodaj test progu.
- [x] M3 `crates/core/src/blend.rs`: do `enum BlendMode` dodać `Multiply`. Dodać funkcję `blend_multiply(dst: Rgba8, src: Rgba8) -> Rgba8` dla premultiplied alpha (wzór W3C dla premultiplied: kanał `c = div255_round(s_c*(255-d_a) + d_c*(255-s_a) + s_c*d_c)`, alfa `a = s_a + d_a - div255_round(s_a*d_a)`, wynik min(255)). Dodać test `test_blend_multiply`.
- [x] M4 `crates/core/src/blend.rs`: tryby Screen, Darken, Lighten, Overlay (wzory W3C dla premultiplied alpha, alfa jak w Multiply) + testy.
- [x] M5 `crates/core/src/brush.rs`: funkcja `brush_falloff(dist: f32, radius: f32, hardness: f32) -> u8`. Zwraca krycie 0..=255. Jeśli dist > radius zwraca 0. W promieniu `radius * hardness` zwraca 255, na zewnątrz płynny spadek liniowy. Dodać testy.
- [x] M6 `crates/core/src/stroke.rs`: funkcja `interpolate_stroke(p1: (f32, f32), p2: (f32, f32), spacing: f32) -> Vec<(f32, f32)>`. Generuje punkty co `spacing` px wzdłuż odcinka od p1 do p2. Dodać testy.
- [x] M7 `crates/core/src/color.rs`: funkcje `hex_to_rgb(hex: &str) -> Result<[u8; 3], &'static str>` i `rgb_to_hex(r: u8, g: u8, b: u8) -> String`. Dodać testy formatów 6-znakowych np. `#ff0000`.
- [x] M8 `crates/core/src/filters.rs`: funkcje `adjust_brightness(c: u8, delta: i16) -> u8` (clamp 0..=255) oraz `adjust_contrast(c: u8, factor: f32) -> u8`. Dodać testy.
- [x] M9 `crates/core/src/gaussian.rs`: funkcja `gaussian_kernel_1d(radius: usize, sigma: f32) -> Vec<f32>` wyliczająca symetryczny wektor wag o długości `2 * radius + 1` ze znormalizowaną sumą 1.0. Dodać testy.
- [x] M10 `blend.rs`: `impl BlendMode { pub const ALL: [BlendMode; 6]; pub fn label(self) -> &'static str }` (polskie nazwy) + test.
- [x] M11 `app/src/main.rs`: ComboBox trybu mieszania aktywnej warstwy w panelu warstw (History::push + mark_all_dirty przy zmianie).
- [x] M12 `crates/core/src/hsl.rs`: `rgb_to_hsl([u8;3]) -> [f32;3]` i `hsl_to_rgb([f32;3]) -> [u8;3]` (h w stopniach 0..360, s,l 0..1) + testy round-trip.
- [x] M13 `crates/core/src/blur.rs`: `blur_pass(src, dst, w, h, kernel, horizontal)` — jedno przejście rozmycia separowalnego na buforze Rgba8 (premultiplied), krawędzie clamp + testy.
- [x] M14 `crates/core/src/fill.rs`: `flood_fill_mask(buf, w, h, x, y, tolerance) -> Vec<u8>` (scanline, stos, bez rekurencji) + testy.
- [x] M15 `crates/core/src/gradient.rs`: `linear_t(p, a, b) -> f32` i `lerp_rgba(c0, c1, t) -> Rgba8` + testy.
- [x] M16 `crates/app/src/theme.rs`: parser `colors.toml` motywu Omarchy (bez crate toml) + ścieżka `~/.local/state/omarchy/current/theme/colors.toml` + testy.

## Wznowienie pracy (stan na 2026-09-30)

Zrobione: wszystkie etapy E0–E13 i mikrozadania M1–M23. 171 testów, clippy czysty.
Pomiary (release): binarka 8,5 MB, ~100 MB RAM na starcie z dokumentem 1920x1080, 0% CPU w bezczynności.
Pomysły na dalej: zaznaczenie odręczne (lasso) i różdżka (fill.rs gotowe), tekst, przekształcenia (skala/obrót),
kafle GPU w pamięci tylko dla widocznego obszaru przy dużych obrazach, pomiar i ew. rayon dla filtrów.
Serwer Ollama: adres w zmiennej OLLAMA_API_BASE (np. http://localhost:11434).

## Przeprojektowanie GUI w stylu GIMP 2.10 (etapy G)

Cel: układ i wygląd jak GIMP 2.10 (motyw Dark, ikony symboliczne), zachowując lekkość (0% CPU w bezczynności).
- [x] G0 Podział `app/src/main.rs` na moduły `app/src/ui/{mod,menus,toolbox,tool_options,layers,history_panel,statusbar,canvas,dialogs}.rs` + `app/src/state.rs`. BEZ zmian zachowania. Testy muszą przejść.
- [x] G1 Układ: pasek menu GIMP (Plik, Edycja, Zaznaczenie, Widok, Obraz, Warstwa, Kolory, Narzędzia, Filtry, Okna, Pomoc); lewy dok = skrzynka narzędzi (siatka ikon, podpowiedź z nazwą i skrótem) + kwadraty FG/BG (zamiana X, reset D) + Opcje narzędzia; prawy dok = góra: zakładki Pędzle / Historia cofania, dół: Warstwy; dół: pasek stanu (x,y w px, zoom, rozmiar obrazu, pamięć historii).
- [x] G2 Płótno: linijki górna i lewa, szachownica pod przezroczystością, ramka obrazu, szare tło poza obrazem.
- [x] G3 Panel Warstwy jak GIMP: Tryb + Krycie na górze, wiersze (oko, miniatura 32 px, nazwa), przyciski na dole (nowa, w górę, w dół, duplikuj, usuń) jako ikony.
- [x] G4 Motyw „GIMP 2.10 Dark” (szarości ~#454545/#3c3c3c/#303030, małe odstępy, mniejsza czcionka), akcent z motywu Omarchy.
- [x] M24 `core/src/ruler.rs`: podziałka linijki (pozycje kresek i etykiet dla zoom/przesunięcia) + testy.
- [x] M25 `core/src/thumbnail.rs`: miniatura warstwy/dokumentu (downsampling box, max bok N) + testy.
- [x] M26 `app/src/ui/icons.rs`: ikony narzędzi rysowane `egui::Painter` (wektorowo, jeden kolor) — po G0.
- [x] M27 `app/src/ui/gimp_theme.rs`: `gimp_dark_visuals(accent: Option<Color32>) -> egui::Visuals` + `apply_gimp_style(ctx)` (odstępy, rozmiary czcionek).
- [x] M28 Tab chowa/pokazuje oba doki (jak w GIMP), gdy żadne pole tekstowe nie ma fokusu.
- [x] M29 Dopasowanie widoku przy starcie/otwarciu liczone po ułożeniu doków (pierwsza klatka z rzeczywistym rozmiarem CentralPanel).
- [x] M30 `selection.rs`: `SelectMode {Replace, Add, Subtract, Intersect}` + `combine_mask(&mut self, mask: &[u8], mode)` (maska w*h) + testy.
- [x] M31 `fill.rs`: `mask_alpha(layer, mask)` (mnoży piksele premultiplied przez m/255) + `border_background_mask(buf, w, h, tol)` (flood fill od 4 rogów, suma) + testy.
- [x] M32 nowy crate `crates/ai` (`dziwak-ai`, tract-onnx): `BgRemover::load(path)`, `foreground_mask(rgba_straight, w, h) -> Vec<u8>` (U²-Net-p 320x320, normalizacja jak rembg) + testy.
- [x] M33 app: narzędzie Różdżka (U): tolerancja, próbkuj wszystkie warstwy, Shift=dodaj, Ctrl=odejmij, Shift+Ctrl=przetnij.
- [x] M34 app: Warstwa → Usuń jednolite tło; Warstwa → Usuń tło (AI) i Zaznaczenie → Pierwszy plan (AI) w wątku tła, feature `ai`.
Model AI: `~/.local/share/dziwak/models/u2netp.onnx` (Apache-2.0, z wydań rembg), pobierany przez `scripts/fetch-models.sh`.
- [x] M34a state.rs: remove_uniform_background, start_ai_background, poll_ai_background (kanał mpsc, wątek).
- [x] M35 i18n: polski/angielski/hiszpański (Edycja → Język), tabela w app/src/i18n_table.rs przetłumaczona lokalnym modelem.

## Narzędzia GIMP 2.10 (etapy T)

Skróty jak w GIMP 2.10: R zazn. prostokątne, E zazn. eliptyczne, F odręczne, U różdżka, Shift+O wg koloru, M przesuwanie,
Shift+C kadrowanie, Shift+R obrót, Shift+T skalowanie, Shift+F odbicie, N ołówek, P pędzel, A aerograf, Shift+E gumka,
Shift+B kubełek, G gradient, O pipeta, C klonowanie, Shift+S rozmazywanie, Shift+D rozjaśnianie/ściemnianie,
Shift+U rozmywanie/wyostrzanie, T tekst, Z lupa, Shift+M miarka, Spacja przesuwanie widoku.
- [x] T0 Skróty jak wyżej + skrzynka narzędzi w grupach (zaznaczenia | transformacje | malowanie | kolory/inne) + ikony nowych narzędzi.
- [x] T1 Przesuwanie (M): przeciąganie przesuwa piksele aktywnej warstwy (lub zawartość zaznaczenia), jedna pozycja historii.
- [x] T2 Kadrowanie (Shift+C): prostokąt → Enter przycina dokument (wszystkie warstwy); Obraz → Przytnij do zaznaczenia.
- [x] T3 Zaznaczenie odręczne (F): klikane wielokąty / przeciąganie, zamknięcie dwuklikiem lub Enter; maska wielokąta z AA.
- [x] T4 Zaznaczenie wg koloru (Shift+O): wszystkie piksele o podobnym kolorze w całym obrazie.
- [x] T5 Ołówek (N): twarde krawędzie bez wygładzania; Aerograf (A): nakładanie w czasie przy przytrzymaniu.
- [x] T6 Transformacje warstwy: Odbicie poziome/pionowe (Shift+F), Obrót (Shift+R, dowolny kąt), Skalowanie (Shift+T), interpolacja dwuliniowa.
- [x] T7 Obraz: Skaluj obraz, Rozmiar płótna, Obróć 90°/180°, Odbij obraz.
- [x] T8 Klonowanie (C): Ctrl+klik = źródło, malowanie kopiuje z przesunięciem.
- [x] T9 Rozmazywanie (Shift+S), Rozjaśnianie/Ściemnianie (Shift+D), Rozmywanie/Wyostrzanie pędzlem (Shift+U).
- [x] T10 Tekst (T): warstwa tekstowa rastrowana czcionką (ab_glyph, czcionka z systemu lub wbudowana DejaVu).
- [x] T11 Lupa (Z): klik = powiększ, Alt/Ctrl+klik = pomniejsz, przeciągnięcie = powiększ obszar; Miarka (Shift+M): długość i kąt w pasku stanu.
Mikrozadania rdzenia (aider): 
- [x] M36 `core/src/polygon.rs`: maska wielokąta (scanline even-odd, AA 4x w pionie) + testy.
- [x] M37 `core/src/fill.rs`: `color_select_mask(buf, w, h, color, tol)` (globalnie) + testy.
- [x] M38 `core/src/transform.rs`: flip_h/flip_v/rotate90/rotate180 dla TiledLayer + testy.
- [x] M39 `core/src/transform.rs`: `resample_bilinear(src, sw, sh, dw, dh)` i `rotate_bilinear(src, w, h, angle)` na buforach Rgba8 + testy.
- [x] M40 `core/src/document.rs`: `crop(rect)` i `resize_canvas(w, h, offset)` dla wszystkich warstw + testy.
- [x] M41 `core/src/brush.rs`: dab ołówka bez AA; `dodge_burn`, `smudge` na pikselu (czysta matematyka) + testy.
- [x] M42 `brush.rs`: `clone_dab(layer, source, cx, cy, dx, dy, radius, hardness, opacity, sel)` (klonowanie) + testy. Ołówek (T5) = apply_dab z hardness 1.0 (maska bez AA).
- [x] M43 `core/src/text.rs`: `rasterize_text(font_data, text, size_px) -> Option<TextMask>` (ab_glyph, wiele linii) + `stamp_mask(layer, &TextMask, x, y, color)` + testy.
