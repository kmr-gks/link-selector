use crate::picker_size::Dimensions;
use std::collections::HashMap;
use tauri::Url;

pub const MAIN_WINDOW: &str = "main";

#[derive(Default)]
pub struct PendingLinks {
    next_id: u64,
    by_window: HashMap<String, Link>,
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

    pub fn finish(&mut self, window: &str, id: u64) -> Option<Link> {
        self.get_with_id(window, id)?;
        self.by_window.remove(window)
    }

    pub fn remove(&mut self, window: &str) -> Option<Link> {
        self.by_window.remove(window)
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
                    ["htm", "html", "shtml", "xht", "xhtml"]
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
}
