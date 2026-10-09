pub trait TextInjector: Send + Sync {
    /// Insert `text` at the current cursor position.
    ///
    /// `write_clipboard` writes the current text before a clipboard paste.
    /// The caller supplies the fallible backend adapter.
    fn insert(
        &self,
        text: &str,
        write_clipboard: &dyn Fn() -> Result<(), String>,
    ) -> Result<(), String>;
}

#[cfg(target_os = "macos")]
mod macos;
#[cfg(any(test, target_os = "windows"))]
mod windows;

pub fn create_injector() -> Box<dyn TextInjector> {
    #[cfg(target_os = "macos")]
    return Box::new(macos::MacosInjector);
    #[cfg(target_os = "windows")]
    return Box::new(windows::WindowsInjector);
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    compile_error!("text_injector: unsupported platform");
}

#[cfg(any(test, target_os = "windows"))]
pub(crate) const WINDOWS_INPUT_EVENT_MARKER: usize = 0x4152_4941;

pub fn insert_text(
    text: &str,
    write_clipboard: &dyn Fn() -> Result<(), String>,
) -> Result<(), String> {
    create_injector().insert(text, write_clipboard)
}
