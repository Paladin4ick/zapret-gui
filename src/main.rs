#![cfg_attr(windows, windows_subsystem = "windows")]

use anyhow::{Context, Result};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::{
    cell::RefCell,
    process::Command,
    rc::Rc,
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::Duration,
};
use zapret_gui::{
    auto_select::{AutoSelector, TestEvent},
    config::{self, AppConfig},
    lists::{self, ListFile},
    paths::AppPaths,
    process::ProcessManager,
    strategy::{self, Strategy},
    tray::{TrayAction, TrayController},
    updater::{UpdateEvent, UpdateResult, Updater},
};

slint::include_modules!();

enum UiEvent {
    OperationFinished(Result<bool, String>),
    UpdateProgress(UpdateEvent),
    UpdateFinished(Result<UpdateResult, String>, bool),
    TestProgress(TestEvent),
    TestFinished(Result<String, String>),
    Log(String),
}

fn main() -> Result<()> {
    let instance = single_instance::SingleInstance::new("Paladin4ick-zapret-gui")
        .context("не удалось проверить другой экземпляр приложения")?;
    if !instance.is_single() {
        return Ok(());
    }

    let paths = AppPaths::discover()?;
    let _ = strategy::prepare_background_launch(&paths.zapret_dir);
    let config = Rc::new(RefCell::new(AppConfig::load(&paths)));
    let process = ProcessManager::new(paths.clone());
    let strategies = Rc::new(RefCell::new(strategy::scan(&paths.zapret_dir)?));
    let list_files = Rc::new(RefCell::new(lists::scan(&paths.zapret_dir)?));
    let auto_selector = AutoSelector::default();
    let (sender, receiver) = mpsc::channel::<UiEvent>();

    select_available_strategy(&paths, &config, &strategies.borrow())?;
    let window = MainWindow::new().context("не удалось создать главное окно")?;
    initialize_window(
        &window,
        &paths,
        &config.borrow(),
        &strategies.borrow(),
        &list_files.borrow(),
        process.is_running(),
    );

    let tray = Rc::new(RefCell::new(TrayController::new(
        &strategies.borrow(),
        process.is_running(),
    )?));
    wire_callbacks(
        &window,
        paths.clone(),
        config.clone(),
        strategies.clone(),
        list_files.clone(),
        process.clone(),
        auto_selector.clone(),
        sender.clone(),
    );

    let should_minimize = config.clone();
    let close_process = process.clone();
    let close_paths = paths.clone();
    let close_window = window.as_weak();
    window.window().on_close_requested(move || {
        if should_minimize.borrow().minimize_to_tray {
            return slint::CloseRequestResponse::HideWindow;
        }
        if let Some(window) = close_window.upgrade() {
            let size = window.window().size();
            let scale = window.window().scale_factor().max(0.1);
            let mut settings = should_minimize.borrow().clone();
            settings.window_width = size.width as f32 / scale;
            settings.window_height = size.height as f32 / scale;
            let _ = settings.save(&close_paths);
        }
        let _ = close_process.stop();
        let _ = slint::quit_event_loop();
        slint::CloseRequestResponse::HideWindow
    });

    let poll_window = window.as_weak();
    let poll_paths = paths.clone();
    let poll_config = config.clone();
    let poll_strategies = strategies.clone();
    let poll_lists = list_files.clone();
    let poll_process = process.clone();
    let poll_tray = tray.clone();
    let poll_sender = sender.clone();
    let startup_sender = poll_sender.clone();
    let tick = Rc::new(RefCell::new(0_u8));
    let timer = slint::Timer::default();
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(150),
        move || {
            let Some(window) = poll_window.upgrade() else {
                return;
            };
            drain_events(
                &window,
                &receiver,
                &poll_paths,
                &poll_config,
                &poll_strategies,
                &poll_lists,
                &poll_process,
                &poll_tray,
                &poll_sender,
            );
            handle_tray_action(
                &window,
                &poll_paths,
                &poll_tray,
                &poll_process,
                &poll_strategies,
                &poll_config,
                &poll_sender,
            );
            let mut value = tick.borrow_mut();
            *value = value.wrapping_add(1);
            if (*value).is_multiple_of(14) {
                set_running_state(&window, &poll_tray.borrow(), poll_process.is_running());
            }
        },
    );

    let update_on_start =
        !paths.zapret_dir.join("service.bat").is_file() || config.borrow().auto_update;
    if update_on_start {
        spawn_update(
            paths.clone(),
            !paths.zapret_dir.join("service.bat").is_file(),
            sender,
            config.borrow().auto_start_zapret,
        );
    } else if config.borrow().auto_start_zapret && !process.is_running() {
        spawn_toggle(
            process,
            strategies.borrow().clone(),
            config.borrow().selected_strategy.clone(),
            startup_sender,
        );
    }

    window.show().context("не удалось показать главное окно")?;
    slint::run_event_loop_until_quit().context("ошибка цикла интерфейса")?;
    let _ = window.hide();
    drop(timer);
    drop(tray);
    drop(instance);
    Ok(())
}

