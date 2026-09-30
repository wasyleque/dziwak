# Dziwak

**Polski** | [English](README.en.md)

Lekki edytor grafiki rastrowej w Ruście dla Omarchy Linux (Wayland/Hyprland), z interfejsem w stylu GIMP 2.10.
Licencja GPL-3.0-or-later.

Około 100 MB RAM, 0% CPU w bezczynności, bez wymagań co do karty graficznej.

## Możliwości

- **Warstwy:** krycie, 6 trybów mieszania, miniatury
- **Zaznaczenia:** prostokątne, eliptyczne, odręczne (lasso), różdżka, wg koloru; łączenie (Shift = dodaj, Ctrl = odejmij, Shift+Ctrl = przetnij)
- **Malowanie:** pędzel, ołówek, aerograf, gumka, kubełek, gradient, klonowanie, rozmazywanie, rozjaśnianie/ściemnianie, rozmywanie/wyostrzanie
- **Przekształcenia:** przesuwanie, kadrowanie, obrót, skalowanie, odbicie; skalowanie obrazu i rozmiar płótna
- **Tekst:** czcionki systemowe (przez fontconfig), tekst na nowej warstwie
- **Filtry z podglądem:** jasność/kontrast, odcień/nasycenie, rozmycie Gaussa, wyostrzanie
- **Usuwanie tła:** jednolitego (od krawędzi) oraz AI (U²-Net-p lokalnie na CPU, ok. 1–2 s, bez GPU)
- **Pliki:** PNG, JPEG, WebP oraz własny format `.dziwak` (warstwy, kompresja LZ4)
- **Interfejs:** układ GIMP 2.10 Dark z akcentem z motywu Omarchy; język polski, angielski, hiszpański (Edycja → Język)
- Cofanie i ponawianie z limitem pamięci (migawki kafli copy-on-write)

## Budowanie

Wymagany Rust stable (np. `mise use -g rust@stable`).

```bash
cargo run -p dziwak --release
```

Wersja bez AI (mniejsza binarka, szybsza kompilacja): `cargo build --release -p dziwak --no-default-features`.

## Instalacja

```bash
scripts/install-local.sh
```

Instaluje do `~/.local` (program, ikona, wpis w launcherze) i pobiera model AI (4,6 MB, Apache-2.0) do `~/.local/share/dziwak/models/`.

## Skróty klawiszowe

Narzędzia mają skróty jak w GIMP 2.10.

| Skrót | Narzędzie | Skrót | Narzędzie |
|---|---|---|---|
| R | Zaznaczenie prostokątne | P | Pędzel |
| E | Zaznaczenie eliptyczne | N | Ołówek |
| F | Zaznaczenie odręczne | A | Aerograf |
| U | Różdżka | Shift+E | Gumka |
| Shift+O | Zaznaczenie wg koloru | Shift+B | Kubełek |
| M | Przesuwanie | G | Gradient |
| Shift+C | Kadrowanie | C | Klonowanie (Ctrl+klik = źródło) |
| Shift+R | Obrót | Shift+S | Rozmazywanie |
| Shift+T | Skalowanie | Shift+D | Rozjaśnianie/Ściemnianie |
| Shift+F | Odbicie | Shift+U | Rozmywanie/Wyostrzanie |
| O | Pipeta | T | Tekst |
| Z | Lupa | Shift+M | Miarka |

| Skrót | Działanie |
|---|---|
| Ctrl+O / Ctrl+Shift+S | Otwórz / Zapisz jako |
| Ctrl+Z / Ctrl+Shift+Z, Ctrl+Y | Cofnij / Ponów |
| Ctrl+A / Ctrl+Shift+A / Ctrl+I | Zaznacz wszystko / Odznacz / Odwróć zaznaczenie |
| X / D | Zamień kolory / Domyślne kolory |
| Tab | Pokaż/ukryj doki |
| Shift+J / Shift+Ctrl+J | Wyśrodkuj obraz / Dopasuj do okna |
| Kółko myszy | Zoom wokół kursora |
| Środkowy przycisk, Spacja+LPM | Przesuwanie widoku |

## Architektura

- `crates/core`: logika bez UI. Warstwy jako kafle 64×64 (copy-on-write), piksele RGBA8 premultiplied, historia na migawkach kafli, filtry, zaznaczenia, przekształcenia, tekst, format `.dziwak`.
- `crates/app`: interfejs w `egui`/`eframe` (backend `glow`); na GPU trafiają tylko zmienione kafle.
- `crates/ai`: usuwanie tła modelem U²-Net-p przez `tract` (czysty Rust, bez ONNX Runtime).

## Rozwój

Reguły projektu i plan etapów: `AGENTS.md`. Testy: `cargo test --workspace` (test z modelem AI: `cargo test -p dziwak-ai -- --ignored`).
