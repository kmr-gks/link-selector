use crate::picker_size::Dimensions;
use std::collections::{HashMap, VecDeque};
use tauri::Url;

pub const MAIN_WINDOW: &str = "main";

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
    pub presented: Option<Dimensions>,
    pub slot: usize,
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
                    presented: None,
                    slot,
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

    pub fn set_presented(&mut self, window: &str, id: u64, size: Dimensions) {
        if let Some(link) = self.by_window.get_mut(window).filter(|link| link.id == id) {
            link.presented = Some(size);
        }
    }

    pub fn record_resize(&mut self, window: &str, size: Dimensions) -> bool {
        let Some(last) = self
            .by_window
            .get_mut(window)
            .and_then(|link| link.presented.as_mut())
        else {
            return false;
        };
        if !size.is_valid() || !size.differs_from(*last) {
            return false;
        }
        *last = size;
        true
    }

    pub fn finish(&mut self, window: &str, id: u64) -> Option<Link> {
        self.get_with_id(window, id)?;
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
        link.presented = None;
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
