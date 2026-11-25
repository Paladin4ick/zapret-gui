use anyhow::Result;
use std::sync::mpsc::Sender;

#[cfg(windows)]
mod windows {
    use super::*;
    use anyhow::{Context, bail};
    use std::{ptr, thread, time::Duration};
    use winapi::{
        shared::minwindef::FALSE,
        um::{
            handleapi::CloseHandle,
            synchapi::{CreateEventW, OpenEventW, SetEvent, WaitForSingleObject},
            winbase::{INFINITE, WAIT_OBJECT_0},
            winnt::{EVENT_MODIFY_STATE, SYNCHRONIZE},
        },
    };

    const EVENT_NAME: &str = "Local\\Paladin4ick-zapret-gui-show";

    pub fn start_listener(sender: Sender<()>) -> Result<()> {
        let name = wide(EVENT_NAME);
        // SAFETY: The name is NUL-terminated, security attributes are intentionally default,
        // and the returned handle is owned by the listener thread until CloseHandle.
        let handle = unsafe { CreateEventW(ptr::null_mut(), FALSE, FALSE, name.as_ptr()) };
        if handle.is_null() {
            bail!("не удалось создать сигнал открытия основного окна");
        }
        let handle_value = handle as usize;
        thread::Builder::new()
            .name("instance-signal".into())
            .spawn(move || {
                let handle = handle_value as winapi::um::winnt::HANDLE;
                loop {
                    // SAFETY: handle remains owned by this thread for the duration of the wait.
                    if unsafe { WaitForSingleObject(handle, INFINITE) } != WAIT_OBJECT_0 {
                        break;
                    }
                    if sender.send(()).is_err() {
                        break;
                    }
                }
                // SAFETY: This is the single closing point for the owned event handle.
                unsafe { CloseHandle(handle) };
            })
            .context("не удалось запустить обработчик повторного открытия")?;
        Ok(())
    }

    pub fn signal_existing() -> Result<()> {
        let name = wide(EVENT_NAME);
        for _ in 0..10 {
            // SAFETY: The name is NUL-terminated and the handle is closed below.
            let handle =
                unsafe { OpenEventW(EVENT_MODIFY_STATE | SYNCHRONIZE, FALSE, name.as_ptr()) };
            if !handle.is_null() {
                // SAFETY: handle was opened with EVENT_MODIFY_STATE.
                let signaled = unsafe { SetEvent(handle) } != 0;
                // SAFETY: handle is valid and no longer used after this call.
                unsafe { CloseHandle(handle) };
                if signaled {
                    return Ok(());
                }
                bail!("не удалось передать сигнал основному окну");
            }
            thread::sleep(Duration::from_millis(50));
        }
        bail!("основной экземпляр приложения не принимает команды")
    }

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }
}

#[cfg(windows)]
pub use windows::{signal_existing, start_listener};

#[cfg(not(windows))]
pub fn start_listener(_sender: Sender<()>) -> Result<()> {
    Ok(())
}

#[cfg(not(windows))]
pub fn signal_existing() -> Result<()> {
    Ok(())
}