fn initialize_window(
    window: &MainWindow,
    paths: &AppPaths,
    config: &AppConfig,
    strategies: &[Strategy],
    list_files: &[ListFile],
    running: bool,
) {
    window.set_gui_version(env!("CARGO_PKG_VERSION").into());
    window.set_zapret_running(running);
    window.set_strategies(string_model(
        strategies.iter().map(|item| item.name.clone()),
    ));
    window.set_selected_strategy(
        strategy_name_for_file(strategies, config.selected_strategy.as_deref()).into(),
    );
    window.set_minimize_to_tray(config.minimize_to_tray);
    window.set_auto_update(config.auto_update);
    window.set_auto_start(config.auto_start_zapret);
    window.set_game_mode(game_mode_title(&config.game_filter_mode).into());
    window.set_game_tcp(config.game_filter_tcp.clone().into());
    window.set_game_udp(config.game_filter_udp.clone().into());
    window.set_ipset_mode(ipset_mode_title(&config.ipset_mode).into());
    set_fake_controls(window, paths, config);
    window.set_list_titles(string_model(
        list_files.iter().map(|item| item.title.clone()),
    ));
    if let Some(first) = list_files.first() {
        load_list_into_window(window, paths, first);
    }
    let version = Updater::new(paths.clone())
        .ok()
        .and_then(|updater| updater.installed_version());
    window.set_version_text(
        version
            .map(|value| format!("Zapret {value}"))
            .unwrap_or_else(|| "Zapret не установлен".into())
            .into(),
    );
    window.set_update_text("Ожидание проверки".into());
    window.window().set_size(slint::LogicalSize::new(
        config.window_width,
        config.window_height,
    ));
}

