package com.zhongbai233.epub.reader.ui.reader

import com.zhongbai233.epub.reader.model.*
import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Test

class AppearanceLinkageTest {
    @Test fun firstInstallUsesAmbientWithoutChangingLegacyDecode() {
        val first=AppearanceLinkage.firstRun()
        assertEquals(AutomaticAppearanceSource.ambient,first.source)
        assertTrue(first.appLinked && first.readingLinked)
        assertFalse(first.readingDark(true,false))
        assertTrue(first.readingDark(true,null))
        assertEquals(first,AppearanceLinkage.decode(first.encoded()))
        assertEquals(AutomaticAppearanceSource.system,AppearanceLinkage.decode("{}")!!.source)
    }
    @Test fun sharedPlatformContract() {
        val cases=Json.parseToJsonElement(javaClass.getResourceAsStream("/appearance-linkage.json")!!.bufferedReader().use {it.readText()}).jsonArray
        for(item in cases.map {it.jsonObject}) {
            var value=AppearanceLinkage.decode(item.getValue("initial").toString())!!
            var system=item.getValue("system").jsonPrimitive.boolean
            var ambient=item.getValue("ambient").jsonPrimitive.booleanOrNull
            for(action in item.getValue("actions").jsonArray.map {it.jsonPrimitive.content}) {
                val parts=action.split(":");val arg=parts.getOrElse(1){""}
                when(parts[0]) {
                    "source"->value=value.copy(source=AutomaticAppearanceSource.valueOf(arg))
                    "system"->system=arg=="dark"
                    "ambient"->ambient=arg=="dark"
                    "appLink"->value=value.toggleAppLink(system,ambient)
                    "readingLink"->value=value.toggleReadingLink(system,ambient)
                    "app"->value=value.setApp(arg=="dark")
                    "reading"->value=value.copy(readingMode=ReadingDisplayMode.valueOf(arg))
                    "readingSource"->value=value.copy(readingSource=AutomaticAppearanceSource.valueOf(arg))
                    "shortcut"->value=value.toggleReading(system,ambient)
                    else->error(action)
                }
                assertEquals(value,AppearanceLinkage.decode(value.encoded()))
                value=AppearanceLinkage.decode(value.encoded())!!
            }
            val expected=item.getValue("expected").jsonObject;val name=item.getValue("name").jsonPrimitive.content
            assertEquals(name,expected.getValue("app").jsonPrimitive.boolean,value.appDark(system,ambient))
            assertEquals(name,expected.getValue("reading").jsonPrimitive.boolean,value.readingDark(system,ambient))
            assertEquals(name,expected.getValue("appLinked").jsonPrimitive.boolean,value.appLinked)
            assertEquals(name,expected.getValue("readingLinked").jsonPrimitive.boolean,value.readingLinked)
        }
    }
    @Test fun migrationRetainsIndependentSurfaces() {
        for(app in listOf(false,true)) for(mode in ReadingDisplayMode.entries) {
            val value=AppearanceLinkage.migrate(app,mode)
            assertEquals(app,value.appDark(!app,null))
            assertEquals(mode.isDark(true),value.readingDark(true,true))
        }
        assertNull(AppearanceLinkage.decode("invalid"))
    }
}
