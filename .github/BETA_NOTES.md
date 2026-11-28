# Miku Focus · beta.5

Pomodoro-таймер с Мику, озвучкой и дневником занятий.

## Скачать

- [Windows 10/11 x64 — установщик](https://github.com/Demian4ick/MikuTime/releases/download/v0.1.13-beta.5/Miku-Focus-0.1.13-windows-x64-setup.exe)
- [Arch Linux x86_64 — пакет](https://github.com/Demian4ick/MikuTime/releases/download/v0.1.13-beta.5/miku-focus-0.1.13-1-x86_64.pkg.tar.zst)
- [Контрольные суммы](https://github.com/Demian4ick/MikuTime/releases/download/v0.1.13-beta.5/SHA256SUMS.txt)

В **Настройки → Оформление** можно переключаться между светлой и тёмной темой, добавлять свои обои и выбирать их по миниатюрам. Поддерживаются PNG/JPEG до 20 МиБ. Удаление из галереи не удаляет файлы, а прежний фон остаётся доступным после обновления.

## Установка

**Windows:** запустите `.exe`. Установщик без цифровой подписи, поэтому SmartScreen может показать предупреждение. Если WebView2 отсутствует, для его загрузки нужен интернет.

**Arch Linux:**

```sh
sudo pacman -U ./miku-focus-0.1.13-1-x86_64.pkg.tar.zst
```

Перед обновлением полностью закройте старую версию. История, настройки и свои записи сохраняются. Озвучка и звуки уже включены; Node.js и Rust для установки не нужны.

[Сообщить об ошибке](https://github.com/Demian4ick/MikuTime/issues) · [Инструкция](https://github.com/Demian4ick/MikuTime#readme)
