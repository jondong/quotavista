//! Platform capability adapters for desktop integration.
//!
//! Every answer here is a *detection*, never an assumption about a desktop
//! environment: the composition asks the running platform what it offers and
//! reports that as a capability. A host without a tray, or without a usable
//! notification channel, is a normal state rather than a failure, and the
//! renderer receives an explicit answer it can explain to the user.

use std::sync::Arc;

use quotavista_contracts::{
    ErrorCode, Locale, NotificationCapabilityDto, NotificationPreviewDto, NotificationTemplateKey,
    NotificationTextDto, PlatformCapabilitiesDto, SafeError, TrayAvailabilityDto,
};

/// One rendered synthetic notification.
///
/// The text always comes from the closed catalog in [`render_notice`], never
/// from renderer input, and never names an account, a credential, or a
/// provider.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyntheticNotice {
    pub template_key: NotificationTemplateKey,
    pub locale: Locale,
    pub title: String,
    pub body: String,
}

/// What happened when one synthetic notification was handed to the platform.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeliveryOutcome {
    /// The platform channel accepted the notification.
    Delivered,
    /// It could not be handed over; the carried capability says why.
    Refused(NotificationCapabilityDto),
}

/// The OS notification channel behind a capability answer.
///
/// Detection is kept separate from delivery on purpose: `capability` describes
/// what the platform offers, while `deliver` is only ever reached from an
/// explicit user action. Nothing here sends a notification on a timer, on
/// launch, or on wake.
pub trait NotificationPort: Send + Sync {
    /// What this platform offers, as detected at composition time.
    fn capability(&self) -> NotificationCapabilityDto;

    /// Hand one already-rendered notice to the platform.
    fn deliver(&self, notice: &SyntheticNotice) -> DeliveryOutcome;
}

/// A channel that does not exist on this host.
///
/// Non-desktop builds and every test composition use this, so a capability
/// answer can never claim a desktop that is not there.
pub struct UnsupportedNotifications;

impl NotificationPort for UnsupportedNotifications {
    fn capability(&self) -> NotificationCapabilityDto {
        NotificationCapabilityDto::Unsupported
    }

    fn deliver(&self, _notice: &SyntheticNotice) -> DeliveryOutcome {
        DeliveryOutcome::Refused(NotificationCapabilityDto::Unsupported)
    }
}

/// Render one template in one locale.
///
/// This is the only catalog of notification text in the product, and it is
/// keyed by template rather than by translated episode identity: switching the
/// interface language re-renders the same template and never creates a new
/// alert episode. Later threshold and expiration alerts add their own keys here
/// instead of reusing this one.
pub fn render_notice(template_key: NotificationTemplateKey, locale: Locale) -> SyntheticNotice {
    let (title, body) = match (template_key, locale) {
        (NotificationTemplateKey::SyntheticProbe, Locale::En) => (
            "QuotaVista test notification",
            "Synthetic message from the QuotaVista prototype. It carries no account data.",
        ),
        (NotificationTemplateKey::SyntheticProbe, Locale::ZhCn) => (
            "QuotaVista 测试通知",
            "来自 QuotaVista 原型程序的合成消息，不包含任何账号数据。",
        ),
    };
    SyntheticNotice {
        template_key,
        locale,
        title: title.to_owned(),
        body: body.to_owned(),
    }
}

/// Detected desktop integration capabilities of one running composition.
pub struct PlatformState {
    /// Whether a tray (or macOS menu-bar) icon was actually installed.
    pub tray: TrayAvailabilityDto,
    /// The notification channel behind the reported capability.
    pub notifications: Arc<dyn NotificationPort>,
}

impl PlatformState {
    /// Headless composition: no tray, no notification channel.
    pub fn headless() -> Self {
        Self {
            tray: TrayAvailabilityDto::Unavailable,
            notifications: Arc::new(UnsupportedNotifications),
        }
    }

    /// The capabilities a renderer reads before it explains anything.
    pub fn capabilities(&self) -> PlatformCapabilitiesDto {
        PlatformCapabilitiesDto {
            tray: self.tray,
            notification: self.notifications.capability(),
        }
    }