#[allow(clippy::too_many_arguments)]
fn wire_callbacks(
    window: &MainWindow,
    paths: AppPaths,
    config: Rc<RefCell<AppConfig>>,
    strategies: Rc<RefCell<Vec<Strategy>>>,
    list_files: Rc<RefCell<Vec<ListFile>>>,
    process: ProcessManager,
    selector: AutoSelector,
    sender: Sender<UiEvent>,
) {
    let toggle_process = process.clone();
    let toggle_strategies = strategies.clone();
    let toggle_config = config.clone();
    let toggle_sender = sender.clone();
    let weak = window.as_weak();
    window.on_toggle_zapret(move || {
        if let Some(window) = weak.upgrade() {
            window.set_busy(true);
            window.set_error_text("".into());
        }
        spawn_toggle(
            toggle_process.clone(),
            toggle_strategies.borrow().clone(),
            toggle_config.borrow().selected_strategy.clone(),
            toggle_sender.clone(),
        );
    });

    let selection_paths = paths.clone();
    let selection_config = config.clone();
    let selection_strategies = strategies.clone();
    let selection_process = process.clone();
    let selection_sender = sender.clone();
    window.on_strategy_selected(move |name| {
        let selected = selection_strategies
            .borrow()
            .iter()
            .find(|item| item.name == name.as_str())
            .cloned();
        let Some(selected) = selected else { return };
        let mut settings = selection_config.borrow().clone();
        settings.selected_strategy = Some(selected.file_name.clone());
        if let Err(error) = settings.save(&selection_paths) {
            let _ = selection_sender.send(UiEvent::Log(error.to_string()));
            return;
        }
        *selection_config.borrow_mut() = settings;
        if selection_process.is_running() {
            let process = selection_process.clone();
            let sender = selection_sender.clone();
            thread::spawn(move || {
                let result = process
                    .restart(&selected)
                    .map(|_| true)
                    .map_err(|error| error.to_string());
                let _ = sender.send(UiEvent::OperationFinished(result));
            });
        }
    });

    let minimize_paths = paths.clone();
    let minimize_config = config.clone();
    window.on_minimize_changed(move |value| {
        let mut settings = minimize_config.borrow().clone();
        settings.minimize_to_tray = value;
        if settings.save(&minimize_paths).is_ok() {
            *minimize_config.borrow_mut() = settings;
        }
    });

    let update_paths = paths.clone();
    let update_process = process.clone();
    let update_sender = sender.clone();
    let update_weak = window.as_weak();
    window.on_check_updates(move || {
        if update_process.is_running() {
            if let Some(window) = update_weak.upgrade() {
                window.set_error_text("Выключите zapret перед обновлением".into());
            }
            return;
        }
        if let Some(window) = update_weak.upgrade() {
            window.set_busy(true);
        }
        spawn_update(update_paths.clone(), false, update_sender.clone(), false);
    });

    let list_paths = paths.clone();
    let callback_lists = list_files.clone();
    let list_weak = window.as_weak();
    window.on_list_selected(move |title| {
        let Some(list) = callback_lists
            .borrow()
            .iter()
            .find(|item| item.title == title.as_str())
            .cloned()
        else {
            return;
        };
        if let Some(window) = list_weak.upgrade() {
            load_list_into_window(&window, &list_paths, &list);
        }
    });

    let save_list_paths = paths.clone();
    let save_list_files = list_files.clone();
    let save_list_weak = window.as_weak();
    window.on_save_list(move |content| {
        let Some(window) = save_list_weak.upgrade() else {
            return;
        };
        let title = window.get_selected_list();
        let Some(list) = save_list_files
            .borrow()
            .iter()
            .find(|item| item.title == title.as_str())
            .cloned()
        else {
            return;
        };
        match lists::save(&save_list_paths.zapret_dir, &list, content.as_str()) {
            Ok(()) => window.set_error_text("Список сохранён".into()),
            Err(error) => window.set_error_text(error.to_string().into()),
        }
    });

    let settings_paths = paths.clone();
    let settings_config = config.clone();
    let settings_process = process.clone();
    let settings_strategies = strategies.clone();
    let settings_sender = sender.clone();
    let settings_weak = window.as_weak();
    window.on_save_settings(
        move |mode, tcp, udp, ipset, auto_update, auto_start, discord_fake, game_fake| {
            let mut settings = settings_config.borrow().clone();
            settings.game_filter_mode = game_mode_value(mode.as_str()).into();
            settings.game_filter_tcp = tcp.into();
            settings.game_filter_udp = udp.into();
            settings.ipset_mode = ipset_mode_value(ipset.as_str()).into();
            settings.auto_update = auto_update;
            settings.auto_start_zapret = auto_start;
            settings.discord_fake = (!discord_fake.is_empty()).then(|| discord_fake.to_string());
            settings.game_fake = (!game_fake.is_empty()).then(|| game_fake.to_string());
            let result = config::apply_flowseal_settings(&settings_paths, &settings)
                .and_then(|_| settings.save(&settings_paths));
            match result {
                Ok(()) => {
                    *settings_config.borrow_mut() = settings.clone();
                    if let Some(window) = settings_weak.upgrade() {
                        window.set_error_text("Настройки сохранены".into());
                    }
                    if settings_process.is_running()
                        && let Some(strategy) = find_selected_strategy(
                            &settings_strategies.borrow(),
                            settings.selected_strategy.as_deref(),
                        )
                    {
                        let process = settings_process.clone();
                        let sender = settings_sender.clone();
                        thread::spawn(move || {
                            let result = process
                                .restart(&strategy)
                                .map(|_| true)
                                .map_err(|error| error.to_string());
                            let _ = sender.send(UiEvent::OperationFinished(result));
                        });
                    }
                }
                Err(error) => {
                    if let Some(window) = settings_weak.upgrade() {
                        window.set_error_text(error.to_string().into());
                    }
                }
            }
        },
    );

    let test_paths = paths.clone();
    let test_process = process.clone();
    let test_selector = selector.clone();
    let test_sender = sender.clone();
    let test_weak = window.as_weak();
    window.on_auto_select(move || {
        let was_running = test_process.is_running();
        if let Err(error) = test_process.stop() {
            if let Some(window) = test_weak.upgrade() {
                window.set_error_text(error.to_string().into());
            }
            return;
        }
        if let Some(window) = test_weak.upgrade() {
            window.set_testing(true);
            window.set_test_current(0);
            window.set_test_total(0);
            window.set_error_text("".into());
        }
        let selector = test_selector.clone();
        let paths = test_paths.clone();
        let sender = test_sender.clone();
        thread::spawn(move || {
            let result = selector
                .run(&paths, |event| {
                    let _ = sender.send(UiEvent::TestProgress(event));
                })
                .map(|strategy| format!("{}|{}", usize::from(was_running), strategy))
                .map_err(|error| format!("{}|{}", usize::from(was_running), error));
            let _ = sender.send(UiEvent::TestFinished(result));
        });
    });

    let cancel_selector = selector;
    window.on_cancel_test(move || cancel_selector.cancel());

    let results_dir = paths.zapret_dir.join("utils/test results");
    let results_weak = window.as_weak();
    window.on_open_test_results(move || {
        let result = std::fs::create_dir_all(&results_dir)
            .context("не удалось создать папку отчётов")
            .and_then(|_| {
                Command::new("explorer.exe")
                    .arg(&results_dir)
                    .spawn()
                    .context("не удалось открыть папку отчётов")
                    .map(|_| ())
            });
        if let Err(error) = result
            && let Some(window) = results_weak.upgrade()
        {
            window.set_error_text(error.to_string().into());
        }
    });
}

