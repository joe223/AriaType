#![cfg_attr(all(test, not(target_os = "windows")), allow(dead_code))]

use enigo::{Direction, Enigo, Key, Keyboard, Settings};
use tracing::info;

pub struct WindowsInjector;

impl super::TextInjector for WindowsInjector {
    fn insert(
        &self,
        text: &str,
        write_clipboard: &dyn Fn() -> Result<(), String>,
    ) -> Result<(), String> {
        info!(text_len = text.len(), "text_injection_started");
        insert_with_clipboard(text, write_clipboard, &mut || self.paste_from_clipboard())?;
        info!("text_injection_completed-clipboard");
        Ok(())
    }
}

fn insert_with_clipboard(
    text: &str,
    write_clipboard: &dyn Fn() -> Result<(), String>,
    paste: &mut dyn FnMut() -> Result<(), String>,
) -> Result<(), String> {
    if text.is_empty() {
        return Ok(());
    }
    write_clipboard().map_err(|error| format!("clipboard write failed: {error}"))?;
    paste().map_err(|error| format!("clipboard paste failed: {error}"))
}

fn keyboard_settings() -> Settings {
    Settings {
        windows_dw_extra_info: Some(super::WINDOWS_INPUT_EVENT_MARKER),
        ..Settings::default()
    }
}

impl WindowsInjector {
    fn paste_from_clipboard(&self) -> Result<(), String> {
        let mut enigo = Enigo::new(&keyboard_settings())
            .map_err(|error| format!("failed to create keyboard injector: {error}"))?;
        std::thread::sleep(std::time::Duration::from_millis(20));
        send_clipboard_paste_shortcut(&mut enigo)?;
        info!("clipboard_paste_ctrlv_sent");
        Ok(())
    }
}

trait KeyboardDriver {
    fn key(&mut self, key: Key, direction: Direction) -> Result<(), String>;
}

impl KeyboardDriver for Enigo {
    fn key(&mut self, key: Key, direction: Direction) -> Result<(), String> {
        Keyboard::key(self, key, direction).map_err(|error| error.to_string())
    }
}

