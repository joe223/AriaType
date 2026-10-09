#[cfg(any(windows, test))]
#[path = "autostart/windows.rs"]
mod windows;
#[cfg(windows)]
pub(super) use windows::WindowsStartupRegistration;

pub(super) trait StartupRegistration {
    type Snapshot;

    fn is_enabled(&self) -> Result<bool, String>;
    fn enable(&self) -> Result<(), String>;
    fn disable(&self) -> Result<(), String>;
    fn snapshot(&self) -> Result<Self::Snapshot, String>;
    fn restore(&self, snapshot: &Self::Snapshot) -> Result<(), String>;
}

#[cfg(test)]
fn update_auto_start<R: StartupRegistration>(
    saved: &mut bool,
    requested: bool,
    registration: &R,
) -> Result<(), String> {
    update_auto_start_with_persistence(saved, requested, registration, || Ok(()))
}

pub(super) fn reconcile_registration<R: StartupRegistration>(
    requested: bool,
    registration: &R,
) -> Result<(), String> {
    // Refresh the executable path after installation or an application update.
    change_with_persistence(requested, registration, true, || Ok(()))
}

pub(super) fn update_auto_start_with_persistence<R, F>(
    saved: &mut bool,
    requested: bool,
    registration: &R,
    persist: F,
) -> Result<(), String>
where
    R: StartupRegistration,
    F: FnOnce() -> Result<(), String>,
{
    change_with_persistence(requested, registration, false, persist)?;
    *saved = requested;
    Ok(())
}

fn change_with_persistence<R, F>(
    requested: bool,
    registration: &R,
    refresh: bool,
    persist: F,
) -> Result<(), String>
where
    R: StartupRegistration,
    F: FnOnce() -> Result<(), String>,
{
    let snapshot = registration.snapshot()?;
    let registered = registration.is_enabled()?;
    let changed = registered != requested || (refresh && requested);
    let result = if changed {
        if requested {
            registration.enable()
        } else {
            registration.disable()
        }
    } else {
        Ok(())
    }
    .and_then(|()| persist());

    match result {
        Ok(()) => Ok(()),
        Err(error) if changed => match registration.restore(&snapshot) {
            Ok(()) => Err(error),
            Err(rollback_error) => Err(format!(
                "{error}; startup rollback failed: {rollback_error}"
            )),
        },
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        reconcile_registration, update_auto_start, update_auto_start_with_persistence,
        StartupRegistration,
    };
    use std::cell::{Cell, RefCell};

    struct Registration {
        enabled: Cell<bool>,
        fail: Option<&'static str>,
        actions: RefCell<Vec<&'static str>>,
    }

    impl Registration {
        fn new(enabled: bool, fail: Option<&'static str>) -> Self {
            Self {
                enabled: Cell::new(enabled),
                fail,
                actions: RefCell::new(Vec::new()),
            }
        }
    }

    impl StartupRegistration for Registration {
        type Snapshot = bool;

        fn snapshot(&self) -> Result<Self::Snapshot, String> {
            self.is_enabled()
        }

        fn restore(&self, snapshot: &Self::Snapshot) -> Result<(), String> {
            self.enabled.set(*snapshot);
            Ok(())
        }

        fn is_enabled(&self) -> Result<bool, String> {
            if self.fail == Some("query") {
                return Err("startup registration unavailable".to_string());
            }
            Ok(self.enabled.get())
        }

        fn enable(&self) -> Result<(), String> {
            self.actions.borrow_mut().push("enable");
            if self.fail == Some("enable") {
                return Err("startup registration denied".to_string());
            }
            self.enabled.set(true);
            Ok(())
        }

        fn disable(&self) -> Result<(), String> {
            self.actions.borrow_mut().push("disable");
            if self.fail == Some("disable") {
                return Err("startup removal denied".to_string());
            }
            self.enabled.set(false);
            Ok(())
        }
    }

    #[test]
    fn enabling_login_startup_registers_before_updating_setting() {
        let registration = Registration::new(false, None);
        let mut saved = false;
        update_auto_start(&mut saved, true, &registration).unwrap();
        assert!(registration.enabled.get());
        assert!(saved);
        assert_eq!(*registration.actions.borrow(), ["enable"]);
    }

    #[test]
    fn disabling_login_startup_removes_registration() {
        let registration = Registration::new(true, None);
        let mut saved = true;
        update_auto_start(&mut saved, false, &registration).unwrap();
        assert!(!registration.enabled.get());
        assert!(!saved);
        assert_eq!(*registration.actions.borrow(), ["disable"]);
    }

    #[test]
    fn failed_enable_preserves_disabled_setting() {
        let registration = Registration::new(false, Some("enable"));
        let mut saved = false;
        assert!(update_auto_start(&mut saved, true, &registration).is_err());
        assert!(!saved);
    }

    #[test]
    fn failed_disable_preserves_enabled_setting() {
        let registration = Registration::new(true, Some("disable"));
        let mut saved = true;
        assert!(update_auto_start(&mut saved, false, &registration).is_err());
        assert!(saved);
    }

    #[test]
    fn failed_status_query_does_not_change_saved_setting() {
        let registration = Registration::new(false, Some("query"));
        let mut saved = false;
        assert!(update_auto_start(&mut saved, true, &registration).is_err());
        assert!(!saved);
        assert!(registration.actions.borrow().is_empty());
    }

    #[test]
    fn matching_registration_is_idempotent_and_reconciles_imported_setting() {
        let registration = Registration::new(true, None);
        let mut saved = false;
        update_auto_start(&mut saved, true, &registration).unwrap();
        assert!(saved);
        assert!(registration.actions.borrow().is_empty());
    }

    #[test]
    fn startup_reconciliation_refreshes_existing_enabled_registration() {
        let registration = Registration::new(true, None);
        reconcile_registration(true, &registration).unwrap();
        assert_eq!(*registration.actions.borrow(), ["enable"]);
    }

    #[test]
    fn failed_persistence_restores_disabled_registration_and_setting() {
        let registration = Registration::new(false, None);
        let mut saved = false;
        let error = update_auto_start_with_persistence(&mut saved, true, &registration, || {
            Err("settings disk is read-only".to_string())
        })
        .unwrap_err();
        assert_eq!(error, "settings disk is read-only");
        assert!(!registration.enabled.get());
        assert!(!saved);
    }

    #[test]
    fn failed_persistence_restores_enabled_registration_and_setting() {
        let registration = Registration::new(true, None);
        let mut saved = true;
        assert!(
            update_auto_start_with_persistence(&mut saved, false, &registration, || {
                Err("settings disk is read-only".to_string())
            })
            .is_err()
        );
        assert!(registration.enabled.get());
        assert!(saved);
    }
}