#[allow(clippy::too_many_arguments)]
fn drain_events(
    window: &MainWindow,
    receiver: &Receiver<UiEvent>,
    paths: &AppPaths,
    config: &Rc<RefCell<AppConfig>>,
    strategies: &Rc<RefCell<Vec<Strategy>>>,
    list_files: &Rc<RefCell<Vec<ListFile>>>,
    process: &ProcessManager,
    tray: &Rc<RefCell<TrayController>>,
    sender: &Sender<UiEvent>,
) {
    while let Ok(event) = receiver.try_recv() {
        match event {
            UiEvent::OperationFinished(result) => {
                window.set_busy(false);
                match result {
                    Ok(running) => set_running_state(window, &tray.borrow(), running),
                    Err(error) => {
                        append_log(window, format!("Ошибка запуска: {error}"));
                        window.set_error_text(
                            error
                                .lines()
                                .next()
                                .unwrap_or("Не удалось запустить zapret")
                                .into(),
                        );
                    }
                }
            }
            UiEvent::UpdateProgress(progress) => {
                let text = match progress {
                    UpdateEvent::Checking => "Проверка обновлений…".into(),
                    UpdateEvent::Available(version) => format!("Доступна версия {version}"),
                    UpdateEvent::Downloading(percent) => format!("Загрузка: {percent}%"),
                    UpdateEvent::Installing => "Установка обновления…".into(),
                    UpdateEvent::Current(version) => {
                        format!("Установлена актуальная версия {version}")
                    }
                    UpdateEvent::Completed(version) => format!("Обновлено до версии {version}"),
                };
                window.set_update_text(text.into());
            }
            UiEvent::UpdateFinished(result, start_after) => {
                window.set_busy(false);
                match result {
                    Ok(result) => {
                        window.set_version_text(format!("Zapret {}", result.version).into());
                        if result.changed {
                            let _ = lists::ensure_user_lists(&paths.zapret_dir);
                            let _ = strategy::prepare_background_launch(&paths.zapret_dir);
                            if let Ok(found) = strategy::scan(&paths.zapret_dir) {
                                *strategies.borrow_mut() = found;
                                let _ =
                                    select_available_strategy(paths, config, &strategies.borrow());
                                window.set_strategies(string_model(
                                    strategies.borrow().iter().map(|item| item.name.clone()),
                                ));
                                window.set_selected_strategy(
                                    strategy_name_for_file(
                                        &strategies.borrow(),
                                        config.borrow().selected_strategy.as_deref(),
                                    )
                                    .into(),
                                );
                                let _ = tray
                                    .borrow_mut()
                                    .rebuild(&strategies.borrow(), process.is_running());
                            }
                            if let Ok(found) = lists::scan(&paths.zapret_dir) {
                                *list_files.borrow_mut() = found;
                                window.set_list_titles(string_model(
                                    list_files.borrow().iter().map(|item| item.title.clone()),
                                ));
                                if let Some(first) = list_files.borrow().first() {
                                    load_list_into_window(window, paths, first);
                                }
                            }
                            set_fake_controls(window, paths, &config.borrow());
                        }
                    }
                    Err(error) => {
                        window.set_update_text("Не удалось проверить обновления".into());
                        if !paths.zapret_dir.join("service.bat").is_file() {
                            window.set_error_text(error.into());
                        } else {
                            append_log(window, format!("Ошибка обновления: {error}"));
                        }
                    }
                }
                if start_after && !process.is_running() && !strategies.borrow().is_empty() {
                    spawn_toggle(
                        process.clone(),
                        strategies.borrow().clone(),
                        config.borrow().selected_strategy.clone(),
                        sender.clone(),
                    );
                }
            }
            UiEvent::TestProgress(TestEvent::Progress { current, total }) => {
                window.set_test_current(current as i32);
                window.set_test_total(total as i32);
            }
            UiEvent::TestProgress(TestEvent::Log(line)) | UiEvent::Log(line) => {
                append_log(window, line);
            }
            UiEvent::TestFinished(result) => {
                window.set_testing(false);
                match result {
                    Ok(payload) => {
                        let (was_running, best) = split_test_payload(&payload);
                        let selected = strategies
                            .borrow()
                            .iter()
                            .find(|item| {
                                item.name
                                    .eq_ignore_ascii_case(best.trim_end_matches(".bat"))
                                    || item.file_name.eq_ignore_ascii_case(best)
                            })
                            .cloned();
                        if let Some(selected) = selected {
                            let mut settings = config.borrow().clone();
                            settings.selected_strategy = Some(selected.file_name.clone());
                            let _ = settings.save(paths);
                            *config.borrow_mut() = settings;
                            window.set_selected_strategy(selected.name.clone().into());
                            if was_running {
                                window.set_busy(true);
                                let process = process.clone();
                                let sender = sender.clone();
                                thread::spawn(move || {
                                    let result = process
                                        .start(&selected)
                                        .map(|_| true)
                                        .map_err(|error| error.to_string());
                                    let _ = sender.send(UiEvent::OperationFinished(result));
                                });
                            }
                        } else {
                            window.set_error_text(
                                format!("Тест выбрал неизвестную стратегию: {best}").into(),
                            );
                        }
                    }
                    Err(payload) => {
                        let (was_running, error) = split_test_payload(&payload);
                        window.set_error_text(error.into());
                        if was_running
                            && let Some(selected) = find_selected_strategy(
                                &strategies.borrow(),
                                config.borrow().selected_strategy.as_deref(),
                            )
                        {
                            window.set_busy(true);
                            let process = process.clone();
                            let sender = sender.clone();
                            thread::spawn(move || {
                                let result = process
                                    .start(&selected)
                                    .map(|_| true)
                                    .map_err(|error| error.to_string());
                                let _ = sender.send(UiEvent::OperationFinished(result));
                            });
                        }
                    }
                }
            }
        }
    }
}

