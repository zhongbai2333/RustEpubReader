//! Committed progress notifications move the viewport, never rewrite it from a queue.
use crate::app::ReaderApp;

impl ReaderApp {
    pub(crate) fn poll_received_progress(&mut self) {
        if self.backup.busy() {
            return;
        }
        let updates: Vec<_> = self
            .pending_sync_updates
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .drain(..)
            .collect();
        for notice in updates {
            if !self.cloud_accept_progress(&notice.record) {
                continue;
            }
            if self.book_path.as_deref() != Some(notice.path.as_str()) {
                continue;
            }
            let p = &notice.record.progress;
            if self
                .book
                .as_ref()
                .is_none_or(|b| p.chapter >= b.chapters.len())
            {
                continue;
            }
            if (p.chapter, p.block, p.char_offset)
                != (
                    self.current_chapter,
                    self.current_block,
                    self.current_char_offset,
                )
            {
                self.navigate_reader(
                    crate::ui::reader_navigation::ReaderLocation {
                        chapter: p.chapter,
                        block: p.block,
                        char_offset: p.char_offset,
                    },
                    crate::ui::reader_navigation::NavigationKind::Remote,
                );
            }
        }
    }

    pub(crate) fn record_outgoing_progress(&mut self) {
        if self.backup.busy()
            || self
                .reader_navigation
                .pending_remote_from(self.reader_location())
            || (
                Some(self.current_chapter),
                Some(self.current_block),
                Some(self.current_char_offset),
            ) == (
                self.last_synced_chapter,
                self.last_synced_block,
                self.last_synced_char_offset,
            )
        {
            return;
        }
        if self.position_save_modified_ms == 0 {
            self.position_save_modified_ms = reader_core::now_millis();
        }
        let Some(hash) = self.current_book_hash.clone() else {
            return;
        };
        let progress = reader_core::sharing::ProgressEntry {
            book_hash: hash,
            title: self
                .book
                .as_ref()
                .map(|b| b.title.clone())
                .unwrap_or_default(),
            chapter: self.current_chapter,
            block: self.current_block,
            char_offset: self.current_char_offset,
            chapter_title: self
                .book
                .as_ref()
                .and_then(|b| b.chapters.get(self.current_chapter))
                .map(|c| c.title.clone()),
            timestamp: self.position_save_modified_ms / 1000,
        };
        if let Some(record) = self.cloud_record_progress(progress, self.position_save_modified_ms) {
            // Preserve this exact event for a deferred disk flush, including same-ms ordering.
            self.position_save_modified_ms = record.version.modified_ms;
            let mut store = self.peer_store.lock().unwrap_or_else(|e| e.into_inner());
            let versions =
                std::iter::once((record.progress.book_hash.clone(), record.version)).collect();
            store.merge_progress_versioned(&[record.progress], &versions);
            store.save(&self.data_dir);
        }
        self.last_synced_chapter = Some(self.current_chapter);
        self.last_synced_block = Some(self.current_block);
        self.last_synced_char_offset = Some(self.current_char_offset);
    }
}
