# Сборка и выпуск

## Проверки

Для разработки нужны Rust ≥ 1.89, Node.js и [системные зависимости Tauri 2](https://v2.tauri.app/start/prerequisites/). Интерфейс написан на Svelte 5 и TypeScript. `npm run tauri dev` запускает приложение из исходников.

```sh
npm ci
npm run check
npm test
cargo test --locked --manifest-path src-tauri/Cargo.toml
npm run tauri build -- --no-bundle -- --locked
```

Для локальной Rust-сборки используйте `CARGO_BUILD_JOBS=1`. Не запускайте несколько сборок одновременно.

## GitHub Actions

Workflow [release.yml](../.github/workflows/release.yml) запускается вручную или по тегу `vX.Y.Z` / `vX.Y.Z-beta.N`. Обычные коммиты и pull request сборку релиза не запускают.

- **Ручной запуск:** выбранные сборки доступны в Artifacts три дня.
- **Тег:** после успешных сборок обеих платформ создаётся черновик релиза.
- **Windows:** NSIS-установщик для текущего пользователя, x64, русский язык.
- **Arch:** пакет x86_64, собранный внутри Arch-контейнера.

Публичный репозиторий использует стандартные GitHub-hosted runner. Для закрытого репозитория требуется `allow_private_build` при ручном запуске или переменная `ENABLE_PRIVATE_BUILDS=true`. Перед включением проверьте тариф и бюджет Actions в аккаунте.

## Подготовка версии

Core-версия должна совпадать в `package.json`, `package-lock.json`, `Cargo.toml`, `Cargo.lock`, `tauri.conf.json` и `packaging/PKGBUILD`. Проверка:

```sh
node scripts/release.mjs check
```

Для очередной беты можно оставить ту же core-версию и увеличить `beta.N`. Номер беты хранится в теге и `BUILD_INFO.json`. Обновите ссылку скачивания в README и текст `.github/BETA_NOTES.md`.

После коммита и отправки кода создайте новый тег:

```sh
# Укажите новый, ещё не опубликованный тег.
git tag v0.1.13-beta.6
git push origin v0.1.13-beta.6
```

Не передвигайте опубликованные теги и не заменяйте их файлы.

## Публикация

1. Дождитесь успеха обеих сборок.
2. Скачайте оба пакета, проверьте `SHA256SUMS.txt` и исходный коммит в `BUILD_INFO.json`.
3. Проверьте установку, запуск, обновление, звук и сохранность данных на целевых ОС. Если часть ручных проверок недоступна, не выдавайте компиляцию за их выполнение.
4. Просмотрите описание черновика и опубликуйте бету:

   ```sh
   gh release edit v0.1.13-beta.6 --draft=false --prerelease --latest=false
   ```

Для беты используйте ссылку на конкретный тег, а не `/releases/latest`.

В релиз входят только установщик Windows, пакет Arch, контрольные суммы и `BUILD_INFO.json`. Встроенное аудио берётся из `public`; локальные профили, дополнительные импорты и `.local-test` не публикуются. Windows-установщик пока не подписан. При отсутствии WebView2 он скачивает runtime Microsoft.
