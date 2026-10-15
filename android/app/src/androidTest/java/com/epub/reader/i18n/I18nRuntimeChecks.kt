package com.zhongbai233.epub.reader.i18n

import android.app.Activity
import android.app.Instrumentation
import android.os.Bundle
import com.zhongbai233.epub.reader.RustBridge
import org.json.JSONObject

object I18nRuntimeChecks {
    fun run(runner: Instrumentation) {
        val report=JSONObject()
        try {
            for(code in listOf("en","zh_cn")) {
                val asset=JSONObject(runner.targetContext.assets.open("i18n/$code.json").bufferedReader().use {it.readText()})
                val native=JSONObject(checkNotNull(RustBridge.getTranslations(code)))
                check(asset.length()==native.length())
                asset.keys().forEach {key -> check(asset.getString(key)==native.getString(key)) {"Native catalog differs: $code/$key"}}
            }
            report.put("native_catalogs",true)
            runner.runOnMainSync {
                I18n.initialize(runner.targetContext)
                val preference=I18n.preference
                try {
                    I18n.setLanguage("zh_cn")
                    check(I18n.t("common.cancel")=="取消")
                    val stored=I18n.t("tts.playing")
                    val formatted=I18n.tf1("common.books_imported_count","1")
                    I18n.setLanguage("en")
                    I18n.refreshSystemLanguage("zh-CN")
                    check(I18n.currentCode=="en" && I18n.preference=="en")
                    check(I18n.message(stored)==I18n.t("tts.playing"))
                    check(I18n.message(formatted)=="Books imported: 1")
                    check(I18n.count("ui.library_count",1)=="1 book")
                    check(I18n.count("ui.library_count",2)=="2 books")
                    check(I18n.t("missing")=="Unavailable")
                    I18n.setLanguage("auto");I18n.refreshSystemLanguage("zh-Hant-HK")
                    check(I18n.preference=="auto" && I18n.currentCode=="zh_cn")
                    I18n.refreshSystemLanguage("fr-FR");check(I18n.currentCode=="en")
                    check(I18n.format("{1}/{0}",listOf("😀","{}"))=="{}/😀")
                    report.put("passed",true).put("catalogs",true).put("system_refresh",true).put("stored_status",true).put("plural",true).put("braces",true)
                } finally {I18n.setLanguage(preference)}
            }
        } catch(failure:Throwable) {report.put("passed",false).put("error",failure.toString())}
        java.io.File(runner.targetContext.cacheDir,"i18n-verification.json").writeText(report.toString(2))
        runner.finish(if(report.optBoolean("passed")) Activity.RESULT_OK else Activity.RESULT_CANCELED,Bundle().apply {putString("stream",report.toString(2))})
    }
}
