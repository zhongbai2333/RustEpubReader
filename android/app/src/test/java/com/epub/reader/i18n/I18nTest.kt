package com.zhongbai233.epub.reader.i18n
import org.junit.Assert.assertEquals
import org.junit.Test
class I18nTest {
    @Test fun preferenceAndSystemLocaleStaySeparate() {
        assertEquals("zh_cn",I18n.resolve("auto","zh-Hant-HK"))
        assertEquals("en",I18n.resolve("auto","fr-FR"))
        assertEquals("en",I18n.resolve("en","zh-CN"))
        assertEquals("auto",I18n.normalize("system"))
        assertEquals("en",I18n.normalize(" EN_us "))
    }
    @Test fun parametersAreNeverRescannedAsTemplateSyntax() {
        assertEquals("literal {} / second",I18n.format("{} / {}",listOf("literal {}","second")))
        assertEquals("{1} / 😀",I18n.format("{1} / {0}",listOf("😀","{1}")))
        assertEquals("{{}}",I18n.format("{{{name}}}",emptyList(),mapOf("name" to "{}")))
        assertEquals("{4}",I18n.format("{4}",listOf("x")))
    }
}