    /// Render one synthetic notification and hand it to the platform.
    ///
    /// The rendered text is returned whether or not delivery was possible, so
    /// the interface always has something to show: either what the OS accepted,
    /// or the same localized text plus the reason it stayed in the application.
    /// A refused delivery is a reported capability, not an error envelope.
    pub fn preview(
        &self,
        template_key: NotificationTemplateKey,
        locale: Locale,
    ) -> Result<NotificationPreviewDto, SafeError> {
        let notice = render_notice(template_key, locale);
        let (delivered, capability) = match self.notifications.deliver(&notice) {
            DeliveryOutcome::Delivered => (true, NotificationCapabilityDto::Available),
            DeliveryOutcome::Refused(capability) => (false, capability),
        };
        Ok(NotificationPreviewDto {
            template_key,
            locale,
            capability,
            delivered,
            title: bounded_text(&notice.title)?,
            body: bounded_text(&notice.body)?,
        })
    }
}

/// Bind a rendered string into the contract scalar.
///
/// The catalog is fixed and inside the scalar's own bounds, so a rejection here
/// is a defect in this file rather than a runtime condition; it is still
/// reported as a typed error instead of panicking inside a command handler.
fn bounded_text(value: &str) -> Result<NotificationTextDto, SafeError> {
    NotificationTextDto::try_from(value.to_owned())
        .map_err(|_| SafeError::new(ErrorCode::InternalError, None))
}

/// Turn a tray registration result into the capability the lifecycle policy
/// consumes.
///
/// Registration success means different things per platform, and the difference
/// decides whether hiding the main window can strand the user:
///
/// * macOS: the menu bar is a surface of the operating system shell, so a
///   registered status item is displayed. Registration is proof.
/// * Linux: `tray-icon` uses libappindicator by default, whose registration
///   succeeds even when no indicator host is running, so the icon may be
///   invisible. Registration is therefore *not* proof, and this prototype
///   reports the tray as unavailable: closing the main window exits instead of
///   hiding behind an icon nobody can see. Promoting Linux to `Available` needs
///   a host probe validated by a native run, which is P1-13 work.
///
/// A failed registration reports `Unavailable` on every platform, so the close
/// policy always falls back to the branch that cannot orphan the user.
pub fn detect_tray(registered: bool) -> TrayAvailabilityDto {
    const REGISTRATION_IS_PROOF: bool = !cfg!(target_os = "linux");
    match (registered, REGISTRATION_IS_PROOF) {
        (true, true) => TrayAvailabilityDto::Available,
        _ => TrayAvailabilityDto::Unavailable,
    }
}

/// Environment override that reports the tray as unavailable.
///
/// The override is deliberately one-way: it can only make the application claim
/// *less* capability than it detected, never more, so it can never manufacture a
/// false "the tray works" result. It exists because the no-tray close policy has
/// to be reproducible on a host where the menu bar always works (macOS), and
/// because P1-13 must be able to run the same procedure on Ubuntu without
/// removing the shell's indicator host.
pub const TRAY_DISABLE_ENV: &str = "QUOTAVISTA_TRAY";

/// Whether the operator asked for the no-tray behaviour.
pub fn tray_disabled_from_env() -> bool {
    tray_disabled(std::env::var(TRAY_DISABLE_ENV).ok().as_deref())
}

/// Parse the override value. Only the exact string `off` disables the tray, so a
/// typo cannot silently change what closing the main window does.
pub fn tray_disabled(value: Option<&str>) -> bool {
    value == Some("off")
}

/// Notification channel backed by the Tauri notification plugin.
///
/// The plugin is registered from Rust only and no capability permission exposes
/// it to the renderer, so this adapter stays the only path to the OS channel.
pub struct TauriNotifications {
    handle: tauri::AppHandle,
    detected: NotificationCapabilityDto,
}

impl TauriNotifications {
    /// Ask the plugin what this platform offers.
    ///
    /// Observed from the plugin source (tauri-plugin-notification 2.7.0,
    /// `src/desktop.rs`): on desktop `permission_state` always resolves to
    /// `Granted` because desktop applications do not request that permission,
    /// and `show` hands the notification to a background task without reporting
    /// the OS response. A desktop answer is therefore *channel presence*, not a
    /// delivery guarantee - which is why the renderer always keeps an in-app
    /// status next to whatever this reports.
    pub fn detect(handle: tauri::AppHandle) -> Self {
        use tauri::Manager;
        use tauri_plugin_notification::{Notification, PermissionState};

        let detected = if handle.try_state::<Notification<tauri::Wry>>().is_none() {
            // The plugin is not registered, so no channel exists to address.
            NotificationCapabilityDto::Unsupported
        } else {
            use tauri_plugin_notification::NotificationExt;
            match handle.notification().permission_state() {
                Ok(PermissionState::Granted) => NotificationCapabilityDto::Available,
                // Denied, Prompt and PromptWithRationale all mean the channel
                // may not be used right now; the desktop plugin never returns
                // them, and a future mobile target must not read them as "on".
                _ => NotificationCapabilityDto::PermissionDenied,
            }
        };
        Self { handle, detected }
    }
}

