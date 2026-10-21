//! Cheap change detection for caches derived from a [`BimDocument`].

use sketchcad_bim::BimDocument;
use sketchcad_core::ElementId;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

/// Tracks the document revision plus content hashes of elements edited in place through
/// `get_*_mut` (which do not bump the revision).
#[derive(Debug, Default)]
pub struct DocWatcher {
    revision: Option<u64>,
    edited_hashes: HashMap<ElementId, u64>,
}

impl DocWatcher {
    /// Returns true when the document may have changed since the previous call.
    pub fn poll(&mut self, doc: &BimDocument) -> bool {
        let mut changed = self.revision != Some(doc.revision());
        if changed {
            self.revision = Some(doc.revision());
            self.edited_hashes.clear();
        }
        for id in doc.edited_elements() {
            let hash = element_hash(doc, id);
            if self.edited_hashes.insert(id, hash) != Some(hash) {
                changed = true;
            }
        }
        changed
    }

    /// Forces the next [`Self::poll`] to report a change.
    pub fn invalidate(&mut self) {
        self.revision = None;
    }
}

fn element_hash(doc: &BimDocument, id: ElementId) -> u64 {
    struct HashWriter<'a>(&'a mut std::collections::hash_map::DefaultHasher);
    impl std::fmt::Write for HashWriter<'_> {
        fn write_str(&mut self, s: &str) -> std::fmt::Result {
            self.0.write(s.as_bytes());
            Ok(())
        }
    }

    let mut h = std::collections::hash_map::DefaultHasher::new();
    doc.is_visible(id).hash(&mut h);
    if let Some(elem) = doc.get_element(id) {
        let _ = std::fmt::Write::write_fmt(&mut HashWriter(&mut h), format_args!("{:?}", elem));
    }
    h.finish()
}
