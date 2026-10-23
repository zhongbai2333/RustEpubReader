package com.zhongbai233.epub.reader.ui.reader

import android.app.Activity
import android.app.Instrumentation
import android.content.Intent
import android.os.Bundle
import android.view.accessibility.AccessibilityNodeInfo
import androidx.activity.compose.setContent
import androidx.compose.material3.MaterialTheme
import androidx.lifecycle.ViewModelProvider
import com.zhongbai233.epub.reader.MainActivity
import com.zhongbai233.epub.reader.RustBridge
import com.zhongbai233.epub.reader.i18n.I18n
import com.zhongbai233.epub.reader.viewmodel.ReaderViewModel
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.net.ServerSocket
import java.util.concurrent.atomic.AtomicReference

/** Isolated synthetic inputs only. Never submits a contribution or touches user accounts. */
internal object PlatformParityRuntimeChecks {
 fun run(test:Instrumentation){
  val context=test.targetContext
  check(context.packageName.startsWith("local.reader."))
  val out=File(context.cacheDir,"parity-ui").apply{mkdirs()};val report=JSONObject()
  fun nodes():List<AccessibilityNodeInfo>{val found=mutableListOf<AccessibilityNodeInfo>();fun visit(n:AccessibilityNodeInfo?){if(n!=null){found+=n;for(i in 0 until n.childCount)visit(n.getChild(i))}};visit(test.uiAutomation.rootInActiveWindow);return found}
  fun await(message:String,condition:()->Boolean){repeat(200){test.waitForIdleSync();if(condition())return;Thread.sleep(100)};error(message)}
  fun reveal(label:String){
   fun visible()=nodes().any{it.isVisibleToUser && (it.text?.toString()==label || it.contentDescription?.toString()==label)}
   test.waitForIdleSync();if(visible())return
   repeat(10){nodes().lastOrNull{it.isScrollable}?.performAction(AccessibilityNodeInfo.ACTION_SCROLL_BACKWARD);test.waitForIdleSync();Thread.sleep(80);if(visible())return}
   repeat(15){nodes().lastOrNull{it.isScrollable}?.performAction(AccessibilityNodeInfo.ACTION_SCROLL_FORWARD);test.waitForIdleSync();Thread.sleep(80);if(visible())return}
   await("Action not visible: $label"){visible()}
  }
  fun click(label:String){reveal(label);var node=nodes().lastOrNull{it.isVisibleToUser && (it.text?.toString()==label || it.contentDescription?.toString()==label)}?:error("Action missing: $label");repeat(7){if(!node.isClickable)node=node.parent?:node};check(node.performAction(AccessibilityNodeInfo.ACTION_CLICK))}
  fun screen(name:String){test.waitForIdleSync();Thread.sleep(200);test.uiAutomation.takeScreenshot()?.let{bmp->File(out,"$name.png").outputStream().use{bmp.compress(android.graphics.Bitmap.CompressFormat.PNG,100,it)};bmp.recycle()}}
  lateinit var vm:ReaderViewModel
  var diagnostics:()->Unit={}
  try{
   val activity=test.startActivitySync(Intent(context,MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TASK)) as MainActivity
   test.runOnMainSync{vm=ViewModelProvider(activity)[ReaderViewModel::class.java];vm.updateLanguage("zh_cn");vm.markReaderIntroductionShown()}
   diagnostics={report.put("txt_count",vm.txtHeadingCount).put("txt_analyzing",vm.txtAnalyzing).put("txt_error",vm.txtImportError)}
   val source=File(out,"manual.txt");val body="这是保留的完整正文，不应删除。".repeat(12)
   source.writeText("第一章 初到\n\n$body\n\n第2章 相逢\n\n$body\n\n§ 未知格式 §\n\n$body")
   // Keep the manual-review fixture independent of future heuristic improvements.
   test.runOnMainSync{vm.openTxtImportDialog(source.path);vm.setTxtRule(heuristic=false)}
   await("TXT analysis not ready"){!vm.txtAnalyzing && vm.txtHeadingCount==2}
   await("Native heading checkbox missing"){nodes().any{it.isCheckable&&it.isChecked}}
   click("第一章 初到")
   await("Excerpt page not opened"){nodes().any{it.text?.toString()?.contains("这是保留的完整正文")==true}}
   screen("android-txt-excerpt");click(I18n.t("settings.back"))
   click(I18n.t("txt.advanced"));await("Rules page not opened"){nodes().any{it.text?.toString()==I18n.t("txt.mode_heuristic")}}
   screen("android-txt-rules");click(I18n.t("settings.back"))
   await("Heading checkboxes not restored after navigating back"){nodes().any{it.isCheckable&&it.isChecked}}
   reveal("第一章 初到")
   val checkbox=nodes().firstOrNull{it.isCheckable&&it.isChecked}?:error("Native heading checkbox missing")
   check(checkbox.performAction(AccessibilityNodeInfo.ACTION_CLICK))
   await("Heading exclusion not applied"){!vm.txtAnalyzing && vm.txtExcluded.isNotEmpty() && vm.txtHeadingCount==1}
   click(I18n.t("txt.add_heading"))
   test.runOnMainSync{vm.txtSourceQuery="§"}
   click(I18n.t("txt.preview"))
   await("Original source search returned no line"){vm.txtSourceHits.size==1}
   await("Original source row not displayed"){nodes().any{it.text?.toString()=="§ 未知格式 §"}}
   screen("android-txt-source");click("§ 未知格式 §")
   await("Manual heading not applied"){!vm.txtAnalyzing && vm.txtHeadingCount==2}
   screen("android-txt-heading-review")
   click(I18n.t("txt.convert_import"))
   await("Selected TXT conversion failed"){!vm.showTxtImport && vm.currentBook!=null && !vm.isLoading}
   check(source.readText().contains("第一章 初到"));report.put("txt_exclusion_addition_conversion",true)
   test.runOnMainSync{vm.updateLatinFontFamily("Monospace")}
   check(context.getSharedPreferences("reader_settings",0).getString("reader.latin_font_family",null)=="Monospace")
   report.put("latin_font_selection_persisted",true)
   val server=ServerSocket(0,2,java.net.InetAddress.getByName("127.0.0.1")).apply{soTimeout=30000}
   val failure=AtomicReference<Throwable?>(null)
   val worker=Thread{
    try{repeat(2){index->server.accept().use{socket->socket.soTimeout=8000;val reader=socket.getInputStream().bufferedReader();val first=reader.readLine();check(first.startsWith("POST /lookup "));var length=0;while(true){val line=reader.readLine();if(line.isEmpty())break;if(line.startsWith("Content-Length:",true))length=line.substringAfter(':').trim().toInt()};val chars=CharArray(length);var at=0;while(at<length){val n=reader.read(chars,at,length-at);check(n>0);at+=n};val body=JSONObject(String(chars));check(body.getString("text")=="synthetic selected text");check(body.getString("kind")=="dictionary")
     val payload=if(index==0)"{}" else JSONObject().put("text","合成词典释义").toString();val bytes=payload.toByteArray();socket.getOutputStream().write("HTTP/1.1 ${if(index==0)503 else 200} Test\r\nContent-Type: application/json\r\nContent-Length: ${bytes.size}\r\nConnection: close\r\n\r\n".toByteArray()+bytes)
    }}}catch(e:Throwable){failure.set(e)}finally{server.close()}
   }.apply{isDaemon=true;start()}
   test.runOnMainSync{vm.closeBook();activity.setContent{MaterialTheme{DictionaryDialog("synthetic selected text","http://127.0.0.1:${server.localPort}/lookup","",{})}}}
   await("Service failure not shown"){nodes().any{it.text?.toString()?.contains("503")==true}}
   screen("android-service-retry");click(I18n.t("services.retry"))
   await("Service result not shown"){nodes().any{it.text?.toString()=="合成词典释义"}}
   worker.join(1000);failure.get()?.let{throw it};check(!worker.isAlive);screen("android-dictionary-result")
   report.put("native_service_failure_retry_result",true)
   val fixture=File(out,"illustrations.epub");test.context.assets.open("illustrations.epub").use{input->fixture.outputStream().use{input.copyTo(it)}}
   val added=JSONObject(checkNotNull(RustBridge.addOrUpdateBook(context.filesDir.path,"Parity illustrations",fixture.path,0,"")))
   val managed=added.getString("uri")
   val exports=JSONObject(checkNotNull(RustBridge.prepareBookExports(context.filesDir.path,JSONArray(listOf(managed)).toString(),File(out,"exports").path,"full")))
   check(exports.optBoolean("ok")){exports.toString()};val exported=File(exports.getJSONArray("paths").getString(0))
   fun members(file:File):Map<String,ByteArray>{return java.util.zip.ZipFile(file).use{zip->zip.entries().asSequence().associate{entry->entry.name to zip.getInputStream(entry).use{it.readBytes()}}}}
   val before=members(File(managed));val after=members(exported);check(before.keys==after.keys);before.forEach{(name,bytes)->check(bytes.contentEquals(after[name])){"Export altered $name"}}
   report.put("jni_full_export_preserves_resources",true)
   val discovered=AtomicReference<List<com.zhongbai233.epub.reader.model.DiscoveredPeer>>(emptyList())
   lateinit var browse:com.zhongbai233.epub.reader.util.NearbyDiscovery
   val syntheticId="339036a1-0663-40b7-8d50-33e508269e9d"
   val nsd=context.getSystemService(android.content.Context.NSD_SERVICE) as android.net.nsd.NsdManager
   val registration=object:android.net.nsd.NsdManager.RegistrationListener{
    override fun onServiceRegistered(info:android.net.nsd.NsdServiceInfo){}
    override fun onRegistrationFailed(info:android.net.nsd.NsdServiceInfo,code:Int){}
    override fun onServiceUnregistered(info:android.net.nsd.NsdServiceInfo){}
    override fun onUnregistrationFailed(info:android.net.nsd.NsdServiceInfo,code:Int){}
   }
   try{
    test.runOnMainSync{
     browse=com.zhongbai233.epub.reader.util.NearbyDiscovery(context){discovered.set(it)};browse.start()
     val info=android.net.nsd.NsdServiceInfo().apply{serviceName="parity-synthetic";serviceType="_epubreader._tcp.";port=34562;setAttribute("id",syntheticId);setAttribute("name","Synthetic Android peer")}
     nsd.registerService(info,android.net.nsd.NsdManager.PROTOCOL_DNS_SD,registration)
    }
    await("Native DNS-SD peer not discovered"){discovered.get().any{it.deviceId==syntheticId && it.addr.endsWith(":34562")}}
    report.put("native_dns_sd_discovery",true)
   }finally{test.runOnMainSync{browse.close();runCatching{nsd.unregisterService(registration)}}}
   val slot="service_parity_test";com.zhongbai233.epub.reader.util.GitHubTokenStore.save(context,"synthetic-api-key",slot).getOrThrow()
   check(com.zhongbai233.epub.reader.util.GitHubTokenStore.load(context,slot)=="synthetic-api-key")
   check(!File(context.applicationInfo.dataDir,"shared_prefs/github_credentials.xml").readText().contains("synthetic-api-key"))
   com.zhongbai233.epub.reader.util.GitHubTokenStore.delete(context,slot);report.put("keystore_service_key_roundtrip",true)
   report.put("passed",true)
  }catch(error:Throwable){report.put("passed",false).put("error",error.toString());diagnostics();screen("failure")}
  File(out,"result.json").writeText(report.toString(2));test.finish(if(report.optBoolean("passed"))Activity.RESULT_OK else Activity.RESULT_CANCELED,Bundle().apply{putString("stream",report.toString(2))})
 }
}
