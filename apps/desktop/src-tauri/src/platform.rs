//! Platform capability adapters for desktop integration.
//!
//! Every answer here is a *detection*, never an assumption about a desktop
//! environment: the composition asks the running platform what it offers and
//! reports that as a capability. A host without a tray, or without a usable
//! notification channel, is a normal state rather than a failure, and the
//! renderer receives an explicit answer it can explain to the user.

use std::sync::Arc;

use quotavista_contracts::{
    ErrorCode, Locale, NotificationCapabilityDto, NotificationDeliveryDto, NotificationPreviewDto,
    NotificationTemplateKey, NotificationTextDto, PlatformCapabilitiesDto, SafeError,
    TrayAvailabilityDto,
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
    /// The platform confirmed delivery.
    ///
    /// Only an adapter that can read the operating system's answer may report
    /// this. The pinned desktop plugin cannot, so the real adapter never does.
    Delivered,
    /// The adapter handed the notice over and the platform reported no error,
    /// but the operating-system result was never observed. This is all
    /// `tauri-plugin-notification`'s desktop `show()` can say: it spawns the
    /// delivery and returns `Ok` regardless of what happens next.
    Queued,
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
    /// A refused delivery is a reported capability rather than an error
    /// envelope, and a queued one is reported as queued - never as confirmed.
    pub fn preview(
        &self,
        template_key: NotificationTemplateKey,
        locale: Locale,
    ) -> Result<NotificationPreviewDto, SafeError> {
        let notice = render_notice(template_key, locale);
        let (delivery, capability) = match self.notifications.deliver(&notice) {
            // Only a confirmed delivery may upgrade the capability answer: an
            // adapter that read the operating system's result has proven the
            // channel works. A queued one has proven nothing new, so it keeps
            // whatever was detected.
            DeliveryOutcome::Delivered => (
                NotificationDeliveryDto::Delivered,
                NotificationCapabilityDto::Available,
            ),
            DeliveryOutcome::Queued => (
                NotificationDeliveryDto::Queued,
                self.notifications.capability(),
            ),
            DeliveryOutcome::Refused(reason) => (NotificationDeliveryDto::Refused, reason),
        };
        Ok(NotificationPreviewDto {
            template_key,
            locale,
            capability,
            delivery,
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

/// Map the notification plugin's registration and permission answers onto the
/// contract.
///
/// Split out of [`TauriNotifications::detect`] so the mapping itself is covered
/// by a test instead of only being reachable through a window server. An
/// unregistered plugin has no channel to address; a registered one is
/// *unconfirmed* rather than available, because the plugin's desktop
/// `permission_state` always answers granted without asking the operating system
/// anything, so it cannot distinguish a host with a notification service from a
/// host without one.
pub fn capability_from_plugin(
    registered: bool,
    permission_granted: bool,
) -> NotificationCapabilityDto {
    match (registered, permission_granted) {
        (false, _) => NotificationCapabilityDto::Unsupported,
        (_, false) => NotificationCapabilityDto::PermissionDenied,
        (true, true) => NotificationCapabilityDto::Unconfirmed,
    }
}

/// Map the plugin's `show` result onto the contract.
///
/// `Ok` means "the plugin accepted the notice and spawned its delivery", never
/// "the operating system displayed it": on desktop the plugin discards the OS
/// response and answers `Ok` either way. Only a synchronous failure - the plugin
/// cannot even build the request - is a refusal.
pub fn delivery_from_plugin(shown: bool) -> DeliveryOutcome {
    if shown {
        DeliveryOutcome::Queued
    } else {
        DeliveryOutcome::Refused(NotificationCapabilityDto::Unsupported)
    }
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
    /// `src/desktop.rs`): on desktop `permission_state` unconditionally resolves
    /// to `Granted` because desktop applications do not request that permission,
    /// and `show` hands the notification to a background task whose
    /// operating-system response is never read back. Neither call can therefore
    /// prove that this host has a notification service at all.
    ///
    /// A registered plugin is consequently reported as `Unconfirmed`: the
    /// channel exists inside this process, and whether the operating system will
    /// accept anything through it is not something this adapter can know. Only
    /// an absent plugin, or a permission answer that is not granted, counts as a
    /// refusal. `Available` is reserved for an adapter that can read the
    /// platform's own answer - the mobile target, where the plugin does real
    /// permission handling - and for a native probe validated by a run on the
    /// target, which is P1-13 work.
    pub fn detect(handle: tauri::AppHandle) -> Self {
        use tauri::Manager;
        use tauri_plugin_notification::{Notification, PermissionState};

        let registered = handle.try_state::<Notification<tauri::Wry>>().is_some();
        // `notification()` panics when the plugin is missing, so it is only
        // reached once registration is known.
        let granted = registered && {
            use tauri_plugin_notification::NotificationExt;
            matches!(
                handle.notification().permission_state(),
                Ok(PermissionState::Granted)
            )
        };
        let detected = capability_from_plugin(registered, granted);
        Self { handle, detected }
    }
}

impl NotificationPort for TauriNotifications {
    fn capability(&self) -> NotificationCapabilityDto {
        self.detected
    }

    fn deliver(&self, notice: &SyntheticNotice) -> DeliveryOutcome {
        use tauri_plugin_notification::NotificationExt;

        // A channel that detection already refused is never addressed, so a
        // missing notification service cannot be reported as a success.
        if matches!(
            self.detected,
            NotificationCapabilityDto::Unsupported | NotificationCapabilityDto::PermissionDenied
        ) {
            return DeliveryOutcome::Refused(self.detected);
        }
        let prepared = self
            .handle
            .notification()
            .builder()
            .title(notice.title.clone())
            .body(notice.body.clone());
        // Delegated so the mapping is covered by a test that sends nothing to
        // the operating system.
        delivery_from_plugin(prepared.show().is_ok())
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
    fn a_channel_that_confirms_delivery_reports_delivered_and_available() {
        let fake = Arc::new(FakeNotifications::new(
            NotificationCapabilityDto::Available,
            DeliveryOutcome::Delivered,
        ));
        let preview = state(fake.clone(), TrayAvailabilityDto::Available)
            .preview(NotificationTemplateKey::SyntheticProbe, Locale::ZhCn)
            .expect("preview is built from the fixed catalog");

        assert_eq!(preview.delivery, NotificationDeliveryDto::Delivered);
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
            assert_eq!(preview.delivery, NotificationDeliveryDto::Refused);
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
        assert_eq!(preview.delivery, NotificationDeliveryDto::Refused);
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
    #[test]
    fn a_plugin_that_answers_granted_is_reported_as_unconfirmed() {
        // The exact answers the pinned desktop plugin gives: it is registered,
        // and `permission_state` says granted without ever consulting the
        // operating system. That is not evidence a notification service exists,
        // so the capability must not read as available.
        assert_eq!(
            capability_from_plugin(true, true),
            NotificationCapabilityDto::Unconfirmed
        );
        // No plugin means there is no channel to address at all.
        assert_eq!(
            capability_from_plugin(false, false),
            NotificationCapabilityDto::Unsupported
        );
        assert_eq!(
            capability_from_plugin(false, true),
            NotificationCapabilityDto::Unsupported,
            "an unregistered plugin cannot have been granted anything"
        );
        // A permission model that really says no is a refusal, not an unknown.
        assert_eq!(
            capability_from_plugin(true, false),
            NotificationCapabilityDto::PermissionDenied
        );
    }

    #[test]
    fn plugin_success_is_queued_because_the_os_result_is_never_read() {
        // `show()` answering Ok means the plugin spawned the delivery; nothing
        // observed the operating system, so it may not be reported as delivered.
        assert_eq!(delivery_from_plugin(true), DeliveryOutcome::Queued);
        assert_eq!(
            delivery_from_plugin(false),
            DeliveryOutcome::Refused(NotificationCapabilityDto::Unsupported)
        );
    }

    #[test]
    fn a_queued_notification_stays_unconfirmed_and_keeps_its_text() {
        // The case a fake refusal cannot model: the plugin reports success while
        // the operating system confirms nothing. The renderer has to receive
        // "queued" with the detected capability unchanged, so it can say the
        // delivery is unconfirmed instead of implying a banner appeared.
        let fake = Arc::new(FakeNotifications::new(
            NotificationCapabilityDto::Unconfirmed,
            DeliveryOutcome::Queued,
        ));
        let platform = state(fake.clone(), TrayAvailabilityDto::Available);
        assert_eq!(
            platform.capabilities().notification,
            NotificationCapabilityDto::Unconfirmed
        );

        let preview = platform
            .preview(NotificationTemplateKey::SyntheticProbe, Locale::ZhCn)
            .expect("the catalog is inside the scalar's bounds");
        assert_eq!(preview.delivery, NotificationDeliveryDto::Queued);
        assert_eq!(
            preview.capability,
            NotificationCapabilityDto::Unconfirmed,
            "a queued delivery proves nothing, so it must not upgrade the capability"
        );
        // The port was addressed once, and the interface still gets the
        // localized text to display in-app.
        let seen = fake.delivered();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].locale, Locale::ZhCn);
        assert!(!preview.title.as_str().is_empty());
        assert!(!preview.body.as_str().is_empty());
    }

    #[test]
    fn only_a_confirmed_delivery_reports_delivered() {
        // The three outcomes stay distinguishable end to end, so no adapter can
        // collapse "queued" or "refused" into a success.
        for (outcome, expected) in [
            (
                DeliveryOutcome::Delivered,
                NotificationDeliveryDto::Delivered,
            ),
            (DeliveryOutcome::Queued, NotificationDeliveryDto::Queued),
            (
                DeliveryOutcome::Refused(NotificationCapabilityDto::PermissionDenied),
                NotificationDeliveryDto::Refused,
            ),
        ] {
            let fake = Arc::new(FakeNotifications::new(
                NotificationCapabilityDto::Unconfirmed,
                outcome,
            ));
            let preview = state(fake, TrayAvailabilityDto::Unavailable)
                .preview(NotificationTemplateKey::SyntheticProbe, Locale::En)
                .expect("preview survives every outcome");
            assert_eq!(preview.delivery, expected);
        }
    }
}