fn handle_tray_action(
    window: &MainWindow,
    paths: &AppPaths,
    tray: &Rc<RefCell<TrayController>>,
    process: &ProcessManager,
    strategies: &Rc<RefCell<Vec<Strategy>>>,
    config: &Rc<RefCell<AppConfig>>,
    sender: &Sender<UiEvent>,
) {
    let Some(action) = tray.borrow().poll() else {
        return;
    };
    match action {
        TrayAction::Open => {
            let _ = window.show();
            window.window().set_minimized(false);
        }
        TrayAction::Toggle => {
            window.set_busy(true);
            spawn_toggle(
                process.clone(),
                strategies.borrow().clone(),
                config.borrow().selected_strategy.clone(),
                sender.clone(),
            );
        }
        TrayAction::Strategy(file_name) => {
            let selected = strategies
                .borrow()
                .iter()
                .find(|item| item.file_name == file_name)
                .cloned();
            if let Some(selected) = selected {
                let mut settings = config.borrow().clone();
                settings.selected_strategy = Some(file_name);
                if let Err(error) = settings.save(paths) {
                    window.set_error_text(error.to_string().into());
                    return;
                }
                *config.borrow_mut() = settings;
                window.set_selected_strategy(selected.name.clone().into());
                if process.is_running() {
                    window.set_busy(true);
                    let process = process.clone();
                    let sender = sender.clone();
                    thread::spawn(move || {
                        let result = process
                            .restart(&selected)
                            .map(|_| true)
                            .map_err(|error| error.to_string());
                        let _ = sender.send(UiEvent::OperationFinished(result));
                    });
                }
            }
        }
        TrayAction::Exit => {
            let _ = process.stop();
            let _ = slint::quit_event_loop();
        }
    }
}

fn spawn_update(paths: AppPaths, force_install: bool, sender: Sender<UiEvent>, start_after: bool) {
    thread::spawn(move || {
        let result = Updater::new(paths)
            .and_then(|updater| {
                updater.check_and_update(force_install, |event| {
                    let _ = sender.send(UiEvent::UpdateProgress(event));
                })
            })
            .map_err(|error| error.to_string());
        let _ = sender.send(UiEvent::UpdateFinished(result, start_after));
    });
}

