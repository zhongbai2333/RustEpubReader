package com.zhongbai233.epub.reader.ui.library

import org.junit.Assert.*
import org.junit.Test

class StableShelfOrderTest {
    @Test fun openingTheRightBookKeepsItsOriginalSlotAndFreshMetadata() {
        val order=StableShelfOrder()
        assertEquals(listOf("left","right"),order.resolve(listOf("left","right"),false))
        assertTrue(order.resolve(emptyList(),false).isEmpty())
        // The actual library now ranks the opened right book first.
        assertEquals(listOf("left","right"),order.resolve(listOf("right","left"),false))
        val updated=mapOf("right" to "chapter 8","left" to "chapter 1")
        assertEquals("chapter 8",order.resolve(listOf("right","left"),false).map {updated[it]}[1])
        assertEquals(listOf("left","right","new"),order.resolve(listOf("new","right","left"),false))
        assertEquals(listOf("right","new"),order.resolve(listOf("new","right"),false))
        order.reset()
        assertEquals(listOf("new","right"),order.resolve(listOf("new","right"),false))
    }
    @Test fun closeReleasePromotesALaterRowBookAndShiftsTheOthers() {
        val order=StableShelfOrder()
        val shelf=listOf("a","b","c","d","e","f")
        val recent=listOf("e","a","b","c","d","f")
        assertEquals(shelf,order.resolve(shelf,false))
        assertEquals(shelf,order.resolve(recent,false))
        // BookPresentation calls reset only after the close dissolve completes.
        assertEquals(recent,order.resolve(recent,false,1))
        assertEquals(shelf,order.resolve(shelf,true,1))
        assertEquals(shelf,order.resolve(shelf,true,2))
    }
    @Test fun explicitSortingAppliesTheLatestRanks() {
        val order=StableShelfOrder()
        order.resolve(listOf("left","right"),false)
        assertEquals(listOf("right","left"),order.resolve(listOf("right","left"),true))
        assertEquals(listOf("left","right"),order.resolve(listOf("left","right"),true))
        assertEquals(listOf("right","left"),order.resolve(listOf("right","left"),false))
    }
}
