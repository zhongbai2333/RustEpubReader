package com.zhongbai233.epub.reader.util

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class GithubSyncSessionTest {
    @Test fun syncNeedsFullRepoScope() {
        assertFalse(hasGithubRepoScope(""))
        assertFalse(hasGithubRepoScope("public_repo, repo:status"))
        assertFalse(hasGithubRepoScope("repo_hook"))
        assertTrue(hasGithubRepoScope("read:user, repo"))
    }
    @Test fun resultForPreviousAccountIsDiscarded() { assertFalse(canApplyGithubSync(1, 1, "alice", "bob", true, true)) }
    @Test fun logoutDiscardsResult() { assertFalse(canApplyGithubSync(1, 1, "alice", null, true, true)) }
    @Test fun cancelledGenerationCannotApplyAfterReenable() { assertFalse(canApplyGithubSync(1, 2, "alice", "alice", true, true)) }
    @Test fun disabledOrBackgroundSessionCannotApply() {
        assertFalse(canApplyGithubSync(1, 1, "alice", "alice", false, true))
        assertFalse(canApplyGithubSync(1, 1, "alice", "alice", true, false))
    }
    @Test fun currentForegroundSessionCanApply() { assertTrue(canApplyGithubSync(1, 1, "alice", "alice", true, true)) }
}
