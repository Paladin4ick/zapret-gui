<div align="center">

# zapret GUI

Легковесное приложение для управления zapret в Windows

[![CI](https://github.com/Paladin4ick/zapret-gui/actions/workflows/ci.yml/badge.svg)](https://github.com/Paladin4ick/zapret-gui/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/Paladin4ick/zapret-gui?display_name=tag)](https://github.com/Paladin4ick/zapret-gui/releases/latest)
[![License](https://img.shields.io/badge/license-MIT-a6192e)](LICENSE)

[Скачать последнюю версию](https://github.com/Paladin4ick/zapret-gui/releases/latest) · [Сообщить о проблеме](https://github.com/Paladin4ick/zapret-gui/issues)

</div>

## О проекте

**zapret GUI** — компактная графическая оболочка на Rust и Slint для запуска, настройки и обновления zapret без работы с командной строкой. Она помогает управлять доступом к Discord, YouTube и другим ресурсам, для которых подходят стратегии zapret.

Приложение использует официальные релизы проекта [Flowseal/zapret-discord-youtube](https://github.com/Flowseal/zapret-discord-youtube), автоматически загружая необходимые стратегии, списки и компоненты WinDivert.

> [!IMPORTANT]
> zapret GUI не является VPN, не перенаправляет трафик через сторонние серверы и не устанавливает zapret как постоянную службу Windows.

## Возможности

- включение и выключение zapret из приложения или системного трея;
- автоматическая загрузка и обновление официальной сборки Flowseal;
- выбор и автоматический подбор подходящей стратегии;
- редактирование пользовательских списков доменов и исключений;
- настройка игрового фильтра, IPSet и fake-файлов;
- автозапуск приложения вместе с Windows;
- отдельная настройка запуска zapret при входе в систему;
- работа в системном трее без установки службы;
- файловые журналы с автоматической ротацией;
- адаптивный тёмный интерфейс с поддержкой системного DPI.

## Установка

1. Откройте раздел [Releases](https://github.com/Paladin4ick/zapret-gui/releases/latest).
2. Скачайте архив `zapret-gui-windows-x86_64.zip`.
3. Распакуйте папку `zapret-gui` в любое удобное место с правом записи.
4. Запустите `zapret-gui.exe` и подтвердите запрос прав администратора.
5. При первом запуске дождитесь загрузки файлов Flowseal.
6. Выберите стратегию и нажмите круглую кнопку питания.

Структура релизного архива:

```text
zapret-gui-windows-x86_64.zip
└── zapret-gui/
    └── zapret-gui.exe
```

После первого запуска рядом с программой появятся:

```text
zapret-gui/
├── zapret-gui.exe
├── zapret-gui.json       # настройки приложения
├── zapret/               # официальная сборка Flowseal
└── logs/                 # журналы приложения и запуска
```

## Как это работает

zapret GUI загружает официальный архив `zapret-discord-youtube`, находит доступные сценарии `general*.bat` и запускает выбранную стратегию в фоне. Для перехвата пакетов используется WinDivert, поэтому приложению необходимы права администратора.

Пользовательские списки `*-user.txt` и локальные настройки сохраняются при обновлении файлов Flowseal.

## Системные требования

- Windows 10 или Windows 11 x64;
- права администратора для запуска WinDivert;
- доступ к интернету для первоначальной установки и обновлений;
- PowerShell и `curl.exe` для автоматического тестирования стратегий.

> [!NOTE]
> EXE пока не подписан коммерческим сертификатом. Браузер или антивирус может предупредить о новом или редко скачиваемом файле. Загружайте приложение только со страницы релизов этого репозитория.

## Сборка из исходного кода

Потребуется стабильная версия Rust:

```powershell
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
```

Готовый файл будет находиться в `target/release/zapret-gui.exe`. Проверка и сборка Windows-артефактов также выполняются через GitHub Actions.

## Благодарности

Основные компоненты, стратегии и списки предоставляет проект [zapret-discord-youtube](https://github.com/Flowseal/zapret-discord-youtube) от **Flowseal**.

Авторство и лицензии сторонних файлов сохраняются внутри загружаемого дистрибутива.

## Лицензия

Код zapret GUI распространяется по лицензии [MIT](LICENSE).
