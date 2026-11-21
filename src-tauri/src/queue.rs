use std::collections::VecDeque;
use tauri::Url;

#[derive(Default)]
pub struct PendingLinks {
    next_id: u64,
    links: VecDeque<Link>,
}

#[derive(Clone)]
pub struct Link {
    pub id: u64,
    pub url: Url,
}

impl PendingLinks {
    pub fn enqueue(&mut self, urls: impl IntoIterator<Item = Url>) -> bool {
        let previous_len = self.links.len();
        for url in urls {
            if supported(&url) {
                self.next_id += 1;
                self.links.push_back(Link {
                    id: self.next_id,
                    url,
                });
            }
        }
        self.links.len() != previous_len
    }

    pub fn current(&self) -> Option<&Link> {
        self.links.front()
    }

    pub fn current_with_id(&self, id: u64) -> Option<&Link> {
        self.current().filter(|link| link.id == id)
    }

    pub fn finish(&mut self, id: u64) -> bool {
        if self.current_with_id(id).is_none() {
            return false;
        }
        self.links.pop_front();
        true
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
    fn queues_web_links_in_order_and_ignores_other_schemes() {
        let mut pending = PendingLinks::default();
        assert!(!pending.enqueue([url("file:///tmp/local.txt"), url("mailto:test@example.com")]));
        assert!(pending.enqueue([
            url("https://example.com/first"),
            url("http://example.com/second")
        ]));
        assert!(pending.enqueue([url("https://example.com/third")]));

        for expected in [
            "https://example.com/first",
            "http://example.com/second",
            "https://example.com/third",
        ] {
            let current = pending.current().unwrap();
            assert_eq!(current.url.as_str(), expected);
            assert!(pending.finish(current.id));
        }
        assert!(pending.current().is_none());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn accepts_only_local_html_documents() {
        let mut pending = PendingLinks::default();
        assert!(!pending.enqueue([
            url("file://server/share/index.html"),
            url("file:///tmp/image.png"),
        ]));
        for address in ["file:///tmp/Test%20Page.HTML", "file:///tmp/test.xhtml"] {
            assert!(pending.enqueue([url(address)]));
            let current = pending.current().unwrap();
            assert_eq!(current.url.as_str(), address);
            assert!(pending.finish(current.id));
        }
    }

    #[test]
    fn ignores_stale_actions_and_preserves_repeated_links() {
        let mut pending = PendingLinks::default();
        let link = url("https://example.com/");
        pending.enqueue([link.clone(), link.clone()]);

        let first_id = pending.current().unwrap().id;
        assert!(!pending.finish(first_id + 1));
        assert!(pending.finish(first_id));
        assert!(!pending.finish(first_id));
        assert_eq!(pending.current().unwrap().url, link);
        assert_ne!(pending.current().unwrap().id, first_id);
    }
}