impl NotificationPort for TauriNotifications {
    fn capability(&self) -> NotificationCapabilityDto {
        self.detected
    }

    fn deliver(&self, notice: &SyntheticNotice) -> DeliveryOutcome {
        use tauri_plugin_notification::NotificationExt;

        if self.detected != NotificationCapabilityDto::Available {
            return DeliveryOutcome::Refused(self.detected);
        }
        let prepared = self
            .handle
            .notification()
            .builder()
            .title(notice.title.clone())
            .body(notice.body.clone());
        match prepared.show() {
            Ok(()) => DeliveryOutcome::Delivered,
            Err(_) => DeliveryOutcome::Refused(NotificationCapabilityDto::Unsupported),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Records what was asked of it and answers with a fixed capability.
    struct FakeNotifications {
        capability: NotificationCapabilityDto,
        outcome: DeliveryOutcome,
        delivered: std::sync::Mutex<Vec<SyntheticNotice>>,
    }

    impl FakeNotifications {
        fn new(capability: NotificationCapabilityDto, outcome: DeliveryOutcome) -> Self {
            Self {
                capability,
                outcome,
                delivered: std::sync::Mutex::new(Vec::new()),
            }
        }

        fn delivered(&self) -> Vec<SyntheticNotice> {
            self.delivered.lock().expect("fake stays usable").clone()
        }
    }

    impl NotificationPort for FakeNotifications {
        fn capability(&self) -> NotificationCapabilityDto {
            self.capability
        }

        fn deliver(&self, notice: &SyntheticNotice) -> DeliveryOutcome {
            self.delivered
                .lock()
                .expect("fake stays usable")
                .push(notice.clone());
            self.outcome
        }
    }

    fn state(port: Arc<dyn NotificationPort>, tray: TrayAvailabilityDto) -> PlatformState {
        PlatformState {
            tray,
            notifications: port,
        }
    }

    #[test]
    fn a_headless_composition_claims_no_desktop_capability() {
        let capabilities = PlatformState::headless().capabilities();
        assert_eq!(capabilities.tray, TrayAvailabilityDto::Unavailable);
        assert_eq!(
            capabilities.notification,
            NotificationCapabilityDto::Unsupported
        );
    }

    #[test]
    fn every_template_renders_in_both_locales_with_distinct_bounded_text() {
        for key in [NotificationTemplateKey::SyntheticProbe] {
            let en = render_notice(key, Locale::En);
            let zh = render_notice(key, Locale::ZhCn);
            for notice in [&en, &zh] {
                assert!(!notice.title.is_empty());
                assert!(!notice.body.is_empty());
                assert!(notice.title.chars().count() <= 200);
                assert!(notice.body.chars().count() <= 200);
                assert!(!notice.title.chars().any(char::is_control));
                assert!(!notice.body.chars().any(char::is_control));
                assert!(bounded_text(&notice.title).is_ok());
                assert!(bounded_text(&notice.body).is_ok());
            }
            // A locale switch re-renders the same template; it is not a new
            // episode and the two locales must not collapse into one string.
            assert_eq!(en.template_key, zh.template_key);
            assert_ne!(en.title, zh.title);
            assert_ne!(en.body, zh.body);
        }
    }

    #[test]
    fn rendered_notification_text_names_no_account_or_credential() {
        for locale in [Locale::En, Locale::ZhCn] {
            let notice = render_notice(NotificationTemplateKey::SyntheticProbe, locale);
            let joined = format!("{} {}", notice.title, notice.body).to_lowercase();
            for forbidden in [
                "api",
                "key",
                "token",
                "secret",
                "credential",
                "password",
                "bearer",
                "demo-kimi",
                "demo-openrouter",
                "synthetic-connection",
                "kimi",
                "openrouter",
            ] {
                assert!(
                    !joined.contains(forbidden),
                    "notification text must not mention {forbidden:?}: {joined}"
                );
            }
        }
    }

    #[test]
    fn an_available_channel_reports_the_text_it_accepted() {
        let fake = Arc::new(FakeNotifications::new(
            NotificationCapabilityDto::Available,
            DeliveryOutcome::Delivered,
        ));
        let preview = state(fake.clone(), TrayAvailabilityDto::Available)
            .preview(NotificationTemplateKey::SyntheticProbe, Locale::ZhCn)
            .expect("preview is built from the fixed catalog");

        assert!(preview.delivered);
        assert_eq!(preview.capability, NotificationCapabilityDto::Available);
        assert_eq!(preview.locale, Locale::ZhCn);
        assert_eq!(
            preview.title.as_str(),
            render_notice(NotificationTemplateKey::SyntheticProbe, Locale::ZhCn).title
        );
        // The port saw exactly one notice, and it was the localized one.
        let delivered = fake.delivered();
        assert_eq!(delivered.len(), 1);
        assert_eq!(delivered[0].locale, Locale::ZhCn);
    }

    #[test]
    fn an_unusable_channel_keeps_the_explanation_in_the_application() {
        for (capability, outcome) in [
            (
                NotificationCapabilityDto::Unsupported,
                DeliveryOutcome::Refused(NotificationCapabilityDto::Unsupported),
            ),
            (
                NotificationCapabilityDto::PermissionDenied,
                DeliveryOutcome::Refused(NotificationCapabilityDto::PermissionDenied),
            ),
        ] {
            let fake = Arc::new(FakeNotifications::new(capability, outcome));
            let platform = state(fake.clone(), TrayAvailabilityDto::Unavailable);
            assert_eq!(platform.capabilities().notification, capability);

            let preview = platform
                .preview(NotificationTemplateKey::SyntheticProbe, Locale::En)
                .expect("a refused delivery is still a report");
            assert!(!preview.delivered);
            assert_eq!(preview.capability, capability);
            // The port was still handed the notice, and the interface still
            // gets the localized text to display in-app even though nothing
            // reached the operating system.
            assert_eq!(fake.delivered().len(), 1);
            assert!(!preview.title.as_str().is_empty());
            assert!(!preview.body.as_str().is_empty());
        }
    }

    #[test]
    fn a_channel_that_refuses_at_delivery_time_reports_why() {
        // Detection said "available" but the platform refused the send: the
        // answer the renderer receives is the refusal, not a false success.
        let fake = Arc::new(FakeNotifications::new(
            NotificationCapabilityDto::Available,
            DeliveryOutcome::Refused(NotificationCapabilityDto::Unsupported),
        ));
        let preview = state(fake, TrayAvailabilityDto::Available)
            .preview(NotificationTemplateKey::SyntheticProbe, Locale::En)
            .expect("preview survives a refused delivery");
        assert!(!preview.delivered);
        assert_eq!(preview.capability, NotificationCapabilityDto::Unsupported);
    }

    #[test]
    fn capabilities_report_the_tray_as_detected_not_assumed() {
        let port: Arc<dyn NotificationPort> = Arc::new(UnsupportedNotifications);
        assert_eq!(
            state(port.clone(), TrayAvailabilityDto::Available)
                .capabilities()
                .tray,
            TrayAvailabilityDto::Available
        );
        assert_eq!(
            state(port, TrayAvailabilityDto::Unavailable)
                .capabilities()
                .tray,
            TrayAvailabilityDto::Unavailable
        );
    }

    #[test]
    fn a_failed_tray_registration_never_claims_a_usable_tray() {
        // This is the branch that decides whether closing the main window may
        // hide instead of exit: a registration failure always reports
        // unavailable, on every platform, so the close policy always takes the
        // branch that cannot leave an orphan collector behind.
        assert_eq!(
            detect_tray(false),
            TrayAvailabilityDto::Unavailable,
            "a failed registration is never a usable tray"
        );
        // Registration is only proof where the tray surface belongs to the
        // operating system shell. Linux libappindicator registration succeeds
        // without a host, so it is deliberately not treated as proof here.
        let expected = if cfg!(target_os = "linux") {
            TrayAvailabilityDto::Unavailable
        } else {
            TrayAvailabilityDto::Available
        };
        assert_eq!(detect_tray(true), expected);
    }

    #[test]
    fn the_tray_override_only_accepts_an_exact_off() {
        // The override is one-way: it can only remove a capability, never add
        // one, so it can never manufacture a "the tray works" result. Only the
        // exact value disables it, so a typo cannot silently change what closing
        // the main window does.
        assert!(tray_disabled(Some("off")));
        assert!(!tray_disabled(Some("on")));
        assert!(!tray_disabled(Some("OFF")));
        assert!(!tray_disabled(Some("")));
        assert!(!tray_disabled(None));
        // A forced-off run registers no icon at all, so `detect_tray(false)` is
        // the only answer it can reach, on every platform.
        assert_eq!(detect_tray(false), TrayAvailabilityDto::Unavailable);
    }
}
