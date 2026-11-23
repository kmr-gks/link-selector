use crate::picker_size::Dimensions;
use std::{
    collections::{HashMap, VecDeque},
    io,
    sync::Mutex,
};
use tauri::Url;

pub const MAIN_WINDOW: &str = "main";

pub struct StartupLinks(Option<Vec<Url>>);

impl Default for StartupLinks {
    fn default() -> Self {
        Self(Some(Vec::new()))
    }
}

impl StartupLinks {
    pub fn route(&mut self, urls: Vec<Url>) -> Option<Vec<Url>> {
        if let Some(queued) = &mut self.0 {
            queued.extend(urls);
            None
        } else {
            Some(urls)
        }
    }

    pub fn ready(&mut self, initial: impl IntoIterator<Item = Url>) -> Vec<Url> {
        initial
            .into_iter()
            .chain(self.0.take().unwrap_or_default())
            .collect()
    }
}

#[derive(Default)]
pub struct PendingLinks {
    next_id: u64,
    by_window: HashMap<String, Link>,
    deferred: VecDeque<Link>,
}

#[derive(Clone)]
pub struct Link {
    pub id: u64,
    pub url: Url,
    resize: ResizeTracking,
    fit_padding: Option<f64>,
    pub slot: usize,
    launch: LaunchState,
}

#[derive(Clone, Copy)]
enum ResizeTracking {
    NotShown,
    Loading(Dimensions),
    AwaitingNormalSize,
    Presented(Dimensions),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LaunchState {
    Idle,
    Submitting { dismiss: bool },
    Submitted,
    Dismissing,
}

impl PendingLinks {
    pub fn add(&mut self, urls: impl IntoIterator<Item = Url>) -> Vec<String> {
        let mut windows = Vec::new();
        for url in urls.into_iter().filter(supported) {
            self.next_id += 1;
            let id = self.next_id;
            let slot = (0..)
                .find(|slot| self.by_window.values().all(|link| link.slot != *slot))
                .expect("a finite collection always has an unused slot");
            let window = if slot == 0 {
                MAIN_WINDOW.to_owned()
            } else {
                format!("picker-{id}")
            };
            self.by_window.insert(
                window.clone(),
                Link {
                    id,
                    url,
                    resize: ResizeTracking::NotShown,
                    fit_padding: None,
                    slot,
                    launch: LaunchState::Idle,
                },
            );
            windows.push(window);
        }
        windows
    }

    pub fn get(&self, window: &str) -> Option<&Link> {
        self.by_window.get(window)
    }

    pub fn get_with_id(&self, window: &str, id: u64) -> Option<&Link> {
        self.get(window).filter(|link| link.id == id)
    }

    pub fn can_fit(&self, window: &str, id: Option<u64>, grow_only: bool) -> bool {
        match (self.get(window), id) {
            (Some(link), Some(id)) => {
                link.id == id && (grow_only || !matches!(link.resize, ResizeTracking::Presented(_)))
            }
            (None, None) => true,
            _ => false,
        }
    }

    pub fn track_loading_size(&mut self, window: &str, id: u64, size: Dimensions) {
        if let Some(link) = self.by_window.get_mut(window).filter(|link| {
            link.id == id && matches!(link.resize, ResizeTracking::NotShown) && size.is_valid()
        }) {
            link.resize = ResizeTracking::Loading(size);
        }
    }

    pub fn record_scale_change(&mut self, window: &str, size: Dimensions) {
        if let Some(link) = self.by_window.get_mut(window).filter(|_| size.is_valid()) {
            match &mut link.resize {
                ResizeTracking::Loading(last) | ResizeTracking::Presented(last) => *last = size,
                _ => {}
            }
        }
    }

    pub fn set_presented(&mut self, window: &str, id: u64, size: Dimensions) {
        if let Some(link) = self.by_window.get_mut(window).filter(|link| link.id == id) {
            link.resize = ResizeTracking::Presented(size);
        }
    }

