#[cfg(windows)]
use super::StartupRegistration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StartupKey {
    Run,
    Approval,
}

#[derive(Debug, PartialEq)]
pub(in crate::commands::settings) struct RegistrySnapshot<T> {
    run: Option<T>,
    approval: Option<T>,
}

trait RegistryStore {
    type Value;

    fn read(&self, key: StartupKey, name: &str) -> Result<Option<Self::Value>, String>;
    fn write(&self, key: StartupKey, name: &str, value: &Self::Value) -> Result<(), String>;
    fn remove(&self, key: StartupKey, name: &str) -> Result<(), String>;
    fn command_value(&self, command: &str) -> Self::Value;
    fn enabled_value(&self) -> Self::Value;
}

fn startup_command(executable: &str) -> Result<String, String> {
    if executable.contains(['"', '\0']) {
        return Err("Login startup executable contains an invalid character".to_string());
    }
    let bytes = executable.as_bytes();
    let drive_absolute = bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'\\' | b'/');
    let unc_absolute = executable.strip_prefix(r"\\").is_some_and(|path| {
        let mut parts = path.split(['\\', '/']);
        parts.next().is_some_and(|server| !server.is_empty())
            && parts.next().is_some_and(|share| !share.is_empty())
    });
    if !drive_absolute && !unc_absolute {
        return Err("Login startup executable must have an absolute Windows path".to_string());
    }
    Ok(format!("\"{executable}\""))
}

fn registration_snapshot<R: RegistryStore>(
    registry: &R,
    name: &str,
) -> Result<RegistrySnapshot<R::Value>, String> {
    Ok(RegistrySnapshot {
        run: registry.read(StartupKey::Run, name)?,
        approval: registry.read(StartupKey::Approval, name)?,
    })
}

fn registration_is_present<R: RegistryStore>(registry: &R, name: &str) -> Result<bool, String> {
    registry
        .read(StartupKey::Run, name)
        .map(|value| value.is_some())
}