fn spawn_toggle(
    process: ProcessManager,
    strategies: Vec<Strategy>,
    selected_file: Option<String>,
    sender: Sender<UiEvent>,
) {
    thread::spawn(move || {
        let result = if process.is_running() {
            process.stop().map(|_| false)
        } else {
            find_selected_strategy(&strategies, selected_file.as_deref())
                .context("выберите доступную стратегию")
                .and_then(|strategy| process.start(&strategy))
                .map(|_| true)
        }
        .map_err(|error| error.to_string());
        let _ = sender.send(UiEvent::OperationFinished(result));
    });
}

fn select_available_strategy(
    paths: &AppPaths,
    config: &Rc<RefCell<AppConfig>>,
    strategies: &[Strategy],
) -> Result<()> {
    let current_exists = config
        .borrow()
        .selected_strategy
        .as_deref()
        .is_some_and(|file| strategies.iter().any(|item| item.file_name == file));
    if !current_exists {
        let mut settings = config.borrow().clone();
        settings.selected_strategy = strategies.first().map(|item| item.file_name.clone());
        settings.save(paths)?;
        *config.borrow_mut() = settings;
    }
    Ok(())
}

fn find_selected_strategy(strategies: &[Strategy], file: Option<&str>) -> Option<Strategy> {
    file.and_then(|file| {
        strategies
            .iter()
            .find(|item| item.file_name == file)
            .cloned()
    })
    .or_else(|| strategies.first().cloned())
}

fn strategy_name_for_file(strategies: &[Strategy], file: Option<&str>) -> String {
    find_selected_strategy(strategies, file)
        .map(|item| item.name)
        .unwrap_or_default()
}

fn string_model(values: impl IntoIterator<Item = String>) -> ModelRc<SharedString> {
    let values = values
        .into_iter()
        .map(SharedString::from)
        .collect::<Vec<_>>();
    ModelRc::from(Rc::new(VecModel::from(values)))
}

fn load_list_into_window(window: &MainWindow, paths: &AppPaths, list: &ListFile) {
    window.set_selected_list(list.title.clone().into());
    window.set_list_editable(list.editable);
    match lists::read(&paths.zapret_dir, list) {
        Ok(content) => window.set_list_content(content.into()),
        Err(error) => window.set_error_text(error.to_string().into()),
    }
}

fn set_fake_controls(window: &MainWindow, paths: &AppPaths, settings: &AppConfig) {
    let files = config::scan_fake_files(paths).unwrap_or_default();
    window.set_fake_files(string_model(files.iter().cloned()));
    let discord = settings
        .discord_fake
        .clone()
        .or_else(|| config::detect_active_fake(paths, "ACTIVE_DISCORD_UDP.bin", &files));
    let game = settings
        .game_fake
        .clone()
        .or_else(|| config::detect_active_fake(paths, "ACTIVE_GAME_UDP.bin", &files));
    window.set_discord_fake(discord.unwrap_or_default().into());
    window.set_game_fake(game.unwrap_or_default().into());
}

fn set_running_state(window: &MainWindow, tray: &TrayController, running: bool) {
    window.set_zapret_running(running);
    let _ = tray.set_running(running);
}

fn append_log(window: &MainWindow, line: String) {
    let mut log = window.get_log_text().to_string();
    if log.len() > 100_000 {
        log.drain(..50_000);
    }
    if !log.is_empty() {
        log.push('\n');
    }
    log.push_str(&line);
    window.set_log_text(log.into());
}

fn split_test_payload(payload: &str) -> (bool, &str) {
    payload
        .split_once('|')
        .map(|(running, value)| (running == "1", value))
        .unwrap_or((false, payload))
}

fn game_mode_title(value: &str) -> &'static str {
    match value {
        "all" => "TCP и UDP",
        "tcp" => "Только TCP",
        "udp" => "Только UDP",
        _ => "Отключён",
    }
}

fn game_mode_value(value: &str) -> &'static str {
    match value {
        "TCP и UDP" => "all",
        "Только TCP" => "tcp",
        "Только UDP" => "udp",
        _ => "disabled",
    }
}

fn ipset_mode_title(value: &str) -> &'static str {
    match value {
        "none" => "Без IPSet",
        "any" => "Любой IP",
        _ => "Загруженный",
    }
}

fn ipset_mode_value(value: &str) -> &'static str {
    match value {
        "Без IPSet" => "none",
        "Любой IP" => "any",
        _ => "loaded",
    }
}
