#!/bin/bash

# Ustawienie opcji basha
set -euo pipefail

# Przejdź do katalogu głównego repozytorium
cd "$(dirname "$0")/.."

# Skompiluj projekt w trybie release
cargo build --release -p dziwak -j 3

# Zainstaluj plik wykonywalny
install -Dm755 target/release/dziwak ~/.local/bin/dziwak

# Zainstaluj plik desktop
install -Dm644 data/dziwak.desktop ~/.local/share/applications/dziwak.desktop

# Zainstaluj ikonę
install -Dm644 data/dziwak.svg ~/.local/share/icons/hicolor/scalable/apps/dziwak.svg

# Uruchom update-desktop-database jeśli jest dostępne
if command -v update-desktop-database &> /dev/null; then
    update-desktop-database ~/.local/share/applications || true
fi

# Pobierz model AI do usuwania tła (U²-Net-p, ~4.6 MB), jeśli jeszcze go nie ma
"$(dirname "$0")/fetch-models.sh" || echo "Uwaga: nie pobrano modelu AI (Usuń tło (AI) nie zadziała)"

# Wyświetl komunikat o zakończeniu instalacji
echo "Zainstalowano Dziwaka w ~/.local"
