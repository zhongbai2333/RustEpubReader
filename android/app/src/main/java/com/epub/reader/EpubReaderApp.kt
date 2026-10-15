package com.zhongbai233.epub.reader

import android.app.Application
import android.content.res.Configuration
import com.zhongbai233.epub.reader.i18n.I18n

class EpubReaderApp : Application() {
    override fun onCreate() {super.onCreate();I18n.initialize(this)}
    override fun onConfigurationChanged(config:Configuration) {super.onConfigurationChanged(config);I18n.refreshSystemLanguage(config.locales[0].toLanguageTag())}
}