    pub fn fit_padding(&mut self, window: &str, id: u64, measured: Option<f64>) -> f64 {
        let Some(link) = self.by_window.get_mut(window).filter(|link| link.id == id) else {
            return 0.0;
        };
        // Later layout messages can still describe the viewport before a resize.
        link.fit_padding = link.fit_padding.or(measured);
        link.fit_padding.unwrap_or_default()
    }

    pub fn track_restored_size(&mut self, window: &str, id: u64) {
        if let Some(link) = self
            .by_window
            .get_mut(window)
            .filter(|link| link.id == id && !matches!(link.resize, ResizeTracking::Presented(_)))
        {
            link.resize = ResizeTracking::AwaitingNormalSize;
        }
    }

    pub fn record_resize(&mut self, window: &str, size: Dimensions) -> bool {
        let Some(link) = self.by_window.get_mut(window).filter(|_| size.is_valid()) else {
            return false;
        };
        match link.resize {
            ResizeTracking::NotShown => false,
            ResizeTracking::AwaitingNormalSize => {
                link.resize = ResizeTracking::Presented(size);
                false
            }
            ResizeTracking::Loading(last) | ResizeTracking::Presented(last) => {
                if !size.differs_from(last) {
                    return false;
                }
                link.resize = ResizeTracking::Presented(size);
                true
            }
        }
    }

    pub fn finish(&mut self, window: &str, id: u64) -> Option<Link> {
        let link = self
            .by_window
            .get_mut(window)
            .filter(|link| link.id == id)?;
        if let LaunchState::Submitting { dismiss } = &mut link.launch {
            *dismiss = true;
            return None;
        }
        self.by_window.remove(window)
    }

    pub fn remove(&mut self, window: &str) -> Option<Link> {
        self.by_window.remove(window)
    }

    pub fn defer(&mut self, window: &str) -> bool {
        let Some(link) = self.remove(window) else {
            return false;
        };
        self.deferred.push_back(link);
        self.show_next_deferred()
    }

    pub fn show_next_deferred(&mut self) -> bool {
        if self.by_window.contains_key(MAIN_WINDOW) {
            return false;
        }
        let Some(mut link) = self.deferred.pop_front() else {
            return false;
        };
        link.slot = 0;
        link.resize = ResizeTracking::NotShown;
        link.fit_padding = None;
        self.by_window.insert(MAIN_WINDOW.to_owned(), link);
        true
    }

    pub fn deferred_count(&self) -> usize {
        self.deferred.len()
    }

    pub fn windows(&self) -> impl Iterator<Item = (&String, &Link)> {
        self.by_window.iter()
    }
}

#[derive(Debug)]
pub enum LaunchError {
    DismissedOrAlreadySubmitted,
    Platform(io::Error),
}

impl From<io::Error> for LaunchError {
    fn from(error: io::Error) -> Self {
        Self::Platform(error)
    }
}

pub struct LaunchRequest<'a> {
    pending: &'a Mutex<PendingLinks>,
    window: &'a str,
    id: u64,
    submitted: bool,
    dismiss: &'a dyn Fn(),
}

impl<'a> LaunchRequest<'a> {
    pub fn new(
        pending: &'a Mutex<PendingLinks>,
        window: &'a str,
        id: u64,
        dismiss: &'a dyn Fn(),
    ) -> Self {
        Self {
            pending,
            window,
            id,
            submitted: false,
            dismiss,
        }
    }

    pub fn submit<T>(&mut self, submit: impl FnOnce() -> io::Result<T>) -> Result<T, LaunchError> {
        {
            let mut pending = self.pending.lock().unwrap();
            let Some(link) = pending
                .by_window
                .get_mut(self.window)
                .filter(|link| link.id == self.id && link.launch == LaunchState::Idle)
            else {
                return Err(LaunchError::DismissedOrAlreadySubmitted);
            };
            // Defer dismissal only for this picker until the synchronous handoff returns.
            link.launch = LaunchState::Submitting { dismiss: false };
            self.submitted = true;
        }

        let result = submit().map_err(LaunchError::from);
        let next = if result.is_ok() {
            LaunchState::Submitted
        } else {
            LaunchState::Idle
        };
        let dismiss = self.set_launch_state(next);
        self.submitted = result.is_ok() && !dismiss;
        if dismiss {
            (self.dismiss)();
        }
        result
    }

