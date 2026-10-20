package com.zhongbai233.epub.reader.ui.library

import org.junit.Assert.assertEquals
import org.junit.Test

class LibraryBackupPresentationTest {
    @Test fun onlyKnownCausesReceiveSpecificRecoveryAdvice() {
        assertEquals("backup.error_version", backupFailureHint("Unsupported library backup version"))
        assertEquals("backup.error_changed", backupFailureHint("Backup changed after verification; select and verify it again"))
        assertEquals("backup.error_space", backupFailureHint("No space left on device (os error 28)"))
        assertEquals("backup.error_permission", backupFailureHint("Permission denied (os error 13)"))
        assertEquals("backup.error_hint", backupFailureHint("Cannot read library backup: invalid Zip archive"))
    }
}
