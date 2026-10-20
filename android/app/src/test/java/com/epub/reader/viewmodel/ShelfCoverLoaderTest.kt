package com.zhongbai233.epub.reader.viewmodel

import kotlinx.coroutines.*
import kotlinx.coroutines.channels.Channel
import org.junit.Assert.*
import org.junit.Test

class ShelfCoverLoaderTest {
    @Test fun duplicateRequestsShareWorkAndArchivedBooksDoNotReceiveLateResults() = runBlocking {
        val scope=CoroutineScope(coroutineContext+SupervisorJob(coroutineContext[Job]))
        val gate=CompletableDeferred<Unit>();val started=CompletableDeferred<Unit>()
        val members=mutableSetOf("book");var reads=0;var requests=0
        val loader=ShelfCoverLoader(scope,{it in members},{requests++},{reads++;started.complete(Unit);gate.await();byteArrayOf(1)},Dispatchers.Unconfined)
        try {
            loader.request("book");loader.request("book");started.await()
            assertEquals(1,reads);assertEquals(1,requests)
            members.clear();gate.complete(Unit)
            scope.coroutineContext[Job]!!.children.toList().joinAll()
            assertFalse(loader.cache.containsKey("book"))
            members.add("book");loader.request("book")
            scope.coroutineContext[Job]!!.children.toList().joinAll()
            assertArrayEquals(byteArrayOf(1),loader.cache["book"])
        } finally {scope.cancel()}
    }

    @Test fun concurrencyAndCacheStayBoundedAndMissingCoversAreCached() = runBlocking {
        val scope=CoroutineScope(coroutineContext+SupervisorJob(coroutineContext[Job]))
        val starts=Channel<String>(Channel.UNLIMITED)
        val permits=Channel<Unit>(Channel.UNLIMITED)
        var active=0;var peak=0;var reads=0
        val loader=ShelfCoverLoader(scope,{true},{},{uri->
            reads++;active++;peak=maxOf(peak,active);starts.send(uri);permits.receive();active--
            if(uri=="missing") null else uri.toByteArray()
        },Dispatchers.Unconfined,capacity=3)
        try {
            for(uri in listOf("a","b","c","d","missing")) loader.request(uri)
            withTimeout(5_000) {starts.receive();starts.receive()}
            yield();assertEquals(2,reads);assertEquals(2,peak)
            repeat(5) {permits.send(Unit)}
            withTimeout(5_000) {scope.coroutineContext[Job]!!.children.toList().joinAll()}
            assertEquals(2,peak);assertEquals(3,loader.cache.size)
            assertFalse(loader.cache.containsKey("a"));assertFalse(loader.cache.containsKey("b"))
            assertTrue(loader.cache.containsKey("missing"));assertNull(loader.cache["missing"])
            loader.request("missing");yield();assertEquals(5,reads)
            loader.request("c") // Touch c before admitting another cover.
            loader.request("e");permits.send(Unit)
            withTimeout(5_000) {scope.coroutineContext[Job]!!.children.toList().joinAll()}
            assertTrue(loader.cache.containsKey("c"));assertFalse(loader.cache.containsKey("d"))
        } finally {scope.cancel()}
    }
}