fn send_clipboard_paste_shortcut(keyboard: &mut dyn KeyboardDriver) -> Result<(), String> {
    let mut errors = release_keyboard_modifiers(keyboard);
    if !errors.is_empty() {
        errors.extend(release_keyboard_modifiers(keyboard));
        return Err(errors.join("; "));
    }

    if let Err(error) = keyboard.key(Key::Control, Direction::Press) {
        errors.push(format!("ctrl_press_failed: {error}"));
        errors.extend(release_keyboard_modifiers(keyboard));
        return Err(errors.join("; "));
    }

    if let Err(error) = keyboard.key(paste_shortcut_key(), Direction::Click) {
        errors.push(format!("v_click_failed: {error}"));
    }

    if let Err(error) = keyboard.key(Key::Control, Direction::Release) {
        errors.push(format!("ctrl_release_failed: {error}"));
    }

    errors.extend(release_keyboard_modifiers(keyboard));

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

fn paste_shortcut_key() -> Key {
    #[cfg(target_os = "windows")]
    {
        Key::V
    }
    #[cfg(not(target_os = "windows"))]
    {
        Key::Unicode('v')
    }
}

fn release_keyboard_modifiers(keyboard: &mut dyn KeyboardDriver) -> Vec<String> {
    [
        Key::Control,
        Key::LControl,
        Key::RControl,
        Key::Shift,
        Key::LShift,
        Key::RShift,
        Key::Alt,
        Key::Option,
        Key::Meta,
        #[cfg(target_os = "windows")]
        Key::LMenu,
        #[cfg(target_os = "windows")]
        Key::RMenu,
        #[cfg(target_os = "windows")]
        Key::LWin,
        #[cfg(target_os = "windows")]
        Key::RWin,
    ]
    .into_iter()
    .filter_map(|key| {
        keyboard
            .key(key, Direction::Release)
            .err()
            .map(|error| format!("modifier_release_failed({key:?}): {error}"))
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        paste_shortcut_key, release_keyboard_modifiers, send_clipboard_paste_shortcut,
        KeyboardDriver,
    };
    use enigo::{Direction, Key};

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct KeyEvent {
        key: Key,
        direction: Direction,
    }

    #[derive(Default)]
    struct FakeKeyboard {
        events: Vec<KeyEvent>,
        failures: Vec<(Key, Direction, &'static str)>,
        matches_to_skip: usize,
    }

    impl FakeKeyboard {
        fn fail_on(mut self, key: Key, direction: Direction, message: &'static str) -> Self {
            self.failures.push((key, direction, message));
            self
        }

        fn skip_matching_failures(mut self, count: usize) -> Self {
            self.matches_to_skip = count;
            self
        }
    }

    impl KeyboardDriver for FakeKeyboard {
        fn key(&mut self, key: Key, direction: Direction) -> Result<(), String> {
            self.events.push(KeyEvent { key, direction });
            if let Some((_, _, message)) =
                self.failures
                    .iter()
                    .find(|(failed_key, failed_direction, _)| {
                        *failed_key == key && *failed_direction == direction
                    })
            {
                if self.matches_to_skip > 0 {
                    self.matches_to_skip -= 1;
                } else {
                    return Err((*message).to_string());
                }
            }

            Ok(())
        }
    }

    #[test]
    fn paste_shortcut_releases_control_when_v_click_fails() {
        let mut keyboard = FakeKeyboard::default().fail_on(
            paste_shortcut_key(),
            Direction::Click,
            "v was blocked",
        );

        let error = send_clipboard_paste_shortcut(&mut keyboard).unwrap_err();

        assert!(error.contains("v_click_failed: v was blocked"));
        let control_press = keyboard
            .events
            .iter()
            .position(|event| event.key == Key::Control && event.direction == Direction::Press)
            .unwrap();
        assert!(keyboard.events[control_press..].starts_with(&[
            KeyEvent {
                key: Key::Control,
                direction: Direction::Press,
            },
            KeyEvent {
                key: paste_shortcut_key(),
                direction: Direction::Click,
            },
            KeyEvent {
                key: Key::Control,
                direction: Direction::Release,
            },
        ]));
    }

    #[test]
    fn paste_shortcut_reports_control_release_failure_after_cleanup_attempt() {
        let mut keyboard = FakeKeyboard::default()
            .fail_on(Key::Control, Direction::Release, "ctrl release blocked")
            .skip_matching_failures(1);

        let error = send_clipboard_paste_shortcut(&mut keyboard).unwrap_err();

        assert!(error.contains("ctrl_release_failed: ctrl release blocked"));
        assert!(error.contains("modifier_release_failed(Control): ctrl release blocked"));
    }

    #[test]
    fn modifier_cleanup_releases_common_modifier_keys() {
        let mut keyboard = FakeKeyboard::default();

        let errors = release_keyboard_modifiers(&mut keyboard);

        assert!(errors.is_empty());
        assert_eq!(
            keyboard.events,
            vec![
                KeyEvent {
                    key: Key::Control,
                    direction: Direction::Release,
                },
                KeyEvent {
                    key: Key::LControl,
                    direction: Direction::Release,
                },
                KeyEvent {
                    key: Key::RControl,
                    direction: Direction::Release,
                },
                KeyEvent {
                    key: Key::Shift,
                    direction: Direction::Release,
                },
                KeyEvent {
                    key: Key::LShift,
                    direction: Direction::Release,
                },
                KeyEvent {
                    key: Key::RShift,
                    direction: Direction::Release,
                },
                KeyEvent {
                    key: Key::Alt,
                    direction: Direction::Release,
                },
                KeyEvent {
                    key: Key::Option,
                    direction: Direction::Release,
                },
                KeyEvent {
                    key: Key::Meta,
                    direction: Direction::Release,
                },
                #[cfg(target_os = "windows")]
                KeyEvent {
                    key: Key::LMenu,
                    direction: Direction::Release
                },
                #[cfg(target_os = "windows")]
                KeyEvent {
                    key: Key::RMenu,
                    direction: Direction::Release
                },
                #[cfg(target_os = "windows")]
                KeyEvent {
                    key: Key::LWin,
                    direction: Direction::Release
                },
                #[cfg(target_os = "windows")]
                KeyEvent {
                    key: Key::RWin,
                    direction: Direction::Release
                },
            ]
        );
    }

    #[test]
    fn literal_delivery_writes_current_text_before_one_paste() {
        use std::cell::RefCell;
        for text in ["hello", "first line\nsecond line\tcolumn", &"x".repeat(401)] {
            let calls = RefCell::new(Vec::new());
            let result = super::insert_with_clipboard(
                text,
                &|| {
                    calls.borrow_mut().push(format!("write:{text}"));
                    Ok(())
                },
                &mut || {
                    calls.borrow_mut().push("paste".to_string());
                    Ok(())
                },
            );
            assert!(result.is_ok());
            assert_eq!(
                *calls.borrow(),
                vec![format!("write:{text}"), "paste".to_string()]
            );
        }
    }

    #[test]
    fn clipboard_write_failure_prevents_paste() {
        let mut pasted = false;
        let result = super::insert_with_clipboard(
            "hello",
            &|| Err("clipboard unavailable".to_string()),
            &mut || {
                pasted = true;
                Ok(())
            },
        );
        assert!(result.unwrap_err().contains("clipboard unavailable"));
        assert!(!pasted);
    }

    #[test]
    fn paste_failure_is_returned_without_retrying_text() {
        let mut attempts = 0;
        let result = super::insert_with_clipboard(&"x".repeat(150), &|| Ok(()), &mut || {
            attempts += 1;
            Err("input blocked".to_string())
        });
        assert!(result.unwrap_err().contains("input blocked"));
        assert_eq!(attempts, 1);
    }

    #[test]
    fn empty_text_does_not_modify_clipboard_or_paste() {
        let result = super::insert_with_clipboard(
            "",
            &|| Err("clipboard must not be used".to_string()),
            &mut || Err("paste must not be used".to_string()),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn windows_injector_marks_its_keyboard_events() {
        assert_eq!(
            super::keyboard_settings().windows_dw_extra_info,
            Some(crate::text_injector::WINDOWS_INPUT_EVENT_MARKER)
        );
    }

    #[test]
    fn paste_releases_held_modifiers_before_pressing_control() {
        let mut keyboard = FakeKeyboard::default();
        send_clipboard_paste_shortcut(&mut keyboard).unwrap();
        let control_press = keyboard
            .events
            .iter()
            .position(|event| event.key == Key::Control && event.direction == Direction::Press)
            .unwrap();
        assert!(control_press > 0);
        assert!(keyboard.events[..control_press]
            .iter()
            .all(|event| event.direction == Direction::Release));
        for key in [
            Key::Control,
            Key::LControl,
            Key::RControl,
            Key::Shift,
            Key::LShift,
            Key::RShift,
            Key::Alt,
            Key::Meta,
        ] {
            assert!(keyboard.events[..control_press]
                .iter()
                .any(|event| event.key == key));
        }
    }

    #[test]
    fn held_modifier_release_failure_prevents_paste_and_retries_cleanup() {
        let mut keyboard = FakeKeyboard::default().fail_on(
            Key::Shift,
            Direction::Release,
            "shift release blocked",
        );
        let error = send_clipboard_paste_shortcut(&mut keyboard).unwrap_err();
        assert!(error.contains("shift release blocked"));
        assert!(!keyboard
            .events
            .iter()
            .any(|event| event.direction == Direction::Click));
        assert!(
            keyboard
                .events
                .iter()
                .filter(|event| event.key == Key::Shift && event.direction == Direction::Release)
                .count()
                >= 2
        );
    }
}
