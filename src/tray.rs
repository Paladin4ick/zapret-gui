use crate::strategy::Strategy;
use anyhow::{Context, Result};
use tray_icon::{
    Icon, TrayIcon, TrayIconBuilder,
    menu::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu},
};

#[derive(Clone, Debug)]
pub enum TrayAction {
    Toggle,
    Open,
    Strategy(String),
    Exit,
}

pub struct TrayController {
    tray: TrayIcon,
    toggle_id: MenuId,
    open_id: MenuId,
    exit_id: MenuId,
    strategy_ids: Vec<(MenuId, String)>,
    state_item: MenuItem,
    toggle_item: MenuItem,
}

impl TrayController {
    pub fn new(strategies: &[Strategy], running: bool) -> Result<Self> {
        let (menu, state_item, toggle_item, toggle_id, open_id, exit_id, strategy_ids) =
            build_menu(strategies, running)?;
        let tray = TrayIconBuilder::new()
            .with_guid(0x8f23_629d_5cf1_4b96_a10d_17ca_391e_62a1)
            .with_tooltip(if running {
                "Zapret включён"
            } else {
                "Zapret выключен"
            })
            .with_icon(load_icon(running)?)
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .build()
            .context("не удалось создать значок системного трея")?;
        Ok(Self {
            tray,
            toggle_id,
            open_id,
            exit_id,
            strategy_ids,
            state_item,
            toggle_item,
        })
    }

    pub fn set_running(&self, running: bool) -> Result<()> {
        self.tray.set_icon(Some(load_icon(running)?))?;
        self.tray.set_tooltip(Some(if running {
            "Zapret включён"
        } else {
            "Zapret выключен"
        }))?;
        self.toggle_item.set_text(if running {
            "Выключить zapret"
        } else {
            "Включить zapret"
        });
        self.state_item.set_text(if running {
            "Состояние: включён"
        } else {
            "Состояние: выключен"
        });
        Ok(())
    }

    pub fn rebuild(&mut self, strategies: &[Strategy], running: bool) -> Result<()> {
        let (menu, state_item, toggle_item, toggle_id, open_id, exit_id, strategy_ids) =
            build_menu(strategies, running)?;
        self.tray.set_menu(Some(Box::new(menu)));
        self.toggle_item = toggle_item;
        self.toggle_id = toggle_id;
        self.open_id = open_id;
        self.exit_id = exit_id;
        self.strategy_ids = strategy_ids;
        self.state_item = state_item;
        Ok(())
    }

    pub fn poll(&self) -> Option<TrayAction> {
        let event = MenuEvent::receiver().try_recv().ok()?;
        if event.id == self.toggle_id {
            return Some(TrayAction::Toggle);
        }
        if event.id == self.open_id {
            return Some(TrayAction::Open);
        }
        if event.id == self.exit_id {
            return Some(TrayAction::Exit);
        }
        self.strategy_ids
            .iter()
            .find(|(id, _)| *id == event.id)
            .map(|(_, file_name)| TrayAction::Strategy(file_name.clone()))
    }
}

type MenuParts = (
    Menu,
    MenuItem,
    MenuItem,
    MenuId,
    MenuId,
    MenuId,
    Vec<(MenuId, String)>,
);

fn build_menu(strategies: &[Strategy], running: bool) -> Result<MenuParts> {
    let menu = Menu::new();
    let state = MenuItem::new(
        if running {
            "Состояние: включён"
        } else {
            "Состояние: выключен"
        },
        false,
        None,
    );
    let toggle = MenuItem::new(
        if running {
            "Выключить zapret"
        } else {
            "Включить zapret"
        },
        true,
        None,
    );
    let open = MenuItem::new("Открыть приложение", true, None);
    let submenu = Submenu::new("Стратегия", true);
    let mut strategy_ids = Vec::new();
    for strategy in strategies {
        let item = MenuItem::new(&strategy.name, true, None);
        strategy_ids.push((item.id().clone(), strategy.file_name.clone()));
        submenu.append(&item)?;
    }
    let exit = MenuItem::new("Выход", true, None);
    menu.append(&state)?;
    menu.append(&toggle)?;
    menu.append(&open)?;
    menu.append(&submenu)?;
    menu.append(&PredefinedMenuItem::separator())?;
    menu.append(&exit)?;
    Ok((
        menu,
        state,
        toggle.clone(),
        toggle.id().clone(),
        open.id().clone(),
        exit.id().clone(),
        strategy_ids,
    ))
}

fn load_icon(running: bool) -> Result<Icon> {
    let bytes: &[u8] = if running {
        include_bytes!("../assets/tray/zapret-on-32.png")
    } else {
        include_bytes!("../assets/tray/zapret-off-32.png")
    };
    let image = image::load_from_memory(bytes)
        .context("не удалось прочитать значок трея")?
        .into_rgba8();
    let (width, height) = image.dimensions();
    Icon::from_rgba(image.into_raw(), width, height).context("некорректный значок трея")
}
