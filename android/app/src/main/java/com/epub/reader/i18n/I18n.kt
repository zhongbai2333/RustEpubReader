package com.zhongbai233.epub.reader.i18n

import android.content.Context
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import org.json.JSONObject
import java.util.Locale

object I18n {
    var currentCode: String by mutableStateOf("en"); private set
    var preference: String by mutableStateOf("auto"); private set
    val isAuto: Boolean get()=preference=="auto"
    var version: Int by mutableIntStateOf(0); private set
    private val messageCache=mutableMapOf<String,String>()
    private var catalogs: Map<String,Map<String,String>> = emptyMap()
    private var translations: Map<String,String> by mutableStateOf(emptyMap())
    val locale: Locale get()=if(currentCode=="zh_cn") Locale.SIMPLIFIED_CHINESE else Locale.ENGLISH
    val availableLanguages: List<Pair<String,String>> get()=listOf("auto" to t("language.auto"),"zh_cn" to "中文","en" to "English")

    fun initialize(context: Context) {
        val loaded=mutableMapOf<String,Map<String,String>>()
        for(code in listOf("en","zh_cn")) {
            runCatching {
                val text=context.assets.open("i18n/$code.json").bufferedReader().use {it.readText()}
                val json=JSONObject(text)
                buildMap<String,String> { json.keys().forEach {key -> put(key,json.getString(key))} }
            }.getOrNull()?.takeIf {it.isNotEmpty()}?.let {loaded[code]=it}
        }
        if(loaded["en"]!=null) catalogs=loaded
        setLanguage(context.getSharedPreferences("reader_settings",Context.MODE_PRIVATE).getString("reader_language","auto") ?: "auto")
    }
    fun normalize(code: String): String=when(code.trim().lowercase(Locale.ROOT).replace('_','-').substringBefore('-')) {"zh"->"zh_cn";"en"->"en";else->"auto"}
    fun resolve(code:String,system:String):String {
        val selected=normalize(code)
        return if(selected!="auto") selected else if(system.lowercase(Locale.ROOT).replace('_','-').substringBefore('-')=="zh") "zh_cn" else "en"
    }
    fun setLanguage(code:String) {preference=normalize(code);refreshSystemLanguage()}
    fun detectSystemLanguage():String=resolve("auto",Locale.getDefault().toLanguageTag())
    fun refreshSystemLanguage(system:String=Locale.getDefault().toLanguageTag()) {
        val resolved=resolve(preference,system)
        val selected=catalogs[resolved] ?: catalogs["en"] ?: return
        if(resolved!=currentCode || translations.isEmpty()) {
            translations=(catalogs["en"] ?: emptyMap())+selected;currentCode=resolved;version++
        }
    }
    fun t(key:String):String=translations[key] ?: catalogs["en"]?.get(key) ?: if(currentCode=="zh_cn") "暂时不可用" else "Unavailable"
    fun tf(key:String,vararg args:String):String=format(t(key),args.toList())
    fun tf1(key:String,arg:String):String=tf(key,arg)
    fun tf2(key:String,arg1:String,arg2:String):String=tf(key,arg1,arg2)
    fun count(key:String,value:Int):String {
        val variant="$key.${if(value==1) "one" else "other"}"
        return tf(if(translations.containsKey(variant)) variant else key,value.toString())
    }
    // Re-render stored app statuses after a language change; never translate book/user data.
    fun message(value:String):String = synchronized(messageCache) {
        val key="$currentCode:$value"
        messageCache[key]?.let {return@synchronized it}
        val translated=resolveMessage(value)
        if(messageCache.size>=128) messageCache.clear()
        messageCache[key]=translated;translated
    }
    private fun resolveMessage(value:String):String {
        for(code in listOf("en","zh_cn")) {
            val catalog=catalogs[code] ?: continue
            catalog.entries.firstOrNull {it.value==value}?.let {return t(it.key)}
            val slots=Regex("\\{(\\d*)\\}")
            for((key,template) in catalog.entries.sortedByDescending {it.value.length}) {
                val matches=slots.findAll(template).toList();if(matches.isEmpty()) continue
                val indices=mutableListOf<Int>();var last=0
                val pattern=buildString {
                    append("^")
                    matches.forEach {m -> append(Regex.escape(template.substring(last,m.range.first)));append("(.*?)");indices.add(m.groupValues[1].toIntOrNull() ?: indices.size);last=m.range.last+1}
                    append(Regex.escape(template.substring(last)));append("$")
                }
                val match=Regex(pattern).matchEntire(value) ?: continue
                val args=MutableList((indices.maxOrNull() ?: 0)+1) {""}
                indices.forEachIndexed {group,index -> args[index]=match.groupValues[group+1]}
                return format(t(key),args)
            }
        }
        return value
    }
    fun format(template:String,args:List<String>,named:Map<String,String> = emptyMap()):String {
        val out=StringBuilder();var at=0;var next=0
        while(at<template.length) {
            if(template.startsWith("{{",at)) {out.append('{');at+=2;continue}
            if(template.startsWith("}}",at)) {out.append('}');at+=2;continue}
            if(template[at]=='{') {
                val end=template.indexOf('}',at+1)
                if(end>=0) {
                    val token=template.substring(at+1,end)
                    val value=if(token.isEmpty()) args.getOrNull(next++) else token.toIntOrNull()?.let {args.getOrNull(it)} ?: named[token]
                    if(value!=null) {out.append(value);at=end+1;continue}
                }
            }
            out.append(template[at++])
        }
        return out.toString()
    }
}