    fn set_launch_state(&self, next: LaunchState) -> bool {
        let mut pending = self.pending.lock().unwrap();
        let Some(link) = pending
            .by_window
            .get_mut(self.window)
            .filter(|link| link.id == self.id)
        else {
            return false;
        };
        let dismiss = link.launch == (LaunchState::Submitting { dismiss: true });
        link.launch = if dismiss {
            LaunchState::Dismissing
        } else {
            next
        };
        dismiss
    }

    pub fn complete(mut self) {
        self.submitted = false;
    }
}

impl Drop for LaunchRequest<'_> {
    fn drop(&mut self) {
        if self.submitted && self.set_launch_state(LaunchState::Idle) {
            (self.dismiss)();
        }
    }
}

fn supported(url: &Url) -> bool {
    match url.scheme() {
        "http" | "https" => url.has_host(),
        #[cfg(target_os = "macos")]
        "file" => url.to_file_path().ok().is_some_and(|path| {
            path.extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| {
                    ["htm", "html", "shtm", "shtml", "xht", "xhtm", "xhtml"]
                        .iter()
                        .any(|supported| extension.eq_ignore_ascii_case(supported))
                })
        }),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(value: &str) -> Url {
        Url::parse(value).unwrap()
    }

    #[test]
    fn dismissal_during_preparation_prevents_submission() {
        let address = url("https://example.com/");
        let pending = Mutex::new(PendingLinks::default());
        pending.lock().unwrap().add([address.clone()]);
        let mut launch = LaunchRequest::new(&pending, MAIN_WINDOW, 1, &|| {});

        pending.lock().unwrap().finish(MAIN_WINDOW, 1).unwrap();
        pending.lock().unwrap().add([address]);
        assert!(matches!(
            launch
                .submit(|| -> io::Result<()> { panic!("the dismissed URL must not be submitted") }),
            Err(LaunchError::DismissedOrAlreadySubmitted)
        ));
        assert_eq!(pending.lock().unwrap().get(MAIN_WINDOW).unwrap().id, 2);
    }

    #[test]
    fn dismissing_during_handoff_waits_only_for_that_picker() {
        let address = url("https://example.com/");
        let pending = Mutex::new(PendingLinks::default());
        pending.lock().unwrap().add([address.clone(), address]);
        let dismissed = std::cell::Cell::new(false);
        let dismiss = || {
            assert!(pending.lock().unwrap().finish(MAIN_WINDOW, 1).is_some());
            dismissed.set(true);
        };
        let mut request = LaunchRequest::new(&pending, MAIN_WINDOW, 1, &dismiss);
        request
            .submit(|| {
                let mut links = pending.try_lock().unwrap();
                assert!(links.finish(MAIN_WINDOW, 1).is_none());
                assert!(links.finish(MAIN_WINDOW, 1).is_none());
                assert!(links.get(MAIN_WINDOW).is_some());
                assert!(links.finish("picker-2", 2).is_some());
                assert!(!dismissed.get());
                Ok(())
            })
            .unwrap();
        assert!(dismissed.get());
        assert!(pending.lock().unwrap().get(MAIN_WINDOW).is_none());
    }

    #[test]
    fn failed_handoff_cannot_retry_while_dismissal_is_queued() {
        let pending = Mutex::new(PendingLinks::default());
        pending.lock().unwrap().add([url("https://example.com/")]);
        let dismissals = std::cell::Cell::new(0);
        let dismiss = || dismissals.set(dismissals.get() + 1);
        let mut request = LaunchRequest::new(&pending, MAIN_WINDOW, 1, &dismiss);
        assert!(matches!(
            request.submit::<()>(|| {
                assert!(pending.lock().unwrap().finish(MAIN_WINDOW, 1).is_none());
                Err(io::Error::other("dispatch failed"))
            }),
            Err(LaunchError::Platform(_))
        ));
        drop(request);
        assert_eq!(dismissals.get(), 1);
        assert!(matches!(
            LaunchRequest::new(&pending, MAIN_WINDOW, 1, &dismiss).submit(|| Ok(())),
            Err(LaunchError::DismissedOrAlreadySubmitted)
        ));
        assert!(pending.lock().unwrap().finish(MAIN_WINDOW, 1).is_some());
    }

    #[test]
    fn os_submission_and_response_leave_other_pickers_available() {
        let address = url("https://example.com/");
        let pending = Mutex::new(PendingLinks::default());
        pending
            .lock()
            .unwrap()
            .add([address.clone(), address.clone()]);
        let mut first = LaunchRequest::new(&pending, MAIN_WINDOW, 1, &|| {});
        first
            .submit(|| {
                let mut links = pending
                    .try_lock()
                    .expect("OS handoff must not lock the queue");
                assert!(links.finish("picker-2", 2).is_some());
                assert_eq!(links.add([address.clone()]), ["picker-3"]);
                Ok(())
            })
            .unwrap();
        assert!(matches!(
            LaunchRequest::new(&pending, MAIN_WINDOW, 1, &|| {}).submit(|| Ok(())),
            Err(LaunchError::DismissedOrAlreadySubmitted)
        ));

        pending.try_lock().unwrap().finish(MAIN_WINDOW, 1).unwrap();
        pending.lock().unwrap().add([address]);
        let mut second = LaunchRequest::new(&pending, MAIN_WINDOW, 4, &|| {});
        second.submit(|| Ok(())).unwrap();
        drop(first);
        assert!(matches!(
            LaunchRequest::new(&pending, MAIN_WINDOW, 4, &|| {}).submit(|| Ok(())),
            Err(LaunchError::DismissedOrAlreadySubmitted)
        ));

        drop(second);
        let mut retry = LaunchRequest::new(&pending, MAIN_WINDOW, 4, &|| {});
        assert!(retry
            .submit::<()>(|| Err(io::Error::other("failed submission")))
            .is_err());
        drop(retry);
        let mut retry = LaunchRequest::new(&pending, MAIN_WINDOW, 4, &|| {});
        retry.submit(|| Ok(())).unwrap();
        retry.complete();
        assert!(matches!(
            LaunchRequest::new(&pending, MAIN_WINDOW, 4, &|| {}).submit(|| Ok(())),
            Err(LaunchError::DismissedOrAlreadySubmitted)
        ));
    }

    #[test]
    fn delivers_all_cold_start_documents_only_after_setup() {
        let mut startup = StartupLinks::default();
        let first = url("file:///tmp/first.html");
        let second = url("file:///tmp/second.html");
        let third = url("https://example.com/next");
        assert_eq!(startup.route(vec![first.clone()]), None);
        assert_eq!(startup.route(vec![second.clone()]), None);
        assert_eq!(startup.ready([]), vec![first, second]);
        assert!(startup.ready([]).is_empty());
        assert_eq!(startup.route(vec![third.clone()]), Some(vec![third]));
    }

    #[test]
    fn opens_initial_windows_arguments_before_links_forwarded_during_startup() {
        let mut startup = StartupLinks::default();
        let mut pending = PendingLinks::default();

        for address in ["https://example.com/forwarded", "http://example.com/next"] {
            let urls = startup.route(vec![url(address)]);
            assert!(urls.is_none());
        }
        assert!(pending.get(MAIN_WINDOW).is_none());

        let ready = startup.ready([url("https://example.com/initial")]);
        let windows = pending.add(ready);
        assert_eq!(windows, ["main", "picker-2", "picker-3"]);
        let paths: Vec<_> = windows
            .iter()
            .map(|window| pending.get(window).unwrap().url.path())
            .collect();
        assert_eq!(paths, ["/initial", "/forwarded", "/next"]);
        assert!(startup.ready([]).is_empty());

        let running = startup
            .route(vec![url("https://example.com/running")])
            .unwrap();
        assert_eq!(pending.add(running), ["picker-4"]);
    }

    #[test]
    fn links_get_independent_windows_and_can_finish_out_of_order() {
        let mut links = PendingLinks::default();
        assert!(links
            .add([url("file:///tmp/local.txt"), url("mailto:test@example.com")])
            .is_empty());
        let windows = links.add([
            url("https://example.com/first"),
            url("http://example.com/second"),
        ]);
        assert_eq!(windows, ["main", "picker-2"]);
        assert_eq!(links.get(MAIN_WINDOW).unwrap().url.path(), "/first");
        assert_eq!(links.get("picker-2").unwrap().url.path(), "/second");
        assert!(links.finish(MAIN_WINDOW, 2).is_none());
        assert!(links.finish("picker-2", 2).is_some());
        assert!(links.get(MAIN_WINDOW).is_some());
        assert!(links.get("picker-2").is_none());
    }

    #[test]
    fn repeated_urls_stay_independent_and_reusing_main_rejects_old_actions() {
        let mut links = PendingLinks::default();
        let link = url("https://example.com/");
        links.add([link.clone(), link.clone()]);
        assert!(links.finish(MAIN_WINDOW, 1).is_some());
        assert_eq!(links.add([link.clone()]), [MAIN_WINDOW]);
        assert!(links.finish(MAIN_WINDOW, 1).is_none());
        assert_eq!(links.get_with_id(MAIN_WINDOW, 3).unwrap().url, link);
        assert_eq!(links.get_with_id("picker-2", 2).unwrap().url, link);
    }

    #[test]
    fn later_fit_requests_reuse_the_original_viewport_padding() {
        let mut links = PendingLinks::default();
        links.add([url("https://example.com/first")]);
        assert_eq!(links.fit_padding(MAIN_WINDOW, 1, Some(32.0)), 32.0);
        assert_eq!(links.fit_padding(MAIN_WINDOW, 1, Some(55.0)), 32.0);
        links.finish(MAIN_WINDOW, 1).unwrap();
        links.add([url("https://example.com/next")]);
        assert_eq!(links.fit_padding(MAIN_WINDOW, 2, Some(25.0)), 25.0);
    }

    #[test]
    fn tracks_manual_resize_after_a_link_is_first_shown_maximized() {
        let mut links = PendingLinks::default();
        let restored = Dimensions {
            width: 460.0,
            height: 465.0,
        };
        let manual = Dimensions {
            width: 530.0,
            height: 575.0,
        };
        links.add([url("https://example.com/first")]);
        links.track_restored_size(MAIN_WINDOW, 1);
        assert!(!links.record_resize(MAIN_WINDOW, restored));
        assert!(!links.can_fit(MAIN_WINDOW, Some(1), false));
        assert!(links.record_resize(MAIN_WINDOW, manual));
        assert!(!links.record_resize(MAIN_WINDOW, manual));
    }

    #[test]
    fn keeps_the_users_resize_while_the_first_browser_list_is_loading() {
        let mut links = PendingLinks::default();
        let start = Dimensions {
            width: 460.0,
            height: 465.0,
        };
        let chosen = Dimensions {
            width: 530.0,
            height: 575.0,
        };
        links.add([url("https://example.com/first")]);
        assert!(!links.record_resize(MAIN_WINDOW, start));
        links.track_loading_size(MAIN_WINDOW, 1, start);
        assert!(!links.record_resize(MAIN_WINDOW, start));
        assert!(links.can_fit(MAIN_WINDOW, Some(1), false));

        assert!(links.record_resize(MAIN_WINDOW, chosen));
        assert!(!links.can_fit(MAIN_WINDOW, Some(1), false));
        assert!(!links.record_resize(MAIN_WINDOW, chosen));
        assert!(!links.can_fit(MAIN_WINDOW, None, false));
        links.finish(MAIN_WINDOW, 1).unwrap();
        assert!(links.can_fit(MAIN_WINDOW, None, false));
    }

    #[test]
    fn a_monitor_size_change_while_loading_does_not_replace_the_preference() {
        let mut links = PendingLinks::default();
        let start = Dimensions {
            width: 460.0,
            height: 465.0,
        };
        let display_size = Dimensions {
            width: 455.0,
            height: 460.0,
        };
        links.add([url("https://example.com/first")]);
        links.track_loading_size(MAIN_WINDOW, 1, start);
        links.record_scale_change(MAIN_WINDOW, display_size);
        assert!(!links.record_resize(MAIN_WINDOW, display_size));
        assert!(links.can_fit(MAIN_WINDOW, Some(1), false));
    }

    #[test]
    fn records_resizes_in_event_order_before_either_window_closes() {
        let mut links = PendingLinks::default();
        let mut size = crate::picker_size::PickerSize::default();
        let default = Dimensions {
            width: 460.0,
            height: 465.0,
        };
        let first = Dimensions {
            width: 530.0,
            height: 550.0,
        };
        let second = Dimensions {
            width: 580.0,
            height: 600.0,
        };
        links.add([url("https://example.com/first")]);
        assert!(!links.record_resize(MAIN_WINDOW, default));
        links.set_presented(MAIN_WINDOW, 1, default);
        assert!(!links.record_resize(MAIN_WINDOW, default));
        assert!(links.record_resize(MAIN_WINDOW, first));
        size.remember(first);

        links.add([url("https://example.com/second")]);
        links.set_presented("picker-2", 2, size.preferred.unwrap());
        assert!(!links.record_resize("picker-2", first));
        assert!(links.record_resize("picker-2", second));
        size.remember(second);
        links.finish("picker-2", 2).unwrap();
        links.finish(MAIN_WINDOW, 1).unwrap();
        assert_eq!(size.preferred, Some(second));
    }

    #[test]
    fn failed_windows_keep_their_original_links_in_the_main_picker_queue() {
        let mut links = PendingLinks::default();
        let first = url("https://example.com/first");
        let second = url("https://example.com/second?q=a%20b");
        let third = url("https://example.com/third");
        links.add([first, second.clone(), third.clone()]);
        assert!(!links.defer("picker-2"));
        assert!(!links.defer("picker-3"));
        assert!(!links.defer("picker-3"));
        assert_eq!(links.deferred_count(), 2);
        links.finish(MAIN_WINDOW, 1).unwrap();
        assert!(links.show_next_deferred());
        assert_eq!(links.get_with_id(MAIN_WINDOW, 2).unwrap().url, second);
        links.finish(MAIN_WINDOW, 2).unwrap();
        assert!(links.show_next_deferred());
        assert_eq!(links.get_with_id(MAIN_WINDOW, 3).unwrap().url, third);
        assert_eq!(links.deferred_count(), 0);
    }

    #[test]
    fn immediately_recovers_a_failed_window_if_the_main_picker_is_free() {
        let mut links = PendingLinks::default();
        let address = url("https://example.com/");
        links.add([address.clone(), address.clone()]);
        links.finish(MAIN_WINDOW, 1).unwrap();
        assert!(links.defer("picker-2"));
        assert_eq!(links.get_with_id(MAIN_WINDOW, 2).unwrap().url, address);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn accepts_only_local_html_documents() {
        let mut links = PendingLinks::default();
        assert!(links
            .add([
                url("file://server/share/index.html"),
                url("file:///tmp/image.png"),
            ])
            .is_empty());
        let windows = links.add([
            url("file:///tmp/Test%20Page.HTML"),
            url("file:///tmp/test.xhtml"),
        ]);
        assert_eq!(windows.len(), 2);
        assert!(links
            .get(MAIN_WINDOW)
            .unwrap()
            .url
            .as_str()
            .ends_with("Test%20Page.HTML"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn accepts_short_shtml_and_xhtml_extensions_registered_by_macos() {
        let mut links = PendingLinks::default();
        let windows = links.add([
            url("file:///tmp/example.SHTM"),
            url("file:///tmp/example.xhtm"),
        ]);
        assert_eq!(windows.len(), 2);
    }
}
