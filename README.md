# Dziwak

Lekki edytor grafiki rastrowej w Ruście dla Omarchy Linux, w stylu GIMP-a; licencja GPL-3.0-or-later.

## Możliwości

- Warstwy z kryciem i 6 trybami mieszania
- Pędzel i gumka z twardością
- Zaznaczenia prostokątne i eliptyczne
- Filtry: jasność/kontrast, odcień/nasycenie, rozmycie Gaussa, wyostrzanie z podglądem
- Kubełek, pipeta, gradient, różdżka
- Usuwanie tła: jednolitego (od krawędzi) oraz AI (U²-Net-p na CPU, bez GPU, ok. 1–2 s) — Warstwa → Usuń tło (AI), Zaznaczenie → Pierwszy plan (AI)
- Cofanie/ponawianie
- PNG/JPEG/WebP oraz własny format .dziwak z kompresją LZ4
- Kolory z motywu Omarchy
- Język interfejsu: polski, angielski, hiszpański (Edycja → Język; zapis w `~/.config/dziwak/settings`, domyślnie wg `$LANG`)

## Budowanie

`cargo run -p dziwak --release`
Wymagany Rust stable, np. przez `mise use -g rust@stable`

## Instalacja

Model AI pobiera `scripts/fetch-models.sh` (wywoływany przez instalator) do `~/.local/share/dziwak/models/`. Budowanie bez AI: `cargo build --release -p dziwak --no-default-features`.

`scripts/install-local.sh` instaluje do `~/.local` z ikoną i wpisem w launcherze

## Skróty klawiszowe

| Skrót             | Działanie                   |
|-------------------|-----------------------------|
| Ctrl+O            | Otwórz                      |
| Ctrl+Shift+S      | Zapisz jako                 |
| Ctrl+Z            | Cofnij                      |
| Ctrl+Shift+Z / Ctrl+Y | Ponów                   |
| B                 | Pędzel                      |
| E                 | Gumka                       |
| M                 | Zaznaczenie prostokątne     |
| G                 | Kubełek                     |
| U                 | Różdżka (Shift=dodaj, Ctrl=odejmij) |
| I                 | Pipeta                      |
| Shift+G           | Gradient                    |
| X                 | Zamiana kolorów             |
| Alt+klik          | Pipeta w pędzlu             |
| Ctrl+A            | Zaznacz wszystko            |
| Ctrl+Shift+A      | Odznacz                     |
| Ctrl+I            | Odwróć zaznaczenie          |
| Tab               | Pokaż/ukryj doki            |
| Shift+J           | Wyśrodkuj obraz             |
| Shift+Ctrl+J      | Dopasuj do okna             |
| Kółko myszy       | Zoom                        |
| Środkowy przycisk lub Spacja+LPM | Przesuwanie     |

## Architektura

`crates/core` bez UI, kafle 64x64 copy-on-write, historia na migawkach kafli. `crates/app` w egui/glow.

## Rozwój

Reguły w AGENTS.md. Testy: `cargo test --workspace`. Kopia zapasowa: `scripts/backup-usb.sh`.
