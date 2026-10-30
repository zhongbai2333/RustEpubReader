package com.zhongbai233.epub.reader.model

import org.json.JSONObject

data class BookCategory(val id: String, val name: String)
data class BookClassification(
    val categories: List<BookCategory> = emptyList(),
    val assignments: Map<String, String> = emptyMap(),
) {
    fun matches(id: String, filter: String?): Boolean = when (filter) {
        null -> true
        "" -> id !in assignments
        else -> assignments[id] == filter
    }
    companion object {
        fun parse(value: JSONObject): BookClassification {
            val categories = value.getJSONArray("categories")
            val assignments = value.getJSONObject("assignments")
            return BookClassification(
                (0 until categories.length()).map { categories.getJSONObject(it).let { c -> BookCategory(c.getString("id"), c.getString("name")) } },
                assignments.keys().asSequence().associateWith { assignments.getString(it) },
            )
        }
    }
}
