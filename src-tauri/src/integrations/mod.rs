//! Windows integrations: selection capture, Explorer context menu,
//! browser native messaging, and the authenticated local bridge used by
//! the browser and VS Code extensions. Every integration funnels into the
//! same `CapturedContext` inbox shown in the overlay.

pub mod bridge;
#[cfg(windows)]
pub mod clipboard;
pub mod context;
pub mod explorer;
pub mod mcp_stdio;
pub mod native_host;
pub mod selection;

use context::CapturedContext;
use std::sync::Mutex;

/// Pending captures waiting for the user. The UI pulls these when the
/// overlay/toolbar opens, so nothing is lost if an event fires before the
/// webview is listening.
#[derive(Default)]
pub struct Inbox {
    items: Mutex<Vec<CapturedContext>>,
}

impl Inbox {
    pub fn push(&self, c: CapturedContext) {
        let mut v = self.items.lock().unwrap();
        v.retain(|x| x.id != c.id);
        v.push(c);
        // keep the inbox bounded
        let n = v.len();
        if n > 20 {
            v.drain(..n - 20);
        }
    }

    pub fn list(&self) -> Vec<CapturedContext> {
        self.items.lock().unwrap().clone()
    }

    pub fn take(&self, ids: &[String]) -> Vec<CapturedContext> {
        let mut v = self.items.lock().unwrap();
        let (taken, keep): (Vec<_>, Vec<_>) = v.drain(..).partition(|c| ids.contains(&c.id));
        *v = keep;
        taken
    }

    pub fn remove(&self, id: &str) {
        self.items.lock().unwrap().retain(|c| c.id != id);
    }

    pub fn clear(&self) {
        self.items.lock().unwrap().clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inbox_take_and_bound() {
        let inbox = Inbox::default();
        for i in 0..25 {
            let mut c = CapturedContext::new("browser", "text", format!("{i}"));
            c.id = i.to_string();
            inbox.push(c);
        }
        assert_eq!(inbox.list().len(), 20);
        let t = inbox.take(&["24".into(), "nope".into()]);
        assert_eq!(t.len(), 1);
        assert_eq!(inbox.list().len(), 19);
    }
}
