use std::collections::HashMap;

use lsp_types::Uri;

/// Tracks the content of open documents.
pub struct DocumentStore {
    docs: HashMap<Uri, String>,
}

impl DocumentStore {
    pub fn new() -> Self {
        Self {
            docs: HashMap::new(),
        }
    }

    pub fn open(&mut self, uri: Uri, text: String) {
        self.docs.insert(uri, text);
    }

    pub fn change(&mut self, uri: &Uri, text: String) {
        self.docs.insert(uri.clone(), text);
    }

    pub fn close(&mut self, uri: &Uri) {
        self.docs.remove(uri);
    }

    pub fn get(&self, uri: &Uri) -> Option<&str> {
        self.docs.get(uri).map(String::as_str)
    }
}
