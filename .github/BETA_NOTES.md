# Miku Focus · beta.4

Pomodoro-таймер с Мику, озвучкой и дневником занятий.

## Скачать

- [Windows 10/11 x64 — установщик](https://github.com/Demian4ick/MikuTime/releases/download/v0.1.13-beta.4/Miku-Focus-0.1.13-windows-x64-setup.exe)
- [Arch Linux x86_64 — пакет](https://github.com/Demian4ick/MikuTime/releases/download/v0.1.13-beta.4/miku-focus-0.1.13-1-x86_64.pkg.tar.zst)
- [Контрольные суммы](https://github.com/Demian4ick/MikuTime/releases/download/v0.1.13-beta.4/SHA256SUMS.txt)

В этой бете исправлен порядок запуска Windows, из-за которого появлялась ошибка `state not managed`. Категории можно создавать и удалять в настройках; при пустом названии поле подскажет, что нужно заполнить. Занятия удалённой категории остаются в дневнике.

## Установка

**Windows:** запустите `.exe`. Установщик без цифровой подписи, поэтому SmartScreen может показать предупреждение. Если WebView2 отсутствует, для его загрузки нужен интернет.

**Arch Linux:**

```sh
sudo pacman -U ./miku-focus-0.1.13-1-x86_64.pkg.tar.zst
```

Перед обновлением полностью закройте старую версию. История, настройки и свои записи сохраняются. Озвучка и звуки уже включены; Node.js и Rust для установки не нужны.

[Сообщить об ошибке](https://github.com/Demian4ick/MikuTime/issues) · [Инструкция](https://github.com/Demian4ick/MikuTime#readme)