fn restore_registration<R: RegistryStore>(
    registry: &R,
    name: &str,
    snapshot: &RegistrySnapshot<R::Value>,
) -> Result<(), String> {
    let mut errors = Vec::new();
    for (key, value) in [
        (StartupKey::Run, &snapshot.run),
        (StartupKey::Approval, &snapshot.approval),
    ] {
        let result = match value {
            Some(value) => registry.write(key, name, value),
            None => registry.remove(key, name),
        };
        if let Err(error) = result {
            errors.push(error);
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

fn change_registration<R: RegistryStore>(
    registry: &R,
    name: &str,
    command: &str,
    enabled: bool,
) -> Result<(), String> {
    let snapshot = registration_snapshot(registry, name)?;
    let result = if enabled {
        registry
            .write(StartupKey::Run, name, &registry.command_value(command))
            .and_then(|()| registry.write(StartupKey::Approval, name, &registry.enabled_value()))
    } else {
        registry
            .remove(StartupKey::Run, name)
            .and_then(|()| registry.remove(StartupKey::Approval, name))
    };

    match result {
        Ok(()) => Ok(()),
        Err(error) => match restore_registration(registry, name, &snapshot) {
            Ok(()) => Err(error),
            Err(rollback_error) => Err(format!(
                "{error}; startup rollback failed: {rollback_error}"
            )),
        },
    }
}

#[cfg(windows)]
pub(in crate::commands::settings) struct WindowsStartupRegistration {
    app_name: String,
    command: String,
}

#[cfg(windows)]
impl WindowsStartupRegistration {
    pub(in crate::commands::settings) fn new(
        app_name: &str,
        executable: &std::path::Path,
    ) -> Result<Self, String> {
        if app_name.is_empty() || app_name.contains('\0') {
            return Err("Login startup application name is invalid".to_string());
        }
        if !executable.is_absolute() {
            return Err("Login startup executable must have an absolute Windows path".to_string());
        }
        let executable = executable
            .to_str()
            .ok_or_else(|| "Login startup executable is not valid Unicode".to_string())?;
        Ok(Self {
            app_name: app_name.to_string(),
            command: startup_command(executable)?,
        })
    }
}

#[cfg(windows)]
impl StartupRegistration for WindowsStartupRegistration {
    type Snapshot = RegistrySnapshot<winreg::RegValue>;

    fn is_enabled(&self) -> Result<bool, String> {
        // Presence controls removal even when Task Manager has disabled the entry.
        registration_is_present(&NativeRegistry, &self.app_name)
    }

    fn enable(&self) -> Result<(), String> {
        change_registration(&NativeRegistry, &self.app_name, &self.command, true)
    }

    fn disable(&self) -> Result<(), String> {
        change_registration(&NativeRegistry, &self.app_name, &self.command, false)
    }

    fn snapshot(&self) -> Result<Self::Snapshot, String> {
        registration_snapshot(&NativeRegistry, &self.app_name)
    }

    fn restore(&self, snapshot: &Self::Snapshot) -> Result<(), String> {
        restore_registration(&NativeRegistry, &self.app_name, snapshot)
    }
}

#[cfg(windows)]
struct NativeRegistry;

#[cfg(windows)]
fn registry_key_path(key: StartupKey) -> &'static str {
    match key {
        StartupKey::Run => r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run",
        StartupKey::Approval => {
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run"
        }
    }
}

#[cfg(windows)]
fn registry_error(action: &str, key: StartupKey, error: std::io::Error) -> String {
    format!("Failed to {action} Windows login startup {key:?} value: {error}")
}

#[cfg(windows)]
impl RegistryStore for NativeRegistry {
    type Value = winreg::RegValue;

    fn read(&self, key: StartupKey, name: &str) -> Result<Option<Self::Value>, String> {
        use std::io::ErrorKind;
        use winreg::enums::{HKEY_CURRENT_USER, KEY_READ};
        use winreg::RegKey;

        let root = RegKey::predef(HKEY_CURRENT_USER);
        let registry_key = match root.open_subkey_with_flags(registry_key_path(key), KEY_READ) {
            Ok(registry_key) => registry_key,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(registry_error("read", key, error)),
        };
        match registry_key.get_raw_value(name) {
            Ok(value) => Ok(Some(value)),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
            Err(error) => Err(registry_error("read", key, error)),
        }
    }

    fn write(&self, key: StartupKey, name: &str, value: &Self::Value) -> Result<(), String> {
        use winreg::enums::{HKEY_CURRENT_USER, KEY_SET_VALUE};
        use winreg::RegKey;

        let root = RegKey::predef(HKEY_CURRENT_USER);
        let (registry_key, _) = root
            .create_subkey_with_flags(registry_key_path(key), KEY_SET_VALUE)
            .map_err(|error| registry_error("write", key, error))?;
        registry_key
            .set_raw_value(name, value)
            .map_err(|error| registry_error("write", key, error))
    }

    fn remove(&self, key: StartupKey, name: &str) -> Result<(), String> {
        use std::io::ErrorKind;
        use winreg::enums::{HKEY_CURRENT_USER, KEY_SET_VALUE};
        use winreg::RegKey;

        let root = RegKey::predef(HKEY_CURRENT_USER);
        let registry_key = match root.open_subkey_with_flags(registry_key_path(key), KEY_SET_VALUE)
        {
            Ok(registry_key) => registry_key,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(registry_error("remove", key, error)),
        };
        match registry_key.delete_value(name) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
            Err(error) => Err(registry_error("remove", key, error)),
        }
    }

    fn command_value(&self, command: &str) -> Self::Value {
        use winreg::types::ToRegValue;
        command.to_reg_value()
    }

    fn enabled_value(&self) -> Self::Value {
        winreg::RegValue {
            vtype: winreg::enums::REG_BINARY,
            bytes: vec![2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        change_registration, registration_is_present, registration_snapshot, restore_registration,
        startup_command, RegistryStore, StartupKey,
    };
    use std::cell::RefCell;
    use std::collections::BTreeMap;

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum Value {
        Command(String),
        Binary(Vec<u8>),
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Operation {
        Read,
        Write,
        Remove,
    }

    #[derive(Default)]
    struct Registry {
        values: RefCell<BTreeMap<(u8, String), Value>>,
        failures: RefCell<Vec<(Operation, StartupKey)>>,
        actions: RefCell<Vec<(Operation, StartupKey, String)>>,
    }

    fn key_index(key: StartupKey) -> u8 {
        match key {
            StartupKey::Run => 0,
            StartupKey::Approval => 1,
        }
    }

    impl Registry {
        fn with_existing() -> Self {
            let registry = Self::default();
            registry.put(
                StartupKey::Run,
                "AriaType",
                Value::Command("old command".to_string()),
            );
            registry.put(
                StartupKey::Approval,
                "AriaType",
                Value::Binary(vec![3, 7, 9, 11]),
            );
            registry.put(
                StartupKey::Run,
                "AnotherApp",
                Value::Command("another command".to_string()),
            );
            registry
        }

        fn put(&self, key: StartupKey, name: &str, value: Value) {
            self.values
                .borrow_mut()
                .insert((key_index(key), name.to_string()), value);
        }

        fn fail(&self, operation: Operation, key: StartupKey) {
            self.failures.borrow_mut().push((operation, key));
        }

        fn perform(&self, operation: Operation, key: StartupKey, name: &str) -> Result<(), String> {
            self.actions
                .borrow_mut()
                .push((operation, key, name.to_string()));
            let mut failures = self.failures.borrow_mut();
            if failures.first() == Some(&(operation, key)) {
                failures.remove(0);
                return Err(format!("Registry {operation:?} denied for {key:?}"));
            }
            Ok(())
        }
    }

    impl RegistryStore for Registry {
        type Value = Value;

        fn read(&self, key: StartupKey, name: &str) -> Result<Option<Value>, String> {
            self.perform(Operation::Read, key, name)?;
            Ok(self
                .values
                .borrow()
                .get(&(key_index(key), name.to_string()))
                .cloned())
        }

        fn write(&self, key: StartupKey, name: &str, value: &Value) -> Result<(), String> {
            self.perform(Operation::Write, key, name)?;
            self.put(key, name, value.clone());
            Ok(())
        }

        fn remove(&self, key: StartupKey, name: &str) -> Result<(), String> {
            self.perform(Operation::Remove, key, name)?;
            self.values
                .borrow_mut()
                .remove(&(key_index(key), name.to_string()));
            Ok(())
        }

        fn command_value(&self, command: &str) -> Value {
            Value::Command(command.to_string())
        }

        fn enabled_value(&self) -> Value {
            Value::Binary(vec![2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])
        }
    }

    #[test]
    fn quotes_windows_executable_paths_with_spaces_and_unicode() {
        assert_eq!(
            startup_command(r"C:\Users\Jane Doe\程序\AriaType\ariatype.exe").unwrap(),
            r#""C:\Users\Jane Doe\程序\AriaType\ariatype.exe""#,
        );
        assert_eq!(
            startup_command(r"\\server\shared apps\AriaType.exe").unwrap(),
            r#""\\server\shared apps\AriaType.exe""#,
        );
    }

    #[test]
    fn rejects_relative_drive_relative_and_invalid_executable_paths() {
        for path in [
            "AriaType.exe",
            r"C:AriaType.exe",
            r"\AriaType.exe",
            "C:\\invalid\".exe",
            "C:\\invalid\0.exe",
        ] {
            assert!(startup_command(path).is_err(), "accepted {path:?}");
        }
    }

    #[test]
    fn missing_run_entries_are_disabled_and_permission_errors_are_returned() {
        let registry = Registry::default();
        assert!(!registration_is_present(&registry, "AriaType").unwrap());
        registry.fail(Operation::Read, StartupKey::Run);
        assert!(registration_is_present(&registry, "AriaType")
            .unwrap_err()
            .contains("denied"));
    }

    #[test]
    fn task_manager_disabled_registration_remains_present_for_removal() {
        let registry = Registry::with_existing();
        assert!(registration_is_present(&registry, "AriaType").unwrap());
        change_registration(&registry, "AriaType", "unused", false).unwrap();
        assert!(!registration_is_present(&registry, "AriaType").unwrap());
        assert!(registry
            .read(StartupKey::Approval, "AriaType")
            .unwrap()
            .is_none());
        assert!(registration_is_present(&registry, "AnotherApp").unwrap());
    }

    #[test]
    fn enabling_registers_quoted_command_and_resets_task_manager_approval() {
        let registry = Registry::default();
        let command = startup_command(r"C:\Program Files\AriaType\ariatype.exe").unwrap();
        change_registration(&registry, "AriaType", &command, true).unwrap();
        assert_eq!(
            registry.read(StartupKey::Run, "AriaType").unwrap(),
            Some(Value::Command(command))
        );
        assert_eq!(
            registry.read(StartupKey::Approval, "AriaType").unwrap(),
            Some(registry.enabled_value())
        );
    }

    #[test]
    fn enabling_refreshes_an_existing_executable_path() {
        let registry = Registry::with_existing();
        change_registration(&registry, "AriaType", "new command", true).unwrap();
        assert_eq!(
            registry.read(StartupKey::Run, "AriaType").unwrap(),
            Some(Value::Command("new command".to_string()))
        );
    }

    #[test]
    fn disabling_missing_entries_is_idempotent() {
        let registry = Registry::default();
        change_registration(&registry, "AriaType", "unused", false).unwrap();
        change_registration(&registry, "AriaType", "unused", false).unwrap();
        assert!(registry.values.borrow().is_empty());
    }

    #[test]
    fn query_failure_prevents_registry_mutation() {
        let registry = Registry::with_existing();
        let before = registry.values.borrow().clone();
        registry.fail(Operation::Read, StartupKey::Approval);
        assert!(change_registration(&registry, "AriaType", "new command", true).is_err());
        assert_eq!(*registry.values.borrow(), before);
        assert!(registry
            .actions
            .borrow()
            .iter()
            .all(|(operation, _, _)| *operation == Operation::Read));
    }

    #[test]
    fn failed_run_write_preserves_previous_registry_values() {
        let registry = Registry::with_existing();
        let before = registry.values.borrow().clone();
        registry.fail(Operation::Write, StartupKey::Run);
        assert!(change_registration(&registry, "AriaType", "new command", true).is_err());
        assert_eq!(*registry.values.borrow(), before);
    }

    #[test]
    fn failed_approval_write_rolls_back_changed_run_command() {
        for registry in [Registry::default(), Registry::with_existing()] {
            let before = registry.values.borrow().clone();
            registry.fail(Operation::Write, StartupKey::Approval);
            assert!(
                change_registration(&registry, "AriaType", "new command", true)
                    .unwrap_err()
                    .contains("denied")
            );
            assert_eq!(*registry.values.borrow(), before);
        }
    }

    #[test]
    fn failed_approval_removal_restores_exact_disabled_registration() {
        let registry = Registry::with_existing();
        let before = registry.values.borrow().clone();
        registry.fail(Operation::Remove, StartupKey::Approval);
        assert!(change_registration(&registry, "AriaType", "unused", false).is_err());
        assert_eq!(*registry.values.borrow(), before);
    }

    #[test]
    fn failed_run_removal_preserves_registration_and_reports_error() {
        let registry = Registry::with_existing();
        let before = registry.values.borrow().clone();
        registry.fail(Operation::Remove, StartupKey::Run);
        assert!(change_registration(&registry, "AriaType", "unused", false)
            .unwrap_err()
            .contains("denied"));
        assert_eq!(*registry.values.borrow(), before);
    }

    #[test]
    fn snapshot_restore_preserves_both_original_raw_values() {
        let registry = Registry::with_existing();
        let before = registry.values.borrow().clone();
        let snapshot = registration_snapshot(&registry, "AriaType").unwrap();
        change_registration(&registry, "AriaType", "new command", true).unwrap();
        restore_registration(&registry, "AriaType", &snapshot).unwrap();
        assert_eq!(*registry.values.borrow(), before);
    }

    #[test]
    fn rollback_failure_is_reported_and_other_entry_is_still_restored() {
        let registry = Registry::with_existing();
        registry.fail(Operation::Write, StartupKey::Approval);
        registry.fail(Operation::Write, StartupKey::Run);
        let error = change_registration(&registry, "AriaType", "new command", true).unwrap_err();
        assert!(error.contains("rollback failed"));
        assert_eq!(
            registry.read(StartupKey::Approval, "AriaType").unwrap(),
            Some(Value::Binary(vec![3, 7, 9, 11]))
        );
    }
}
